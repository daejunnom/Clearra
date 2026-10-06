import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8')
  .replace(/\r\n/gu, '\n');
const workflow = read('.github/workflows/pc24-source-boundary.yml');

function assertSafePlainRunScalars(source) {
  for (const match of source.matchAll(/^\s+run:\s+([^\n]+)$/gmu)) {
    const value = match[1].trim();
    if (/^[|>"']/u.test(value)) continue;
    assert.doesNotMatch(value, /:\s/u,
      'run commands containing colon-space must use a quoted or block YAML scalar');
  }
}

test('workflow run commands do not introduce unquoted YAML mapping delimiters', () => {
  assertSafePlainRunScalars(workflow);
  assert.ok(workflow.includes('run: |\n          cargo test --locked -p clearra-core-domain --lib solution:: -- --test-threads=1'));
});

test('the scalar guard rejects the actual pre-job CTK2 workflow regression', () => {
  const invalidWorkflow = workflow.replace(
    'run: |\n          cargo test --locked -p clearra-core-domain --lib solution:: -- --test-threads=1',
    'run: cargo test --locked -p clearra-core-domain --lib solution:: -- --test-threads=1',
  );
  assert.notEqual(invalidWorkflow, workflow);
  assert.throws(() => assertSafePlainRunScalars(invalidWorkflow), /colon-space/u);
  assertSafePlainRunScalars('        run: "printf \'value: other\'"\n');
});

test('extended functional proofs run independently of existing release and product jobs', () => {
  const branches = ['codex/converge-pc24-integration-20261005', 'codex/converge-main-product-fixes-20261005', 'codex/pc24-target-boundary-20261006', 'codex/converge-pc24-family-20261006', 'codex/converge-v081-document-frame-20261006'];
  assert.ok(workflow.includes(`branches: [${branches.map((branch) => JSON.stringify(branch)).join(', ')}]`));
  for (const job of ['input-contract', 'inverse-lock-clear', 'document-wire']) {
    const section = workflow.slice(workflow.indexOf(`\n  ${job}:`));
    assert.ok(section.includes("if: github.ref == 'refs/heads/codex/converge-pc24-integration-20261005'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-main-product-fixes-20261005'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/pc24-target-boundary-20261006'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-pc24-family-20261006'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-v081-document-frame-20261006'"));
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

test('extended document publication preserves full words and the explicit format limit', () => {
  assert.ok(workflow.includes('cargo test --locked -p clearra-fumen --lib adapter::'));
  assert.ok(workflow.includes('full_height_document_keeps_ctk3_and_reports_fumens_real_limit'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-host-contract --lib solution_set_artifact_payload'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-app --lib extended_document_error_tests'));
  assert.ok(workflow.includes('unsupported_fumen_height_preserves_its_explicit_cli_error'));
  assert.ok(workflow.includes('crates/clearra-fumen/**'));
  const projection = read('crates/clearra-output/src/artifact/solution_document.rs');
  assert.ok(projection.includes('ColoredSolutionFumenExporter::encode_extended(&pages)'));
  assert.ok(projection.includes('identity.initial_board()'));
  assert.ok(projection.includes('FumenHeightUnsupported { height }'));
  assert.ok(!projection.includes('initial_board().words()[0]'));
  assert.ok(read('packages/clearra-ui/test/extendedSolutionKey.contract.ts').includes('cell < 230'));
  assert.ok(workflow.includes('packages/clearra-ui/test/productResultPager.contract.ts'));
  assert.ok(read('packages/clearra-ui/src/lib/workspace/productResultPager.ts')
    .includes("format.format === 'fumen' && format.unavailable_reason === 'fumen-height-unsupported'"));
  assert.ok(read('crates/clearra-host-contract/src/solution_set_artifact_payload.rs')
    .includes('self.unavailable_reason.as_deref() == Some("fumen-height-unsupported")'));
  assert.ok(read('crates/clearra-app/src/app_response/solution_set_artifact.rs')
    .includes('fn full_height_native_payload_preserves_ctk3_when_fumen_is_unavailable()'));
  assert.ok(read('packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts')
    .includes("| 'fumen-height-unsupported'"));
});

test('shared four-word codec and projections are tested without enabling compact PC authority', () => {
  assert.ok(workflow.includes('cargo test --locked -p clearra-core-domain --lib solution::'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-output --no-default-features --lib artifact::solution_document::tests'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-cli-command --test pinned_pc'));
  assert.ok(workflow.includes('packages/clearra-ui/test/extendedSolutionKey.contract.ts'));
  assert.ok(workflow.includes('apps/clearra-discord-bot/test/extended-solution-key.test.mjs'));
  assert.ok(workflow.includes('tests/fixtures/contracts/extended_solution_keys.v1.tsv'));
  const execution = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  assert.ok(execution.includes('ExtendedTilingSolutionKey::parse_canonical(key)'));
  assert.ok(!execution.includes('fn parse_extended_board_hex('));
  for (const path of ['crates/clearra-app/src/pc_minimum_cover_result.rs', 'crates/clearra-cli-command/src/web_command_request.rs']) {
    assert.ok(read(path).includes('.and_then(|key| key.standard_board64_identity())'));
  }
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
  assert.ok(compiler.includes('board.occupied_words()'));
  assert.ok(compiler.includes('.to_standard_target_frame(state_layout.target_lines())'));
  assert.ok(compiler.includes('pub fn compile_standard_query('));
  assert.ok(compiler.includes('.to_standard_target_frame(target_lines)'));
  assert.ok(compiler.includes('query: query.map_initial_board(|_| board)'));
  assert.ok(compiler.includes('let target_frame = self.target_frame.clone()'));
  assert.ok(compiler.includes('BuildProbabilityField::from_words_preserving_height('));
  assert.ok(compiler.includes('.map_initial_board(|_| normalized)'));
  const proof = read('crates/clearra-core-executor/tests/extended_pc_ilc.rs');
  assert.ok(proof.includes('let height = 24_u8;'));
  assert.ok(proof.includes('field.target_piece_count(), 1'));
  assert.ok(!proof.includes('top_down_t_field'));
  assert.ok(proof.includes('assert!(pieces <= 6)'));
  assert.ok(proof.includes('for height in [7_u8, 8, 12, 24]'));
  assert.ok(proof.includes('for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()]'));
  assert.ok(proof.includes('ExtendedPcSearchContract::compile_standard_query(query, height)'));
  assert.ok(proof.includes('result.bool_field("build_path_multiplicity_counted"),'));
  assert.ok(proof.includes('WasmBuildProbabilityBackend::execute_with_control('));
  assert.ok(proof.includes('result.normalized_solution_keys()'));
  assert.ok(proof.includes('ctk2|height={height}|initial='));
  assert.ok(proof.includes('result.path_steps()[0].cleared_lines()'));
});

test('extended input proof preserves source policies without bypassing compact or public authority', () => {
  const cases = read('crates/clearra-problem/tests/extended_pc_execution.rs');
  for (const name of [
    'shared_input_bridge_leaves_every_compact_target_on_the_legacy_contract',
    'shared_input_bridge_uses_the_explicit_target_not_the_initial_field_height',
    'shared_input_bridge_preserves_initial_clear_and_all_nonboard_policies',
    'shared_input_bridge_cannot_hide_occupancy_above_a_smaller_target',
    'shared_input_bridge_allows_a_tall_initial_field_only_after_real_line_clear',
  ]) assert.ok(cases.includes(`fn ${name}()`));
  assert.ok(cases.includes('execution.target_frame().initial_cleared_rows(), 2'));
  assert.ok(cases.includes('assert!(!contract.runtime_capability().connected_exact())'));
  assert.ok(cases.includes('.with_count_policy(PcCountPolicy::CountAll)'));
  assert.ok(cases.includes('WorkerPolicy::Fixed(11)'));
});
