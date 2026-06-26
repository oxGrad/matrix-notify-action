use anyhow::{anyhow, bail, Result};
use url::Url;

pub struct Config {
    pub homeserver: String,
    pub token: String,
    pub room_id: String,
    pub message: String,
    pub format: String,
    pub msgtype: String,
    pub store_path: String,
}

impl Config {
    pub fn from_env() -> Result<Config> {
        let homeserver = require_env("MATRIX_HOMESERVER")?;
        let token = require_env("MATRIX_TOKEN")?;
        let room_id = require_env("MATRIX_ROOM_ID")?;
        let message = require_env("MATRIX_MESSAGE")?;
        let format = std::env::var("MATRIX_FORMAT").unwrap_or_else(|_| "markdown".into());
        let msgtype = std::env::var("MATRIX_MSGTYPE").unwrap_or_else(|_| "m.notice".into());
        let store_path = std::env::var("MATRIX_STORE_PATH").unwrap_or_default();

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

        Ok(Config { homeserver, token, room_id, message, format, msgtype, store_path })
    }
}

fn require_env(key: &str) -> Result<String> {
    let val = std::env::var(key)
        .map_err(|_| anyhow!("{} is required but not set", key))?;
    if val.is_empty() {
        bail!("{} is set but empty", key);
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_required(homeserver: &str, token: &str, room_id: &str, message: &str) {
        std::env::set_var("MATRIX_HOMESERVER", homeserver);
        std::env::set_var("MATRIX_TOKEN", token);
        std::env::set_var("MATRIX_ROOM_ID", room_id);
        std::env::set_var("MATRIX_MESSAGE", message);
    }

    fn clear_all() {
        for key in &["MATRIX_HOMESERVER","MATRIX_TOKEN","MATRIX_ROOM_ID",
                     "MATRIX_MESSAGE","MATRIX_FORMAT","MATRIX_MSGTYPE","MATRIX_STORE_PATH"] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn parses_required_fields() {
        clear_all();
        set_required("https://matrix.org", "syt_token", "!room:matrix.org", "hello");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.homeserver, "https://matrix.org");
        assert_eq!(cfg.token, "syt_token");
        assert_eq!(cfg.room_id, "!room:matrix.org");
        assert_eq!(cfg.message, "hello");
        assert_eq!(cfg.format, "markdown");
        assert_eq!(cfg.msgtype, "m.notice");
        assert_eq!(cfg.store_path, "");
    }

    #[test]
    fn applies_defaults() {
        clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.format, "markdown");
        assert_eq!(cfg.msgtype, "m.notice");
        assert_eq!(cfg.store_path, "");
    }

    #[test]
    fn overrides_defaults() {
        clear_all();
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
        clear_all();
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_homeserver_url() {
        clear_all();
        set_required("not-a-url", "tok", "!r:s", "msg");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_format() {
        clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_FORMAT", "xml");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn errors_on_invalid_msgtype() {
        clear_all();
        set_required("https://matrix.org", "tok", "!r:s", "msg");
        std::env::set_var("MATRIX_MSGTYPE", "m.image");
        assert!(Config::from_env().is_err());
    }
}
