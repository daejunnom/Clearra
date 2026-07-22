#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
hash -r
case "$PATH" in *"/mnt/"*) printf 'Windows PATH entry leaked into WSL native execution\n' >&2; exit 2 ;; esac

ROOT="${CLEARRA_WSL_WORKSPACE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
ROOT="$(cd "$ROOT" && pwd)"
case "$ROOT" in /mnt/*) printf 'Clearra WSL tests require an ext4 workspace: %s\n' "$ROOT" >&2; exit 2 ;; esac
case "$(stat -f -c %T "$ROOT")" in
    9p | v9fs | drvfs | fuseblk)
        printf 'Clearra WSL tests require the Linux filesystem: %s\n' "$ROOT" >&2
        exit 2
        ;;
esac

WORKERS=1
SANITIZER=none
PROFILE=0
TEST_NAME=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --workers) WORKERS="$2"; shift 2 ;;
        --sanitizer) SANITIZER="$2"; shift 2 ;;
        --profile) PROFILE=1; shift ;;
        --test) TEST_NAME="$2"; shift 2 ;;
        *) printf 'Unknown WSL C-test argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done
[[ "$WORKERS" =~ ^[0-9]+$ ]] || { printf 'Invalid worker count\n' >&2; exit 2; }
[[ "$SANITIZER" =~ ^(none|address|undefined)$ ]] || { printf 'Invalid sanitizer\n' >&2; exit 2; }
[[ -z "$TEST_NAME" || "$TEST_NAME" =~ ^[A-Za-z0-9_]+$ ]] || {
    printf 'Invalid aggregate test selector\n' >&2
    exit 2
}
WORKERS=$(( WORKERS < 1 ? 1 : WORKERS ))
CPU_COUNT="$(nproc)"
WORKERS=$(( WORKERS > CPU_COUNT ? CPU_COUNT : WORKERS ))

CORE_ROOT="$ROOT/core-c"
SOURCE_MANIFEST="$CORE_ROOT/cmake/source_manifest.cmake"
TEST_MANIFEST="$CORE_ROOT/cmake/test_targets.cmake"
[[ -f "$SOURCE_MANIFEST" && -f "$TEST_MANIFEST" && -f "$CORE_ROOT/tests/all_tests.c" ]] || {
    printf 'Clearra C manifests are incomplete in %s\n' "$CORE_ROOT" >&2
    exit 2
}
command -v gcc >/dev/null 2>&1 || { printf 'gcc is required in WSL\n' >&2; exit 2; }

read_cmake_set() {
    local target="$1"
    local file="$2"
    awk -v target="$target" '
        {
            line = $0
            if (!active) {
                pattern = "^[[:space:]]*set\\(" target "([[:space:]]|$)"
                if (line !~ pattern) next
                active = 1
                sub(pattern, "", line)
            }
            sub(/#.*/, "", line)
            done = line ~ /\)/
            sub(/\).*/, "", line)
            count = split(line, token, /[[:space:]]+/)
            for (token_index = 1; token_index <= count; ++token_index) {
                if (token[token_index] != "") print token[token_index]
            }
            if (done) exit
        }
    ' "$file"
}

mapfile -t CORE_SOURCES < <(grep -oE 'src/[^[:space:])]+\.c' "$SOURCE_MANIFEST")
mapfile -t TEST_NAMES < <(read_cmake_set CLEARRA_CORE_TEST_NAMES "$TEST_MANIFEST")
mapfile -t TEST_SOURCES < <(read_cmake_set CLEARRA_CORE_TEST_SOURCES "$TEST_MANIFEST")
if [[ "${#CORE_SOURCES[@]}" -eq 0 || "${#TEST_NAMES[@]}" -eq 0 ||
      "${#TEST_NAMES[@]}" -ne "${#TEST_SOURCES[@]}" ]]; then
    printf 'Clearra C source/test manifests could not be resolved exactly\n' >&2
    exit 2
fi

if [[ -n "$TEST_NAME" ]]; then
    found=0
    for name in "${TEST_NAMES[@]}"; do
        [[ "$name" == "$TEST_NAME" ]] && found=1
    done
    [[ "$found" -eq 1 ]] || { printf 'Unknown C test: %s\n' "$TEST_NAME" >&2; exit 2; }
fi

CFLAGS=(-std=c11 -O2 -Wall -Wextra -I"$CORE_ROOT/include" -I"$CORE_ROOT/src")
LDFLAGS=()
if [[ "$PROFILE" -eq 1 ]]; then
    CFLAGS+=(-DCLEARRA_ENABLE_STAGE_PROFILING=1)
fi
case "$SANITIZER" in
    address) CFLAGS+=(-fsanitize=address -fno-omit-frame-pointer); LDFLAGS+=(-fsanitize=address) ;;
    undefined) CFLAGS+=(-fsanitize=undefined -fno-omit-frame-pointer); LDFLAGS+=(-fsanitize=undefined) ;;
esac

CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}/Clearra/build"
VARIANT="aggregate-$SANITIZER-profile$PROFILE"
BUILD_ROOT="$CACHE_ROOT/core-c-tests/$VARIANT"
mkdir -p "$BUILD_ROOT/core" "$BUILD_ROOT/tests"

INPUT_DIGEST="$({
    printf '%s\0' "$(gcc --version | head -n 1)" "${CFLAGS[@]}" "${LDFLAGS[@]}"
    find "$CORE_ROOT/include" "$CORE_ROOT/src" "$CORE_ROOT/tests" \
        -type f \( -name '*.c' -o -name '*.h' \) -print0 \
        | sort -z | xargs -0 sha256sum
    sha256sum "$SOURCE_MANIFEST" "$TEST_MANIFEST" "$ROOT/scripts/tools/wsl-core-c-tests.sh"
} | sha256sum | cut -d' ' -f1)"
STAMP="$BUILD_ROOT/source-tree.sha256"
EXECUTABLE="$BUILD_ROOT/clearra_core_all_tests"

compile_core_source() {
    local source="$1"
    local object="$2"
    shift 2
    gcc "$@" -c "$CORE_ROOT/$source" -o "$object"
}

if [[ ! -x "$EXECUTABLE" || ! -f "$STAMP" || "$(<"$STAMP")" != "$INPUT_DIGEST" ]]; then
    rm -f -- "$BUILD_ROOT/core/"*.o "$BUILD_ROOT/tests/"*.o "$EXECUTABLE"
    declare -a compile_pids=()
    declare -a core_objects=()
    wait_for_compile_slot() {
        while [[ "${#compile_pids[@]}" -ge "$WORKERS" ]]; do
            wait "${compile_pids[0]}"
            compile_pids=("${compile_pids[@]:1}")
        done
    }
    for source in "${CORE_SOURCES[@]}"; do
        object="$BUILD_ROOT/core/${source//\//_}.o"
        core_objects+=("$object")
        wait_for_compile_slot
        compile_core_source "$source" "$object" "${CFLAGS[@]}" &
        compile_pids+=("$!")
    done

    declare -a test_objects=()
    for index in "${!TEST_NAMES[@]}"; do
        name="${TEST_NAMES[$index]}"
        main_name="${name}_main"
        mapfile -t group_sources < <(
            printf '%s\n' "${TEST_SOURCES[$index]}"
            read_cmake_set "${name}_EXTRA_SOURCES" "$TEST_MANIFEST"
        )
        source_index=0
        for source in "${group_sources[@]}"; do
            object="$BUILD_ROOT/tests/${name}_${source_index}_${source//\//_}.o"
            test_objects+=("$object")
            wait_for_compile_slot
            gcc "${CFLAGS[@]}" -DCLEARRA_CORE_TEST=1 -Dmain="$main_name" \
                -c "$CORE_ROOT/$source" -o "$object" &
            compile_pids+=("$!")
            source_index=$((source_index + 1))
        done
    done
    for pid in "${compile_pids[@]}"; do wait "$pid"; done

    gcc "${CFLAGS[@]}" "$CORE_ROOT/tests/all_tests.c" \
        "${core_objects[@]}" "${test_objects[@]}" "${LDFLAGS[@]}" \
        -o "$EXECUTABLE"
    printf '%s\n' "$INPUT_DIGEST" > "$STAMP"
fi

if [[ -n "$TEST_NAME" ]]; then
    "$EXECUTABLE" "$TEST_NAME"
else
    "$EXECUTABLE"
fi
printf 'clearra-wsl-core-c-tests: aggregate=passed internal=%d workers=%d cache=%s\n' \
    "${#TEST_NAMES[@]}" "$WORKERS" "$BUILD_ROOT"
