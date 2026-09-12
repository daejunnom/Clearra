#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
hash -r
case "$PATH" in *"/mnt/"*) printf 'Windows PATH entry leaked into WSL native execution\n' >&2; exit 2 ;; esac

# Explicit WSL runtime/build surface. Windows never invokes this as an implicit
# fallback. A Windows host may deploy the source package into WSL ext4 and then
# select this surface deliberately.

AUTHORITY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT="${CLEARRA_WSL_WORKSPACE:-$AUTHORITY_ROOT}"
ROOT="$(cd "$ROOT" && pwd)"
ROOT_FS="$(stat -f -c %T "$ROOT")"
case "$ROOT_FS" in
    9p | v9fs | drvfs | fuseblk)
        printf 'Clearra WSL workspaces must use the Linux filesystem, not a mounted Windows path: root=%s fs=%s\n' \
            "$ROOT" "$ROOT_FS" >&2
        exit 2
        ;;
esac
case "$ROOT" in
    /mnt/*)
        printf 'Clearra WSL workspaces under /mnt are forbidden: %s\n' "$ROOT" >&2
        exit 2
        ;;
esac
if [[ ! -f "$ROOT/Cargo.toml" || ! -f "$ROOT/core-c/cmake/source_manifest.cmake" ]]; then
    printf 'Clearra WSL workspace is incomplete: %s\n' "$ROOT" >&2
    exit 2
fi
command -v node >/dev/null 2>&1 || { printf 'node is required for the managed Clearra build owner\n' >&2; exit 2; }
# Cargo can otherwise create an alternate target before the Rust wrapper runs.
for argument in "$@"; do
    case "$argument" in
        --target-dir|--target-dir=*|--build-dir|--build-dir=*|--artifact-dir|--artifact-dir=*|--out-dir|--out-dir=*|--config|--config=*)
            printf 'Independent Cargo output/config overrides are forbidden: %s\n' "$argument" >&2
            exit 2
            ;;
    esac
done
if [[ -z "${CLEARRA_BUILD_SESSION_ID:-}" ]]; then
    for override in CLEARRA_WSL_NATIVE_BUILD_ROOT CLEARRA_CORE_C_BUILD_DIR CLEARRA_RELEASE_BUILD_ROOT; do
        [[ -z "${!override:-}" ]] || { printf 'Unmanaged build override is forbidden: %s\n' "$override" >&2; exit 2; }
    done
    exec node "$AUTHORITY_ROOT/scripts/tools/invoke-clearra-build.mjs" \
        --source-root "$ROOT" --purpose "${CLEARRA_BUILD_PURPOSE:-experiment}" \
        -- bash "$AUTHORITY_ROOT/scripts/tools/wsl-native-cargo.sh" "$@"
fi
BUILD_TRANSACTION="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field transaction)"
MANAGED_CARGO_TARGET="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field cargo-target)"
BUILD_VARIANT=standard
if [[ "${CLEARRA_WSL_ENABLE_STAGE_PROFILING:-0}" == "1" ]]; then
    BUILD_VARIANT=stage-profiled
fi
BUILD_ROOT="$BUILD_TRANSACTION/core-c-native-$BUILD_VARIANT"
[[ -z "${CLEARRA_WSL_NATIVE_BUILD_ROOT:-}" || "$CLEARRA_WSL_NATIVE_BUILD_ROOT" == "$BUILD_ROOT" ]] || {
    printf 'WSL native build root must equal the managed transaction output\n' >&2; exit 2;
}
[[ -z "${CLEARRA_WSL_CARGO_TARGET_DIR:-}" || "$CLEARRA_WSL_CARGO_TARGET_DIR" == "$MANAGED_CARGO_TARGET" ]] || {
    printf 'WSL Cargo target must equal the managed transaction target\n' >&2; exit 2;
}
[[ -z "${CLEARRA_CORE_C_BUILD_DIR:-}" || "$CLEARRA_CORE_C_BUILD_DIR" == "$BUILD_ROOT" ]] || {
    printf 'Core C output must equal the managed transaction output\n' >&2; exit 2;
}
[[ -z "${CLEARRA_RELEASE_BUILD_ROOT:-}" || "$CLEARRA_RELEASE_BUILD_ROOT" == "$BUILD_TRANSACTION" ]] || {
    printf 'Release build root must equal the managed transaction root\n' >&2; exit 2;
}
export CARGO_TARGET_DIR="$MANAGED_CARGO_TARGET"
mkdir -p "$BUILD_ROOT"

if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    printf 'cargo is required in the WSL distribution\n' >&2
    exit 2
fi

mapfile -t sources < <(
    grep -oE 'src/[^[:space:])]+\.c' \
        "$ROOT/core-c/cmake/source_manifest.cmake"
)
if [[ "${#sources[@]}" -eq 0 ]]; then
    printf 'C source manifest is empty\n' >&2
    exit 2
fi

CFLAGS=(-std=c11 -O2 -fPIC -I"$ROOT/core-c/include" -I"$ROOT/core-c/src")
if [[ "${CLEARRA_WSL_ENABLE_STAGE_PROFILING:-0}" == "1" ]]; then
    CFLAGS+=(-DCLEARRA_ENABLE_STAGE_PROFILING=1)
fi
compiler_identity="$(gcc --version | head -n 1)"
header_digest="$({
    printf '%s\0' "$compiler_identity" "${CFLAGS[@]}"
    find "$ROOT/core-c/include" "$ROOT/core-c/src" \
        -type f -name '*.h' -print0 \
        | sort -z \
        | xargs -0 sha256sum
} | sha256sum | cut -d' ' -f1)"
input_digest="$({
    printf '%s\0' "$compiler_identity" "${CFLAGS[@]}"
    find "$ROOT/core-c/include" "$ROOT/core-c/src" \
        -type f \( -name '*.c' -o -name '*.h' \) -print0 \
        | sort -z \
        | xargs -0 sha256sum
    sha256sum "$ROOT/core-c/cmake/source_manifest.cmake"
} | sha256sum | cut -d' ' -f1)"
stamp="$BUILD_ROOT/source-tree.sha256"
library="$BUILD_ROOT/libclearra_core.a"
objects=()
native_core_rebuilt=0
for source in "${sources[@]}"; do
    object="$BUILD_ROOT/${source//\//_}.o"
    object_stamp="$object.sha256"
    source_digest="$({
        printf '%s\0' "$header_digest"
        sha256sum "$ROOT/core-c/$source"
    } | sha256sum | cut -d' ' -f1)"
    if [[ ! -f "$object" || ! -f "$object_stamp" || \
          "$(<"$object_stamp")" != "$source_digest" ]]; then
        gcc "${CFLAGS[@]}" \
            -c "$ROOT/core-c/$source" \
            -o "$object"
        printf '%s\n' "$source_digest" > "$object_stamp.tmp.$$"
        mv -f -- "$object_stamp.tmp.$$" "$object_stamp"
        native_core_rebuilt=1
    fi
    objects+=("$object")
done

if [[ ! -f "$library" || ! -f "$stamp" || \
      "$(<"$stamp")" != "$input_digest" || "$native_core_rebuilt" == 1 ]]; then
    temporary_library="$library.tmp.$$"
    ar rcs "$temporary_library" "${objects[@]}"
    mv -f -- "$temporary_library" "$library"
    printf '%s\n' "$input_digest" > "$stamp"
fi

export CLEARRA_RUNTIME_ENVIRONMENT=wsl
export CLEARRA_RUNTIME_ROOT="$ROOT"
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=$BUILD_ROOT"
cd "$ROOT"

# Cargo cannot observe content changes inside an externally built archive.
# Keep C flag variants side by side and invalidate only the FFI owner when a
# variant's archive changes; Cargo then relinks the dependent binaries.
if [[ "$native_core_rebuilt" == 1 ]]; then
    touch "$ROOT/crates/clearra-core-ffi/src/lib.rs"
fi
# The native archive lives outside Cargo's source graph. The keyed archive
# cache and FFI-owner invalidation above make content changes observable while
# the target-scoped linker search path avoids a generated Rust build script.
cargo "$@"
