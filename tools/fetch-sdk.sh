#!/usr/bin/env bash
# Fetches the pinned Horizon SDK's API crate into .horizon/sdk/api/, as `cargo horizon-mod` does: the
# api-source.tar.xz asset of the signed release sdk-<commit>, checked against its SHA-256 (the SDK's sdk.json).
# The commit is the one horizon-mod.toml's [native] pins; .horizon/ is git-ignored.
set -euo pipefail

COMMIT="2b77287de099c9316278ba7ac4d83590fdb7ef30"
SHA256="ace895c548279e50c706d50c27f18fb4f3f6f48f742bffc3165998f11d0a26b3"
URL="https://github.com/horizon-rust/horizon/releases/download/sdk-$COMMIT/api-source.tar.xz"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="$root/.horizon/sdk/api"
if [ -f "$dest/crates/horizon-mod-api/Cargo.toml" ] && [ "${1:-}" != "--force" ]; then
  echo "SDK already in $dest (--force fetches it again)"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --proto '=https' --retry 2 -o "$tmp/api-source.tar.xz" "$URL"
echo "$SHA256  $tmp/api-source.tar.xz" | sha256sum --check --status \
  || { echo "api-source.tar.xz does not match its SHA-256" >&2; exit 1; }
rm -rf "$dest"
mkdir -p "$dest"
tar -xJf "$tmp/api-source.tar.xz" -C "$dest"
echo "SDK $COMMIT in $dest"
