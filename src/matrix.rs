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

use crate::config::Config;
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
        .await?
        .error_for_status()
        .map_err(|e| anyhow!("whoami failed (check token and homeserver): {}", e))?
        .json::<WhoamiResponse>()
        .await?;
    Ok(resp)
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
        let identity = whoami(&config.homeserver, &config.token).await?;
        let session = MatrixSession {
            meta: SessionMeta {
                user_id: identity.user_id,
                device_id: identity
                    .device_id
                    .unwrap_or_else(|| OwnedDeviceId::from("MATRIX_NOTIFY")),
            },
            tokens: MatrixSessionTokens {
                access_token: config.token.clone(),
                refresh_token: None,
            },
        };
        client.restore_session(session).await?;
    }

    Ok(client)
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
