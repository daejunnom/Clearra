// Source contracts for the non-publishing v0.8.1 feedback workflow. These
// checks neither execute Cargo nor create a synthetic qualification receipt.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/v081-selective-source-ci.yml', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');
const fixture = readFileSync(new URL('../../crates/clearra-cli/src/accelerator_asset_store_repair_tests.rs', import.meta.url), 'utf8').replace(/\r\n/gu, '\n');

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
