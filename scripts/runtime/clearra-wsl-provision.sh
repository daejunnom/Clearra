#!/usr/bin/env bash
set -euo pipefail

[[ "$(id -u)" -eq 0 ]] || { printf 'Clearra WSL provisioning requires root\n' >&2; exit 2; }

if [[ "${1:-}" == "--verify-owner" ]]; then
    [[ $# -eq 2 && "$2" =~ ^[A-Za-z0-9._-]+$ ]] || exit 2
    exec python3 - /etc/clearra/runtime.json "$2" <<'PY'
import json
import pathlib
import sys

marker = pathlib.Path(sys.argv[1])
if not marker.is_file():
    raise SystemExit(3)
try:
    value = json.loads(marker.read_text(encoding="utf-8"))
except (OSError, ValueError):
    raise SystemExit(3)
raise SystemExit(0 if value.get("created_transaction") == sys.argv[2] else 4)
PY
fi

TRANSACTION=
SOURCE_DISTRO=
TOOLCHAIN_DIGEST=
NODE_VERSION=
NODE_SHA256=
NPM_VERSION=
PNPM_VERSION=
RUST_VERSION=
CARGO_VERSION=
WASM_BINDGEN_VERSION=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --transaction) TRANSACTION="$2"; shift 2 ;;
        --source-distro) SOURCE_DISTRO="$2"; shift 2 ;;
        --toolchain-digest) TOOLCHAIN_DIGEST="$2"; shift 2 ;;
        --node-version) NODE_VERSION="$2"; shift 2 ;;
        --node-sha256) NODE_SHA256="$2"; shift 2 ;;
        --npm-version) NPM_VERSION="$2"; shift 2 ;;
        --pnpm-version) PNPM_VERSION="$2"; shift 2 ;;
        --rust-version) RUST_VERSION="$2"; shift 2 ;;
        --cargo-version) CARGO_VERSION="$2"; shift 2 ;;
        --wasm-bindgen-version) WASM_BINDGEN_VERSION="$2"; shift 2 ;;
        *) printf 'Unknown Clearra WSL provision argument\n' >&2; exit 2 ;;
    esac
done
[[ "$TRANSACTION" =~ ^[A-Za-z0-9._-]+$ ]] || exit 2
[[ "$SOURCE_DISTRO" =~ ^[A-Za-z0-9._-]+$ ]] || exit 2
[[ "$TOOLCHAIN_DIGEST" =~ ^[0-9a-f]{64}$ ]] || exit 2
[[ "$NODE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ "$NODE_SHA256" =~ ^[0-9a-f]{64}$ ]] || exit 2
[[ "$NPM_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ "$PNPM_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ "$RUST_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ "$CARGO_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ "$WASM_BINDGEN_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2

python3 - /etc/clearra/runtime.json "$TRANSACTION" "$SOURCE_DISTRO" <<'PY'
import json
import pathlib
import sys

path, transaction, source = sys.argv[1:]
value = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
if (
    value.get("schema_id") != "clearra.wsl-provision-owner.v1"
    or value.get("created_transaction") != transaction
    or value.get("source_distribution") != source
    or value.get("status") != "sanitized"
):
    raise SystemExit("Clearra WSL sanitized ownership marker does not match")
PY
[[ -x /opt/clearra/cargo/bin/rustup ]] || {
    printf 'The sanitized Clearra rustup bootstrap is missing\n' >&2
    exit 2
}

node_archive="/var/tmp/node-v${NODE_VERSION}-linux-x64.tar.xz"
node_root="/opt/clearra/node/${NODE_VERSION}"
if [[ ! -x "$node_root/bin/node" ]]; then
    command -v curl >/dev/null 2>&1 || { printf 'curl is required for pinned Node bootstrap\n' >&2; exit 2; }
    curl --fail --location --proto '=https' --tlsv1.2 \
        "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
        --output "$node_archive"
    printf '%s  %s\n' "$NODE_SHA256" "$node_archive" | sha256sum --check --status
    rm -rf -- "$node_root"
    mkdir -p "$node_root"
    tar -xJf "$node_archive" --strip-components=1 -C "$node_root"
    rm -f -- "$node_archive"
fi
chown -R clearra:clearra /opt/clearra

export PATH="$node_root/bin:/opt/clearra/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
export CARGO_HOME=/opt/clearra/cargo
export RUSTUP_HOME=/opt/clearra/rustup
export COREPACK_HOME=/opt/clearra/corepack

runuser -u clearra -- env PATH="$PATH" CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" \
    COREPACK_HOME="$COREPACK_HOME" \
    /opt/clearra/cargo/bin/rustup toolchain install "$RUST_VERSION" \
    --profile minimal --component rustfmt --component clippy --target wasm32-unknown-unknown
runuser -u clearra -- env PATH="$PATH" COREPACK_HOME="$COREPACK_HOME" \
    "$node_root/bin/corepack" enable --install-directory "$node_root/bin" pnpm
runuser -u clearra -- env PATH="$PATH" COREPACK_HOME="$COREPACK_HOME" \
    "$node_root/bin/corepack" prepare "pnpm@${PNPM_VERSION}" --activate
runuser -u clearra -- env PATH="$PATH" CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" \
    COREPACK_HOME="$COREPACK_HOME" CARGO_TARGET_DIR=/home/clearra/.cache/Clearra/tool-install \
    cargo "+${RUST_VERSION}" install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" \
    --locked --root "/opt/clearra/tools/wasm-bindgen-cli/${WASM_BINDGEN_VERSION}"
rm -rf -- /home/clearra/.cache/Clearra/tool-install

node_actual="$(node --version)"
pnpm_actual="$(pnpm --version)"
npm_actual="$(npm --version)"
rust_actual="$(rustc "+${RUST_VERSION}" --version)"
cargo_actual="$(cargo "+${RUST_VERSION}" --version)"
wasm_actual="$(/opt/clearra/tools/wasm-bindgen-cli/${WASM_BINDGEN_VERSION}/bin/wasm-bindgen --version)"
[[ "$node_actual" == "v${NODE_VERSION}" ]]
[[ "$pnpm_actual" == "$PNPM_VERSION" ]]
[[ "$npm_actual" == "$NPM_VERSION" ]]
[[ "$rust_actual" == "rustc ${RUST_VERSION}"* ]]
[[ "$cargo_actual" == "cargo ${CARGO_VERSION}"* ]]
[[ "$wasm_actual" == "wasm-bindgen ${WASM_BINDGEN_VERSION}" ]]

install -d -m 0755 /etc/clearra
python3 - "$TRANSACTION" "$SOURCE_DISTRO" "$TOOLCHAIN_DIGEST" \
    "$NODE_VERSION" "$NPM_VERSION" "$PNPM_VERSION" "$RUST_VERSION" \
    "$CARGO_VERSION" "$WASM_BINDGEN_VERSION" <<'PY'
import datetime
import json
import pathlib
import sys

transaction, source, digest, node, npm, pnpm, rust, cargo, wasm = sys.argv[1:]
marker = {
    "schema_id": "clearra.wsl-runtime.v1",
    "source_distribution": source,
    "created_transaction": transaction,
    "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "toolchain_digest": digest,
    "toolchains": {
        "node": node,
        "npm": npm,
        "pnpm": pnpm,
        "rust": rust,
        "cargo": cargo,
        "wasm_bindgen": wasm,
    },
    "runtime_user": "clearra",
}
pathlib.Path("/etc/clearra/runtime.json").write_text(
    json.dumps(marker, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
)
PY
chmod 0644 /etc/clearra/runtime.json
printf 'clearra_wsl_provision=complete transaction=%s\n' "$TRANSACTION"
