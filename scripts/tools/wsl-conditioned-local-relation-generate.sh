#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${CLEARRA_WSL_MARKER_DIGEST:-}" ]]; then
    export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
fi
hash -r
case "$PATH" in *"/mnt/"*) printf 'Windows PATH entry leaked into WSL local-relation generation\n' >&2; exit 2 ;; esac

AUTHORITY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT="${CLEARRA_WSL_WORKSPACE:-$AUTHORITY_ROOT}"
ROOT="$(cd "$ROOT" && pwd)"
ROOT_FS="$(stat -f -c %T "$ROOT")"
case "$ROOT_FS" in
    9p | v9fs | drvfs | fuseblk)
        printf 'Clearra WSL source must use the Linux filesystem: root=%s fs=%s\n' "$ROOT" "$ROOT_FS" >&2
        exit 2
        ;;
esac
case "$ROOT" in
    /mnt/*) printf 'Clearra WSL source under /mnt is forbidden: %s\n' "$ROOT" >&2; exit 2 ;;
esac
[[ -f "$ROOT/Cargo.toml" && -f "$ROOT/core-c/cmake/source_manifest.cmake" ]] || {
    printf 'Clearra WSL source is incomplete: %s\n' "$ROOT" >&2
    exit 2
}

PROFILE=
QUERIES=
PACK=
CATALOG=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile) PROFILE="${2:-}"; shift 2 ;;
        --queries) QUERIES="${2:-}"; shift 2 ;;
        --pack) PACK="${2:-}"; shift 2 ;;
        --catalog) CATALOG="${2:-}"; shift 2 ;;
        *) printf 'Unsupported local-relation generation argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

case "$PROFILE" in srs | srs-plus | srs-x | jstris-180 | no-kick) ;; *)
    printf 'Unsupported local-relation kick profile: %s\n' "$PROFILE" >&2
    exit 2
    ;;
esac
[[ "$QUERIES" == /mnt/?/* && -f "$QUERIES" && ! -L "$QUERIES" ]] || {
    printf 'Local-relation queries must be a validated host file\n' >&2
    exit 2
}
for output in "$PACK" "$CATALOG"; do
    [[ "$output" == /mnt/?/* && -d "$(dirname "$output")" && ! -L "$(dirname "$output")" && ! -L "$output" ]] || {
        printf 'Local-relation output must have a validated host parent\n' >&2
        exit 2
    }
done
[[ "$PACK" != "$CATALOG" && "$PACK" != "$QUERIES" && "$CATALOG" != "$QUERIES" ]] || {
    printf 'Local-relation query and output paths must be distinct\n' >&2
    exit 2
}

for override in CLEARRA_WSL_NATIVE_BUILD_ROOT CLEARRA_WSL_CARGO_TARGET_DIR CLEARRA_CORE_C_BUILD_DIR CLEARRA_RELEASE_BUILD_ROOT; do
    [[ -z "${!override:-}" ]] || {
        printf 'Unmanaged build override is forbidden: %s\n' "$override" >&2
        exit 2
    }
done
if [[ -z "${CLEARRA_BUILD_SESSION_ID:-}" ]]; then
    exec node "$AUTHORITY_ROOT/scripts/tools/invoke-clearra-build.mjs" \
        --source-root "$ROOT" --purpose "${CLEARRA_BUILD_PURPOSE:-experiment}" \
        -- bash "$AUTHORITY_ROOT/scripts/tools/wsl-conditioned-local-relation-generate.sh" \
        --profile "$PROFILE" --queries "$QUERIES" --pack "$PACK" --catalog "$CATALOG"
fi

BUILD_TRANSACTION="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field transaction)"
MANAGED_CARGO_TARGET="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field cargo-target)"
export CARGO_TARGET_DIR="$MANAGED_CARGO_TARGET"
mkdir -p "$BUILD_TRANSACTION/conditioned-local-relation-generator"

bash "$AUTHORITY_ROOT/scripts/tools/wsl-native-cargo.sh" \
    build --locked --release --no-default-features \
    --features legal-board-assets \
    --package clearra-pc4-qualifier --bin clearra-conditioned-local-relation

BINARY="$MANAGED_CARGO_TARGET/release/clearra-conditioned-local-relation"
[[ -x "$BINARY" && ! -L "$BINARY" ]] || {
    printf 'Managed local-relation generator is missing: %s\n' "$BINARY" >&2
    exit 2
}
exec "$BINARY" --profile "$PROFILE" --queries "$QUERIES" \
    --pack "$PACK" --catalog "$CATALOG"
