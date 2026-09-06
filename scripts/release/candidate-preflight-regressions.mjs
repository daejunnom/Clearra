// Closed, non-publishing feedback for the current minimum-continuation change.
// This is deliberately not canonical acceptance and accepts no caller filters.
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
export const CANDIDATE_REF = 'refs/heads/codex/v0.8.0-preflight-20260906-rng';
const APP_FAILURES = [
  'cooperative_execution::pc_allspin_projection_tests::cooperative_build_product_memory_authority_is_bounded_and_fail_closed',
  'portfolio_alternative_store::tests::candidate_ids_and_portfolios_are_numeric_lexicographic_and_one_based',
  'portfolio_alternative_store::tests::checkpoint_resume_is_exact_and_identity_bound',
  'portfolio_alternative_store::tests::runtime_page_store_retains_prefetched_pages_for_backward_member_navigation',
  'product_capability_contract_tests::direct_wasm_pc_minimals_enumerates_every_iiooo_single_member_tie',
  'product_capability_contract_tests::pc_failed_queue_wasm_and_distributed_paths_fail_closed_without_fallback',
  'product_capability_contract_tests::pc_minimals_deferred_boundary_rejects_status_and_full_source_tampering',
  'product_capability_contract_tests::pc_minimals_derives_selected_metadata_and_probabilities_from_the_app_canonical_page',
  'product_capability_contract_tests::pc_minimals_unique_execution_preserves_the_v074_count_all_public_field_identity',
];
export const CANDIDATE_RUST_REGRESSIONS = Object.freeze([
  ...APP_FAILURES.map((filter) => ({ package: 'clearra-app', filter, exact: true, parallel: true })),
  ...[
    'cooperative_score_minimum_enters_shared_guarded_driver_and_cancels',
    'cooperative_build_cover_uses_real_build_source_and_shared_minimum_state',
    'cooperative_build_score_products_match_direct_and_distributed_typed_evidence',
    'build_solution_probability_result::build_v2_result::tests::',
    'pc_score_minimum_cover_contract_tests::cooperative_score_minimum_',
    'minimum_preparation_constructor_',
    'build_score_minimum_',
  ].map((filter) => ({ package: 'clearra-app', filter, exact: false, parallel: true })),
  { package: 'clearra-core-executor', filter: 'native_parallel_minimum_cover_', exact: false, parallel: true },
  { package: 'clearra-pc-graph', filter: 'compiled_query_equality_', exact: false, parallel: false },
  ...['build_cover_', 'build_score_minimum_', 'build_minimum_source_preparation_'].map((filter) => ({
    package: 'clearra-wasm', filter, exact: false, parallel: false,
  })),
].map(Object.freeze));

export function assertCandidateHost(environment, platform) {
  if (platform !== 'win32' || environment.GITHUB_ACTIONS !== 'true' || environment.GITHUB_REF !== CANDIDATE_REF) {
    throw new Error('Focused native feedback requires the isolated Windows Actions candidate job');
  }
  const sha = environment.GITHUB_SHA;
  if (!/^[0-9a-f]{40}$/u.test(sha ?? '') || environment.CLEARRA_SOURCE_COMMIT !== sha || environment.CLEARRA_ENGINE_BUILD_ID !== sha) {
    throw new Error('Focused candidate source and engine must match the exact Actions SHA');
  }
  if (environment.RUST_MIN_STACK !== '16777216' || Object.keys(environment).some((key) => key.startsWith('CLEARRA_ACCEPTED_'))) {
    throw new Error('Focused candidate requires its bounded stack and no acceptance authority');
  }
}

export function candidateRegressionArguments(spec) {
  if (!CANDIDATE_RUST_REGRESSIONS.includes(spec)) throw new Error('Unknown candidate regression');
  return ['test', '--locked', '--package', spec.package, '--lib',
    ...(spec.parallel ? ['--features', 'parallel'] : []), spec.filter,
    '--', '--test-threads=1', ...(spec.exact ? ['--exact'] : [])];
}

export function assertNonemptyRustSuccess(result) {
  if (result?.error || result?.status !== 0 || result?.signal) throw new Error('Focused Rust regression process failed');
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  const summaries = [...output.matchAll(/^test result: ok\. (\d+) passed; (\d+) failed;/gmu)];
  if (summaries.length !== 1 || Number(summaries[0][1]) < 1 || Number(summaries[0][2]) !== 0) {
    throw new Error('Focused Rust regression must execute at least one passing test (zero/ignored is not evidence)');
  }
}

export function runCandidateRegressions({ environment = process.env, platform = process.platform,
  repositoryRoot = ROOT, spawnImplementation = spawnSync, write = (text) => process.stdout.write(text) } = {}) {
  assertCandidateHost(environment, platform);
  const failures = [];
  for (const spec of CANDIDATE_RUST_REGRESSIONS) {
    write(`candidate_regression=start package=${spec.package} filter=${spec.filter}\n`);
    const result = spawnImplementation('cargo', candidateRegressionArguments(spec), {
      cwd: repositoryRoot, env: environment, shell: false, windowsHide: true,
      encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    });
    if (result?.stdout) write(result.stdout);
    if (result?.stderr) write(result.stderr);
    try { assertNonemptyRustSuccess(result); }
    catch { failures.push(`${spec.package}:${spec.filter}`); }
  }
  if (failures.length) throw new Error(`Focused candidate regressions failed (${failures.length}): ${failures.join(', ')}`);
  write(`candidate_regressions=passed selections=${CANDIDATE_RUST_REGRESSIONS.length} release_authority=false\n`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (process.argv.length !== 2) throw new Error('Candidate regression filters are fixed; no arguments accepted');
    runCandidateRegressions();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
