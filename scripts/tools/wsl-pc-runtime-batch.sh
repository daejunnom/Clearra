#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
hash -r
case "$PATH" in *"/mnt/"*) printf 'Windows PATH entry leaked into WSL runtime\n' >&2; exit 2 ;; esac

if [[ "$#" -ne 7 ]]; then
    printf 'usage: wsl-pc-runtime-batch.sh BINARY REPORT_ROOT BACKEND GPU_DEVICE WORKERS GPU_INVENTORY_MODE PROFILE_MODE\n' >&2
    exit 2
fi
BINARY="$1"
REPORT_ROOT="$2"
BACKEND="$3"
GPU_DEVICE="$4"
WORKERS="$5"
GPU_INVENTORY_MODE="$6"
PROFILE_MODE="$7"

case "$BINARY" in
    /home/*/.cache/Clearra/build/cargo-target/*/clearra-pc-artifact) ;;
    *) printf 'unsafe WSL runtime artifact: %s\n' "$BINARY" >&2; exit 2 ;;
esac
case "$REPORT_ROOT" in
    /home/*/.local/state/Clearra/reports/runtime-environments/*) ;;
    *) printf 'unsafe WSL runtime report root: %s\n' "$REPORT_ROOT" >&2; exit 2 ;;
esac
case "$BACKEND" in auto | cpu | gpu | hybrid) ;; *) exit 2 ;; esac
[[ "$GPU_DEVICE" == auto || "$GPU_DEVICE" =~ ^[0-9]+$ ]] || exit 2
[[ "$WORKERS" =~ ^[1-9][0-9]*$ ]] || exit 2
case "$GPU_INVENTORY_MODE" in query | skip) ;; *) exit 2 ;; esac
case "$PROFILE_MODE" in profile | no-profile) ;; *) exit 2 ;; esac
[[ -x "$BINARY" ]] || { printf 'WSL runtime artifact is not executable: %s\n' "$BINARY" >&2; exit 2; }

mkdir -p "$REPORT_ROOT"
REPORT_FS="$(stat -f -c %T "$REPORT_ROOT")"
case "$REPORT_FS" in
    9p | v9fs | drvfs | fuseblk)
        printf 'WSL runtime reports require Linux storage: %s\n' "$REPORT_ROOT" >&2
        exit 2
        ;;
esac

if [[ "$GPU_INVENTORY_MODE" == query ]]; then
    inventory_status=0
    inventory_started_ns="$(date +%s%N)"
    "$BINARY" --list-gpu-devices > "$REPORT_ROOT/gpu-inventory.json" \
        2> "$REPORT_ROOT/gpu-inventory.error" || inventory_status=$?
    inventory_ended_ns="$(date +%s%N)"
    printf '%s\n' "$inventory_status" > "$REPORT_ROOT/gpu-inventory.status"
    printf '%s\n' "$((inventory_ended_ns - inventory_started_ns))" \
        > "$REPORT_ROOT/gpu-inventory-time-ns"
else
    printf 'not-requested\n' > "$REPORT_ROOT/gpu-inventory.status"
    printf '0\n' > "$REPORT_ROOT/gpu-inventory-time-ns"
    rm -f -- "$REPORT_ROOT/gpu-inventory.json" "$REPORT_ROOT/gpu-inventory.error"
fi
status=0
prewarm_args=()
profile_args=()
if [[ "$BACKEND" == gpu || "$BACKEND" == hybrid ]]; then
    prewarm_args=(--prewarm-gpu)
fi
if [[ "$PROFILE_MODE" == profile ]]; then
    profile_args=(--profile-stages)
fi
"$BINARY" \
    --scenario pco-6p \
    --scenario tsar-cannon \
    --rule srs-plus \
    --count all \
    --backend "$BACKEND" \
    --gpu-device "$GPU_DEVICE" \
    "${prewarm_args[@]}" \
    "${profile_args[@]}" \
    --workers "$WORKERS" \
    --max-patterns 5040 \
    --output-dir "$REPORT_ROOT" \
    > "$REPORT_ROOT/stdout.log" \
    2> "$REPORT_ROOT/stderr.log" || status=$?
if [[ "$status" -ne 0 ]]; then
    cat "$REPORT_ROOT/stderr.log" >&2
    exit "$status"
fi
awk -F '\t' 'NF == 2 { printf "%s\t%s\t0\n", $1, $2 }' \
    "$REPORT_ROOT/batch-times.tsv" > "$REPORT_ROOT/case-times.tsv"
printf 'wsl_runtime_batch=complete report_root=%s\n' "$REPORT_ROOT"
