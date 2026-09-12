#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
hash -r
AUTHORITY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT="${CLEARRA_WSL_WORKSPACE:?An ext4 source copy is required}"
[[ "$#" -eq 7 ]] || { printf 'expected FEATURES REPORT_ROOT BACKEND GPU_DEVICE WORKERS INVENTORY PROFILE\n' >&2; exit 2; }
FEATURES="$1"
REPORT_ROOT="$2"
BACKEND="$3"
GPU_DEVICE="$4"
WORKERS="$5"
INVENTORY="$6"
PROFILE="$7"
case "$FEATURES" in gpu-backend | gpu-backend,stage-profiling) ;; *) exit 2 ;; esac
[[ "$REPORT_ROOT" == "$HOME/.local/state/Clearra/reports/runtime-environments/latest" ]] || exit 2
case "$BACKEND" in auto | cpu | gpu | hybrid) ;; *) exit 2 ;; esac
[[ "$GPU_DEVICE" == auto || "$GPU_DEVICE" =~ ^[0-9]+$ ]] || exit 2
[[ "$WORKERS" =~ ^[1-9][0-9]*$ ]] || exit 2
case "$INVENTORY" in query | skip) ;; *) exit 2 ;; esac
case "$PROFILE" in profile | no-profile) ;; *) exit 2 ;; esac

# The existing owner surrounds BOTH the build and measured runtime. The Cargo
# runner is nested, so no second transaction can replace this current mid-run.
if [[ -z "${CLEARRA_BUILD_SESSION_ID:-}" ]]; then
    exec node "$AUTHORITY_ROOT/scripts/tools/invoke-clearra-build.mjs" \
        --source-root "$ROOT" --purpose experiment \
        -- bash "$AUTHORITY_ROOT/scripts/tools/wsl-pc-runtime-build-and-batch.sh" "$@"
fi
MANAGED_CARGO_TARGET="$(node "$AUTHORITY_ROOT/scripts/tools/clearra-build-paths.mjs" --source-root "$ROOT" --field cargo-target)"
prepare_started_ns="$(date +%s%N)"
bash "$AUTHORITY_ROOT/scripts/tools/wsl-native-cargo.sh" \
    build --release -p clearra-pc-artifact --features "$FEATURES"
prepare_ended_ns="$(date +%s%N)"
batch_started_ns="$(date +%s%N)"
bash "$AUTHORITY_ROOT/scripts/tools/wsl-pc-runtime-batch.sh" \
    "$MANAGED_CARGO_TARGET/release/clearra-pc-artifact" "$REPORT_ROOT" \
    "$BACKEND" "$GPU_DEVICE" "$WORKERS" "$INVENTORY" "$PROFILE"
batch_ended_ns="$(date +%s%N)"
printf 'wsl_preparation_elapsed_ns=%s\n' "$((prepare_ended_ns - prepare_started_ns))"
printf 'wsl_host_batch_elapsed_ns=%s\n' "$((batch_ended_ns - batch_started_ns))"
