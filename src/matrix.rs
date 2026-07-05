use anyhow::{anyhow, Result};
use matrix_sdk::{
    authentication::matrix::{MatrixSession, MatrixSessionTokens},
    config::SyncSettings,
    ruma::{
        events::room::message::{
            MessageType, NoticeMessageEventContent, RoomMessageEventContent,
            TextMessageEventContent,
        },
        OwnedDeviceId, OwnedUserId, RoomId,
    },
    Client, SessionMeta,
};
use serde::Deserialize;
use std::time::Duration;

use crate::config::{Auth, Config};
use crate::render::RenderedMessage;

#[derive(Deserialize)]
struct WhoamiResponse {
    user_id: OwnedUserId,
    device_id: Option<OwnedDeviceId>,
}

async fn whoami(homeserver: &str, token: &str) -> Result<WhoamiResponse> {
    let url = format!(
        "{}/_matrix/client/v3/account/whoami",
        homeserver.trim_end_matches('/')
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(whoami_error(status, &body)));
    }
    Ok(resp.json::<WhoamiResponse>().await?)
}

fn whoami_error(status: reqwest::StatusCode, body: &str) -> String {
    let errcode = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("errcode").and_then(|c| c.as_str().map(String::from)));
    match errcode.as_deref() {
        Some("M_UNKNOWN_TOKEN") => "Matrix access token rejected (M_UNKNOWN_TOKEN): it has been \
             revoked or expired. Generate a new token, or switch to MATRIX_USER/MATRIX_PASSWORD \
             login, which obtains a fresh token on every run."
            .into(),
        _ => format!(
            "whoami failed (check token and homeserver): HTTP {}: {}",
            status, body
        ),
    }
}

pub async fn build_client(config: &Config) -> Result<Client> {
    let client = if config.store_path.is_empty() {
        Client::builder()
            .homeserver_url(&config.homeserver)
            .build()
            .await?
    } else {
        std::fs::create_dir_all(&config.store_path)?;
        Client::builder()
            .homeserver_url(&config.homeserver)
            .sqlite_store(&config.store_path, None)
            .build()
            .await?
    };

    if !client.logged_in() {
        match &config.auth {
            Auth::Token(token) => {
                let identity = whoami(&config.homeserver, token).await?;
                let session = MatrixSession {
                    meta: SessionMeta {
                        user_id: identity.user_id,
                        device_id: identity
                            .device_id
                            .unwrap_or_else(|| OwnedDeviceId::from("MATRIX_NOTIFY")),
                    },
                    tokens: MatrixSessionTokens {
                        access_token: token.clone(),
                        refresh_token: None,
                    },
                };
                client.restore_session(session).await?;
            }
            Auth::Password { user, password } => {
                let mut login = client
                    .matrix_auth()
                    .login_username(user, password)
                    .initial_device_display_name("matrix-notify-action");
                // A fixed device ID keeps the cached E2EE crypto store valid
                // across runs; without a store the server assigns one and the
                // device is deleted again on logout.
                if !config.store_path.is_empty() {
                    login = login.device_id(&config.device_id);
                }
                login
                    .send()
                    .await
                    .map_err(|e| anyhow!("password login failed: {}", e))?;
            }
        }
    }

    Ok(client)
}

/// Log out password-login sessions so devices don't accumulate on the
/// homeserver. Skipped for E2EE stores (logout would delete the device and
/// its keys) and for token auth (the token must stay valid for future runs).
/// Best-effort: the message is already sent, so failure only warns.
pub async fn maybe_logout(client: &Client, config: &Config) {
    if matches!(config.auth, Auth::Password { .. }) && config.store_path.is_empty() {
        if let Err(e) = client.matrix_auth().logout().await {
            eprintln!("Warning: logout failed (device may linger): {}", e);
        }
    }
}

pub async fn send_message(
    client: &Client,
    config: &Config,
    rendered: &RenderedMessage,
) -> Result<String> {
    let sync_settings = SyncSettings::default().timeout(Duration::from_secs(30));
    tokio::time::timeout(Duration::from_secs(35), client.sync_once(sync_settings))
        .await
        .map_err(|_| anyhow!("Matrix sync timed out after 30s"))?
        .map_err(|e| anyhow!("Matrix sync failed: {}", e))?;

    let room_id = RoomId::parse(&config.room_id)
        .map_err(|_| anyhow!("Invalid room_id format: {}", config.room_id))?;

    let room = client
        .get_room(&room_id)
        .ok_or_else(|| anyhow!("Room {} not found — is the bot a member?", config.room_id))?;

    if room.is_encrypted().await? && config.store_path.is_empty() {
        return Err(anyhow!(
            "Room {} is encrypted but MATRIX_STORE_PATH is not set. \
             Provide a store_path to enable E2EE.",
            config.room_id
        ));
    }

    let msg_type = match config.msgtype.as_str() {
        "m.text" => match &rendered.formatted_body {
            Some(html) => MessageType::Text(TextMessageEventContent::html(
                rendered.body.clone(),
                html.clone(),
            )),
            None => MessageType::Text(TextMessageEventContent::plain(rendered.body.clone())),
        },
        _ => match &rendered.formatted_body {
            Some(html) => MessageType::Notice(NoticeMessageEventContent::html(
                rendered.body.clone(),
                html.clone(),
            )),
            None => MessageType::Notice(NoticeMessageEventContent::plain(rendered.body.clone())),
        },
    };

    let content = RoomMessageEventContent::new(msg_type);
    let response = room.send(content).await?;

    Ok(response.event_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(auth: Auth, homeserver: String, store_path: &str) -> Config {
        Config {
            homeserver,
            auth,
            room_id: "!room:example.org".into(),
            message: "hi".into(),
            format: "plain".into(),
            msgtype: "m.notice".into(),
            store_path: store_path.into(),
            device_id: "MATRIX_NOTIFY".into(),
        }
    }

    async fn mock_versions(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/_matrix/client/versions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "versions": ["v1.1", "v1.10"] })),
            )
            .mount(server)
            .await;
    }

    fn login_response() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "user_id": "@bot:example.org",
            "access_token": "fresh_token",
            "device_id": "MATRIX_NOTIFY"
        }))
    }

    #[tokio::test]
    async fn unknown_token_error_is_actionable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "errcode": "M_UNKNOWN_TOKEN",
                "error": "Token is not active",
                "soft_logout": false
            })))
            .mount(&server)
            .await;

        let config = test_config(Auth::Token("dead_token".into()), server.uri(), "");
        let err = build_client(&config).await.unwrap_err().to_string();
        assert!(err.contains("M_UNKNOWN_TOKEN"), "got: {}", err);
        assert!(err.contains("revoked or expired"), "got: {}", err);
        assert!(err.contains("MATRIX_USER"), "got: {}", err);
    }

    #[tokio::test]
    async fn password_login_logs_in() {
        let server = MockServer::start().await;
        mock_versions(&server).await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .respond_with(login_response())
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config(
            Auth::Password {
                user: "@bot:example.org".into(),
                password: "s3cret".into(),
            },
            server.uri(),
            "",
        );
        let client = build_client(&config).await.unwrap();
        assert!(client.logged_in());
    }

    #[tokio::test]
    async fn password_login_without_store_lets_server_assign_device() {
        let server = MockServer::start().await;
        mock_versions(&server).await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .respond_with(login_response())
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config(
            Auth::Password {
                user: "@bot:example.org".into(),
                password: "s3cret".into(),
            },
            server.uri(),
            "",
        );
        build_client(&config).await.unwrap();

        let login_request = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.url.path().ends_with("/login"))
            .expect("no login request received");
        let body: serde_json::Value = login_request.body_json().unwrap();
        assert!(
            body.get("device_id").is_none(),
            "device_id should be server-assigned without a store, got: {}",
            body
        );
    }

    #[tokio::test]
    async fn password_login_with_store_uses_stable_device_id() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        mock_versions(&server).await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .and(body_partial_json(json!({ "device_id": "MATRIX_NOTIFY" })))
            .respond_with(login_response())
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config(
            Auth::Password {
                user: "@bot:example.org".into(),
                password: "s3cret".into(),
            },
            server.uri(),
            dir.path().to_str().unwrap(),
        );
        let client = build_client(&config).await.unwrap();
        assert!(client.logged_in());
    }

    #[tokio::test]
    async fn logs_out_after_password_login_without_store() {
        let server = MockServer::start().await;
        mock_versions(&server).await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .respond_with(login_response())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/logout"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;

        let config = test_config(
            Auth::Password {
                user: "@bot:example.org".into(),
                password: "s3cret".into(),
            },
            server.uri(),
            "",
        );
        let client = build_client(&config).await.unwrap();
        maybe_logout(&client, &config).await;
    }

    #[tokio::test]
    async fn keeps_device_when_store_is_used() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        mock_versions(&server).await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/login"))
            .respond_with(login_response())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/logout"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(0)
            .mount(&server)
            .await;

        let config = test_config(
            Auth::Password {
                user: "@bot:example.org".into(),
                password: "s3cret".into(),
            },
            server.uri(),
            dir.path().to_str().unwrap(),
        );
        let client = build_client(&config).await.unwrap();
        maybe_logout(&client, &config).await;
    }

    #[tokio::test]
    async fn never_logs_out_token_sessions() {
        let server = MockServer::start().await;
        mock_versions(&server).await;
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user_id": "@bot:example.org",
                "device_id": "TOKENDEVICE"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/_matrix/client/v3/logout"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(0)
            .mount(&server)
            .await;

        let config = test_config(Auth::Token("live_token".into()), server.uri(), "");
        let client = build_client(&config).await.unwrap();
        maybe_logout(&client, &config).await;
    }
}
