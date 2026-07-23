#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${1:?usage: package-release-cli.sh OUTPUT_DIR VERSION [TARGET_TRIPLE]}"
VERSION="${2:?usage: package-release-cli.sh OUTPUT_DIR VERSION [TARGET_TRIPLE]}"
TARGET_TRIPLE="${3:-x86_64-unknown-linux-gnu}"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]]; then
    printf 'invalid release version: %s\n' "$VERSION" >&2
    exit 2
fi
if [[ "$TARGET_TRIPLE" != "x86_64-unknown-linux-gnu" ]]; then
    printf 'unsupported CLI release target: %s\n' "$TARGET_TRIPLE" >&2
    exit 2
fi

BUILD_ROOT="${CLEARRA_RELEASE_BUILD_ROOT:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/clearra-release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BUILD_ROOT/cargo-target}"

mkdir -p "$CARGO_TARGET_DIR" "$OUTPUT_DIR"
cargo build \
    --manifest-path "$ROOT/Cargo.toml" \
    --locked \
    --release \
    --package clearra-cli \
    --features wasm-cpu-runtime,webgpu-search

BINARY="$CARGO_TARGET_DIR/release/clearra"
if [[ ! -x "$BINARY" ]]; then
    printf 'clearra release binary was not produced: %s\n' "$BINARY" >&2
    exit 2
fi
"$BINARY" --format json rules list >/dev/null
"$BINARY" --format json pc --lines 2 --queue IJLOO --fixed --no-hold >/dev/null

RELEASE_BINARY="$OUTPUT_DIR/Clearra-CLI-v${VERSION}-linux-x86_64"
install -m 0755 "$BINARY" "$RELEASE_BINARY"

printf 'cli_release_binary=%s\n' "$RELEASE_BINARY"
