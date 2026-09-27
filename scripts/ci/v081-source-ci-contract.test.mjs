// Source contracts for the non-publishing v0.8.1 feedback workflow. These
// checks neither execute Cargo nor create a synthetic qualification receipt.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/v081-selective-source-ci.yml', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const fixture = readFileSync(new URL('../../crates/clearra-cli/src/accelerator_asset_store_repair_tests.rs', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const signedMetadata = [
  'config/accelerator-activation-keyring.v1.json',
  'config/legal-board-product-catalog.v1.json',
  'config/conditioned-reachability-product-catalog.v1.json',
];

test('embedded signed metadata retains canonical LF bytes without parser normalization', () => {
  for (const path of signedMetadata) {
    const bytes = readFileSync(new URL(`../../${path}`, import.meta.url));
    assert.equal(bytes.includes(13), false, `${path}: CR changes catalog authority`);
    assert.equal(bytes.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf])), false,
      `${path}: BOM changes catalog authority`);
    assert.equal(bytes.at(-1), 10, `${path}: canonical final LF is required`);
    assert.notEqual(bytes.at(-2), 10, `${path}: extra final LF changes catalog authority`);
    assert.doesNotThrow(() => JSON.parse(bytes.toString('utf8')));
  }
});

test('Windows autocrlf checkout cannot alter embedded signed metadata', () => {
  const result = spawnSync('git', [
    '-c', 'core.autocrlf=true', '-c', 'core.eol=crlf',
    'check-attr', '-z', 'text', 'eol', '--', ...signedMetadata,
  ], { cwd: fileURLToPath(new URL('../..', import.meta.url)),
    encoding: 'utf8', timeout: 10_000, windowsHide: true });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  const attributes = result.stdout.split('\0');
  assert.equal(attributes.pop(), '');
  const expected = signedMetadata.flatMap(path => [path, 'text', 'set', path, 'eol', 'lf']);
  assert.deepEqual(attributes, expected);
});

test('CI uses the existing narrow Rust fixture root and checks it before compilation', () => {
  const requiredLeaf = fixture.match(/assert_eq!\(normalized\.file_name\(\)\.unwrap\(\),\s*"([^"]+)"\)/u)?.[1];
  assert.equal(requiredLeaf, 'accelerator-store-tests');
  const expected = `CLEARRA_FOCUSED_TEST_OUTPUT_ROOT: \${{ github.workspace }}/_local/artifacts/${requiredLeaf}`;
  assert.ok(workflow.includes(expected));
  assert.ok(workflow.includes(`test "$CLEARRA_FOCUSED_TEST_OUTPUT_ROOT" = "$GITHUB_WORKSPACE/_local/artifacts/${requiredLeaf}"`));
  assert.ok(workflow.indexOf('Prepare the exact bounded asset-test root') < workflow.indexOf('Typecheck integrated native products'));
});

test('an asset-test failure remains failure without suppressing independent proof feedback', () => {
  assert.ok(!workflow.includes('continue-on-error:'));
  assert.match(workflow, /if cargo test --locked -p clearra-cli --lib "\$filter"[\s\S]*failed=\$\(\(failed \+ 1\)\)/u);
  assert.ok(workflow.includes('test "$failed" -eq 0'));
  const proofStep = workflow.slice(workflow.indexOf('- name: Test bounded generation and parser proofs'));
  assert.ok(proofStep.startsWith("- name: Test bounded generation and parser proofs, not full profile regeneration\n        if: ${{ !cancelled() && steps.native-check.outcome == 'success' }}\n"));
});

test('CI exercises the authoritative CLI ingress and real Desktop/Discord accelerator paths', () => {
  assert.ok(workflow.includes('cargo test --locked -p clearra-cli-command --lib exact_accelerator --'));
  assert.ok(workflow.includes('exact_accelerator_flags \\\n'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-gui-host --test exact_accelerator_surface_parity --no-default-features --'));
  assert.ok(workflow.includes('apps/clearra-discord-bot/test/exact-accelerator-command.test.mjs'));
});
