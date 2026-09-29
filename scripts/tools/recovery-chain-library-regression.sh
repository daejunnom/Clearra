#!/usr/bin/env bash
# Focused library/transport checks, not release acceptance or a performance run.
# Multi-middle product execution must stay disabled until catalog/host wiring
# has its own exact end-to-end coverage. Do not use these checks to bypass it.
set -euo pipefail
cd "$(dirname "$0")/../.."

out=_local/artifacts/test/recovery-chain-library
mkdir -p "$out"
git rev-parse HEAD > "$out/source-sha.txt"
git diff --binary > "$out/uncommitted-source.patch"

# Rust and Node are deliberately NOT replaced with a convenient local version.
# The repository's rust-toolchain.toml owns Rust, and .node-version owns Node.
rustup show active-toolchain | tee "$out/toolchain.txt"
node -e '
  const fs = require("node:fs");
  const expected = fs.readFileSync(".node-version", "utf8").trim().replace(/^v/, "");
  if (process.versions.node !== expected) {
    throw new Error(`Expected Node ${expected}; got ${process.versions.node}`);
  }
'

cargo fmt --all --check 2>&1 | tee "$out/format.log"
cargo test --locked -p clearra-forward-search recovery_build -- --test-threads=1 \
  2>&1 | tee "$out/core.log"
cargo test --locked -p clearra-cli-command --test recovery_build -- --test-threads=1 \
  2>&1 | tee "$out/cli.log"
cargo test --locked -p clearra-wasm --test recovery_build_wire -- --test-threads=1 \
  2>&1 | tee "$out/wasm-contract.log"
cargo check --locked -p clearra-wasm --target wasm32-unknown-unknown \
  2>&1 | tee "$out/wasm-target.log"

node packages/clearra-ui/scripts/prepare-test-dependencies.mjs \
  2>&1 | tee "$out/ui-dependencies.log"
node --test packages/clearra-ui/test/recoveryBuild.test.mjs \
  packages/clearra-ui/test/recoveryBuildReferences.test.mjs \
  packages/clearra-ui/test/recoveryResultFrame.test.mjs \
  packages/clearra-ui/test/recoveryStages.test.mjs \
  packages/clearra-ui/test/recoveryChainEvidence.test.mjs \
  2>&1 | tee "$out/ui.log"
node packages/clearra-ui/node_modules/typescript/bin/tsc \
  --noEmit -p packages/clearra-ui/tsconfig.contract.json \
  2>&1 | tee "$out/types.log"
printf '%s\n' 'Focused library checks completed; this is not product/release approval.'
