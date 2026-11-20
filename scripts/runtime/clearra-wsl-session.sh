#!/usr/bin/env bash

start_lease_watch() {
    local host_stdin
    exec {host_stdin}<&0
    # Noninteractive Bash otherwise redirects unredirected background stdin
    # to /dev/null. Read the host supervisor's actual lease pipe.
    lease_watch <&"$host_stdin" &
    exec {host_stdin}<&-
}

owned_cgroup_drained() {
    local path="$1" expected="$2" observed="$3" active_state="$4"
    [[ "$active_state" =~ ^(inactive|failed)$ && ( -z "$observed" || "$observed" == "$expected" ) ]] || return 1
    for _ in {1..50}; do
        [[ -e "$path" ]] || return 0
        sleep 0.1
    done
    return 1
}

append_wsl_build_environment() {
    local -n arguments="$1"
    local distribution="$2" anchor="$3"
    [[ -n "$distribution" && "${#distribution}" -le 256 && "$distribution" != *[[:cntrl:]]* ]] || return 2
    [[ "$anchor" =~ ^[a-zA-Z]:[\\/].+$ && "$anchor" != *[[:cntrl:]]* ]] || return 2
    arguments+=(
        --setenv "WSL_DISTRO_NAME=$distribution"
        --setenv "LOCALAPPDATA=$anchor"
        --setenv "CLEARRA_WSL_DISTRIBUTION=$distribution"
    )
}

# Unit tests can exercise the real pipe without invoking systemd or WSL.
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
    return
fi

set -euo pipefail

(( EUID != 0 )) || { printf 'Clearra WSL requires a non-root user\n' >&2; exit 2; }
UNIT=
MEMORY_MAX=
TASKS_MAX=
RUNTIME_MAX=
GRACE=
GUEST=
ENTRY=
HOST_BUILD_ANCHOR=
NODE_VERSION=
NPM_VERSION=
PNPM_VERSION=
RUST_VERSION=
CARGO_VERSION=
WASM_BINDGEN_VERSION=
while [[ $# -gt 0 ]]; do
    [[ "$1" == -- || $# -ge 2 ]] || exit 2
    case "$1" in
        --unit) UNIT="$2"; shift 2 ;;
        --memory-max) MEMORY_MAX="$2"; shift 2 ;;
        --tasks-max) TASKS_MAX="$2"; shift 2 ;;
        --runtime-max) RUNTIME_MAX="$2"; shift 2 ;;
        --grace) GRACE="$2"; shift 2 ;;
        --guest) GUEST="$2"; shift 2 ;;
        --entry) ENTRY="$2"; shift 2 ;;
        --host-build-anchor) HOST_BUILD_ANCHOR="$2"; shift 2 ;;
        --node-version) NODE_VERSION="$2"; shift 2 ;;
        --npm-version) NPM_VERSION="$2"; shift 2 ;;
        --pnpm-version) PNPM_VERSION="$2"; shift 2 ;;
        --rust-version) RUST_VERSION="$2"; shift 2 ;;
        --cargo-version) CARGO_VERSION="$2"; shift 2 ;;
        --wasm-bindgen-version) WASM_BINDGEN_VERSION="$2"; shift 2 ;;
        --) shift; break ;;
        *) exit 2 ;;
    esac
done
[[ "$UNIT" =~ ^clearra-rust-[0-9]+-[0-9]+$ ]] || exit 2
[[ "$MEMORY_MAX" =~ ^[1-9][0-9]+$ ]] || exit 2
[[ "$TASKS_MAX" =~ ^[1-9][0-9]*$ ]] || exit 2
[[ "$RUNTIME_MAX" =~ ^[1-9][0-9]*$ ]] || exit 2
[[ "$GRACE" =~ ^[1-9][0-9]{0,3}$ ]] || exit 2
[[ "$ENTRY" =~ ^[a-z0-9-]+$ && -f "$GUEST" && ! -L "$GUEST" ]] || exit 2
source "${GUEST%/*}/clearra-wsl-source-storage.sh"
for version in "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" "$RUST_VERSION" "$CARGO_VERSION" "$WASM_BINDGEN_VERSION"; do
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
done
distribution="${WSL_DISTRO_NAME:?Missing WSL distribution identity}"
build_environment=()
append_wsl_build_environment build_environment "$distribution" "$HOST_BUILD_ANCHOR" || exit 2

# The already-running user manager owns one transient group. Never stop
# the distribution, a shared slice, another user's process or service.
runtime_dir="/run/user/$EUID"
[[ -d "$runtime_dir" && -O "$runtime_dir" && ! -L "$runtime_dir" && -S "$runtime_dir/bus" ]] || {
    printf 'Clearra WSL requires a bounded current-user systemd service\n' >&2
    exit 2
}
[[ "$(stat -f -c %T /sys/fs/cgroup)" == cgroup2fs ]] || exit 2
export XDG_RUNTIME_DIR="$runtime_dir"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus"
user_cgroup="$(timeout --kill-after=2 5 systemctl --user show --property=ControlGroup --value -- -.slice)"
[[ "$user_cgroup" == "/user.slice/user-$EUID.slice/user@$EUID.service" ]] || {
    printf 'Clearra WSL could not verify the current user cgroup\n' >&2
    exit 2
}
unit_name="$UNIT.service"
control_group="$user_cgroup/app.slice/$unit_name"
cgroup_path="/sys/fs/cgroup$control_group"
[[ ! -e "$cgroup_path" ]] || exit 2

# If EOF arrives before a queued service starts, the guest sees cancellation
# or no lease directory, and declines to start any work.
lease_dir="$(mktemp -d -- "$runtime_dir/clearra-lease.XXXXXXXX")"
parent_lost_marker="$lease_dir/parent-lost"
trap 'rm -f -- "$parent_lost_marker"; rmdir -- "$lease_dir"' EXIT
source_slot= source_identity=
case "$ENTRY" in
    wasm-build | core-c-tests | native-cargo | pc-runtime-build-batch)
        source_root="$(clearra_wsl_source_root)" || exit 2
        source_identity="$(create_clearra_wsl_source_slot "$source_root" "$lease_dir")" || exit 2
        source_slot="$source_root/runtime-source"
        ;;
esac
stop_unit() {
    timeout --kill-after=2 "$(( GRACE + 5 ))" systemctl --user stop -- "$unit_name" >/dev/null 2>&1
}
mark_parent_lost() {
    : >"$parent_lost_marker"
    stop_unit || true
}
host_signal_cleanup() {
    trap '' HUP TERM INT
    mark_parent_lost
    exit 125
}
trap host_signal_cleanup HUP TERM INT
lease_watch() {
    trap 'exit 0' HUP TERM INT
    while IFS= read -r _; do :; done
    trap '' HUP TERM INT
    mark_parent_lost
}
start_lease_watch
watcher=$!

status=0
systemd_arguments=(
    --user --quiet --wait --pipe --slice=app.slice --expand-environment=no
    --unit "$UNIT"
    --property "MemoryMax=$MEMORY_MAX"
    --property "MemorySwapMax=0"
    --property "TasksMax=$TASKS_MAX"
    --property "OOMPolicy=kill"
    --property "RuntimeMaxSec=${RUNTIME_MAX}s"
    --property "TimeoutStopSec=${GRACE}s"
    --property "KillMode=control-group"
    --setenv "CLEARRA_WSL_LEASE_DIR=$lease_dir"
    --setenv "CLEARRA_WSL_EXPECTED_CGROUP=$control_group"
    --setenv "CLEARRA_WSL_MEMORY_MAX=$MEMORY_MAX"
    --setenv "CLEARRA_WSL_TASKS_MAX=$TASKS_MAX"
    --setenv "CLEARRA_WSL_SOURCE_SLOT=$source_slot"
    --setenv "CLEARRA_WSL_SOURCE_IDENTITY=$source_identity"
    --setenv "XDG_STATE_HOME=${XDG_STATE_HOME:-$HOME/.local/state}"
    "${build_environment[@]}"
    --setenv "CLEARRA_WSL_NODE_VERSION=$NODE_VERSION"
    --setenv "CLEARRA_WSL_NPM_VERSION=$NPM_VERSION"
    --setenv "CLEARRA_WSL_PNPM_VERSION=$PNPM_VERSION"
    --setenv "CLEARRA_WSL_RUST_VERSION=$RUST_VERSION"
    --setenv "CLEARRA_WSL_CARGO_VERSION=$CARGO_VERSION"
    --setenv "CLEARRA_WSL_WASM_BINDGEN_VERSION=$WASM_BINDGEN_VERSION"
)
if [[ ! -e "$parent_lost_marker" ]]; then
    systemd-run "${systemd_arguments[@]}" -- /bin/bash "$GUEST" "$ENTRY" "$@" </dev/null || status=$?
fi

kill "$watcher" >/dev/null 2>&1 || true
wait "$watcher" >/dev/null 2>&1 || true
unit_property() {
    timeout --kill-after=2 5 systemctl --user show --property="$1" --value -- "$unit_name" 2>/dev/null
}
result="$(unit_property Result || true)"
memory_peak="$(unit_property MemoryPeak || true)"
stop_unit || true
active_state="$(unit_property ActiveState || true)"
observed_cgroup="$(unit_property ControlGroup || true)"
cgroup_removed=false
if owned_cgroup_drained "$cgroup_path" "$control_group" "$observed_cgroup" "$active_state"; then
    cgroup_removed=true
    if [[ "$active_state" == failed ]]; then
        timeout --kill-after=2 5 systemctl --user reset-failed -- "$unit_name" >/dev/null 2>&1 || true
    fi
fi
if [[ "$cgroup_removed" != true || -e "$parent_lost_marker" ]]; then
    status=125
fi
if [[ -n "$source_slot" ]]; then
    if [[ "$status" == 0 ]] &&
        retire_clearra_wsl_source_slot "$source_slot" "$lease_dir" "$source_identity" "$cgroup_removed"; then
        printf 'CLEARRA_WSL_SOURCE_STORAGE=released\n'
    else
        printf 'CLEARRA_WSL_SOURCE_STORAGE=preserved\n'
        printf 'Clearra WSL source slot retained for review: %s\n' "$source_slot" >&2
        [[ "$status" != 0 ]] || status=125
    fi
fi
if [[ -f "$parent_lost_marker" ]]; then
    printf 'CLEARRA_WSL_RESULT=parent-lost\n'
else
    printf 'CLEARRA_WSL_RESULT=%s\n' "${result:-exit-code}"
fi
printf 'CLEARRA_WSL_CONTROL_GROUP=%s\n' "$control_group"
printf 'CLEARRA_WSL_MEMORY_PEAK=%s\n' "${memory_peak:-unknown}"
if [[ "$cgroup_removed" == true ]]; then
    printf 'CLEARRA_WSL_REMAINING_TASKS=0\n'
else
    printf 'CLEARRA_WSL_REMAINING_TASKS=unknown\n'
fi
printf 'CLEARRA_WSL_UNIT_ACTIVE_STATE=%s\n' "${active_state:-unknown}"
printf 'CLEARRA_WSL_CGROUP_REMOVED=%s\n' "$cgroup_removed"
exit "$status"
