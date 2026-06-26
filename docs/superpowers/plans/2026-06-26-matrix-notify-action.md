# matrix-notify-action Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder Rust binary with a real Matrix notification action that sends messages to Matrix rooms using matrix-sdk, with optional E2EE support.

**Architecture:** A single statically-linked Linux binary reads all inputs from `MATRIX_*` environment variables, optionally opens a SQLite crypto store for E2EE, syncs once to verify the session, renders the message, sends it, and writes `event_id` or `error` to `$GITHUB_OUTPUT`. The composite `action.yml` wraps the binary with cache management for the crypto store and pre-flight HTML validation.

**Tech Stack:** Rust 1.82+, matrix-sdk 0.10 (e2e-encryption + sqlite features), tokio, pulldown-cmark, reqwest, anyhow, serde/serde_json, url.

## Global Constraints

- Linux runners only — target `x86_64-unknown-linux-musl`
- No Windows, no macOS build artifacts
- `rust-version = "1.82"` in Cargo.toml (matrix-sdk 0.10 MSRV)
- All Matrix inputs passed via `MATRIX_*` env vars — never via CLI args
- Access token must never appear in logs — never call `println!` or `eprintln!` with `MATRIX_TOKEN`
- `GITHUB_OUTPUT` file path comes from the `GITHUB_OUTPUT` env var
- Outputs written as `key=value\n` lines appended to `$GITHUB_OUTPUT`
- Default `msgtype`: `m.notice`; default `format`: `markdown`
- Binary exits 0 on success, exits 1 on error (after writing `error=` to GITHUB_OUTPUT)
- Spec: `docs/superpowers/specs/2026-06-26-matrix-notify-action-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Add all dependencies, bump rust-version |
| `src/main.rs` | Modify | Async entry point, orchestrates all modules |
| `src/config.rs` | Create | `Config` struct, `Config::from_env()`, validation |
| `src/render.rs` | Create | `render()` — markdown/plain/html → `RenderedMessage` |
| `src/output.rs` | Create | `write_output()`, `write_event_id()`, `write_error()` |
| `src/matrix.rs` | Create | `build_client()`, `send_message()` |
| `action.yml` | Modify | Full rewrite — 9-step composite action |
| `.github/workflows/release.yml` | Modify | Linux-only single-artifact release |
| `.github/workflows/integration_tests.yml` | Modify | Update inputs for new action interface |

---

## Task 1: Cargo.toml & module scaffolding

**Files:**
- Modify: `Cargo.toml`
- Create: `src/config.rs`
- Create: `src/render.rs`
- Create: `src/output.rs`
- Create: `src/matrix.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: module stubs that compile cleanly; `Cargo.toml` with all deps resolved

- [ ] **Step 1: Replace Cargo.toml**

```toml
[package]
name = "matrix-notify-action"
description = "Send notification to Matrix"
version = "0.1.0"
edition = "2021"
rust-version = "1.82"

[[bin]]
name = "matrix-notify-action"
path = "src/main.rs"

[dependencies]
matrix-sdk     = { version = "0.10", features = ["e2e-encryption", "sqlite"] }
tokio          = { version = "1", features = ["full"] }
pulldown-cmark = "0.12"
anyhow         = "1"
serde          = { version = "1", features = ["derive"] }
serde_json     = "1"
url            = "2"
reqwest        = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create empty module stubs**

`src/config.rs`:
```rust
pub struct Config;
```

`src/render.rs`:
```rust
pub struct RenderedMessage;
```

`src/output.rs`:
```rust
```

`src/matrix.rs`:
```rust
```

- [ ] **Step 3: Replace src/main.rs with module declarations**

```rust
mod config;
mod matrix;
mod output;
mod render;

fn main() {}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check
```

Expected: no errors (warnings about unused modules are fine).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "chore: add dependencies and module stubs"
```

---

## Task 2: Config module

**Files:**
- Modify: `src/config.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Config {
      pub homeserver: String,
      pub token: String,
      pub room_id: String,
      pub message: String,
      pub format: String,   // "markdown" | "plain" | "html"
      pub msgtype: String,  // "m.notice" | "m.text"
      pub store_path: String, // empty string = no E2EE
  }
  impl Config {
      pub fn from_env() -> anyhow::Result<Config>
  }
  ```

- [ ] **Step 1: Write failing tests**

Add to the bottom of `src/config.rs`:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test config:: 2>&1 | head -30
```

Expected: compile errors (Config not defined yet).

- [ ] **Step 3: Implement Config**

Replace the stub in `src/config.rs` with:
```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test config:: -- --test-threads=1
```

Expected: all 7 tests pass. (`--test-threads=1` avoids env var race between tests.)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add Config struct with env var parsing and validation"
```

---

## Task 3: Render module

**Files:**
- Modify: `src/render.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct RenderedMessage {
      pub body: String,            // plain text fallback
      pub formatted_body: Option<String>, // HTML, or None for plain format
  }
  pub fn render(message: &str, format: &str) -> RenderedMessage
  ```

- [ ] **Step 1: Write failing tests**

Add to `src/render.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_has_no_formatted_body() {
        let r = render("hello **world**", "plain");
        assert_eq!(r.body, "hello **world**");
        assert!(r.formatted_body.is_none());
    }

    #[test]
    fn markdown_renders_bold() {
        let r = render("**bold**", "markdown");
        assert!(r.formatted_body.as_ref().unwrap().contains("<strong>bold</strong>"));
        assert_eq!(r.body, "**bold**");
    }

    #[test]
    fn markdown_renders_inline_code() {
        let r = render("`code`", "markdown");
        assert!(r.formatted_body.as_ref().unwrap().contains("<code>code</code>"));
    }

    #[test]
    fn html_passes_through_formatted_body() {
        let r = render("<b>bold</b>", "html");
        assert_eq!(r.formatted_body.as_deref(), Some("<b>bold</b>"));
    }

    #[test]
    fn html_strips_tags_for_body() {
        let r = render("<b>bold</b> and <i>italic</i>", "html");
        assert_eq!(r.body, "bold and italic");
    }

    #[test]
    fn unknown_format_falls_back_to_markdown() {
        let r = render("**x**", "whatever");
        assert!(r.formatted_body.is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test render:: 2>&1 | head -20
```

Expected: compile errors (render not defined).

- [ ] **Step 3: Implement render module**

Replace stub in `src/render.rs`:
```rust
use pulldown_cmark::{html, Options, Parser};

pub struct RenderedMessage {
    pub body: String,
    pub formatted_body: Option<String>,
}

pub fn render(message: &str, format: &str) -> RenderedMessage {
    match format {
        "plain" => RenderedMessage {
            body: message.to_string(),
            formatted_body: None,
        },
        "html" => RenderedMessage {
            body: strip_tags(message),
            formatted_body: Some(message.to_string()),
        },
        _ => {
            let html = markdown_to_html(message);
            RenderedMessage {
                body: message.to_string(),
                formatted_body: Some(html),
            }
        }
    }
}

fn markdown_to_html(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test render::
```

Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/render.rs
git commit -m "feat: add message render module (markdown/plain/html)"
```

---

## Task 4: Output module

**Files:**
- Modify: `src/output.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn write_event_id(event_id: &str) -> anyhow::Result<()>
  pub fn write_error(msg: &str) -> anyhow::Result<()>
  // both append "key=value\n" to the file at $GITHUB_OUTPUT
  ```

- [ ] **Step 1: Write failing tests**

Add to `src/output.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn writes_event_id() {
        let f = NamedTempFile::new().unwrap();
        std::env::set_var("GITHUB_OUTPUT", f.path());
        write_event_id("$abc123:matrix.org").unwrap();
        let contents = fs::read_to_string(f.path()).unwrap();
        assert!(contents.contains("event_id=$abc123:matrix.org\n"));
    }

    #[test]
    fn writes_error() {
        let f = NamedTempFile::new().unwrap();
        std::env::set_var("GITHUB_OUTPUT", f.path());
        write_error("something went wrong").unwrap();
        let contents = fs::read_to_string(f.path()).unwrap();
        assert!(contents.contains("error=something went wrong\n"));
    }

    #[test]
    fn errors_when_github_output_not_set() {
        std::env::remove_var("GITHUB_OUTPUT");
        assert!(write_event_id("$x:y").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test output:: 2>&1 | head -20
```

Expected: compile errors.

- [ ] **Step 3: Implement output module**

Replace stub in `src/output.rs`:
```rust
use anyhow::{anyhow, Result};
use std::fs::OpenOptions;
use std::io::Write;

pub fn write_event_id(event_id: &str) -> Result<()> {
    append_output(&format!("event_id={}\n", event_id))
}

pub fn write_error(msg: &str) -> Result<()> {
    append_output(&format!("error={}\n", msg))
}

fn append_output(line: &str) -> Result<()> {
    let path = std::env::var("GITHUB_OUTPUT")
        .map_err(|_| anyhow!("GITHUB_OUTPUT env var is not set"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| anyhow!("Failed to open GITHUB_OUTPUT at {}: {}", path, e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| anyhow!("Failed to write to GITHUB_OUTPUT: {}", e))
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test output:: -- --test-threads=1
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/output.rs
git commit -m "feat: add GitHub Actions output writer"
```

---

## Task 5: Matrix client module

**Files:**
- Modify: `src/matrix.rs`

**Interfaces:**
- Consumes: `Config` from Task 2, `RenderedMessage` from Task 3
- Produces:
  ```rust
  pub async fn build_client(config: &Config) -> anyhow::Result<matrix_sdk::Client>
  pub async fn send_message(client: &matrix_sdk::Client, config: &Config, rendered: &RenderedMessage) -> anyhow::Result<String>
  // send_message returns the event_id as a String
  ```

Notes: This module cannot be unit-tested without a running Matrix server. The implementation is verified manually or via the integration test workflow. The test step below verifies compilation only.

- [ ] **Step 1: Add WhoamiResponse and helper**

Write `src/matrix.rs`:
```rust
use anyhow::{anyhow, Result};
use matrix_sdk::{
    config::SyncSettings,
    matrix_auth::{MatrixSession, MatrixSessionTokens},
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
```

- [ ] **Step 2: Implement build_client**

Append to `src/matrix.rs`:
```rust
pub async fn build_client(config: &Config) -> Result<Client> {
    let client = if config.store_path.is_empty() {
        Client::builder()
            .homeserver_url(&config.homeserver)?
            .build()
            .await?
    } else {
        std::fs::create_dir_all(&config.store_path)?;
        Client::builder()
            .homeserver_url(&config.homeserver)?
            .sqlite_store(&config.store_path, None::<&str>)
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
```

- [ ] **Step 3: Implement sync and send_message**

Append to `src/matrix.rs`:
```rust
pub async fn send_message(
    client: &Client,
    config: &Config,
    rendered: &RenderedMessage,
) -> Result<String> {
    // Sync once: verifies session, fetches room state, enables E2EE key exchange
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

    // Guard: fail clearly if E2EE room but no crypto store
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
            None => {
                MessageType::Notice(NoticeMessageEventContent::plain(rendered.body.clone()))
            }
        },
    };

    let content = RoomMessageEventContent::new(msg_type);
    let response = room.send(content).await?;

    Ok(response.event_id.to_string())
}
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check
```

Expected: no errors. Warnings about unused imports are fine at this stage.

- [ ] **Step 5: Commit**

```bash
git add src/matrix.rs
git commit -m "feat: add Matrix client module (build_client, send_message)"
```

---

## Task 6: Wire up main.rs

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: all modules from Tasks 2–5
- Produces: working binary that reads env vars, sends a Matrix message, exits 0/1

- [ ] **Step 1: Implement main.rs**

Replace `src/main.rs`:
```rust
mod config;
mod matrix;
mod output;
mod render;

use config::Config;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        let msg = e.to_string();
        eprintln!("Error: {}", msg);
        // Best-effort: write error to GITHUB_OUTPUT even if it fails
        let _ = output::write_error(&msg);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let rendered = render::render(&config.message, &config.format);
    let client = matrix::build_client(&config).await?;
    let event_id = matrix::send_message(&client, &config, &rendered).await?;
    output::write_event_id(&event_id)?;
    Ok(())
}
```

- [ ] **Step 2: Run all tests**

```bash
cargo test -- --test-threads=1
```

Expected: all 16 tests pass (config: 7, render: 6, output: 3).

- [ ] **Step 3: Build release binary**

```bash
cargo build --release
```

Expected: binary at `target/release/matrix-notify-action`. No errors.

- [ ] **Step 4: Smoke test with missing env vars**

```bash
./target/release/matrix-notify-action
```

Expected: exits 1, prints `Error: MATRIX_HOMESERVER is required but not set`.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up main.rs — config, render, send, output"
```

---

## Task 7: Rewrite action.yml

**Files:**
- Modify: `action.yml`

**Interfaces:**
- Consumes: binary from GitHub Release tagged `linux.tgz`
- Produces: working composite action with all 9 steps

- [ ] **Step 1: Replace action.yml**

```yaml
name: matrix-notify-action
description: Send notification to a Matrix room
author: oxGrad <dev@graditya.com>
branding:
  icon: message-circle
  color: green

inputs:
  homeserver:
    description: Matrix homeserver URL (e.g. https://matrix.org)
    required: false
    default: 'https://matrix.org'
  token:
    description: Matrix access token
    required: true
  room_id:
    description: Matrix room ID (e.g. !abc:matrix.org)
    required: true
  message:
    description: Message content
    required: true
  format:
    description: Message format — markdown | plain | html
    required: false
    default: markdown
  msgtype:
    description: Matrix message type — m.notice | m.text
    required: false
    default: m.notice
  store_path:
    description: Path to directory for SQLite E2EE crypto store (empty = no E2EE)
    required: false
    default: ''
  gh_token:
    description: GitHub token for downloading the binary from releases
    required: false
    default: ${{ github.token }}

outputs:
  event_id:
    description: Matrix event ID of the sent message
    value: ${{ steps.run.outputs.event_id }}
  error:
    description: Error message if the action failed
    value: ${{ steps.run.outputs.error }}

runs:
  using: composite
  steps:
    - name: Validate HTML message
      if: inputs.format == 'html'
      shell: python3
      run: |
        import sys
        from html.parser import HTMLParser
        class V(HTMLParser): pass
        try:
            V().feed("""${{ inputs.message }}""")
        except Exception as e:
            print(f"::error::Invalid HTML in message input: {e}")
            sys.exit(1)

    - name: Restore E2EE crypto store
      if: inputs.store_path != ''
      uses: actions/cache/restore@v4
      with:
        path: ${{ inputs.store_path }}
        key: matrix-notify-store-${{ runner.os }}-${{ github.repository }}-${{ inputs.room_id }}

    - name: Create store directory
      if: inputs.store_path != ''
      shell: bash
      run: mkdir -p "${{ inputs.store_path }}"

    - name: Set action variables
      shell: bash
      run: |
        echo "action_repo=matrix-notify-action" >> $GITHUB_ENV
        echo "action_org=oxGrad" >> $GITHUB_ENV

    - name: Get action version
      id: version
      shell: bash
      run: |
        final=$(basename "${{ github.action_path }}")
        if [ "$final" = "${{ env.action_repo }}" ]; then
          echo "version=" >> $GITHUB_OUTPUT
        else
          echo "version=$final" >> $GITHUB_OUTPUT
        fi

    - name: Download binary
      shell: bash
      run: |
        gh release download ${{ steps.version.outputs.version }} \
          --repo ${{ env.action_org }}/${{ env.action_repo }} \
          --pattern 'linux.tgz'
        tar -xzf linux.tgz
      env:
        GH_TOKEN: ${{ inputs.gh_token }}

    - name: Send Matrix notification
      id: run
      shell: bash
      run: ./matrix-notify-action
      env:
        MATRIX_HOMESERVER: ${{ inputs.homeserver }}
        MATRIX_TOKEN: ${{ inputs.token }}
        MATRIX_ROOM_ID: ${{ inputs.room_id }}
        MATRIX_MESSAGE: ${{ inputs.message }}
        MATRIX_FORMAT: ${{ inputs.format }}
        MATRIX_MSGTYPE: ${{ inputs.msgtype }}
        MATRIX_STORE_PATH: ${{ inputs.store_path }}

    - name: Save E2EE crypto store
      if: always() && inputs.store_path != ''
      uses: actions/cache/save@v4
      with:
        path: ${{ inputs.store_path }}
        key: matrix-notify-store-${{ runner.os }}-${{ github.repository }}-${{ inputs.room_id }}

    - name: Cleanup
      if: always()
      shell: bash
      run: rm -rf linux.tgz matrix-notify-action
```

- [ ] **Step 2: Verify YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('action.yml'))" && echo "OK"
```

Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add action.yml
git commit -m "feat: rewrite action.yml as 9-step composite action with E2EE cache"
```

---

## Task 8: Update release workflow (Linux-only)

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Replace release.yml**

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      version:
        description: Version to release (e.g. 1.0.0)
        required: true

jobs:
  build:
    runs-on: ubuntu-latest
    name: Build linux-musl binary
    steps:
      - uses: actions/checkout@v4

      - uses: Swatinem/rust-cache@v2

      - name: Install musl target
        run: rustup target add x86_64-unknown-linux-musl

      - name: Install musl-tools
        run: sudo apt-get install -y musl-tools

      - name: Build
        run: cargo build --release --target x86_64-unknown-linux-musl

      - name: Package
        run: |
          cp target/x86_64-unknown-linux-musl/release/matrix-notify-action .
          tar -czf linux.tgz matrix-notify-action

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: linux
          path: linux.tgz

  release:
    needs: [build]
    runs-on: ubuntu-latest
    name: Create GitHub Release
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
      - name: Create release
        uses: ncipollo/release-action@v1
        with:
          artifacts: linux/linux.tgz
          tag: v${{ github.event.inputs.version }}
```

- [ ] **Step 2: Verify YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "OK"
```

Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "chore: simplify release workflow to Linux-only single artifact"
```

---

## Task 9: Update integration test workflow

**Files:**
- Modify: `.github/workflows/integration_tests.yml`

- [ ] **Step 1: Replace integration_tests.yml**

```yaml
name: Test consuming this action

on:
  release:
    types: [released]
  workflow_run:
    workflows: ["Release"]
    types: [completed]

jobs:
  test_success:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - id: notify
        uses: ./
        with:
          token: ${{ secrets.MATRIX_TOKEN }}
          room_id: ${{ secrets.MATRIX_ROOM_ID }}
          message: 'Integration test passed for `${{ github.sha }}`'
      - name: Verify event_id set
        run: |
          if [ -z "${{ steps.notify.outputs.event_id }}" ]; then
            echo "event_id was not set" && exit 1
          fi
          echo "event_id: ${{ steps.notify.outputs.event_id }}"

  test_invalid_format:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - id: notify
        continue-on-error: true
        uses: ./
        with:
          token: ${{ secrets.MATRIX_TOKEN }}
          room_id: ${{ secrets.MATRIX_ROOM_ID }}
          message: 'test'
          format: invalid_format
      - name: Verify it failed with error output
        run: |
          if [ -z "${{ steps.notify.outputs.error }}" ]; then
            echo "Expected error output but got none" && exit 1
          fi
          echo "error: ${{ steps.notify.outputs.error }}"
```

Note: integration tests require `MATRIX_TOKEN` and `MATRIX_ROOM_ID` set as repository secrets.

- [ ] **Step 2: Verify YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/integration_tests.yml'))" && echo "OK"
```

Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/integration_tests.yml
git commit -m "test: update integration workflow for new action inputs"
```

---

## Self-Review Notes

**Spec coverage check:**
- ✅ Section 1 inputs/outputs: Tasks 2, 6, 7
- ✅ Section 2 binary flow (validation, sync timeout, E2EE guard, render, send): Tasks 2, 3, 5, 6
- ✅ Section 3 crate deps: Task 1
- ✅ Section 4 composite steps (HTML validation, cache restore, run, cache save, cleanup): Task 7
- ✅ Release workflow Linux-only: Task 8
- ✅ Integration tests updated: Task 9

**API uncertainty note:** `matrix-sdk` builder method `.sqlite_store()` — verify the exact method signature against the published 0.10 docs/changelog at https://crates.io/crates/matrix-sdk before Task 5. The pattern `Client::builder().homeserver_url(x)?.sqlite_store(path, None::<&str>).build().await?` is the documented form but builder chains evolve between minor versions. If `homeserver_url()` returns `Result`, chain with `?`; if it returns `Self`, omit it.
