# matrix-notify-action Design

Date: 2026-06-26

## Summary

A GitHub Action that sends notifications to Matrix rooms. Built as a Rust binary (statically linked, Linux only) wrapped in a composite action. Improves on [fadenb/Matrix-Chat-Message](https://github.com/fadenb/Matrix-Chat-Message) by using the matrix-rust-sdk, supporting E2EE, fixing the plaintext token logging bug, adding `event_id` output, and supporting multiple message formats.

## Decisions

- **Runtime**: Linux runners only (`x86_64-unknown-linux-musl`). No macOS, no Windows.
- **Auth**: Access token only. No username/password login.
- **Encryption**: E2EE supported via matrix-sdk's `e2e-encryption` + `sqlite` features. Opt-in at runtime by providing `store_path`. Without `store_path`, an in-memory (no-persistence) client is used — suitable for unencrypted rooms only.
- **Message formats**: `markdown` (default), `plain`, `html`. Controlled by `format` input.
- **Architecture**: Single binary; single release artifact (`linux.tgz`). E2EE is compiled in but only activates when `store_path` is set.

## Section 1: Inputs, Outputs & Binary Contract

All inputs are passed to the binary via environment variables. This prevents secrets from appearing in process lists and aligns with GitHub Actions' masking behaviour for `env:` secrets.

### Inputs

| Input | Env var | Required | Default | Notes |
|---|---|---|---|---|
| `homeserver` | `MATRIX_HOMESERVER` | yes | `https://matrix.org` | Full URL |
| `token` | `MATRIX_TOKEN` | yes | — | Matrix access token (masked) |
| `room_id` | `MATRIX_ROOM_ID` | yes | — | `!abc:matrix.org` format |
| `message` | `MATRIX_MESSAGE` | yes | — | Message content |
| `format` | `MATRIX_FORMAT` | no | `markdown` | `markdown` \| `plain` \| `html` |
| `msgtype` | `MATRIX_MSGTYPE` | no | `m.notice` | `m.notice` \| `m.text` |
| `store_path` | `MATRIX_STORE_PATH` | no | `''` | Dir for SQLite crypto store; empty = no E2EE |
| `gh_token` | `GH_TOKEN` | no | `${{ github.token }}` | For downloading binary from release |

`room_id` is renamed from fadenb's `channel` to match Matrix spec terminology.

### Outputs

| Output | Description |
|---|---|
| `event_id` | Matrix event ID of the sent message |
| `error` | Human-readable error message if the action failed |

Both written to `$GITHUB_OUTPUT`.

## Section 2: Binary Execution Flow

```
start
  │
  ├─ read MATRIX_* env vars; validate required fields present and non-empty
  │    └─ missing required → write error= to GITHUB_OUTPUT, exit 1
  │
  ├─ validate MATRIX_HOMESERVER is a well-formed HTTPS URL
  │    └─ invalid → write error=, exit 1
  │
  ├─ build matrix-sdk Client
  │    ├─ MATRIX_STORE_PATH non-empty?
  │    │    yes → SqliteStateStore + SqliteCryptoStore at that path
  │    │    no  → MemoryStore (no persistence)
  │    └─ set homeserver URL
  │
  ├─ restore session with access token (AuthSession)
  │
  ├─ initial sync with 30s timeout
  │    ├─ verifies token is valid
  │    ├─ required for E2EE key exchange
  │    └─ timeout → write error=sync timed out, exit 1
  │
  ├─ if E2EE room detected but store_path is empty → write error=, exit 1
  │    (prevents silent send of unreadable encrypted content)
  │
  ├─ render message
  │    ├─ format=markdown  → pulldown-cmark → HTML
  │    │    send: body (plain) + formatted_body (HTML)
  │    ├─ format=plain     → send: body only, no formatted_body
  │    └─ format=html      → send: formatted_body (user HTML) + body (HTML with tags stripped)
  │
  ├─ send RoomMessageEventContent to room_id with msgtype
  │    └─ error (not member, room not found, rate limited, etc.) → write error=, exit 1
  │
  └─ write event_id= to GITHUB_OUTPUT, exit 0
```

**Key improvements over fadenb:**
- No plaintext token logging (fadenb logs `token: ${token}` directly)
- Sync timeout — fadenb had none; a hung server would block the workflow indefinitely
- E2EE room detection — fails clearly instead of sending unreadable content
- `event_id` output — callers can use it for threading, reactions, or edits in follow-up steps
- No silent `joinRoom` swallow — if the bot is not in the room, the error is surfaced

## Section 3: Crate Dependencies

```toml
[package]
name = "matrix-notify-action"
version = "0.1.0"
edition = "2021"
rust-version = "1.82"

[[bin]]
name = "matrix-notify-action"
path = "src/main.rs"

[dependencies]
matrix-sdk        = { version = "0.10", features = ["e2e-encryption", "sqlite"] }
tokio             = { version = "1", features = ["full"] }
pulldown-cmark    = "0.12"
anyhow            = "1"
serde_json        = "1"
url               = "2"
```

| Crate | Role |
|---|---|
| `matrix-sdk` + `e2e-encryption` + `sqlite` | Matrix client, E2EE via vodozemac, SQLite state/crypto store |
| `tokio` | Async runtime required by matrix-sdk |
| `pulldown-cmark` | Markdown → HTML (pure Rust, no C deps) |
| `anyhow` | Ergonomic error propagation |
| `serde_json` | Build GITHUB_OUTPUT values |
| `url` | Validate MATRIX_HOMESERVER before connecting |

`rust-version` bumped from 1.67 → 1.82 (matrix-sdk 0.10 MSRV). Existing `rust_checks.yml` runs `rustup override set stable` so this has no practical effect on CI.

No HTML sanitisation crate (`ammonia`) — for `format=html` the input is validated structurally by the composite action's Python step before the binary runs. The binary trusts it is Matrix-safe HTML.

## Section 4: Composite Action Structure

```yaml
# action.yml (composite)
steps:

  # 1. Pre-flight: validate HTML structure when format=html
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

  # 2. Restore E2EE crypto store from cache
  - name: Restore crypto store
    if: inputs.store_path != ''
    uses: actions/cache/restore@v4
    with:
      path: ${{ inputs.store_path }}
      key: matrix-notify-store-${{ runner.os }}-${{ github.repository }}-${{ inputs.room_id }}

  # 3. Ensure store directory exists
  - name: Create store directory
    if: inputs.store_path != ''
    shell: bash
    run: mkdir -p "${{ inputs.store_path }}"

  # 4. Set reusable variables
  - name: Set variables
    shell: bash
    run: |
      echo "action_repo=matrix-notify-action" >> $GITHUB_ENV
      echo "action_org=oxGrad" >> $GITHUB_ENV

  # 5. Get action version from path
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

  # 6. Download and extract binary
  - name: Download binary
    shell: bash
    run: |
      gh release download ${{ steps.version.outputs.version }} \
        --repo ${{ env.action_org }}/${{ env.action_repo }} \
        --pattern 'linux.tgz'
      tar -xzf linux.tgz
    env:
      GH_TOKEN: ${{ inputs.gh_token }}

  # 7. Run binary
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

  # 8. Save updated crypto store back to cache
  - name: Save crypto store
    if: always() && inputs.store_path != ''
    uses: actions/cache/save@v4
    with:
      path: ${{ inputs.store_path }}
      key: matrix-notify-store-${{ runner.os }}-${{ github.repository }}-${{ inputs.room_id }}

  # 9. Cleanup downloaded artifacts
  - name: Cleanup
    if: always()
    shell: bash
    run: rm -rf linux.tgz matrix-notify-action
```

Cache notes:
- `cache/restore` and `cache/save` are used separately (not `actions/cache`) so the save step fires even when the binary fails (`always()`), preserving the crypto store.
- Cache key includes `runner.os`, `github.repository`, and `room_id` — multiple rooms in one repo each get an isolated store.
- The store contains megolm session keys and device identity only, not the access token.

## Release Workflow Changes

- Drop Windows and macOS build jobs — Linux musl only.
- Single artifact: `linux.tgz` containing the `matrix-notify-action` binary.
- `rust-version` in build matrix: `stable` (tracks matrix-sdk MSRV via `rust-version` in Cargo.toml).

## What This Fixes vs fadenb/Matrix-Chat-Message

| Issue in fadenb | Fix here |
|---|---|
| `node12` runtime (EOL) | Rust binary, no runtime |
| Plaintext token in logs | Never logged; passed via masked env var |
| No outputs | `event_id` + `error` outputs |
| No E2EE support | E2EE via matrix-sdk + SQLite store, opt-in |
| No timeout on network ops | 30s sync timeout |
| Silent `joinRoom` failure | Bot must already be in the room; clear error if not |
| No format control | `format` input: `markdown` \| `plain` \| `html` |
| No HTML validation | Python `html.parser` pre-flight in composite action |
| Non-unique txn ID pattern | matrix-sdk handles txn IDs internally |
