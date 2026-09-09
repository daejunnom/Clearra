import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/release-build-architecture-ab.yml', import.meta.url), 'utf8');
const nativeIdentity = readFileSync(new URL('./prepare-native-build-identity.ps1', import.meta.url), 'utf8');

test('A/B workflow is isolated, non-publishing, and never grants release authority', () => {
  assert.match(workflow, /branches: \["codex\/release-build-architecture-ab"\]/u);
  assert.match(workflow, /persist-credentials: false/u);
  assert.match(workflow, /authority: 'non-authoritative-product-input'|non-authoritative ProductE2E CLI/u);
  for (const forbidden of [
    'release-acceptance-rust-shard-',
    'canonical-acceptance-evidence-',
    'gh release',
    'deploy-pages',
    'google-github-actions/auth',
    'id-token: write',
    'contents: write',
  ]) {
    assert.equal(workflow.includes(forbidden), false, `forbidden authority marker: ${forbidden}`);
  }
});

test('compiler-cache candidate is content-addressed and never restores a raw Cargo target', () => {
  assert.match(workflow, /run: \|\n\s+"RUSTC_WRAPPER=sccache" >> \$env:GITHUB_ENV/u);
  assert.match(workflow, /SCCACHE_GHA_ENABLED: "on"/u);
  assert.match(nativeIdentity, /SCCACHE_GHA_VERSION=\$compilerCacheNamespace/u);
  assert.match(workflow, /mozilla-actions\/sccache-action@fc920bf0ec8de6ee65d409111f7ec508035751ba/u);
  assert.match(workflow, /version: v0\.16\.0/u);
  assert.equal(/actions\/cache\/(?:restore|save)@/u.test(workflow), false);
  assert.equal(workflow.includes('~\\AppData\\Local\\Clearra\\build'), false);
});

test('exact CLI consumer is bound to producer source, run, attempt, recipe, and bytes', () => {
  for (const marker of [
    '--source-commit $env:GITHUB_SHA',
    '--run-id $env:GITHUB_RUN_ID',
    '--run-attempt $env:GITHUB_RUN_ATTEMPT',
    '--expected-source-commit $env:GITHUB_SHA',
    '--expected-run-id $env:GITHUB_RUN_ID',
    '--expected-run-attempt $env:GITHUB_RUN_ATTEMPT',
    '-UseBuiltBinary',
    'rebuild=false',
  ]) assert.ok(workflow.includes(marker), `missing exact producer/consumer marker: ${marker}`);
  assert.equal(workflow.includes('continue-on-error'), false);
  assert.match(workflow, /run-id: \$\{\{ needs\.ab-source\.outputs\.producer_run_id \}\}/u);
  assert.match(workflow, /producer_reused=true/u);
});

test('warm commit repeats only the compiler-cache candidate', () => {
  assert.match(workflow, /if: needs\.ab-source\.outputs\.phase == 'seed'[\s\S]*?exact-cli-producer/u);
  assert.match(workflow, /phase=\$\{\{ needs\.ab-source\.outputs\.phase \}\}/u);
  assert.match(workflow, /\[\[ "\$phase" == 'seed' \|\| "\$phase" == 'cache-seed' \|\| "\$phase" == 'warm' \]\]/u);
});
