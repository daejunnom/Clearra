#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
command -v node >/dev/null 2>&1 || { printf 'node is required for the managed Clearra build owner\n' >&2; exit 2; }
if [[ -z "${CLEARRA_BUILD_SESSION_ID:-}" ]]; then
    for override in CLEARRA_WSL_NATIVE_BUILD_ROOT CLEARRA_CORE_C_BUILD_DIR CLEARRA_RELEASE_BUILD_ROOT; do
        [[ -z "${!override:-}" ]] || { printf 'Unmanaged build override is forbidden: %s\n' "$override" >&2; exit 2; }
    done
    exec node "$ROOT_DIR/scripts/tools/invoke-clearra-build.mjs" \
        --source-root "$ROOT_DIR" --purpose "${CLEARRA_BUILD_PURPOSE:-experiment}" \
        -- bash "$ROOT_DIR/scripts/build-core-c.sh" "$@"
fi
BUILD_TRANSACTION="$(node "$ROOT_DIR/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT_DIR" --field transaction)"
MANAGED_CARGO_TARGET="$(node "$ROOT_DIR/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT_DIR" --field cargo-target)"
BUILD_DIR="$BUILD_TRANSACTION/core-c-library-cache"
for override in CLEARRA_WSL_NATIVE_BUILD_ROOT CLEARRA_CORE_C_BUILD_DIR; do
    [[ -z "${!override:-}" || "${!override:-}" == "$BUILD_DIR" ]] || {
        printf 'Core C output must equal the managed transaction output: %s\n' "$override" >&2; exit 2;
    }
done
[[ -z "${CLEARRA_WSL_CARGO_TARGET_DIR:-}" || "$CLEARRA_WSL_CARGO_TARGET_DIR" == "$MANAGED_CARGO_TARGET" ]] || {
    printf 'WSL Cargo target must equal the managed transaction target\n' >&2; exit 2;
}
[[ -z "${CLEARRA_RELEASE_BUILD_ROOT:-}" || "$CLEARRA_RELEASE_BUILD_ROOT" == "$BUILD_TRANSACTION" ]] || {
    printf 'Release build root must equal the managed transaction root\n' >&2; exit 2;
}
export CARGO_TARGET_DIR="$MANAGED_CARGO_TARGET"
CONFIGURATION="${CLEARRA_CORE_C_CONFIGURATION:-Debug}"

if ! command -v cmake >/dev/null 2>&1; then
    echo "CMake was not found. Install CMake to build core-c." >&2
    exit 1
fi

cmake -S "$ROOT_DIR" -B "$BUILD_DIR" -DBUILD_TESTING=OFF
cmake --build "$BUILD_DIR" --config "$CONFIGURATION"
