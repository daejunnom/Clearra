#!/usr/bin/env bash
set -euo pipefail
source "${BASH_SOURCE[0]%/*}/clearra-wsl-source-storage.sh"

finite_cgroup_limits_match() {
    local actual_memory="$1" actual_swap="$2" actual_tasks="$3" expected_memory="$4" expected_tasks="$5"
    [[ "$actual_memory" =~ ^[1-9][0-9]{1,15}$ &&
       "$expected_memory" =~ ^[1-9][0-9]{1,15}$ &&
       "$actual_swap" == 0 && "$actual_tasks" == "$expected_tasks" &&
       "$actual_tasks" =~ ^[1-9][0-9]*$ ]] || return 2
    # Linux memory.max is page-granular. Never grant more than requested,
    # but accept its rounding down on hosts with larger page sizes.
    (( actual_memory <= expected_memory && expected_memory - actual_memory < 1048576 ))
}

verify_cgroup_limits() {
    local control_path="$1" expected_memory="$2" expected_tasks="$3"
    local actual_memory actual_swap actual_tasks oom_group
    read -r actual_memory <"$control_path/memory.max" || return 2
    read -r actual_swap <"$control_path/memory.swap.max" || return 2
    read -r actual_tasks <"$control_path/pids.max" || return 2
    read -r oom_group <"$control_path/memory.oom.group" || return 2
    [[ "$oom_group" == 1 ]] || return 2
    finite_cgroup_limits_match "$actual_memory" "$actual_swap" "$actual_tasks" "$expected_memory" "$expected_tasks"
}

ENTRY="${1:?Missing Clearra WSL entry}"
shift
(( EUID != 0 )) || exit 2
lease_dir="${CLEARRA_WSL_LEASE_DIR:?Missing WSL owner lease}"
clearra_wsl_lease_matches "$lease_dir" &&
[[ "$(realpath -e -- "$lease_dir")" == "$lease_dir" &&
   -d "$lease_dir" && -O "$lease_dir" && ! -e "$lease_dir/parent-lost" ]] || exit 125

# Merely starting a systemd unit is not proof of delegated cgroup controls.
# Check the limits the kernel actually exposes before launching toolchains.
control_group="${CLEARRA_WSL_EXPECTED_CGROUP:?Missing WSL cgroup identity}"
[[ "$control_group" =~ ^/user\.slice/user-"$EUID"\.slice/user@"$EUID"\.service/app\.slice/clearra-rust-[0-9]+-[0-9]+\.service$ ]] || exit 2
grep -Fxq -- "0::$control_group" /proc/self/cgroup || exit 2
verify_cgroup_limits "/sys/fs/cgroup$control_group" "${CLEARRA_WSL_MEMORY_MAX:?}" "${CLEARRA_WSL_TASKS_MAX:?}" || {
    printf 'Clearra WSL could not prove delegated memory, swap, and task limits\n' >&2
    exit 2
}

distribution="${CLEARRA_WSL_DISTRIBUTION:?Missing WSL distribution identity}"
[[ "${#distribution}" -le 256 && "$distribution" != *[[:cntrl:]]* ]] || exit 2
NODE_VERSION="${CLEARRA_WSL_NODE_VERSION:?}"
NPM_VERSION="${CLEARRA_WSL_NPM_VERSION:?}"
PNPM_VERSION="${CLEARRA_WSL_PNPM_VERSION:?}"
RUST_VERSION="${CLEARRA_WSL_RUST_VERSION:?}"
CARGO_VERSION="${CLEARRA_WSL_CARGO_VERSION:?}"
WASM_BINDGEN_VERSION="${CLEARRA_WSL_WASM_BINDGEN_VERSION:?}"
# Retain the chosen account's toolchain, never the mounted Windows PATH.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
toolchain_digest="$(printf '%s\n' "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" \
    "$RUST_VERSION" "$CARGO_VERSION" "$WASM_BINDGEN_VERSION" | sha256sum)"
toolchain_digest="${toolchain_digest%% *}"
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER \
    CARGO_BUILD_RUSTFLAGS CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
    CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_OPT_LEVEL \
    CARGO_PROFILE_RELEASE_DEBUG CARGO_PROFILE_RELEASE_INCREMENTAL \
    CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CARGO_PROFILE_RELEASE_PANIC \
    CARGO_PROFILE_RELEASE_STRIP
hash -r

copy_verified_source_archive() {
    local archive="$1" staged_archive="$2" digest="$3" checksum
    # Do not hash one read of the mounted file and extract a second one.
    # Tar must see the very bytes that passed the declared digest check.
    cp -- "$archive" "$staged_archive" || return 2
    checksum="$(sha256sum -- "$staged_archive")" || return 2
    if [[ "${checksum%% *}" != "$digest" ]]; then
        printf 'Clearra WSL source archive digest mismatch\n' >&2
        return 2
    fi
}

count_source_archive_files() {
    python3 - "$1" <<'PY'
from pathlib import PurePosixPath
import sys
import tarfile


def count_files(source):
    names = set()
    logical_bytes = 0
    with tarfile.open(fileobj=source, mode="r|gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            # The input is a list of tracked files, not a directory tree.
            # Do not let a directory, link, or duplicate inflate a receipt.
            if (not member.isfile() or member.issparse() or not path.parts
                    or path.is_absolute() or ".." in path.parts or path in names):
                raise ValueError("not a unique regular-file source archive")
            names.add(path)
            logical_bytes += member.size
            if len(names) > 100_000 or logical_bytes > 256 * 1024 * 1024:
                raise ValueError("source archive exceeds the bounded source-slot capacity")
    if not names:
        raise ValueError("source archive file count is outside the receipt range")
    return len(names)


if __name__ == "__main__":
    try:
        with open(sys.argv[1], "rb") as source:
            print(count_files(source))
    except (ValueError, OSError, tarfile.TarError):
        sys.exit("Clearra WSL source archive has invalid tracked-file metadata")
PY
}

# Qualification-only plan; no guest entry calls it. The future host protocol
# must bind a canonical Windows checkout ID and live lease to an opened ext4
# slot, and all managed readers must use that lease. sync-workspace still
# exports the old digest path, outside this protocol.
stable_source_slot_plan() {
    python3 - <<'PY'
from dataclasses import dataclass, replace
import re


MARKER_NAME = ".clearra-wsl-source-owner.v1"
MARKER = re.compile(
    r"clearra\.wsl-source-owner\.v1\t([0-9a-f]{24})\t"
    r"([0-9a-f]{64})\t([0-9a-f]{64})\t([0-9a-f]{32})\n"
)


@dataclass(frozen=True)
class StableSourceLease:
    checkout: str
    session: str
    device: int
    # Observation supplied by a future fd-bound supervisor, not caller input.
    exclusive_and_readers_drained: bool

    @property
    def incoming(self):
        return ".incoming-" + self.session


@dataclass(frozen=True)
class StableSourceTree:
    name: str
    device: int
    inode: int
    marker: str
    # Current-tree projection from the same verified relative file inventory
    # and bytes as staging (excluding only MARKER_NAME). A marker or archive
    # digest alone is not a proof that Cargo/fmt did not mutate the tree.
    material_sha256: str
    # Exact directory identity beneath the opened slot, with no linked or
    # mounted entries in its owned tree.
    owned_directory: bool


@dataclass(frozen=True)
class StableSourcePlan:
    action: str
    lease: StableSourceLease
    current: StableSourceTree | None
    incoming: StableSourceTree


def validate_lease(lease):
    if (re.fullmatch(r"[0-9a-f]{24}", lease.checkout) is None
            or re.fullmatch(r"[0-9a-f]{32}", lease.session) is None
            or lease.device < 0 or lease.exclusive_and_readers_drained is not True):
        raise ValueError("stable source slot has no live exclusive reader fence")


def validate_tree(lease, tree, name):
    marker = MARKER.fullmatch(tree.marker)
    if (tree.owned_directory is not True or tree.name != name
            or tree.device != lease.device or tree.inode <= 0 or marker is None):
        raise ValueError("unowned stable source directory")
    checkout, archive, material, creator = marker.groups()
    if checkout != lease.checkout or material != tree.material_sha256:
        raise ValueError("stable source marker or content mismatch")
    return archive, material, creator


def plan_install(lease, verified_archive, incoming, current):
    validate_lease(lease)
    archive, material, creator = validate_tree(lease, incoming, lease.incoming)
    if archive != verified_archive or creator != lease.session:
        raise ValueError("incoming tree is not bound to this verified archive and lease")
    # None means ENOENT from a no-follow observation, not a malformed, linked,
    # unreadable or legacy directory. Those observations must fail closed.
    if current is None:
        return StableSourcePlan("rename_noreplace", lease, current, incoming)
    old_archive, old_material, _ = validate_tree(lease, current, "source")
    if current.inode == incoming.inode:
        raise ValueError("stable source directory aliases the incoming tree")
    if old_archive == archive:
        if old_material != material:
            raise ValueError("same source archive has two incompatible material identities")
        return StableSourcePlan("reuse", lease, current, incoming)
    # Exchange source and its marker together on the same device. Do not
    # emulate this through unlink, copy-over or two successive renames.
    return StableSourcePlan("rename_exchange", lease, current, incoming)


def retirement_target(plan, live_lease, current_after, incoming_after):
    validate_lease(live_lease)
    if live_lease != plan.lease:
        raise ValueError("source retirement requires the original live lease")
    new_source = replace(plan.incoming, name="source")
    if plan.action == "rename_noreplace":
        expected = (new_source, None)
    elif plan.action == "reuse":
        expected = (plan.current, plan.incoming)
    elif plan.action == "rename_exchange" and plan.current is not None:
        expected = (new_source, replace(plan.current, name=live_lease.incoming))
    else:
        raise ValueError("unknown stable source transaction")
    if (current_after, incoming_after) != expected:
        raise ValueError("stable source has not reached the expected atomic outcome")
    # Supply the exact sibling identity, including inode+marker, for a future
    # dir-fd-relative recheck and removal; never retire current or a legacy
    # digest directory. This prototype neither issues a rename nor unlinks.
    return incoming_after
PY
}

sync_source() (
    # A command substitution normally clears errexit. Keep digest/copy/tar
    # failures terminal inside this private extraction transaction too.
    set -euo pipefail
    local archive="$1"
    local digest="$2"
    local mode="$3"
    [[ "$archive" == /mnt/?/* && -f "$archive" && ! -L "$archive" ]] || exit 2
    [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || exit 2
    local root base
    root="$(clearra_wsl_source_root)"
    if [[ "$mode" == receipt ]]; then
        base="$root/workspaces/$digest"
        # Explicit sync returns a reusable path. Keep existing copies until
        # the user reviews them; refuse a fifth generation rather than
        # silently evict a workspace with unmanaged external readers.
        if [[ ! -d "$base" ]]; then
            local slots
            shopt -s nullglob
            slots=("$root"/workspaces/*)
            (( ${#slots[@]} < 4 )) || {
                printf 'Clearra WSL retained source limit reached; review existing sync-workspace copies\n' >&2
                exit 2
            }
        fi
    else
        base="${CLEARRA_WSL_SOURCE_SLOT:?Missing runtime source slot}"
        verify_clearra_wsl_source_slot "$base" "$lease_dir" "${CLEARRA_WSL_SOURCE_IDENTITY:?Missing runtime source owner}" || exit 2
    fi
    local source="$base/source"
    local owner="$base/.clearra-source-owner" inner_owner=".clearra-wsl-extracted-source"
    [[ "$HOME" == /* && "$HOME" != / && -O "$HOME" &&
       "$(realpath -m -- "$base")" == "$base" && ! -L "$source" ]] || exit 2
    umask 077
    mkdir -p -- "$base"
    [[ -O "$base" && "$(stat -f -c %T -- "$base")" == ext2/ext3 ]] || exit 2
    local scratch file_count old_identity old_owner new_record
    scratch="$(mktemp -d -- "$base/.incoming.XXXXXXXX")"
    cleanup_source_staging() {
        require_unmounted_source_tree "$scratch" || return
        rm -rf --one-file-system -- "$scratch"
    }
    trap cleanup_source_staging EXIT
    copy_verified_source_archive "$archive" "$scratch/source.tar.gz" "$digest"
    file_count="$(count_source_archive_files "$scratch/source.tar.gz")"
    mkdir -- "$scratch/source"
    tar --no-same-owner -xzf "$scratch/source.tar.gz" -C "$scratch/source"
    [[ ! -e "$scratch/source/$inner_owner" && ! -L "$scratch/source/$inner_owner" ]] || exit 2
    new_record="clearra.wsl-source.v2:$digest:$(stat -c '%d:%i' -- "$scratch/source"):${scratch##*/}"
    (set -o noclobber; printf '%s\n' "$new_record" >"$scratch/new-owner")
    (set -o noclobber; printf '%s\n' "$new_record" >"$scratch/source/$inner_owner")
    # The explicit sync path may have external readers. Compare a repeated
    # request with its freshly verified archive, but never refresh in place.
    if [[ -e "$source" ]]; then
        [[ "$mode" == receipt ]] || exit 2
        [[ -d "$source" && -O "$source" && ! -L "$source" ]] || exit 2
        [[ -f "$owner" && ! -L "$owner" && -O "$owner" &&
           -f "$source/$inner_owner" && ! -L "$source/$inner_owner" && -O "$source/$inner_owner" ]] || exit 2
        old_identity="$(stat -c '%d:%i' -- "$source")"
        old_owner="$(<"$owner")"
        [[ "$old_owner" == "clearra.wsl-source.v2:$digest:$old_identity:.incoming."* &&
           "$(<"$source/$inner_owner")" == "$old_owner" ]] || exit 2
        require_unmounted_source_tree "$source"
        local unsafe
        unsafe="$(find "$source" ! -type d \( ! -type f -o -links +1 \) -exec printf x \; -quit)"
        [[ -z "$unsafe" ]] || exit 2
        unsafe="$(find "$source" -mindepth 2 -name "$inner_owner" -exec printf x \; -quit)"
        [[ -z "$unsafe" ]] || exit 2
        diff --brief --recursive --no-dereference --exclude="$inner_owner" -- "$source" "$scratch/source" >/dev/null || {
            printf 'Clearra WSL synchronized source differs; existing files retained\n' >&2
            exit 2
        }
        [[ "$(stat -c '%d:%i' -- "$source")" == "$old_identity" ]] || exit 2
    else
        [[ ! -e "$owner" && ! -L "$owner" ]] || exit 2
        if ! mv -nT -- "$scratch/source" "$source" || [[ -e "$scratch/source" ]]; then
            exit 2
        fi
        mv -T -- "$scratch/new-owner" "$owner"
    fi
    if [[ "$mode" == receipt ]]; then
        [[ "$source" != *[[:cntrl:]]* ]] || exit 2
        printf 'clearra_wsl_source_receipt_v2=%s\t%s\t%s\t%s\n' "$distribution" "$source" "$digest" "$file_count"
    else
        printf '%s' "$source"
    fi
)

validate_source_arguments() {
    [[ $# -ge 4 && "$1" == "--source-archive" && "$3" == "--source-digest" ]] || exit 2
}

parse_source() {
    validate_source_arguments "$@"
    SOURCE_ROOT="$(sync_source "$2" "$4" path)"
    shift 4
    SOURCE_ARGS=("$@")
}

require_fresh_or_empty_directory() {
    local output="$1" existing
    [[ ! -L "$output" ]] || return 2
    if [[ -e "$output" ]]; then
        [[ -d "$output" ]] || return 2
        # Print a fixed token instead of the name (even a filename containing
        # only newlines must make the directory count as non-empty).
        existing="$(find "$output" -mindepth 1 -maxdepth 1 -exec printf x \; -quit)" || return 2
        [[ -z "$existing" ]] || {
            printf 'Clearra WSL benchmark output must be a new or empty directory\n' >&2
            return 2
        }
    fi
}

require_empty_benchmark_output() {
    local output="$1"
    [[ "$output" == /mnt/?/* && "$(realpath -m -- "$output")" == "$output" ]] || return 2
    require_fresh_or_empty_directory "$output"
}

verify_toolchains() {
    [[ "$(node --version)" == "v${NODE_VERSION}" ]] || return 2
    [[ "$(npm --version)" == "$NPM_VERSION" ]] || return 2
    [[ "$(pnpm --version)" == "$PNPM_VERSION" ]] || return 2
    verify_rust || return 2
    [[ "$(wasm-bindgen --version)" == "wasm-bindgen ${WASM_BINDGEN_VERSION}" ]] || return 2
    node --version
    npm --version
    pnpm --version
    rustup run "$RUST_VERSION" rustc --version
    rustup run "$RUST_VERSION" cargo --version
    wasm-bindgen --version
    python3 - "$distribution" "$toolchain_digest" "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" \
        "$RUST_VERSION" "$CARGO_VERSION" "$WASM_BINDGEN_VERSION" <<'PY'
import json, sys
distribution, digest, node, npm, pnpm, rust, cargo, wasm_bindgen = sys.argv[1:]
print(json.dumps({
    "schema_id": "clearra.wsl-toolchain.v2",
    "distribution": distribution,
    "toolchain_digest": digest,
    "toolchains": dict(node=node, npm=npm, pnpm=pnpm, rust=rust, cargo=cargo, wasm_bindgen=wasm_bindgen),
}, sort_keys=True))
PY
}

verify_rust() {
    # rustup run selects only an already-installed toolchain. A diagnostic
    # lease does not bootstrap the user's account or modify toolchain setup.
    [[ "$(rustup run "$RUST_VERSION" rustc --version)" == "rustc $RUST_VERSION "* &&
       "$(rustup run "$RUST_VERSION" cargo --version)" == "cargo $CARGO_VERSION "* ]]
}

case "$ENTRY" in
    verify)
        [[ $# -eq 0 ]]
        verify_toolchains
        ;;
    sync-workspace)
        validate_source_arguments "$@"
        [[ $# -eq 4 ]]
        sync_source "$2" "$4" receipt
        ;;
    wasm-build)
        parse_source "$@"
        set -- "${SOURCE_ARGS[@]}"
        STAGING=
        VERIFY=0
        STAGE_PROFILING=0
        SOURCE_COMMIT=unverified
        ENGINE_BUILD_ID=unverified
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --staging) STAGING="$2"; shift 2 ;;
                --verify) VERIFY=1; shift ;;
                --stage-profiling) STAGE_PROFILING=1; shift ;;
                --source-commit) SOURCE_COMMIT="$2"; shift 2 ;;
                --engine-build-id) ENGINE_BUILD_ID="$2"; shift 2 ;;
                *) exit 2 ;;
            esac
        done
        [[ "$STAGING" == /mnt/?/* && "$(realpath -m -- "$STAGING")" == "$STAGING" ]] || exit 2
        verify_toolchains
        mkdir -p "$STAGING"
        export CARGO_INCREMENTAL=0
        export CLEARRA_SOURCE_COMMIT="$SOURCE_COMMIT" CLEARRA_ENGINE_BUILD_ID="$ENGINE_BUILD_ID"
        node "$SOURCE_ROOT/scripts/tools/invoke-clearra-build.mjs" --source-root "$SOURCE_ROOT" --purpose experiment \
            -- bash -euo pipefail -c '
                rust="$1"
                staging="$2"
                if [[ "$3" == 1 ]]; then
                    rustup run "$rust" cargo check --locked -p clearra-cli-command --lib --tests
                    rustup run "$rust" cargo check --locked -p clearra-wasm --lib --tests
                    rustup run "$rust" cargo test --locked -p clearra-wasm --test wasm_host_contract
                fi
                args=(build --locked --target wasm32-unknown-unknown --release -p clearra-wasm-abi)
                [[ "$4" == 0 ]] || args+=(--features stage-profiling)
                rustup run "$rust" cargo "${args[@]}"
                wasm-bindgen "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/clearra_wasm.wasm" \
                    --target web --out-dir "$staging" --out-name clearra_wasm --no-typescript
            ' -- "$RUST_VERSION" "$STAGING" "$VERIFY" "$STAGE_PROFILING"
        python3 - "$STAGING/.clearra-wsl-toolchain.json" "$distribution" "$toolchain_digest" \
            "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" \
            "$RUST_VERSION" "$WASM_BINDGEN_VERSION" <<'PY'
import json, pathlib, subprocess, sys
path, distribution, digest, node, npm, pnpm, rust, wasm = sys.argv[1:]
value = {
    "environment": "wsl",
    "distribution": distribution,
    "toolchain_digest": digest,
    "rustc": subprocess.check_output(["rustup", "run", rust, "rustc", "-Vv"], text=True).strip(),
    "cargo": subprocess.check_output(["rustup", "run", rust, "cargo", "-V"], text=True).strip(),
    "wasm_bindgen": subprocess.check_output(["wasm-bindgen", "--version"], text=True).strip(),
    "node": subprocess.check_output(["node", "--version"], text=True).strip(),
    "npm": subprocess.check_output(["npm", "--version"], text=True).strip(),
    "pnpm": subprocess.check_output(["pnpm", "--version"], text=True).strip(),
    "expected_versions": {"node": node, "npm": npm, "pnpm": pnpm, "rust": rust, "wasm_bindgen": wasm},
    "rust_build_environment": "default",
}
with pathlib.Path(path).open("x", encoding="utf-8") as receipt:
    receipt.write(json.dumps(value, sort_keys=True) + "\n")
PY
        ;;
    oracle-local-layers-v080)
        repository_root=
        accepted_ctk3=
        output_root=
        source_commit=
        accepted_run_id=
        accepted_run_attempt=
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --repository-root) repository_root="$2"; shift 2 ;;
                --accepted-ctk3) accepted_ctk3="$2"; shift 2 ;;
                --output) output_root="$2"; shift 2 ;;
                --source-commit) source_commit="$2"; shift 2 ;;
                --accepted-run-id) accepted_run_id="$2"; shift 2 ;;
                --accepted-run-attempt) accepted_run_attempt="$2"; shift 2 ;;
                *) exit 2 ;;
            esac
        done
        [[ "$repository_root" == /mnt/?/* && -d "$repository_root" ]] || exit 2
        [[ "$accepted_ctk3" == /mnt/?/* && -d "$accepted_ctk3" ]] || exit 2
        [[ "$output_root" == /mnt/?/* && -d "$output_root" ]] || exit 2
        [[ "$source_commit" =~ ^[0-9a-f]{40}$ ]] || exit 2
        [[ "$accepted_run_id" =~ ^[1-9][0-9]{0,19}$ ]] || exit 2
        [[ "$accepted_run_attempt" =~ ^[1-9][0-9]{0,19}$ ]] || exit 2
        exec bash "$repository_root/scripts/release/oracle/create-local-layers-v080.sh" \
            "$repository_root" "$accepted_ctk3" "$source_commit" \
            "$accepted_run_id" "$accepted_run_attempt" "$output_root"
        ;;
    core-c-tests)
        parse_source "$@"
        export CLEARRA_WSL_WORKSPACE="$SOURCE_ROOT"
        exec bash "$SOURCE_ROOT/scripts/tools/wsl-core-c-tests.sh" "${SOURCE_ARGS[@]}"
        ;;
    native-cargo)
        verify_rust
        parse_source "$@"
        [[ "${#SOURCE_ARGS[@]}" -gt 0 ]]
        case "${SOURCE_ARGS[0]}" in
            build | check | test | fmt | clippy | metadata) ;;
            *) printf 'Unsupported managed WSL Cargo operation\n' >&2; exit 2 ;;
        esac
        export CLEARRA_WSL_WORKSPACE="$SOURCE_ROOT"
        exec bash "$SOURCE_ROOT/scripts/tools/wsl-native-cargo.sh" "${SOURCE_ARGS[@]}"
        ;;
    pc-runtime-build-batch)
        verify_rust
        parse_source "$@"
        set -- "${SOURCE_ARGS[@]}"
        HOST_OUTPUT=
        ARGUMENTS=()
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --host-output) HOST_OUTPUT="$2"; shift 2 ;;
                *) ARGUMENTS+=("$1"); shift ;;
            esac
        done
        require_empty_benchmark_output "$HOST_OUTPUT"
        [[ "${#ARGUMENTS[@]}" -eq 6 ]] || exit 2
        if [[ "${ARGUMENTS[3]}" == auto ]]; then
            auto_workers="$(($(nproc) - 1))"
            [[ "$auto_workers" -ge 1 ]] || auto_workers=1
            ARGUMENTS[3]="$auto_workers"
        fi
        [[ "${ARGUMENTS[3]}" =~ ^[1-9][0-9]*$ ]] || exit 2
        [[ "${ARGUMENTS[3]}" -le "$(nproc)" ]] || {
            printf 'Requested workers exceed the WSL logical processor count; no silent reduction is permitted\n' >&2
            exit 2
        }
        export CLEARRA_WSL_WORKSPACE="$SOURCE_ROOT"
        report="$(mktemp -d -- "$SOURCE_ROOT/.clearra-pc-runtime.XXXXXXXX")"
        export CLEARRA_WSL_REPORT_ROOT="$report"
        trap 'rm -rf --one-file-system -- "$report"' EXIT
        bash "$SOURCE_ROOT/scripts/tools/wsl-pc-runtime-build-and-batch.sh" \
            "${ARGUMENTS[0]}" "$report" "${ARGUMENTS[@]:1}"
        require_empty_benchmark_output "$HOST_OUTPUT"
        mkdir -p -- "$HOST_OUTPUT"
        require_empty_benchmark_output "$HOST_OUTPUT"
        cp -a --no-clobber -- "$report"/. "$HOST_OUTPUT"/
        filesystem="$(stat -f -c %T "$SOURCE_ROOT")"
        python3 - "$HOST_OUTPUT/clearra-runtime-host.json" "$SOURCE_ROOT" \
            "$filesystem" "$(nproc)" "$toolchain_digest" "${ARGUMENTS[3]}" "$distribution" <<'PY'
import json, pathlib, sys
path, source, filesystem, processors, digest, workers, distribution = sys.argv[1:]
value = {
    "runtime_root": source,
    "runtime_filesystem": filesystem,
    "distribution": distribution,
    "logical_processors": int(processors),
    "workers": int(workers),
    "toolchain_digest": digest,
}
with pathlib.Path(path).open("x", encoding="utf-8") as receipt:
    receipt.write(json.dumps(value, sort_keys=True) + "\n")
PY
        printf 'clearra_wsl_source_filesystem=%s\n' "$filesystem"
        ;;
    fixture-normal)
        [[ $# -eq 0 ]]
        printf 'fixture=normal\n'
        ;;
    posix-syntax-audit)
        [[ $# -gt 0 ]]
        syntax_shell=dash
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --shell)
                    syntax_shell="$2"
                    case "$syntax_shell" in bash | dash) ;; *) exit 2 ;; esac
                    shift 2
                    ;;
                --host-path)
                    [[ "${2:-}" == /mnt/?/* && -f "$2" ]] || exit 2
                    "/usr/bin/${syntax_shell}" -n -- "$2"
                    shift 2
                    ;;
                *) exit 2 ;;
            esac
        done
        ;;
    fixture-nonzero)
        [[ $# -eq 0 ]]
        exit 23
        ;;
    fixture-timeout)
        [[ $# -eq 0 ]]
        sleep 600
        ;;
    fixture-oom)
        [[ $# -eq 0 ]]
        python3 - <<'PY'
import subprocess
import sys
import time

leaf = "chunks=[]\nwhile True:\n chunks.append(bytearray(64*1024*1024));chunks[-1][0]=1\n"
middle = (
    "import subprocess,sys,time;"
    f"subprocess.Popen([sys.executable,'-c',{leaf!r}]);"
    "time.sleep(600)"
)
subprocess.Popen([sys.executable, "-c", middle])
time.sleep(600)
PY
        ;;
    fixture-tree)
        [[ $# -eq 0 ]]
        bash -c 'trap "" TERM; while :; do sleep 10; done' &
        wait
        ;;
    *)
        printf 'Unregistered Clearra WSL guest entrypoint: %s\n' "$ENTRY" >&2
        exit 2
        ;;
esac
