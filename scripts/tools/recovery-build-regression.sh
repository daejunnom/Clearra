#!/usr/bin/env bash
set -euo pipefail
# Focused finite CI run, not ReleaseAcceptance or a deployment gate.
printf 'tested_source=%s\n' "$(git rev-parse HEAD)"
cargo test --locked -p clearra-forward-search recovery_build -- --nocapture
cargo test --locked -p clearra-cli-command --test recovery_build -- --nocapture
cargo test --locked -p clearra-wasm --test recovery_build_wire -- --nocapture
cargo check --locked -p clearra-wasm --target wasm32-unknown-unknown
node packages/clearra-ui/scripts/prepare-test-dependencies.mjs
node --test packages/clearra-ui/test/recoveryBuild*.test.mjs packages/clearra-ui/test/boundaryRecoverySurface.test.mjs
pnpm --filter @clearra/ui exec tsc --noEmit -p tsconfig.contract.json
node scripts/tools/recovery-build-browser-regression.mjs
