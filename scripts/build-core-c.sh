#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
DEFAULT_BUILD_ROOT="${LOCALAPPDATA:-"${XDG_CACHE_HOME:-"$HOME/.cache"}"}"
BUILD_DIR="${CLEARRA_CORE_C_BUILD_DIR:-"$DEFAULT_BUILD_ROOT/Clearra/build/core-c-library-cache"}"
CONFIGURATION="${CLEARRA_CORE_C_CONFIGURATION:-Debug}"

if ! command -v cmake >/dev/null 2>&1; then
    echo "CMake was not found. Install CMake to build core-c." >&2
    exit 1
fi

cmake -S "$ROOT_DIR" -B "$BUILD_DIR" -DBUILD_TESTING=OFF
cmake --build "$BUILD_DIR" --config "$CONFIGURATION"
