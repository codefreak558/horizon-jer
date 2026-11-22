#!/usr/bin/env bash
# Builds, lints and tests the mod with JEI for Horizon installed (the `jei` feature), as the launcher builds it
# when both mods are present. A plain `cargo` build never turns the feature on, so without this the code inside
# `mod_api::if_installed!(host, jei, …)` would never be compiled.
#
# Works on a copy in .horizon/with-jei/ (git-ignored): the mod with JEI added as an optional path dependency
# and a `jei` feature, and JEI at the tag below, both using the one SDK that tools/fetch-sdk.sh fetched.
# Nothing in the repo is changed. Needs the SDK first: tools/fetch-sdk.sh.
set -euo pipefail

JEI_REPO="https://github.com/codefreak558/horizon-jei"
JEI_TAG="v0.5.2"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk="$root/.horizon/sdk/api/crates/horizon-mod-api"
[ -f "$sdk/Cargo.toml" ] || { echo "no SDK in .horizon/sdk: run tools/fetch-sdk.sh first" >&2; exit 1; }
# Git Bash on Windows: cargo wants C:/… paths, not /c/….
if command -v cygpath >/dev/null; then sdk="$(cygpath -m "$sdk")"; fi

work="$root/.horizon/with-jei"
rm -rf "$work"
mkdir -p "$work/jer"
git clone --quiet --depth 1 --branch "$JEI_TAG" "$JEI_REPO" "$work/jei"
cp -r "$root/src" "$root/Cargo.toml" "$root/Cargo.lock" "$root/LICENSE" "$work/jer/"

# Both crates use the same SDK copy, so the lockfile has one of each SDK crate.
sed -i "s#path = \".horizon/sdk/api/crates/horizon-mod-api\"#path = \"$sdk\"#" \
  "$work/jer/Cargo.toml" "$work/jei/Cargo.toml"
sed -i 's#^\[dependencies\]$#[dependencies]\njei = { path = "../jei", optional = true }#' "$work/jer/Cargo.toml"
printf '\n[features]\njei = ["dep:jei"]\n' >> "$work/jer/Cargo.toml"
grep -q '^jei = { path' "$work/jer/Cargo.toml" || { echo "could not add the jei dependency" >&2; exit 1; }

cd "$work/jer"
export CARGO_TARGET_DIR="$root/target/with-jei"
cargo clippy --features jei --all-targets -- -D warnings
cargo test --features jei
