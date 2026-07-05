use anyhow::{anyhow, bail, Result};
use url::Url;

#[derive(Debug)]
pub enum Auth {
    Token(String),
    Password { user: String, password: String },
}

#[derive(Debug)]
pub struct Config {
    pub homeserver: String,
    pub auth: Auth,
    pub room_id: String,
    pub message: String,
    pub format: String,
    pub msgtype: String,
    pub store_path: String,
    pub device_id: String,
}

impl Config {
    pub fn from_env() -> Result<Config> {
        let homeserver = require_env("MATRIX_HOMESERVER")?;
        let auth = auth_from_env()?;
        let room_id = require_env("MATRIX_ROOM_ID")?;
        let message = require_env("MATRIX_MESSAGE")?;
        let format = std::env::var("MATRIX_FORMAT").unwrap_or_else(|_| "markdown".into());
        let msgtype = std::env::var("MATRIX_MSGTYPE").unwrap_or_else(|_| "m.notice".into());
        let store_path = std::env::var("MATRIX_STORE_PATH").unwrap_or_default();
        let device_id = std::env::var("MATRIX_DEVICE_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "MATRIX_NOTIFY".into());

        // Validate homeserver URL
        Url::parse(&homeserver)
            .map_err(|_| anyhow!("MATRIX_HOMESERVER is not a valid URL: {}", homeserver))?;

        // Validate format
        match format.as_str() {
            "markdown" | "plain" | "html" => {}
            other => bail!("MATRIX_FORMAT must be markdown|plain|html, got: {}", other),
        }

        // Validate msgtype
        match msgtype.as_str() {
            "m.notice" | "m.text" => {}
            other => bail!("MATRIX_MSGTYPE must be m.notice|m.text, got: {}", other),
        }

        Ok(Config {
            homeserver,
            auth,
            room_id,
            message,
            format,
            msgtype,
            store_path,
            device_id,
        })
    }
}

fn auth_from_env() -> Result<Auth> {
    let token = std::env::var("MATRIX_TOKEN").ok().filter(|v| !v.is_empty());
    let user = std::env::var("MATRIX_USER").ok().filter(|v| !v.is_empty());
    let password = std::env::var("MATRIX_PASSWORD")
        .ok()
        .filter(|v| !v.is_empty());

    match (token, user, password) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            bail!("Provide either MATRIX_TOKEN or MATRIX_USER+MATRIX_PASSWORD, not both")
        }
        (Some(token), None, None) => Ok(Auth::Token(token)),
        (None, Some(user), Some(password)) => Ok(Auth::Password { user, password }),
        (None, Some(_), None) => bail!("MATRIX_USER is set but MATRIX_PASSWORD is missing"),
        (None, None, Some(_)) => bail!("MATRIX_PASSWORD is set but MATRIX_USER is missing"),
        (None, None, None) => bail!(
            "Authentication is required: set MATRIX_TOKEN, or MATRIX_USER and MATRIX_PASSWORD"
        ),
    }
}

fn require_env(key: &str) -> Result<String> {
    let val = std::env::var(key).map_err(|_| anyhow!("{} is required but not set", key))?;
    if val.is_empty() {
        bail!("{} is set but empty", key);
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // Tests mutate shared process env vars, so they must not run concurrently.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_required(homeserver: &str, token: &str, room_id: &str, message: &str) {
        std::env::set_var("MATRIX_HOMESERVER", homeserver);
        std::env::set_var("MATRIX_TOKEN", token);
        std::env::set_var("MATRIX_ROOM_ID", room_id);
        std::env::set_var("MATRIX_MESSAGE", message);
    }

    fn clear_all() -> MutexGuard<'static, ()> {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for key in &[
            "MATRIX_HOMESERVER",
            "MATRIX_TOKEN",
            "MATRIX_ROOM_ID",
            "MATRIX_MESSAGE",
            "MATRIX_FORMAT",
            "MATRIX_MSGTYPE",
            "MATRIX_STORE_PATH",
            "MATRIX_USER",
            "MATRIX_PASSWORD",
            "MATRIX_DEVICE_ID",
        ] {
            std::env::remove_var(key);
        }
        guard
    }

    #[test]
    fn parses_required_fields() {
        let _guard = clear_all();
        set_required(
            "https://matrix.org",
            "syt_token",
            "!room:matrix.org",
            "hello",
        );
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.homeserver, "https://matrix.org");
        assert_eq!(cfg.room_id, "!room:matrix.org");
        assert_eq!(cfg.message, "hello");
        assert_eq!(cfg.format, "markdown");
        assert_eq!(cfg.msgtype, "m.notice");
        assert_eq!(cfg.store_path, "");
    }

    #[test]
    fn applies_defaults() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.format, "markdown");
        assert_eq!(cfg.msgtype, "m.notice");
        assert_eq!(cfg.store_path, "");
    }

    #[test]
    fn overrides_defaults() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_FORMAT", "plain");
        std::env::set_var("MATRIX_MSGTYPE", "m.text");
        std::env::set_var("MATRIX_STORE_PATH", "/tmp/store");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.format, "plain");
        assert_eq!(cfg.msgtype, "m.text");
        assert_eq!(cfg.store_path, "/tmp/store");
    }

    #[test]
    fn errors_on_missing_required() {
        let _guard = clear_all();
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn parses_token_auth() {
        let _guard = clear_all();
        set_required(
            "https://matrix.org",
            "syt_token",
            "!room:matrix.org",
            "hello",
        );
        let cfg = Config::from_env().unwrap();
        match cfg.auth {
            Auth::Token(t) => assert_eq!(t, "syt_token"),
            _ => panic!("expected token auth"),
        }
    }

    #[test]
    fn parses_password_auth() {
        let _guard = clear_all();
        std::env::set_var("MATRIX_HOMESERVER", "https://matrix.org");
        std::env::set_var("MATRIX_ROOM_ID", "!room:matrix.org");
        std::env::set_var("MATRIX_MESSAGE", "hello");
        std::env::set_var("MATRIX_USER", "@bot:matrix.org");
        std::env::set_var("MATRIX_PASSWORD", "s3cret");
        let cfg = Config::from_env().unwrap();
        match cfg.auth {
            Auth::Password { user, password } => {
                assert_eq!(user, "@bot:matrix.org");
                assert_eq!(password, "s3cret");
            }
            _ => panic!("expected password auth"),
        }
    }

    #[test]
    fn errors_when_no_auth_provided() {
        let _guard = clear_all();
        std::env::set_var("MATRIX_HOMESERVER", "https://matrix.org");
        std::env::set_var("MATRIX_ROOM_ID", "!room:matrix.org");
        std::env::set_var("MATRIX_MESSAGE", "hello");
        let err = Config::from_env().unwrap_err().to_string();
        assert!(err.contains("MATRIX_TOKEN"), "unexpected error: {}", err);
        assert!(err.contains("MATRIX_USER"), "unexpected error: {}", err);
    }

    #[test]
    fn errors_on_both_token_and_password_auth() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_USER", "@bot:matrix.org");
        std::env::set_var("MATRIX_PASSWORD", "s3cret");
        let err = Config::from_env().unwrap_err().to_string();
        assert!(err.contains("not both"), "unexpected error: {}", err);
    }

    #[test]
    fn device_id_defaults_to_matrix_notify() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.device_id, "MATRIX_NOTIFY");
    }

    #[test]
    fn device_id_can_be_overridden() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_DEVICE_ID", "MYREPO_NOTIFY");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.device_id, "MYREPO_NOTIFY");
    }

    #[test]
    fn errors_on_user_without_password() {
        let _guard = clear_all();
        std::env::set_var("MATRIX_HOMESERVER", "https://matrix.org");
        std::env::set_var("MATRIX_ROOM_ID", "!room:matrix.org");
        std::env::set_var("MATRIX_MESSAGE", "hello");
        std::env::set_var("MATRIX_USER", "@bot:matrix.org");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_password_without_user() {
        let _guard = clear_all();
        std::env::set_var("MATRIX_HOMESERVER", "https://matrix.org");
        std::env::set_var("MATRIX_ROOM_ID", "!room:matrix.org");
        std::env::set_var("MATRIX_MESSAGE", "hello");
        std::env::set_var("MATRIX_PASSWORD", "s3cret");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_homeserver_url() {
        let _guard = clear_all();
        set_required("not-a-url", "tok", "!r:s", "msg");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_format() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_FORMAT", "xml");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_msgtype() {
        let _guard = clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_MSGTYPE", "m.image");
        assert!(Config::from_env().is_err());
    }
}
