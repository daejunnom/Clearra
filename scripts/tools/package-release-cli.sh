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
CORE_BUILD="$BUILD_ROOT/core-c"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BUILD_ROOT/cargo-target}"

mkdir -p "$CORE_BUILD" "$CARGO_TARGET_DIR" "$OUTPUT_DIR"
cmake -S "$ROOT" -B "$CORE_BUILD" \
    -DBUILD_TESTING=OFF \
    -DCMAKE_BUILD_TYPE=Release
cmake --build "$CORE_BUILD" --config Release --parallel "${CLEARRA_BUILD_JOBS:-$(nproc)}"

CORE_LIBRARY="$(find "$CORE_BUILD" -type f -name 'libclearra_core.a' -print -quit)"
if [[ -z "$CORE_LIBRARY" ]]; then
    printf 'clearra_core release archive was not produced under %s\n' "$CORE_BUILD" >&2
    exit 2
fi

export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=$(dirname "$CORE_LIBRARY")"
cargo build \
    --manifest-path "$ROOT/Cargo.toml" \
    --locked \
    --release \
    --package clearra-cli \
    --features native-c-core,webgpu-search

BINARY="$CARGO_TARGET_DIR/release/clearra"
if [[ ! -x "$BINARY" ]]; then
    printf 'clearra release binary was not produced: %s\n' "$BINARY" >&2
    exit 2
fi
"$BINARY" --format json rules list >/dev/null

PACKAGE_NAME="clearra-v${VERSION}-${TARGET_TRIPLE}"
STAGING_DIR="$BUILD_ROOT/$PACKAGE_NAME"
ARCHIVE="$OUTPUT_DIR/$PACKAGE_NAME.tar.gz"
rm -rf -- "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
install -m 0755 "$BINARY" "$STAGING_DIR/clearra"
install -m 0644 "$ROOT/LICENSE" "$STAGING_DIR/LICENSE"
install -m 0644 "$ROOT/README.md" "$STAGING_DIR/README.md"
tar -C "$BUILD_ROOT" -czf "$ARCHIVE" "$PACKAGE_NAME"
(
    cd "$OUTPUT_DIR"
    sha256sum "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256"
)

printf 'cli_release_archive=%s\n' "$ARCHIVE"
