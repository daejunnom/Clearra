#!/usr/bin/env bash

# Source copies are separate from the host compiler catalog and from results.
# Use the WSL external root declared by config/clearra-management.v1.json.
clearra_wsl_source_root() {
    local state="${XDG_STATE_HOME:-$HOME/.local/state}"
    local root="$state/Clearra/wsl"
    [[ "$HOME" == /* && "$HOME" != / && -O "$HOME" &&
       "$state" == /* && "$root" != /mnt/* && "$root" != *[[:cntrl:]]* &&
       "$(realpath -m -- "$root")" == "$root" ]] || return 2
    printf '%s' "$root"
}

create_clearra_wsl_source_slot() {
    local root="$1" slot="$1/runtime-source" lease="$2" identity
    [[ "$(clearra_wsl_source_root)" == "$root" ]] || return 2
    mkdir -p -- "$root" || return 2
    [[ -O "$root" && "$(stat -f -c %T -- "$root")" == ext2/ext3 ]] || return 2
    # Keep this path stable across source revisions so the compiler catalog
    # can reuse its experiment slot. An interrupted copy blocks new writers.
    mkdir -m 700 -- "$slot" || {
        printf 'Clearra WSL source slot is already owned or unavailable: %s\n' "$slot" >&2
        return 2
    }
    identity="$(stat -c '%d:%i' -- "$slot")" || return 2
    (umask 077; set -o noclobber; printf '%s\t%s\n' "$lease" "$identity" >"$slot/.clearra-runtime-source-owner") ||
        return 2
    printf '%s' "$identity"
}

verify_clearra_wsl_source_slot() {
    local slot="$1" lease="$2" identity="$3" owner="$1/.clearra-runtime-source-owner"
    [[ "$slot" == "$(clearra_wsl_source_root)/runtime-source" &&
       "$(realpath -m -- "$slot")" == "$slot" && -d "$slot" && -O "$slot" &&
       "$identity" =~ ^[0-9]+:[0-9]+$ && "$(stat -c '%d:%i' -- "$slot")" == "$identity" &&
       -f "$owner" && ! -L "$owner" && -O "$owner" &&
       "$(<"$owner")" == "$lease"$'\t'"$identity" ]]
}

require_unmounted_source_tree() {
    python3 - "$1" <<'PY'
from pathlib import PurePosixPath
import re
import sys

root = PurePosixPath(sys.argv[1])
with open("/proc/self/mountinfo", encoding="utf-8", errors="surrogateescape") as mounts:
    for line in mounts:
        encoded = line.split(" ", 5)[4]
        mount = re.sub(r"\\([0-7]{3})", lambda part: chr(int(part[1], 8)), encoded)
        if PurePosixPath(mount).is_relative_to(root):
            raise SystemExit("Managed WSL source tree has a nested or root mount")
PY
}

retire_clearra_wsl_source_slot() {
    local slot="$1" lease="$2" identity="$3" drained="$4"
    [[ "$drained" == true ]] || return 2
    verify_clearra_wsl_source_slot "$slot" "$lease" "$identity" || return 2
    require_unmounted_source_tree "$slot" || return 2
    # Tests or custom tools may have written evidence instead of using the
    # host result sink. Keep such a slot for a reviewed move, never erase it.
    [[ ! -e "$slot/source/_local" && ! -L "$slot/source/_local" &&
       ! -e "$slot/source/build" && ! -L "$slot/source/build" &&
       ! -e "$slot/source/coverage" && ! -L "$slot/source/coverage" ]] || return 2
    verify_clearra_wsl_source_slot "$slot" "$lease" "$identity" || return 2
    rm -rf --one-file-system -- "$slot"
}
