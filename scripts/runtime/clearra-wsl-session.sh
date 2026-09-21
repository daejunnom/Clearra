#!/usr/bin/env bash
set -euo pipefail

[[ "$(id -u)" -eq 0 ]] || exit 2
UNIT=
MEMORY_MAX=
TASKS_MAX=
RUNTIME_MAX=
GRACE=
GUEST=
ENTRY=
MARKER_DIGEST=
NODE_VERSION=
NPM_VERSION=
PNPM_VERSION=
RUST_VERSION=
CARGO_VERSION=
WASM_BINDGEN_VERSION=
MODE=runtime
RUN_AS=clearra
while [[ $# -gt 0 ]]; do
    case "$1" in
        --unit) UNIT="$2"; shift 2 ;;
        --memory-max) MEMORY_MAX="$2"; shift 2 ;;
        --tasks-max) TASKS_MAX="$2"; shift 2 ;;
        --runtime-max) RUNTIME_MAX="$2"; shift 2 ;;
        --grace) GRACE="$2"; shift 2 ;;
        --guest) GUEST="$2"; shift 2 ;;
        --entry) ENTRY="$2"; shift 2 ;;
        --marker-digest) MARKER_DIGEST="$2"; shift 2 ;;
        --node-version) NODE_VERSION="$2"; shift 2 ;;
        --npm-version) NPM_VERSION="$2"; shift 2 ;;
        --pnpm-version) PNPM_VERSION="$2"; shift 2 ;;
        --rust-version) RUST_VERSION="$2"; shift 2 ;;
        --cargo-version) CARGO_VERSION="$2"; shift 2 ;;
        --wasm-bindgen-version) WASM_BINDGEN_VERSION="$2"; shift 2 ;;
        --mode) MODE="$2"; shift 2 ;;
        --run-as) RUN_AS="$2"; shift 2 ;;
        --) shift; break ;;
        *) exit 2 ;;
    esac
done
[[ "$UNIT" =~ ^clearra-[A-Za-z0-9._-]+$ ]] || exit 2
[[ "$MEMORY_MAX" =~ ^[1-9][0-9]+$ ]] || exit 2
[[ "$TASKS_MAX" =~ ^[1-9][0-9]*$ ]] || exit 2
[[ "$RUNTIME_MAX" =~ ^[1-9][0-9]*$ ]] || exit 2
[[ "$GRACE" =~ ^[0-9]+([.][0-9]+)?$ ]] || exit 2
case "$MODE:$RUN_AS" in
    runtime:clearra)
        [[ "$ENTRY" =~ ^[a-z0-9-]+$ ]] || exit 2
        [[ "$MARKER_DIGEST" =~ ^[0-9a-f]{64}$ ]] || exit 2
        for version in "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" "$RUST_VERSION" "$CARGO_VERSION" "$WASM_BINDGEN_VERSION"; do
            [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
        done
        ;;
    provision:root)
        [[ -z "$ENTRY$MARKER_DIGEST$NODE_VERSION$NPM_VERSION$PNPM_VERSION$RUST_VERSION$CARGO_VERSION$WASM_BINDGEN_VERSION" ]] || exit 2
        ;;
    *) exit 2 ;;
esac
[[ -f "$GUEST" ]] || exit 2

parent_lost_marker="/run/${UNIT}.parent-lost"
rm -f "$parent_lost_marker"
trap 'rm -f "$parent_lost_marker"' EXIT
host_signal_cleanup() {
    trap - HUP TERM INT
    : >"$parent_lost_marker"
    systemctl kill --kill-who=all --signal=TERM "${UNIT}.service" >/dev/null 2>&1 || true
    sleep "$GRACE"
    systemctl kill --kill-who=all --signal=KILL "${UNIT}.service" >/dev/null 2>&1 || true
    # This is the dedicated distribution's own systemd instance. Powering it
    # off cannot affect another WSL distribution and closes the parent-crash
    # gap in which no Windows supervisor remains to issue --terminate.
    systemctl poweroff --force --force --no-block >/dev/null 2>&1 || true
    exit 125
}
trap host_signal_cleanup HUP TERM INT
lease_watch() {
    # The host keeps stdin open for the complete lease. EOF means its
    # supervisor disappeared. Kill the entire transient unit, then escalate.
    trap 'exit 0' HUP TERM INT
    while IFS= read -r _; do :; done
    trap - HUP TERM INT
    : >"$parent_lost_marker"
    systemctl kill --kill-who=all --signal=TERM "${UNIT}.service" >/dev/null 2>&1 || true
    sleep "$GRACE"
    systemctl kill --kill-who=all --signal=KILL "${UNIT}.service" >/dev/null 2>&1 || true
    systemctl poweroff --force --force --no-block >/dev/null 2>&1 || true
}
lease_watch &
watcher=$!

status=0
systemd_arguments=(
    --quiet --wait --pipe
    --unit "$UNIT"
    --uid "$RUN_AS"
    --property "MemoryMax=${MEMORY_MAX}"
    --property "MemorySwapMax=0"
    --property "TasksMax=${TASKS_MAX}"
    --property "MemoryOOMGroup=yes"
    --property "OOMPolicy=kill"
    --property "RuntimeMaxSec=${RUNTIME_MAX}"
)
command=(/bin/bash "$GUEST")
if [[ "$MODE" == runtime ]]; then
    systemd_arguments+=(
        --setenv "CLEARRA_WSL_MARKER_DIGEST=${MARKER_DIGEST}"
        --setenv "CLEARRA_WSL_NODE_VERSION=${NODE_VERSION}"
        --setenv "CLEARRA_WSL_NPM_VERSION=${NPM_VERSION}"
        --setenv "CLEARRA_WSL_PNPM_VERSION=${PNPM_VERSION}"
        --setenv "CLEARRA_WSL_RUST_VERSION=${RUST_VERSION}"
        --setenv "CLEARRA_WSL_CARGO_VERSION=${CARGO_VERSION}"
        --setenv "CLEARRA_WSL_WASM_BINDGEN_VERSION=${WASM_BINDGEN_VERSION}"
    )
    command+=("$ENTRY")
fi
command+=("$@")
systemd-run "${systemd_arguments[@]}" -- "${command[@]}" </dev/null || status=$?

kill "$watcher" >/dev/null 2>&1 || true
wait "$watcher" >/dev/null 2>&1 || true
result="$(systemctl show "${UNIT}.service" --property=Result --value 2>/dev/null || true)"
control_group="$(systemctl show "${UNIT}.service" --property=ControlGroup --value 2>/dev/null || true)"
memory_peak="$(systemctl show "${UNIT}.service" --property=MemoryPeak --value 2>/dev/null || true)"
systemctl stop "${UNIT}.service" >/dev/null 2>&1 || true
n_tasks="$(systemctl show "${UNIT}.service" --property=NTasks --value 2>/dev/null || true)"
active_state="$(systemctl show "${UNIT}.service" --property=ActiveState --value 2>/dev/null || true)"
[[ -n "$n_tasks" ]] || n_tasks=0
systemctl reset-failed "${UNIT}.service" >/dev/null 2>&1 || true
cgroup_removed=false
if [[ "$control_group" == /system.slice/clearra-*.service ]]; then
    cgroup_path="/sys/fs/cgroup${control_group}"
    for _ in {1..50}; do
        if [[ ! -e "$cgroup_path" ]]; then
            cgroup_removed=true
            break
        fi
        sleep 0.1
    done
fi
if [[ "$cgroup_removed" != true ]]; then
    status=125
fi
if [[ "$result" == "oom-kill" ]]; then
    printf 'CLEARRA_WSL_RESULT=oom-kill\n'
elif [[ -f "$parent_lost_marker" ]]; then
    printf 'CLEARRA_WSL_RESULT=parent-lost\n'
else
    printf 'CLEARRA_WSL_RESULT=%s\n' "${result:-exit-code}"
fi
printf 'CLEARRA_WSL_CONTROL_GROUP=%s\n' "$control_group"
printf 'CLEARRA_WSL_MEMORY_PEAK=%s\n' "${memory_peak:-unknown}"
printf 'CLEARRA_WSL_REMAINING_TASKS=%s\n' "$n_tasks"
printf 'CLEARRA_WSL_UNIT_ACTIVE_STATE=%s\n' "${active_state:-unknown}"
printf 'CLEARRA_WSL_CGROUP_REMOVED=%s\n' "$cgroup_removed"
exit "$status"
