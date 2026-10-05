import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8')
  .replace(/\r\n/gu, '\n');
const workflow = read('.github/workflows/pc24-source-boundary.yml');

test('extended functional proofs run independently of existing release and product jobs', () => {
  assert.match(workflow, /branches: \["codex\/converge-pc24-integration-20261005"\]/u);
  for (const job of ['input-contract', 'inverse-lock-clear']) {
    const section = workflow.slice(workflow.indexOf(`\n  ${job}:`));
    assert.ok(section.includes("if: github.ref == 'refs/heads/codex/converge-pc24-integration-20261005'"));
  }
  assert.ok(!workflow.includes('needs:'));
  assert.ok(!workflow.includes('continue-on-error:'));
  assert.ok(!workflow.includes('contents: write'));
  assert.ok(!workflow.includes('gh workflow run'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-problem --test extended_pc_execution'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-core-executor --test extended_pc_ilc'));
  assert.ok(workflow.includes('--target wasm32-unknown-unknown'));
});

test('the compiler bridge owns the original four-word field and invokes existing ILC', () => {
  const compiler = read('crates/clearra-problem/src/extended_pc_search_contract.rs');
  assert.ok(compiler.includes('self.board().occupied_words()'));
  assert.ok(compiler.includes('.to_standard_target_frame(self.board().visible_height())'));
  assert.ok(compiler.includes('BuildProbabilityField::from_words_preserving_height('));
  assert.ok(compiler.includes('.map_initial_board(|_| normalized)'));
  const proof = read('crates/clearra-core-executor/tests/extended_pc_ilc.rs');
  assert.ok(proof.includes('for height in [12_u8, 24]'));
  assert.ok(proof.includes('WasmBuildProbabilityBackend::execute_with_control('));
  assert.ok(proof.includes('result.normalized_solution_keys()'));
  assert.ok(proof.includes('ctk2|height={height}|initial='));
  assert.ok(proof.includes('step.cleared_lines()'));
});
