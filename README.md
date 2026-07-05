# matrix-notify-action

Send a message to a Matrix room from a GitHub Actions workflow.

## Usage

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    user: ${{ secrets.MATRIX_USER }}
    password: ${{ secrets.MATRIX_PASSWORD }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Deployment of `${{ github.sha }}` succeeded.'
```

## Authentication

Two mutually exclusive options:

- **`user` + `password` (recommended):** the action logs in at the start of
  every run, obtains a fresh access token, and logs out afterwards. This can
  never go stale. Use a dedicated bot account.
- **`token`:** a static access token. Beware that on homeservers running
  Matrix Authentication Service (including matrix.org), tokens belong to a
  *session* — if that session is signed out or expires from inactivity, the
  token dies with `M_UNKNOWN_TOKEN`. If you use this option, generate the
  token with a direct login (see [Setup](#setup)), never by copying it out of
  Element.

## Inputs

| Input | Required | Default | Description |
|---|---|---|---|
| `user` | no¹ | — | Matrix user ID for password login (e.g. `@bot:matrix.org`) |
| `password` | no¹ | — | Matrix password for password login |
| `token` | no¹ | — | Matrix access token |
| `room_id` | yes | — | Matrix room ID (e.g. `!abc123:matrix.org`) |
| `message` | yes | — | Message content |
| `homeserver` | no | `https://matrix.org` | Matrix homeserver URL |
| `format` | no | `markdown` | Message format: `markdown`, `plain`, or `html` |
| `msgtype` | no | `m.notice` | Matrix message type: `m.notice` or `m.text` |
| `store_path` | no | `''` | Path to SQLite E2EE crypto store directory. Required for encrypted rooms. |
| `device_id` | no | `MATRIX_NOTIFY` | Stable device ID for password login with E2EE. Set a distinct value per repository if the same bot account is used with E2EE in several repositories. |
| `gh_token` | no | `${{ github.token }}` | GitHub token used to download the binary from releases |

¹ Provide either `user`+`password` or `token`, not both.

## Outputs

| Output | Description |
|---|---|
| `event_id` | Matrix event ID of the sent message |
| `error` | Error message if the action failed |

## Examples

### Notify on deployment

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    user: ${{ secrets.MATRIX_USER }}
    password: ${{ secrets.MATRIX_PASSWORD }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: |
      **${{ github.repository }}** deployed [`${{ github.ref_name }}`](${{ github.server_url }}/${{ github.repository }}/releases/tag/${{ github.ref_name }})
```

### Plain text message

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    user: ${{ secrets.MATRIX_USER }}
    password: ${{ secrets.MATRIX_PASSWORD }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Build failed on ${{ github.ref_name }}'
    format: plain
    msgtype: m.text
```

### With E2EE (encrypted room)

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    user: ${{ secrets.MATRIX_USER }}
    password: ${{ secrets.MATRIX_PASSWORD }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Hello from an encrypted room'
    store_path: .matrix-store
```

The crypto store is automatically cached between runs using `actions/cache`.
The store is bound to a Matrix device, so with password login the action
reuses the stable `device_id` on every login (instead of letting the server
assign a new device) and skips the end-of-run logout, which would delete the
device and its encryption keys. Without E2EE, the action logs out after
sending so devices don't accumulate on the homeserver.

### With a static access token

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Deployment started'
```

### Capture the event ID

```yaml
- id: notify
  uses: oxGrad/matrix-notify-action@v1
  with:
    user: ${{ secrets.MATRIX_USER }}
    password: ${{ secrets.MATRIX_PASSWORD }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Deployment started'

- run: echo "Sent as ${{ steps.notify.outputs.event_id }}"
```

## Setup

1. Create a dedicated Matrix bot account.
2. Invite the bot to your room.
3. Add `MATRIX_USER` and `MATRIX_PASSWORD` as repository secrets.

If you prefer a static token instead, generate one with a direct login so it
isn't tied to a client session you might later sign out of:

```bash
curl -s -X POST https://matrix.org/_matrix/client/v3/login \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"botname"},"password":"...","initial_device_display_name":"gh-actions"}'
```

Store the returned `access_token` as the `MATRIX_TOKEN` secret. Note that it
can still be invalidated by "sign out all devices", a password change, or
server-side session expiry — password login avoids all of these.

## Requirements

- Linux runners only (`ubuntu-*`). The binary is built for `x86_64-unknown-linux-musl`.

## License

MIT
