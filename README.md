# matrix-notify-action

Send a message to a Matrix room from a GitHub Actions workflow.

## Usage

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Deployment of `${{ github.sha }}` succeeded.'
```

## Inputs

| Input | Required | Default | Description |
|---|---|---|---|
| `token` | yes | — | Matrix access token |
| `room_id` | yes | — | Matrix room ID (e.g. `!abc123:matrix.org`) |
| `message` | yes | — | Message content |
| `homeserver` | no | `https://matrix.org` | Matrix homeserver URL |
| `format` | no | `markdown` | Message format: `markdown`, `plain`, or `html` |
| `msgtype` | no | `m.notice` | Matrix message type: `m.notice` or `m.text` |
| `store_path` | no | `''` | Path to SQLite E2EE crypto store directory. Required for encrypted rooms. |
| `gh_token` | no | `${{ github.token }}` | GitHub token used to download the binary from releases |

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
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: |
      **${{ github.repository }}** deployed [`${{ github.ref_name }}`](${{ github.server_url }}/${{ github.repository }}/releases/tag/${{ github.ref_name }})
```

### Plain text message

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Build failed on ${{ github.ref_name }}'
    format: plain
    msgtype: m.text
```

### With E2EE (encrypted room)

```yaml
- uses: oxGrad/matrix-notify-action@v1
  with:
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Hello from an encrypted room'
    store_path: .matrix-store
```

The crypto store is automatically cached between runs using `actions/cache`.

### Capture the event ID

```yaml
- id: notify
  uses: oxGrad/matrix-notify-action@v1
  with:
    token: ${{ secrets.MATRIX_TOKEN }}
    room_id: ${{ secrets.MATRIX_ROOM_ID }}
    message: 'Deployment started'

- run: echo "Sent as ${{ steps.notify.outputs.event_id }}"
```

## Setup

1. Create a Matrix bot account and generate an access token.
2. Invite the bot to your room.
3. Add `MATRIX_TOKEN` and `MATRIX_ROOM_ID` as repository secrets.

## Requirements

- Linux runners only (`ubuntu-*`). The binary is built for `x86_64-unknown-linux-musl`.

## License

MIT
