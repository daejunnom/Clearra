#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${CLEARRA_WSL_MARKER_DIGEST:-}" ]]; then
    export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
fi
hash -r
case "$PATH" in *"/mnt/"*) printf 'Windows PATH entry leaked into WSL legal-board generation\n' >&2; exit 2 ;; esac

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

LAYERS=
PROFILE=
WORKERS=
MAX_NEW_STEPS=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --layers) LAYERS="${2:-}"; shift 2 ;;
        --profile) PROFILE="${2:-}"; shift 2 ;;
        --workers) WORKERS="${2:-}"; shift 2 ;;
        --max-new-steps) MAX_NEW_STEPS="${2:-}"; shift 2 ;;
        *) printf 'Unsupported legal-board generation argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

[[ "$LAYERS" == /mnt/?/* && -d "$LAYERS" && ! -L "$LAYERS" ]] || {
    printf 'Legal-board layers must be an existing validated host directory\n' >&2
    exit 2
}
case "$PROFILE" in srs | srs-plus | srs-x | jstris-180 | no-kick) ;; *)
    printf 'Unsupported legal-board kick profile: %s\n' "$PROFILE" >&2
    exit 2
    ;;
esac
[[ "$WORKERS" =~ ^[1-9][0-9]*$ && "$WORKERS" -le "$(nproc)" ]] || {
    printf 'Legal-board workers must be within 1..nproc; no silent reduction is permitted\n' >&2
    exit 2
}
[[ "$MAX_NEW_STEPS" =~ ^[1-9][0-9]*$ && "$MAX_NEW_STEPS" -le 21 ]] || {
    printf 'Legal-board max-new-steps must be within 1..21\n' >&2
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
        -- bash "$AUTHORITY_ROOT/scripts/tools/wsl-legal-board-generate.sh" \
        --layers "$LAYERS" --profile "$PROFILE" --workers "$WORKERS" \
        --max-new-steps "$MAX_NEW_STEPS"
fi

BUILD_TRANSACTION="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field transaction)"
MANAGED_CARGO_TARGET="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field cargo-target)"
export CARGO_TARGET_DIR="$MANAGED_CARGO_TARGET"
mkdir -p "$BUILD_TRANSACTION/legal-board-generator"

bash "$AUTHORITY_ROOT/scripts/tools/wsl-native-cargo.sh" \
    build --locked --release --no-default-features \
    --features legal-board-assets \
    --package clearra-pc4-qualifier --bin clearra-pc4-legal-board

BINARY="$MANAGED_CARGO_TARGET/release/clearra-pc4-legal-board"
[[ -x "$BINARY" && ! -L "$BINARY" ]] || {
    printf 'Managed legal-board generator is missing: %s\n' "$BINARY" >&2
    exit 2
}
exec "$BINARY" legal-board-run \
    --layers "$LAYERS" --profile "$PROFILE" --workers "$WORKERS" \
    --max-new-steps "$MAX_NEW_STEPS"
