#!/usr/bin/env bash
set -euo pipefail

[[ "$(id -u)" -eq 0 ]] || {
    printf 'Clearra WSL sanitization requires root\n' >&2
    exit 2
}

TRANSACTION=
SOURCE_DISTRO=
MEMORY_MAX=
TASKS_MAX=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --transaction) TRANSACTION="$2"; shift 2 ;;
        --source-distro) SOURCE_DISTRO="$2"; shift 2 ;;
        --memory-max) MEMORY_MAX="$2"; shift 2 ;;
        --tasks-max) TASKS_MAX="$2"; shift 2 ;;
        *) printf 'Unknown Clearra WSL sanitization argument\n' >&2; exit 2 ;;
    esac
done
[[ "$TRANSACTION" =~ ^[A-Za-z0-9._-]+$ ]] || exit 2
[[ "$SOURCE_DISTRO" =~ ^[A-Za-z0-9._-]+$ ]] || exit 2
[[ "$MEMORY_MAX" =~ ^[1-9][0-9]+$ ]] || exit 2
[[ "$TASKS_MAX" =~ ^[1-9][0-9]*$ ]] || exit 2

MARKER=/etc/clearra/runtime.json
python3 - "$MARKER" "$TRANSACTION" "$SOURCE_DISTRO" <<'PY'
import json
import pathlib
import sys

path, transaction, source = sys.argv[1:]
value = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
if (
    value.get("schema_id") != "clearra.wsl-provision-owner.v1"
    or value.get("created_transaction") != transaction
    or value.get("source_distribution") != source
    or value.get("status") != "imported"
):
    raise SystemExit("Clearra WSL import ownership marker does not match")
PY

# systemd is deliberately disabled for this first boot.  Establish the same
# hard aggregate limits directly in cgroup v2 before touching cloned state.
[[ "$(stat -f -c %T /sys/fs/cgroup)" == cgroup2fs ]] || {
    printf 'Clearra WSL sanitization requires cgroup v2\n' >&2
    exit 2
}
CURRENT_CGROUP=
while IFS=: read -r hierarchy controllers path; do
    if [[ "$hierarchy" == 0 && -z "$controllers" ]]; then
        CURRENT_CGROUP="$path"
        break
    fi
done </proc/self/cgroup
[[ -n "$CURRENT_CGROUP" ]] || exit 2
CGROUP_PARENT="/sys/fs/cgroup${CURRENT_CGROUP%/}"
[[ -d "$CGROUP_PARENT" && -w "$CGROUP_PARENT/cgroup.procs" ]] || exit 2
available_controllers=" $(<"$CGROUP_PARENT/cgroup.controllers") "
[[ "$available_controllers" == *" memory "* && "$available_controllers" == *" pids "* ]] || {
    printf 'Clearra WSL sanitization requires memory and pids controllers\n' >&2
    exit 2
}
printf '+memory +pids\n' >"$CGROUP_PARENT/cgroup.subtree_control"
CGROUP="$CGROUP_PARENT/clearra-bootstrap-$TRANSACTION"
[[ ! -e "$CGROUP" ]]
mkdir "$CGROUP"
printf '%s\n' "$MEMORY_MAX" >"$CGROUP/memory.max"
printf '0\n' >"$CGROUP/memory.swap.max"
printf '%s\n' "$TASKS_MAX" >"$CGROUP/pids.max"
printf '1\n' >"$CGROUP/memory.oom.group"
printf '%s\n' "$$" >"$CGROUP/cgroup.procs"

WATCHER=
PARENT_LOST_MARKER="/run/clearra-bootstrap-${TRANSACTION}.parent-lost"
rm -f "$PARENT_LOST_MARKER"
cleanup() {
    status=$?
    trap - EXIT HUP TERM INT
    if [[ -n "$WATCHER" ]]; then
        kill "$WATCHER" >/dev/null 2>&1 || true
        wait "$WATCHER" >/dev/null 2>&1 || true
    fi
    printf '%s\n' "$$" >"$CGROUP_PARENT/cgroup.procs" 2>/dev/null || true
    if [[ -f "$CGROUP/cgroup.kill" ]]; then
        printf '1\n' >"$CGROUP/cgroup.kill" 2>/dev/null || true
    fi
    rmdir "$CGROUP" >/dev/null 2>&1 || true
    if [[ -f "$PARENT_LOST_MARKER" ]]; then
        rm -f "$PARENT_LOST_MARKER"
        # systemd is intentionally disabled on this first boot. The forced
        # in-guest poweroff affects only Clearra-Build if the host lease dies.
        /sbin/poweroff -f >/dev/null 2>&1 || true
    fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 125' HUP TERM INT
(
    trap 'exit 0' HUP TERM INT
    while IFS= read -r _; do :; done
    trap - HUP TERM INT
    : >"$PARENT_LOST_MARKER"
    kill -TERM "$$" >/dev/null 2>&1 || true
) &
WATCHER=$!

# Preserve only a rustup bootstrap executable.  No home configuration,
# credential file, shell history, or cloned toolchain payload is opened.  The
# source distribution may already contain an unrelated /opt/clearra tree, so
# keep the one executable in the in-memory runtime directory and rebuild the
# dedicated prefix from an empty directory.
bootstrap_rustup="/run/clearra-bootstrap-rustup-${TRANSACTION}"
rm -f -- "$bootstrap_rustup"
for candidate in /home/*/.cargo/bin/rustup /root/.cargo/bin/rustup; do
    if [[ -x "$candidate" && ! -e "$bootstrap_rustup" ]]; then
        cp -- "$candidate" "$bootstrap_rustup"
    fi
done
if [[ ! -x "$bootstrap_rustup" ]] && command -v rustup >/dev/null 2>&1; then
    cp -- "$(command -v rustup)" "$bootstrap_rustup"
fi
[[ -x "$bootstrap_rustup" ]] || {
    printf 'A rustup bootstrap executable is required in the source Ubuntu clone\n' >&2
    exit 2
}
rm -rf -- /opt/clearra
install -d -m 0755 /opt/clearra/bootstrap
install -m 0755 "$bootstrap_rustup" /opt/clearra/bootstrap/rustup
rm -f -- "$bootstrap_rustup"

# Remove credential-capable and machine-identity paths by fixed name.  Their
# contents are never read, listed, archived, hashed, or printed.
for home in /home/* /root; do
    [[ -d "$home" ]] || continue
    rm -rf -- \
        "$home/.ssh" "$home/.gnupg" "$home/.aws" "$home/.azure" \
        "$home/.kube" "$home/.docker" "$home/.config/gcloud" \
        "$home/.local/share/keyrings" "$home/.bash_history" \
        "$home/.zsh_history" "$home/.python_history" \
        "$home/.npmrc" "$home/.pypirc" "$home/.cargo/credentials" \
        "$home/.cargo/credentials.toml"
done
rm -rf -- \
    /etc/docker /etc/containers /etc/azure /etc/aws /etc/gcloud \
    /etc/kubernetes /var/lib/cloud /var/lib/docker /var/lib/containerd \
    /var/lib/containers /var/lib/private/docker /var/lib/snapd
rm -f -- \
    /etc/ssh/ssh_host_* /etc/apt/auth.conf /etc/apt/auth.conf.d/* \
    /etc/npmrc /etc/gitconfig /etc/pip.conf /etc/machine-id \
    /var/lib/dbus/machine-id /var/lib/systemd/random-seed
: >/etc/environment
: >/etc/machine-id

# Remove cloned interactive users and their homes without examining contents.
interactive_accounts=()
while IFS=: read -r account _ uid _; do
    if [[ "$uid" =~ ^[0-9]+$ && "$uid" -ge 1000 && "$uid" -lt 65534 && "$account" != clearra ]]; then
        interactive_accounts+=("$account")
    fi
done </etc/passwd
for account in "${interactive_accounts[@]}"; do
    userdel "$account" >/dev/null 2>&1 || true
done
find /home -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
find /root -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
passwd -l root >/dev/null 2>&1 || true

if getent passwd clearra >/dev/null; then
    userdel clearra >/dev/null 2>&1 || true
fi
useradd --system --create-home --home-dir /home/clearra --shell /usr/sbin/nologin clearra
install -d -o clearra -g clearra \
    /opt/clearra/node \
    /opt/clearra/corepack \
    /opt/clearra/cargo/bin \
    /opt/clearra/rustup \
    /opt/clearra/tools \
    /home/clearra/.cache/Clearra \
    /home/clearra/.local/share/Clearra \
    /home/clearra/.local/state/Clearra
install -m 0755 -o clearra -g clearra \
    /opt/clearra/bootstrap/rustup /opt/clearra/cargo/bin/rustup
for proxy in cargo rustc rustdoc rustfmt clippy-driver; do
    ln -sfn rustup "/opt/clearra/cargo/bin/$proxy"
done

# Start subsequent boots with only the minimum systemd target.  Known cloned
# network, container, cloud, and unattended-maintenance services are masked.
install -d -m 0755 /etc/systemd/system
for unit in \
    ssh.service ssh.socket docker.service docker.socket containerd.service \
    snapd.service snapd.socket cloud-init.service cloud-config.service \
    cloud-final.service apt-daily.service apt-daily.timer \
    apt-daily-upgrade.service apt-daily-upgrade.timer; do
    ln -sfn /dev/null "/etc/systemd/system/$unit"
done
ln -sfn /lib/systemd/system/basic.target /etc/systemd/system/default.target
cat >/etc/wsl.conf <<'EOF'
[boot]
systemd=true

[interop]
appendWindowsPath=false

[user]
default=clearra
EOF

python3 - "$MARKER" "$TRANSACTION" "$SOURCE_DISTRO" <<'PY'
import datetime
import json
import pathlib
import sys

path, transaction, source = sys.argv[1:]
marker = {
    "schema_id": "clearra.wsl-provision-owner.v1",
    "source_distribution": source,
    "created_transaction": transaction,
    "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "status": "sanitized",
}
pathlib.Path(path).write_text(
    json.dumps(marker, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
PY
chmod 0644 "$MARKER"
printf 'clearra_wsl_sanitize=complete transaction=%s\n' "$TRANSACTION"
