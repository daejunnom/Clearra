import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8')
  .replace(/\r\n/gu, '\n');
const workflow = read('.github/workflows/pc24-source-boundary.yml');

function jobSection(name) {
  const start = workflow.indexOf(`\n  ${name}:\n`);
  assert.notEqual(start, -1, `missing independent job ${name}`);
  const tail = workflow.slice(start + 1);
  const next = tail.search(/\n  [a-z][a-z0-9-]*:\n/u);
  return next < 0 ? tail : tail.slice(0, next);
}

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
  const branches = ['codex/converge-pc24-integration-20261005', 'codex/converge-main-product-fixes-20261005', 'codex/pc24-target-boundary-20261006', 'codex/converge-pc24-family-20261006', 'codex/converge-v081-document-frame-20261006', 'codex/converge-v081-extended-pc-products-20261007'];
  assert.ok(workflow.includes(`branches: [${branches.map((branch) => JSON.stringify(branch)).join(', ')}]`));
  for (const job of ['input-contract', 'inverse-lock-clear', 'native-family-parallel', 'document-wire']) {
    const section = jobSection(job);
    assert.ok(section.includes("if: github.ref == 'refs/heads/codex/converge-pc24-integration-20261005'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-main-product-fixes-20261005'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/pc24-target-boundary-20261006'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-pc24-family-20261006'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-v081-document-frame-20261006'"));
    assert.ok(section.includes("github.ref == 'refs/heads/codex/converge-v081-extended-pc-products-20261007'"));
  }
  assert.ok(!workflow.includes('needs:'));
  assert.ok(!workflow.includes('continue-on-error:'));
  assert.ok(!workflow.includes('contents: write'));
  assert.ok(!workflow.includes('gh workflow run'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-problem --test extended_pc_execution'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-geometry --lib layout::standard_pc_layout::tests'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-core-executor --no-default-features --features parallel --test extended_pc_ilc'));
  assert.ok(workflow.includes('--target wasm32-unknown-unknown'));
});

test('native feature compilation cannot exhaust the ordinary surface owner or drop its later proofs', () => {
  const ordinary = jobSection('inverse-lock-clear');
  const native = jobSection('native-family-parallel');
  assert.match(ordinary, /timeout-minutes: 20/u);
  assert.match(native, /timeout-minutes: 15/u);
  assert.doesNotMatch(ordinary, /--features parallel/u);
  assert.doesNotMatch(native, /needs:|download-artifact|upload-artifact|wasm32/u);
  assert.ok(native.includes('--features parallel --test extended_pc_family'));
  assert.ok(native.includes('--features parallel --test extended_pc_tiling'));
  for (const selector of [
    '--lib extended_geometry::parallel::tests',
    '--lib pc_family_evidence_admits_snapshot_row_slots_and_owned_bitsets_before_construction',
    '--lib build_pc_resource_projection_field_inventory_is_exhaustive',
    '--lib tiling_solution_store::tests',
    '--lib bounded_catalog_keeps_exact_identity_and_refuses_before_growth',
    '--test extended_pc_ilc',
    '--lib shared_extended_identity_resolves_only_matching_full_height_catalog_rows',
  ]) {
    const command = `cargo test --locked -p clearra-core-executor --no-default-features --features parallel ${selector} -- --test-threads=1`;
    assert.ok(native.includes(command), `native proof was removed or switched build owner: ${selector}`);
    assert.equal(workflow.split(command).length - 1, 1, `duplicate native proof ${selector}`);
  }
  assert.ok(native.includes('cargo clippy --locked -p clearra-replay -p clearra-postprocess -p clearra-core-executor -p clearra-app --no-default-features --features parallel --lib -- -D warnings'));
  assert.ok(ordinary.includes('--test extended_pc_surfaces'));
  assert.ok(ordinary.includes('--lib full_height_score_key_projection'));
  assert.ok(ordinary.includes('--lib pc_pinned_solution_document'));
  assert.ok(ordinary.includes('cargo test --locked -p clearra-accelerator-product-host --lib'));
  assert.ok(ordinary.includes('cargo clippy --locked -p clearra-accelerator-product-host --all-targets -- -D warnings'));
  assert.ok(ordinary.includes('cargo clippy --locked -p clearra-replay -p clearra-postprocess --all-targets -- -D warnings'));
});

test('extended Tiling has its own typed exact producer without relabelling Build coverage', () => {
  const source = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_tiling.rs');
  assert.ok(source.includes('ExtendedGeometrySearch::new(universe, &family, &catalog)'));
  assert.ok(source.includes('ExtendedInverseCatalog::compile_bounded'));
  assert.ok(source.includes('admit_budget_bound_search_execution_under_terminal_authority'));
  assert.ok(source.includes('pc_tiling_family_publication_contract_is_valid'));
  assert.ok(!source.includes('WasmBuildProbabilitySession'));
  assert.ok(workflow.includes('--test extended_pc_tiling'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-cli --no-default-features --features wasm-cpu-runtime --test extended_pc_tiling'));
  assert.ok(/let frame = scenario\s*\.initial_board\(\)\s*\.to_standard_target_frame/u
    .test(read('crates/clearra-cli-command/src/web_command_request.rs')),
    'target-frame validation must consume the complete typed initial field');
  assert.ok(read('crates/clearra-cli-command/src/web_pc_scenario_input.rs').includes('self.initial_board.clone()'));
  assert.ok(read('crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs')
    .includes('extended_pc_tiling_requires_explicit_single_worker'));
  assert.ok(workflow.includes('cargo check --locked -p clearra-wasm-abi --target wasm32-unknown-unknown'));
  assert.ok(workflow.includes('node --test packages/clearra-ui/test/workspaceCommandSerialization.test.mjs'));
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

test('ordinary extended PC evaluates the existing BuildUp language and owns a separate result boundary', () => {
  const backend = read('crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs');
  const session = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs');
  const engine = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  const workflow = read('.github/workflows/pc24-source-boundary.yml');
  assert.ok(backend.includes('ExtendedPcSearchSession::new(problem)'));
  assert.ok(session.includes('new_pc_family('));
  assert.ok(session.includes('extended_pc_family_parallel_not_connected'));
  assert.ok(session.includes('PcChanceEvidencePolicy::Disabled'));
  assert.ok(engine.includes('ExtendedFamilyPurpose::Pc'));
  assert.ok(engine.includes('self.build_pc_family_result()'));
  assert.ok(engine.includes('let count_pc_paths = self.count_pc_build_paths()'));
  assert.ok(engine.includes('product.path_count'));
  assert.ok(engine.includes('field("buildup_executed", true)'));
  assert.ok(engine.includes('field("solution_page_available", false)'));
  assert.ok(workflow.includes('--test extended_pc_family'));
  const tests = read('crates/clearra-core-executor/tests/extended_pc_family.rs');
  assert.ok(tests.includes('full_height_pc_family_runs_buildup_and_clears_the_whole_board_in_all_profiles'));
  assert.ok(tests.includes('step.cleared_lines()'));
  assert.ok(tests.includes('availability.contract_valid()'));
  assert.ok(tests.includes('availability.materialized_key_count_matches(1)'));
  assert.ok(tests.includes('full_height_pc_never_silently_lowers_the_worker_request_or_invents_product_authority'));
  const cli = read('crates/clearra-cli/tests/extended_pc_tiling.rs');
  assert.ok(cli.includes('extended_ordinary_pc_real_cli_verifies_buildup_without_truncating_the_field'));
  assert.ok(cli.includes('json["summary"]["buildup_executed"], "true"'));
});

test('Opening inputs retain the whole target rather than a six-line or twenty-line frame', () => {
  const preset = read('crates/clearra-problem/src/preset/opening_preset.rs');
  assert.ok(preset.includes('STANDARD_PC_MAX_LINES'));
  assert.ok(!preset.includes('matches!(query.target().lines(), 2 | 4 | 6)'));
  assert.ok(read('crates/clearra-problem/src/search_problem.rs')
    .includes('.max(scenario.initial_board().visible_height())'));
  assert.ok(read('crates/clearra-problem/tests/extended_pc_execution.rs')
    .includes('opening_compiler_preserves_even_targets_and_spawn_height_without_enumeration'));
  assert.ok(read('crates/clearra-core-executor/tests/extended_pc_family.rs')
    .includes('empty_extended_opening_reaches_finite_catalog_admission_instead_of_a_false_capability'));
  for (const path of [
    'crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs',
    'crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_tiling.rs',
  ]) assert.ok(read(path).includes('SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-validation --lib validators::pc_query_validator::tests'));
  assert.ok(workflow.includes('--lib ordinary_pc_family_memory_projection_counts_opening_and_scenario_owners'));
  const session = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs');
  assert.match(session, /if problem\.objective\(\)\.score\(\)\.requested\(\)[\s\S]*problem\.checked_pc_score_pointee_retained_bytes\(\)\?[\s\S]*problem\.checked_pc_family_pointee_retained_bytes\(\)\?/u);
  assert.match(read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs'),
    /checked_pc_family_problem_nested_retained_bytes\(\s*&self\.problem/u);
});

test('native CLI assembly does not reintroduce a six-line limit before App', () => {
  const assembler = read('crates/clearra-cli/src/assemble/pc_query_assembler.rs');
  assert.ok(assembler.includes('PcTarget::new(args.lines())'));
  assert.ok(!assembler.includes('!matches!(target.lines(), 2 | 4 | 6)'));
  assert.ok(read('crates/clearra-cli/src/assemble/pc_query_assembler_tests.rs')
    .includes('full_height_even_targets_preserve_the_native_cli_input_contract'));
  assert.ok(workflow.includes('--lib assemble::pc_query_assembler::tests'));
});

test('native extended Tiling shares exact family owners and joins jobs inside one admitted request', () => {
  const backend = read('crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs');
  const authority = read('crates/clearra-app/src/pc_tiling_family_result.rs');
  const plan = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_parallel.rs');
  const runner = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_tiling_parallel.rs');
  assert.ok(backend.includes('supports_extended_native_tiling_workers'));
  assert.match(authority, /try_acquire_full_capacity_with_compute_units\(\s*compute_units,?\s*\)/u);
  assert.ok(plan.includes('seeds.insert(index + 1, right)'));
  assert.ok(plan.includes('first_ordinal: ordinal'));
  assert.ok(plan.includes('checked_live_retained_bytes, future'));
  assert.ok(runner.includes('submitted += 1'));
  assert.ok(runner.includes('for _ in 0..submitted'));
  assert.ok(runner.includes('complete_parallel_enumeration(count)'));
  assert.ok(!runner.includes('build_extended_order_graph'));
  assert.ok(workflow.includes('--features parallel --lib extended_geometry::parallel::tests'));
  assert.ok(workflow.includes('--features parallel --test extended_pc_tiling'));
  assert.ok(read('crates/clearra-cli/tests/extended_pc_tiling.rs')
    .includes('extended_parallel_tiling_real_cli_keeps_complete_keys_and_full_height'));
  assert.ok(read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_parallel_tests.rs')
    .includes('extended_parallel_product_splits_preserve_sixty_row_prefix_and_continuations'));
});

test('ordinary native extended PC shares owners but retains its own exact BuildUp terminal', () => {
  const runner = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_family_parallel.rs');
  assert.ok(runner.includes('new_shared_pc_verifier'));
  assert.ok(runner.includes('Arc::ptr_eq(&self.problem, &worker.problem)'));
  assert.ok(runner.includes('Arc::ptr_eq(&self.catalog, &worker.catalog)'));
  assert.ok(runner.includes('for _ in 0..submitted'));
  assert.ok(runner.includes('segment.first_ordinal != ordinal'));
  assert.ok(runner.includes('self.merge_pc_verifier(worker.engine)'));
  assert.ok(runner.includes('self.complete()'));
  assert.ok(!runner.includes('CoreExecutionResult::new'));
  assert.ok(!runner.includes('terminal_authority'));
  assert.ok(workflow.includes('--features parallel --test extended_pc_family'));
});

test('the connected twenty-four-line surfaces retain whole fields and an exportable PC hash', () => {
  const model = read('packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts');
  const workspace = read('packages/clearra-ui/src/lib/workspace/SolverWorkspace.svelte');
  const editor = read('packages/clearra-ui/src/lib/workspace/WorkspaceBoardEditor.svelte');
  assert.ok(model.includes('WORKSPACE_PC_MAX_LINES = 24'));
  assert.ok(model.includes("runtime === 'web' && execution.workers !== 1"));
  assert.ok(model.includes("!['tiling', 'off', 'minimum-cover', 'failed-queue', 'summary', 'score-finder', 'score-minimals'].includes(execution.scoreMode)"));
  assert.ok(workspace.includes('dimensionMax={WORKSPACE_PC_MAX_LINES}'));
  assert.ok(editor.includes('decodeInterchangeField(source, 24)'));
  assert.ok(read('apps/clearra-discord-bot/src/discord/field-limits.mjs')
    .includes("['pc-tiling-v2', 'pc-v2', 'pc-chance-v2', 'pc-failed-v2', 'pc-score-v2', 'pc-score-finder-v2'].includes(input)"));
  const executor = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  const pcTerminal = executor.slice(executor.indexOf('fn build_pc_family_result('),
    executor.indexOf('pub(super) fn finesse_search_material('));
  assert.ok(pcTerminal.includes('hasher.update_extended_canonical_key(identity)'));
  assert.ok(!pcTerminal.includes('normalized_string_solution_set_hash'));
  assert.ok(workflow.includes('--test extended_pc_surfaces'));
  assert.ok(workflow.includes('tests/fixtures/contracts/extended_pc_surface_input.v1.tsv'));
});

test('full-height minimum uses source-bound coverage proof and the common canonical lazy reducer', () => {
  const producer = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  const evidence = read('crates/clearra-core-executor/src/pc_chance_coverage_evidence.rs');
  const reducer = read('crates/clearra-app/src/pc_minimum_cover_result.rs');
  const identities = read('crates/clearra-app/src/pc_minimum_cover_source_identities.rs');
  assert.ok(producer.includes('bind_extended_minimum_source_keys(&keys)'));
  assert.ok(producer.includes('"deferred-to-coordinator"'));
  assert.ok(evidence.includes('minimum_source_keys_sha256: Option<[u8; 32]>'));
  assert.ok(evidence.includes('keys.windows(2).any(|pair| pair[0] >= pair[1])'));
  assert.ok(reducer.includes('PcMinimumCoverSourceIdentities::validate('));
  assert.ok(identities.includes('producer.matches_extended_minimum_source_keys(keys)'));
  assert.ok(identities.includes('evidence.coverage_bits() != row.covered_patterns()'));
  assert.ok(identities.includes('ExtendedTilingSolutionKey::parse_canonical(key)'));
  assert.ok(identities.includes('Self::Extended { .. } => Some(0)'));
  const integration = read('crates/clearra-cli-command/tests/extended_pc_surfaces.rs');
  assert.ok(integration.includes('extended_minimum_publishes_first_canonical_set_and_keeps_equal_minima_lazy'));
  assert.ok(integration.includes('set.open_store()'));
});

test('full-height chance retains its independent four-word authority and admits evidence memory first', () => {
  const app = read('crates/clearra-app/src/pc_chance_probability_result.rs');
  const memory = read('crates/clearra-core-executor/src/pc_chance_coverage_evidence.rs');
  const ingress = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs');
  assert.ok(app.includes('compiled_board_occupied_words: [u64; 4]'));
  assert.ok(app.includes('compiled_board_occupied_mask(&self) -> Option<u64>'));
  assert.ok(ingress.includes('PcChanceEvidencePolicy::PcProbabilityV2'));
  assert.ok(ingress.includes('problem.output_policy() == SearchOutputPolicy::CoverageSummary'));
  assert.ok(memory.includes('checked_pc_family_creation_future_bytes'));
  assert.ok(memory.includes('size_of::<CoverageRow>()'));
  assert.ok(memory.includes('.div_ceil(64) as u128'));
  assert.ok(workflow.includes('--lib pc_family_evidence_admits_snapshot_row_slots_and_owned_bitsets_before_construction'));
});

test('full-height failed queue consumes only request-owned complete coverage and preserves cancellation', () => {
  const producer = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_failed_queue.rs');
  const authority = read('crates/clearra-app/src/pc_failed_queue_result.rs');
  const app = read('crates/clearra-app/src/app_services.rs');
  const cooperative = read('crates/clearra-app/src/cooperative_execution.rs');
  assert.ok(producer.includes('pub fn new(problem: Arc<SearchProblem>)'));
  assert.ok(producer.includes('pc_failed_queue_example_limit()'));
  assert.ok(producer.includes('source.complete()'));
  assert.ok(producer.includes('.matches_search_problem(self.authority.problem())'));
  assert.ok(producer.includes('PcFailedQueueEvidenceProducer::produce('));
  assert.ok(producer.includes('result.without_pc_chance_transient_evidence()'));
  assert.ok(producer.includes('WasmPcFailedQueueAdvance::Cancelled'));
  assert.ok(authority.includes('.with_pc_failed_queue_v2_evidence(failed_pattern_limit)'));
  assert.ok(app.includes('WasmPcFailedQueueSession::new('));
  assert.ok(cooperative.includes('CompletedPcFailedQueue('));
  assert.ok(cooperative.includes('postprocess_pc_failed_queue_completion('));
  const tests = read('crates/clearra-core-executor/tests/extended_pc_family.rs');
  for (const name of [
    'full_height_failed_queue_owns_the_executed_problem_and_never_borrows_chance_authority',
    'full_height_failed_queue_cancellation_never_returns_unsat_or_a_failure_list',
    'full_height_native_parallel_failed_queue_keeps_complete_source_coverage_and_worker_identity',
  ]) assert.ok(tests.includes(`fn ${name}()`));
});

test('full-height physical execution does not truncate masks or acquire replay-family authority', () => {
  const replay = read('crates/clearra-replay/src/full_height_replay.rs');
  const batch = read('crates/clearra-replay/src/full_height_execution.rs');
  const engine = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  assert.ok(workflow.includes('"crates/clearra-replay/**"'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-replay --lib -- --test-threads=1'));
  assert.ok(replay.includes('cleared_row_mask: u32'));
  assert.ok(replay.includes('ScoringExecutionEdge'));
  assert.ok(replay.includes('PieceDecision::from_selected_hold('));
  assert.ok(replay.includes('memory_guard(requested)'));
  assert.ok(replay.includes('memory_guard(actual)'));
  assert.ok(replay.includes('trk2:h{}:'));
  assert.ok(!replay.includes('words()[0]'));
  assert.ok(batch.includes('key.initial_board() != initial'));
  assert.ok(batch.includes('graph.checked_edges(node)'));
  assert.ok(batch.includes('execution: SpinCoverageExecutionBatch'));
  assert.ok(!batch.includes('CoreExecutionResult'));
  assert.ok(!batch.includes('layout::board64_layout'));
  const terminal = engine.slice(engine.indexOf('fn build_pc_family_result('));
  assert.ok(terminal.includes('self.validate_pc_representative_physical_chain()?'));
  assert.ok(engine.includes('extended_pc_replay_terminal_board_not_empty'));
  assert.ok(!terminal.includes('"pc-path-family.v2"'));
  const tests = read('crates/clearra-replay/src/full_height_replay_tests.rs');
  assert.ok(tests.includes('full_height_projector_is_differentially_equal_to_unchanged_compact_transition'));
  assert.ok(tests.includes('full_height_batch_moves_existing_graph_storage_and_binds_all_four_initial_words'));
});

test('full-height App score finalizers share identities without truncating or granting public batch authority', () => {
  const identity = read('crates/clearra-app/src/pc_score_solution_identity.rs');
  const postprocess = read('crates/clearra-app/src/pc_score_postprocess.rs');
  const authority = read('crates/clearra-app/src/pc_score_summary_result.rs');
  const portfolio = read('crates/clearra-app/src/pc_score_minimum_cover_result.rs');
  const wasm = read('crates/clearra-wasm/src/wasm_command_runtime.rs');
  const parser = read('crates/clearra-cli-command/src/web_command_parser.rs');
  const ingress = read('crates/clearra-cli-command/src/lib_tests.rs');
  const surfaces = read('crates/clearra-cli-command/tests/extended_pc_surfaces.rs');
  const cli = read('crates/clearra-cli/tests/extended_pc_tiling.rs');
  assert.ok(identity.includes('keys: Arc<Vec<String>>'));
  assert.ok(identity.includes('Arc::ptr_eq(previous, keys)'));
  assert.ok(identity.includes('keys.capacity()'));
  assert.ok(identity.includes('mixed_dictionary_owners_do_not_gain_unaccounted_memory_credit'));
  assert.ok(!identity.includes('Box<StandardBoard64TilingIdentity>'));
  assert.ok(postprocess.includes('FullHeightScoreCellMaterializer::materialize_with_profile_and_memory_limit('));
  assert.ok(postprocess.includes('cells = materialized.into_cells()'));
  assert.ok(postprocess.includes('shared_identity_bytes'));
  assert.ok(authority.includes('Arc::ptr_eq(&self.problem, executed_problem)'));
  assert.ok(authority.includes('problem_evidence.matches_search_problem(self.problem.as_ref())'));
  assert.ok(authority.includes('batch.initial() != Board256Mask::from_words(board.occupied_words())'));
  assert.ok(authority.includes('pc_score_full_height_solution_identity_mismatch'));
  assert.ok(portfolio.includes('PcScoreSolutionIdentity::checked_shared_retained_bytes('));
  assert.ok(wasm.includes('identity: &PcScoreSolutionIdentity'));
  assert.match(wasm, /if let Some\(key\) = identity\.extended_canonical_key\(\)\s*\{\s*return try_owned_string\(key, ledger\)/u);
  assert.ok(wasm.includes('try_pc_score_field_key(canonical_winner.solution_identity(), ledger)?'));
  assert.ok(!wasm.includes('let canonical_solution_key = canonical_winner.normalized_solution_key()'));
  assert.ok(wasm.includes('full_height_score_key_projection_preserves_a_long_actual_field_and_admits_one_copy'));
  assert.ok(parser.includes('pc_score_max_source_pieces_for_lines(target_lines)'));
  assert.ok(ingress.includes('pc_score_extended_source_bound_matches_target_before_and_after_translation'));
  assert.ok(surfaces.includes('canonical_full_height_scores_keep_full_fields_and_share_the_gui_finalizer'));
  assert.ok(surfaces.includes('["score", "score-minimals", "score-finder"]'));
  assert.ok(cli.includes('extended_score_real_cli_uses_full_keys_in_existing_score_payloads'));
  assert.ok(workflow.includes('--lib pc_score_solution_identity -- --test-threads=1'));
  assert.ok(workflow.includes('--lib pc_score_minimum_cover_contract_tests -- --test-threads=1'));
  assert.ok(workflow.includes('--lib pc_score_ -- --test-threads=1'));
  assert.ok(workflow.includes('--lib full_height_score_key_projection -- --test-threads=1'));
});

test('full-height score ingress admits connected source layouts without removing the real execution proofs', () => {
  const validator = read('crates/clearra-validation/src/validators/pc_scenario_query_validator.rs');
  const tests = read('crates/clearra-validation/src/validators/pc_query_validator_tests.rs');
  assert.ok(validator.includes('let score_source = query.objective().score().requested()'));
  assert.ok(validator.includes('let full_height_family = (ordinary_source || score_source)'));
  assert.ok(validator.includes('query.objective().execution_constraints().requested()'));
  assert.ok(validator.includes('.requires_observation_policy()'));
  assert.ok(tests.includes('full_height_score_scenario_accepts_all_four_words_without_granting_product_authority'));
  assert.ok(tests.includes('full_height_score_scenario_still_rejects_unconnected_objective_and_constraint_domains'));
  assert.ok(workflow.includes('--test extended_pc_surfaces -- --test-threads=1'));
  assert.ok(workflow.includes('--lib full_height_score_key_projection -- --test-threads=1'));
});

test('PC pinned drawings have their own full-height codec and cannot select an unproved or ambiguous source', () => {
  const decoder = read('crates/clearra-app/src/pc_pinned_solution_document.rs');
  const parser = read('crates/clearra-cli-command/src/web_command_parser.rs');
  const reducer = read('crates/clearra-app/src/pc_minimum_cover_result.rs');
  const contract = read('crates/clearra-app/src/product_capability_contract.rs');
  const surfaces = read('crates/clearra-cli-command/tests/extended_pc_surfaces.rs');
  assert.ok(parser.includes('clearra_app::PcPinnedSolutionDocument::decode(format, &document)'));
  assert.ok(decoder.includes('!(1..=24).contains(&page.height)'));
  assert.ok(decoder.includes('key.height() != height || key.initial_board() != initial'));
  assert.ok(decoder.includes('actual == pieces'));
  assert.ok(reducer.includes('drawing.matches_extended(identity)'));
  assert.ok(reducer.includes('pc pinned drawing matches multiple normalized solutions'));
  assert.ok(reducer.includes('pc pinned minimals source set changed; select drawings again'));
  assert.ok(contract.includes('core::mem::size_of::<PcPinnedDrawing>()'));
  assert.ok(contract.includes('pinned_drawings: command.pinned_minimum_drawing_owner()'));
  assert.ok(surfaces.includes('canonical_full_height_pinned_drawings_resolve_only_against_the_complete_minimum_source'));
  assert.ok(workflow.includes('--lib pc_pinned_solution_document -- --test-threads=1'));
  assert.ok(workflow.includes('--lib build_colored_target_document -- --test-threads=1'));
});

test('full-height score source preserves actual physical graphs and uses the common exact score reducer', () => {
  const session = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs');
  const engine = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs');
  const parallel = read('crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_family_parallel.rs');
  const result = read('crates/clearra-core-executor/src/core_execution_result.rs');
  const traversal = read('crates/clearra-postprocess/src/score_batch/score_cell_traversal.rs');
  const compact = read('crates/clearra-postprocess/src/score_batch/exact_scoring_execution_materializer.rs');
  const extended = read('crates/clearra-postprocess/src/score_batch/full_height_score_cell_materializer.rs');
  const tests = read('crates/clearra-core-executor/tests/extended_pc_family.rs');
  assert.ok(workflow.includes('"crates/clearra-postprocess/**"'));
  assert.ok(workflow.includes('cargo test --locked -p clearra-postprocess --no-default-features --lib score_batch:: -- --test-threads=1'));
  assert.ok(session.includes('admit_budget_bound_search_execution_under_terminal_authority('));
  assert.ok(session.includes('extended_pc_family_parent_authority_not_supplied'));
  assert.ok(engine.includes('PcScoreProblemEvidence::from_executed_score_portfolio_problem('));
  assert.ok(engine.includes('graph.set_candidate_id(id)'));
  assert.ok(engine.includes('extended_pc_score_graph_family_incomplete'));
  assert.ok(engine.includes('core::mem::take(&mut self.spin_execution_graphs)'));
  assert.match(parallel, /self\.spin_execution_graphs\s*\.append\(&mut worker\.spin_execution_graphs\)/u);
  assert.ok(result.includes('pub fn full_height_scoring_execution_batch('));
  assert.ok(result.includes('self.full_height_scoring_execution_batch = None'));
  assert.ok(traversal.includes('for_each_supply_successor('));
  assert.ok(traversal.includes('ScoreModelEvaluator::evaluate_classified_lock('));
  assert.ok(traversal.includes('CompactScoreCellProjection'));
  assert.ok(compact.includes('visit_score_cell_paths('));
  assert.ok(extended.includes('visit_score_cell_paths('));
  assert.ok(extended.includes('FullHeightReplayProjector::project_scoring_step('));
  assert.ok(extended.includes('state.operations != self.required_operations'));
  assert.ok(extended.includes('candidate.canonical_trace < current.canonical_trace'));
  assert.ok(!extended.includes('candidate.attack >'));
  assert.ok(!extended.includes('Board64Layout'));
  assert.ok(tests.includes('full_height_score_source_retains_actual_lock_graphs_and_distinct_problem_authority'));
  assert.ok(tests.includes('full_height_parallel_score_source_and_common_materializer_keep_the_same_cells'));
});
