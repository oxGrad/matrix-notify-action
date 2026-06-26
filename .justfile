_default:
  @just --choose

test:
  cargo test

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
