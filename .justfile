set dotenv-load

_default:
  @just --choose

test:
  cargo test

# Send a real message using credentials from .env (see .env.example)
notify message="Test message from matrix-notify-action":
  #!/usr/bin/env bash
  set -euo pipefail
  export GITHUB_OUTPUT="$(mktemp)"
  trap 'rm -f "$GITHUB_OUTPUT"' EXIT
  MATRIX_MESSAGE="{{message}}" cargo run
  cat "$GITHUB_OUTPUT"

build:
  cargo build --release

install:
  cargo install --path . --force

release-major:
  just _release major

release-minor:
  just _release minor

release-patch:
  just _release patch

# Release a new version: just release patch|minor|major
_release bump:
  cargo release --execute {{bump}}
