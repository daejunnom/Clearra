import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8')
  .replace(/\r\n/gu, '\n');
const workflow = read('.github/workflows/pc24-source-boundary.yml');

test('extended functional proofs run independently of existing release and product jobs', () => {
  const branches = ['codex/converge-pc24-integration-20261005', 'codex/converge-main-product-fixes-20261005'];
  assert.ok(workflow.includes(`branches: [${branches.map((branch) => JSON.stringify(branch)).join(', ')}]`));
  for (const job of ['input-contract', 'inverse-lock-clear']) {
    const section = workflow.slice(workflow.indexOf(`\n  ${job}:`));
    assert.ok(section.includes("if: github.ref == 'refs/heads/codex/converge-pc24-integration-20261005'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-main-product-fixes-20261005'"));
  }
  assert.ok(!workflow.includes('needs:'));
  assert.ok(!workflow.includes('continue-on-error:'));
  assert.ok(!workflow.includes('contents: write'));
  assert.ok(!workflow.includes('gh workflow run'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-problem --test extended_pc_execution'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-geometry --lib layout::standard_pc_layout::tests'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-core-executor --test extended_pc_ilc'));
  assert.ok(workflow.includes('--target wasm32-unknown-unknown'));
});

test('compact and general routing stay separate without claiming disconnected public PC support', () => {
  const normalization = read('crates/clearra-pc-graph/src/request/pc_scenario_query.rs');
  assert.ok(normalization.includes('target_lines > 6 || self.visible_height > 6 || self.has_extended_occupancy()'));
  assert.ok(normalization.includes('return self.to_extended_target_frame(target_lines)'));
  const probability = read('crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs');
  const start = probability.indexOf('fn build_probability_session_for_field(');
  const route = probability.slice(start, probability.indexOf('pub(super) fn merge_symmetry_results(', start));
  assert.ok(route.includes('if field.is_compact()'));
  assert.ok(route.includes('BuildProbabilitySessionKind::Compact(session)'));
  assert.ok(route.includes('BuildProbabilitySessionKind::Extended(session)'));
  const stage = read('crates/clearra-core-executor/src/backend/wasm_cpu/build_stage_domain.rs');
  assert.ok(stage.includes('if field.height() <= 6'));
  assert.ok(stage.includes('ExtendedInverseCatalog::compile(field)'));
  const layout = read('crates/clearra-geometry/src/layout/standard_pc_layout.rs');
  assert.ok(layout.includes('one_to_six_lines_keep_the_compact_fast_path_and_larger_layouts_keep_every_cell'));
  assert.ok(layout.includes('for lines in 1..=24'));
  assert.ok(layout.includes('StandardPcRuntimeUnsupportedReason::ExtendedSearchStagesNotConnected'));
});

test('the compiler bridge owns the original four-word field and invokes existing ILC', () => {
  const compiler = read('crates/clearra-problem/src/extended_pc_search_contract.rs');
  assert.ok(compiler.includes('self.board().occupied_words()'));
  assert.ok(compiler.includes('.to_standard_target_frame(self.board().visible_height())'));
  assert.ok(compiler.includes('BuildProbabilityField::from_words_preserving_height('));
  assert.ok(compiler.includes('.map_initial_board(|_| normalized)'));
  const proof = read('crates/clearra-core-executor/tests/extended_pc_ilc.rs');
  assert.ok(proof.includes('let height = 24_u8;'));
  assert.ok(proof.includes('field.target_piece_count(), 1'));
  assert.ok(!proof.includes('top_down_t_field'));
  assert.ok(!proof.includes('vec![PieceKind::I; pieces]'));
  assert.ok(proof.includes('WasmBuildProbabilityBackend::execute_with_control('));
  assert.ok(proof.includes('result.normalized_solution_keys()'));
  assert.ok(proof.includes('ctk2|height={height}|initial='));
  assert.ok(proof.includes('result.path_steps()[0].cleared_lines()'));
});
