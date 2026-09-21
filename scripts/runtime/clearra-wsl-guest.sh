#!/usr/bin/env bash
set -euo pipefail

ENTRY="${1:-}"
shift || true
MARKER=/etc/clearra/runtime.json
[[ -f "$MARKER" ]] || { printf 'Clearra WSL runtime marker is missing\n' >&2; exit 2; }
python3 - "$MARKER" \
    "${CLEARRA_WSL_MARKER_DIGEST:?}" \
    "${CLEARRA_WSL_NODE_VERSION:?}" \
    "${CLEARRA_WSL_NPM_VERSION:?}" \
    "${CLEARRA_WSL_PNPM_VERSION:?}" \
    "${CLEARRA_WSL_RUST_VERSION:?}" \
    "${CLEARRA_WSL_CARGO_VERSION:?}" \
    "${CLEARRA_WSL_WASM_BINDGEN_VERSION:?}" <<'PY'
import json
import pathlib
import sys

path, digest, node, npm, pnpm, rust, cargo, wasm = sys.argv[1:]
value = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
expected = {
    "node": node,
    "npm": npm,
    "pnpm": pnpm,
    "rust": rust,
    "cargo": cargo,
    "wasm_bindgen": wasm,
}
if (
    value.get("schema_id") != "clearra.wsl-runtime.v1"
    or value.get("runtime_user") != "clearra"
    or value.get("toolchain_digest") != digest
    or value.get("toolchains") != expected
):
    raise SystemExit("Clearra WSL runtime marker does not match the host policy")
PY
actual_digest="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchain_digest"])
PY
)"
[[ "$actual_digest" == "${CLEARRA_WSL_MARKER_DIGEST:?}" ]] || {
    printf 'Clearra WSL toolchain marker does not match this source policy\n' >&2
    exit 2
}

NODE_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["node"])
PY
)"
NPM_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["npm"])
PY
)"
RUST_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["rust"])
PY
)"
CARGO_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["cargo"])
PY
)"
PNPM_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["pnpm"])
PY
)"
WASM_BINDGEN_VERSION="$(python3 - "$MARKER" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["toolchains"]["wasm_bindgen"])
PY
)"
export HOME=/home/clearra
export CARGO_HOME=/opt/clearra/cargo
export RUSTUP_HOME=/opt/clearra/rustup
export COREPACK_HOME=/opt/clearra/corepack
export PATH="/opt/clearra/node/${NODE_VERSION}/bin:/opt/clearra/cargo/bin:/opt/clearra/tools/wasm-bindgen-cli/${WASM_BINDGEN_VERSION}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER \
    CARGO_BUILD_RUSTFLAGS CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
    CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_OPT_LEVEL \
    CARGO_PROFILE_RELEASE_DEBUG CARGO_PROFILE_RELEASE_INCREMENTAL \
    CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CARGO_PROFILE_RELEASE_PANIC \
    CARGO_PROFILE_RELEASE_STRIP
hash -r

sync_source() {
    local archive="$1"
    local digest="$2"
    [[ "$archive" == /mnt/?/* && -f "$archive" ]] || exit 2
    [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || exit 2
    local base="$HOME/.local/share/Clearra/workspaces/$digest"
    local next="$base/source.next.$$"
    local source="$base/source"
    rm -rf -- "$next"
    mkdir -p "$next"
    tar -xzf "$archive" -C "$next"
    rm -rf -- "$source"
    mv -- "$next" "$source"
    printf '%s' "$source"
}

parse_source() {
    [[ "${1:-}" == "--source-archive" && "${3:-}" == "--source-digest" ]] || exit 2
    SOURCE_ROOT="$(sync_source "$2" "$4")"
    shift 4
    SOURCE_ARGS=("$@")
}

verify_toolchains() {
    [[ "$(node --version)" == "v${NODE_VERSION}" ]]
    [[ "$(npm --version)" == "$NPM_VERSION" ]]
    [[ "$(pnpm --version)" == "$PNPM_VERSION" ]]
    [[ "$(rustc "+${RUST_VERSION}" --version)" == "rustc ${RUST_VERSION}"* ]]
    [[ "$(cargo "+${RUST_VERSION}" --version)" == "cargo ${CARGO_VERSION}"* ]]
    [[ "$(wasm-bindgen --version)" == "wasm-bindgen ${WASM_BINDGEN_VERSION}" ]]
    node --version
    npm --version
    pnpm --version
    rustc "+${RUST_VERSION}" --version
    cargo "+${RUST_VERSION}" --version
    wasm-bindgen --version
    python3 - "$MARKER" <<'PY'
import json, pathlib, sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
print(json.dumps({"schema_id": value["schema_id"], "toolchain_digest": value["toolchain_digest"], "toolchains": value["toolchains"]}, sort_keys=True))
PY
}

case "$ENTRY" in
    verify)
        [[ $# -eq 0 ]]
        verify_toolchains
        ;;
    sync-workspace)
        parse_source "$@"
        [[ "${#SOURCE_ARGS[@]}" -eq 0 ]]
        printf 'clearra_wsl_source=%s\n' "$SOURCE_ROOT"
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
        [[ "$STAGING" == /mnt/?/* ]] || exit 2
        mkdir -p "$STAGING"
        TARGET_ROOT="$HOME/.cache/Clearra/build/${CLEARRA_WSL_MARKER_DIGEST}/cargo-target"
        mkdir -p "$TARGET_ROOT"
        export CARGO_TARGET_DIR="$TARGET_ROOT" CARGO_INCREMENTAL=0
        export CLEARRA_SOURCE_COMMIT="$SOURCE_COMMIT" CLEARRA_ENGINE_BUILD_ID="$ENGINE_BUILD_ID"
        if [[ "$VERIFY" -eq 1 ]]; then
            cargo "+${RUST_VERSION}" check --locked --manifest-path "$SOURCE_ROOT/Cargo.toml" --package clearra-cli-command --lib --tests
            cargo "+${RUST_VERSION}" check --locked --manifest-path "$SOURCE_ROOT/Cargo.toml" --package clearra-wasm --lib --tests
            cargo "+${RUST_VERSION}" test --locked --manifest-path "$SOURCE_ROOT/Cargo.toml" --package clearra-wasm --test wasm_host_contract
        fi
        cargo_args=("+${RUST_VERSION}" build --locked --manifest-path "$SOURCE_ROOT/Cargo.toml" --target wasm32-unknown-unknown --release -p clearra-wasm-abi)
        [[ "$STAGE_PROFILING" -eq 0 ]] || cargo_args+=(--features stage-profiling)
        cargo "${cargo_args[@]}"
        wasm-bindgen "$TARGET_ROOT/wasm32-unknown-unknown/release/clearra_wasm.wasm" \
            --target web --out-dir "$STAGING" --out-name clearra_wasm --no-typescript
        verify_toolchains
        python3 - "$STAGING/.clearra-wsl-toolchain.json" "$actual_digest" \
            "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" \
            "$RUST_VERSION" "$WASM_BINDGEN_VERSION" <<'PY'
import json, pathlib, subprocess, sys
path, digest, node, npm, pnpm, rust, wasm = sys.argv[1:]
value = {
    "environment": "wsl",
    "distribution": "Clearra-Build",
    "toolchain_digest": digest,
    "rustc": subprocess.check_output(["rustc", f"+{rust}", "-Vv"], text=True).strip(),
    "cargo": subprocess.check_output(["cargo", f"+{rust}", "-V"], text=True).strip(),
    "wasm_bindgen": subprocess.check_output(["wasm-bindgen", "--version"], text=True).strip(),
    "node": subprocess.check_output(["node", "--version"], text=True).strip(),
    "npm": subprocess.check_output(["npm", "--version"], text=True).strip(),
    "pnpm": subprocess.check_output(["pnpm", "--version"], text=True).strip(),
    "expected_versions": {"node": node, "npm": npm, "pnpm": pnpm, "rust": rust, "wasm_bindgen": wasm},
    "rust_build_environment": "default",
}
pathlib.Path(path).write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")
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
        [[ "$HOST_OUTPUT" == /mnt/?/* ]] || exit 2
        [[ "${#ARGUMENTS[@]}" -eq 7 ]] || exit 2
        if [[ "${ARGUMENTS[4]}" == auto ]]; then
            auto_workers="$(($(nproc) - 1))"
            [[ "$auto_workers" -ge 1 ]] || auto_workers=1
            ARGUMENTS[4]="$auto_workers"
        fi
        [[ "${ARGUMENTS[4]}" =~ ^[1-9][0-9]*$ ]] || exit 2
        [[ "${ARGUMENTS[4]}" -le "$(nproc)" ]] || {
            printf 'Requested workers exceed the dedicated WSL logical processor count; no silent reduction is permitted\n' >&2
            exit 2
        }
        export CLEARRA_WSL_WORKSPACE="$SOURCE_ROOT"
        report="$HOME/.local/state/Clearra/reports/runtime-environments/latest"
        bash "$SOURCE_ROOT/scripts/tools/wsl-pc-runtime-build-and-batch.sh" "${ARGUMENTS[@]}"
        rm -rf -- "$HOST_OUTPUT"
        mkdir -p "$HOST_OUTPUT"
        cp -a -- "$report"/. "$HOST_OUTPUT"/
        filesystem="$(stat -f -c %T "$SOURCE_ROOT")"
        python3 - "$HOST_OUTPUT/clearra-runtime-host.json" "$SOURCE_ROOT" \
            "$filesystem" "$(nproc)" "$CLEARRA_WSL_MARKER_DIGEST" "${ARGUMENTS[4]}" <<'PY'
import json, pathlib, sys
path, source, filesystem, processors, digest, workers = sys.argv[1:]
value = {
    "runtime_root": source,
    "runtime_filesystem": filesystem,
    "logical_processors": int(processors),
    "workers": int(workers),
    "toolchain_digest": digest,
}
pathlib.Path(path).write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")
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
