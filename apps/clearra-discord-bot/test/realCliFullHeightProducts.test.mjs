// Actual exact-source CLI computation and production Discord hops, not a
// manufactured result or performance acceptance. CI supplies an already built
// binary; this file never builds, downloads assets, or publishes anything.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { availableParallelism } from 'node:os';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { decodeCtk3 } from 'ctk3';
import { readCurrentProductRelease } from '../../../scripts/release/current-product-release.mjs';
import { compactReplayProbeIdentity } from '../../../scripts/tools/paired-compact-replay-probe.mjs';
import { assertDiscordCanonicalOnlyResult, prepareClearraArguments } from '../src/clearra/command.mjs';
import { buildCtk3Result } from '../src/clearra/ctk3-result.mjs';
import { ClearraDirectExecutor } from '../src/clearra/direct-executor.mjs';
import { findSlashCommand } from '../src/discord/slash-command-catalog.mjs';
import { buildSlashCommandArguments } from '../src/discord/slash-command-input.mjs';
import { ClearraCommandRunner } from '../src/job-service/runner.mjs';
import { renderDocumentGif } from '../src/viewer/gif.mjs';
import { buildCanonicalPcPathReplayDocument, PC_PATH_REPLAY_FRAME_DELAY_MS }
  from '../src/viewer/pc-path-replay.mjs';
import { realCliProjectionProfiles } from './support/realCliProductProjectionRequests.mjs';
import { realCliFullHeightProductRequests } from './support/realCliFullHeightProductRequests.mjs';
import { assertFixtureCoverage } from './support/fixtureCoverage.mjs';
import { checkedCliPath } from './support/realCliFullHeightArtifact.mjs';

const executable = process.env.CLEARRA_REAL_PC24_CLI;
const sourceCommit = process.env.CLEARRA_REAL_PC24_SOURCE_COMMIT;
const packagedVersion = process.env.CLEARRA_REAL_PC24_RELEASE_VERSION;
const PROCESS_TIMEOUT_MS = 15_000;
const PROCESS_OUTPUT_BYTES = 1024 * 1024;

function processFailure(result) {
  // Report the bounded typed error rather than Node's generic command failure
  // or raw stdout/stderr, which may contain host paths or artifact contents.
  let error;
  try {
    const payload = JSON.parse(result.stdout);
    error = payload.summary?.error ?? payload.error;
  } catch {}
  const code = /^[A-Z][A-Z0-9_]{1,63}$/u.test(error?.code ?? '') ? error.code : null;
  const message = typeof error?.message === 'string' && error.message.length <= 160 &&
    /^(?:pc chance (?:result|compiled authority) rejected: [a-z0-9._ -]+|wasm_[a-z0-9_]+|the (?:operation could not be completed|requested operation is not supported))$/u
      .test(error.message)
    ? error.message : null;
  const processCode = /^E[A-Z0-9_]{1,63}$/u.test(result.error?.code ?? '')
    ? result.error.code : null;
  return [`exit=${result.status ?? result.exitCode ?? 'none'}`,
    ...(result.signal ? [`signal=${result.signal}`] : []),
    ...(processCode ? [`process=${processCode}`] : []),
    ...(code ? [code] : []), ...(message ? [message] : [])].join(' ');
}

function invoke(cliPath, arguments_, { env = process.env, label = 'native CLI' } = {}) {
  const result = spawnSync(cliPath, arguments_, { encoding: 'utf8', timeout: PROCESS_TIMEOUT_MS,
    maxBuffer: PROCESS_OUTPUT_BYTES, windowsHide: true, env, stdio: ['ignore', 'pipe', 'pipe'] });
  if (result.status !== 0) assert.fail(`${label}: ${processFailure(result)}`);
  return result.stdout.trim();
}

function fullHeightKey(key, input) {
  assert.equal(typeof key, 'string');
  const match = /^ctk2\|height=([1-9][0-9]*)\|initial=([0-9a-f]{64})\|placements=(.*)$/u.exec(key);
  assert.ok(match, 'retain the full-height identity, not a narrowed CTK1 or field ID');
  assert.equal(Number(match[1]), input.height);
  assert.equal(match[2], input.mask);
  const placements = match[3].split(',');
  assert.equal(placements.length, input.pieces);
  let occupied = BigInt(`0x${input.mask}`);
  const masks = placements.map(value => {
    assert.match(value, /^I:[0-9a-f]{64}$/u);
    const mask = BigInt(`0x${value.slice(2)}`);
    assert.equal(mask >> BigInt(input.height * 10), 0n);
    assert.equal(occupied & mask, 0n);
    assert.equal(mask.toString(2).replaceAll('0', '').length, 4);
    occupied |= mask;
    return mask;
  });
  assert.equal(occupied, (1n << BigInt(input.height * 10)) - 1n);
  const ctk = buildCtk3Result({ schema_version: 'clearra.solution-data.v1', solution_keys: [key] });
  assert.equal(ctk.complete, true);
  assert.equal(ctk.pageCount, 1);
  const page = decodeCtk3(ctk.source).pages[0];
  assert.equal(page.height, input.height);
  for (let cell = 0; cell < input.height * 10; cell += 1) {
    const bit = 1n << BigInt(cell);
    assert.equal(page.cells[cell], (BigInt(`0x${input.mask}`) & bit) !== 0n ? 'G'
      : masks.some(mask => (mask & bit) !== 0n) ? 'I' : null);
  }
}

function verifyProduct(raw, projected, input) {
  assert.equal(raw.kind, input.kind);
  assert.equal(raw.runtime_identity?.source_commit, sourceCommit);
  assert.equal(raw.runtime_identity?.engine_build_id, sourceCommit);
  assert.equal(projected.kind, input.kind);
  switch (input.product) {
    case 'path': {
      const witness = projected.summary.canonical_witness;
      if (input.preserveB2b && input.height === 7) {
        assert.equal(witness, null);
        assert.equal(raw.summary.witness_count, '0');
        assert.equal(buildCanonicalPcPathReplayDocument(projected), null);
        break;
      }
      assert.ok(witness);
      assert.match(witness.normalized_trace_key, /^trk2:/u);
      assert.equal(witness.steps.length, input.pieces);
      assert.equal(witness.steps[0].board_before_mask, `0x${input.mask}`);
      assert.equal(witness.steps.at(-1).board_after_line_clear_mask, `0x${'0'.repeat(64)}`);
      const replay = buildCanonicalPcPathReplayDocument(projected);
      assert.equal(replay.frameCount, 1 + input.pieces * 2);
      assert.ok(replay.document.pages.every(page => page.height === input.height));
      assert.ok(replay.document.pages.at(-1).cells.every(cell => cell === null));
      const gif = renderDocumentGif(replay.document, { delayMs: PC_PATH_REPLAY_FRAME_DELAY_MS });
      assert.equal(new TextDecoder().decode(gif.subarray(0, 6)), 'GIF89a');
      const delays = [];
      for (let index = 0; index + 7 < gif.length; index += 1) {
        if (gif[index] === 0x21 && gif[index + 1] === 0xf9 && gif[index + 2] === 0x04) {
          delays.push(gif[index + 4] | (gif[index + 5] << 8));
          index += 7;
        }
      }
      assert.deepEqual(delays, Array(replay.frameCount).fill(50));
      break;
    }
    case 'minimals':
      if (input.preserveB2b && input.height === 7) {
        assert.equal(raw.summary.optimal_cardinality, '0');
        assert.deepEqual(raw.summary.members, []);
        assert.equal(raw.summary.canonical_witness, null);
        assert.equal(projected.summary.canonical_candidate, null);
        assert.equal(Object.hasOwn(projected.summary, 'unsatisfiable'), false);
        break;
      }
      assert.equal(raw.summary.optimal_cardinality, '1');
      assert.equal(projected.summary.canonical_candidate.candidate_id, '1');
      fullHeightKey(projected.summary.canonical_candidate.normalized_solution_key, input);
      break;
    case 'score-minimals':
      assert.equal(projected.summary.optimal_cardinality, '1');
      assert.equal(projected.summary.score_minimals_canonical_candidate_id, '1');
      fullHeightKey(projected.summary.score_minimals_canonical_solution_key, input);
      assert.equal(projected.resource_report.truncated, false);
      assert.equal(projected.resource_report.count_complete, true);
      break;
    case 'score':
      assert.equal(projected.summary.score_solution_fields.length, 1);
      fullHeightKey(projected.summary.score_solution_fields[0].normalized_field_key, input);
      assert.equal(projected.summary.score_solution_fields[0].score_complete, true);
      break;
    case 'score-finder':
      assert.equal(projected.summary.canonical_winner.candidate_id, '1');
      fullHeightKey(projected.summary.canonical_winner.normalized_solution_key, input);
      break;
    case 'failed-queue':
      assert.equal(Number(raw.summary.failed_pattern_count), 0);
      assert.equal(Number(raw.summary.failed_queue_probability), 0);
      break;
    case 'chance':
      assert.equal(Number(raw.summary.covered_pattern_count), 1);
      // The WASM-backed CLI preserves its canonical union-probability field;
      // an unsupported legacy alias must not be manufactured by a presenter.
      assert.equal(Number(raw.summary.coverage_probability), 1);
      assert.equal(raw.summary.probability_complete, true);
      assert.equal(Object.hasOwn(raw.summary, 'probability'), false);
      break;
    case '':
      assert.equal(raw.summary.solution_keys_complete, true);
      assert.equal(raw.contract.artifacts.solution_keys.length, 1);
      fullHeightKey(raw.contract.artifacts.solution_keys[0], input);
      break;
    default: assert.fail('unreviewed product readback');
  }
}

async function verifyCompactChanceRunnerParity(cliPath) {
  // Exercise the Cloud runner with the same small search, host budget and
  // already-built CLI as direct execution. Do not run the larger P7P4 probe
  // here: this verifies product lowering and admission, not performance.
  const workers = Math.min(2, availableParallelism());
  const request = ['pc', 'chance', '--board-mask', '0x3c0f03c0f', '--height', '4',
    '--lines', '4', '--pieces', '6', '--patterns', 'P7', '--hold', 'empty',
    '--queue-knowledge', 'oracle', '--backend', 'cpu', '--rule', 'srs-plus',
    '--no-legal-board', '--no-conditioned-reachability'];
  const timeout = PROCESS_TIMEOUT_MS;
  const native = JSON.parse(invoke(cliPath, [
    '--format', 'json', ...request, '--auto-workers', String(workers), '--use-all-cpu-threads',
  ], {
    env: { ...process.env, CLEARRA_EXPECTED_VCPUS: String(workers) },
    label: 'direct compact PC chance',
  }));
  const runner = new ClearraCommandRunner({
    executable: cliPath, processLogicalProcessors: workers, expectedVcpus: workers,
    searchWorkersPerSession: workers, useAllLogicalProcessors: true,
    searchTimeoutMs: timeout, maxOutputBytes: PROCESS_OUTPUT_BYTES, terminationGraceMs: 1_000,
  });
  const actual = await runner.execute({ arguments: request, timeoutClass: 'pc_reverse',
    deadlineUnixMs: Date.now() + timeout, maxOutputBytes: PROCESS_OUTPUT_BYTES });
  if (actual.exitCode !== 0) assert.fail(`Discord compact PC chance: ${processFailure(actual)}`);
  assert.equal(actual.terminationReason, 'exit');
  const discord = JSON.parse(actual.stdout);
  for (const run of [native, discord]) {
    assert.equal(run.kind, 'pc-probability.v2');
    assert.equal(run.runtime_identity?.source_commit, sourceCommit);
    assert.equal(run.runtime_identity?.engine_build_id, sourceCommit);
    assert.equal(run.summary.actual_backend, 'wasm-cpu');
    assert.equal(Number(run.summary.workers_used), workers);
    assert.equal(run.summary.count_complete, true);
    assert.equal(run.summary.probability_complete, true);
    assert.equal(Number(run.summary.coverage_probability), 1);
    assert.equal(Number(run.summary.total_possible_pattern_count), 5040);
    assert.equal(Number(run.summary.covered_pattern_count), 5040);
    // Chance is a union-probability summary, not a materialized solution
    // family. A missing normalized hash is not identity-parity evidence.
    assert.equal(run.summary.unique_solution_count, 'not-calculated');
    assert.equal(run.summary.normalized_solution_set_hash, 'not-calculated');
    assert.ok(Number(run.summary.packing_candidate_count) > 0);
    assert.match(run.summary.geometry_catalog_digest, /^[0-9a-f]{16}$/u);
    assert.equal(String(run.summary.cpu_parallel_execution), String(workers > 1));
  }
  for (const field of ['packing_candidate_count', 'geometry_catalog_digest']) {
    assert.deepEqual(discord.summary[field], native.summary[field], `pc chance result differs: ${field}`);
  }
  return workers;
}

async function verifyCompactReplayRunnerParity(cliPath) {
  // Use the same one-lock fixture as the Chromium smoke. The alias must
  // reach the exact approved CLI's result, not a compatibility reducer.
  const options = [
    { name: 'field', type: 3, value: 'grid:____######' },
    { name: 'next', type: 3, value: 'I' },
    { name: 'lines', type: 4, value: 1 },
    { name: 'hold', type: 3, value: 'disabled' },
    { name: 'kicktable', type: 3, value: 'srs-plus' },
  ];
  const grouped = buildSlashCommandArguments(findSlashCommand('pc'), [
    { name: 'path', type: 1, options },
  ]);
  const alias = buildSlashCommandArguments(findSlashCommand('path'), options);
  assert.deepEqual(alias, grouped);
  assert.deepEqual(alias, ['pc', 'path', '--lines', '1', '--board-mask', '0x3f0',
    '--height', '1', '--pieces', '1', '--queue', 'I', '--no-hold', '--rule', 'srs-plus']);
  const request = [...alias, '--backend', 'cpu'];
  const authority = { workers: 1, logicalProcessors: 1, outputFormat: 'json',
    includeSolutionData: true };
  const expectedVcpus = 1;
  const stdout = invoke(cliPath, prepareClearraArguments(request, authority), {
    env: { ...process.env, CLEARRA_EXPECTED_VCPUS: String(expectedVcpus) },
    label: 'direct compact PC replay',
  });
  const raw = JSON.parse(stdout);
  const identity = compactReplayProbeIdentity(raw.kind, raw.summary);
  assert.equal(raw.runtime_identity?.source_commit, sourceCommit);
  const runner = new ClearraCommandRunner({
    executable: cliPath, processLogicalProcessors: 1, expectedVcpus,
    searchWorkersPerSession: 1, useAllLogicalProcessors: false,
    searchTimeoutMs: PROCESS_TIMEOUT_MS, maxOutputBytes: PROCESS_OUTPUT_BYTES,
    terminationGraceMs: 1_000,
  });
  const actual = await runner.execute({ arguments: request, timeoutClass: 'pc_reverse',
    deadlineUnixMs: Date.now() + PROCESS_TIMEOUT_MS, maxOutputBytes: PROCESS_OUTPUT_BYTES });
  if (actual.exitCode !== 0) assert.fail(`Discord compact PC replay: ${processFailure(actual)}`);
  assert.equal(actual.terminationReason, 'exit');
  const projected = JSON.parse(actual.stdout);
  const direct = assertDiscordCanonicalOnlyResult({ exitCode: 0, signal: null, stderr: '', stdout });
  assert.deepEqual(projected, JSON.parse(direct.stdout));
  assert.equal(projected.summary.canonical_witness.normalized_trace_key, identity.canonical_witness);
  const replay = buildCanonicalPcPathReplayDocument(projected);
  assert.equal(replay.frameCount, 3);
  assert.ok(replay.document.pages.at(-1).cells.every(cell => cell === null));
}

test('real full-height CLI families survive Discord runner and render all physical rows', {
  skip: !executable && !sourceCommit && packagedVersion === undefined, timeout: 120_000,
}, async t => {
  assert.match(sourceCommit ?? '', /^[0-9a-f]{40}$/u);
  const repository = await realpath(fileURLToPath(new URL('../../..', import.meta.url)));
  const cliPath = checkedCliPath({ repository, path: executable, version: packagedVersion,
    release: packagedVersion === undefined ? undefined : readCurrentProductRelease(repository),
    platform: process.platform, arch: process.arch });
  assert.equal(await realpath(executable), cliPath);
  const digest = createHash('sha256').update(await readFile(cliPath)).digest('hex');
  const executor = new ClearraDirectExecutor({ executable: cliPath, processLogicalProcessors: 1,
    searchWorkersPerSession: 1, useAllLogicalProcessors: false, searchTimeoutMs: PROCESS_TIMEOUT_MS,
    interactionDeadlineMs: PROCESS_TIMEOUT_MS, maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 1024 * 1024, terminationGraceMs: 1_000 });
  const planned = new Map(realCliProjectionProfiles.map(profile =>
    [profile, realCliFullHeightProductRequests(profile)]));
  const completed = [];
  for (const profile of realCliProjectionProfiles) {
    for (const input of planned.get(profile)) {
      const label = `${profile}/${input.name}/${input.product}/b2b=${input.preserveB2b}`;
      const stdout = invoke(cliPath, prepareClearraArguments(input.arguments, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true }), { label });
      const raw = JSON.parse(stdout);
      const direct = assertDiscordCanonicalOnlyResult({ exitCode: 0, signal: null, stderr: '', stdout });
      const projected = JSON.parse(direct.stdout);
      verifyProduct(raw, projected, input);
      const actual = await executor.execute(input.arguments);
      if (actual.exitCode !== 0) assert.fail(`${label}: ${processFailure(actual)}`);
      assert.equal(actual.signal, null);
      assert.equal(actual.terminationReason, 'exit');
      assert.deepEqual(JSON.parse(actual.stdout), projected,
        `${label}: actual CLI/Discord product drift`);
      assert.equal(assertDiscordCanonicalOnlyResult(actual).stdout, actual.stdout);
      completed.push([profile, input.name, input.product, input.kind, input.preserveB2b]);
    }
    t.diagnostic(`${profile}: bounded 7/8/12/24L, eight actual products, preserving-B2B/empty, complete keys and 500ms replay`);
  }
  assertFixtureCoverage(completed, [...planned].flatMap(([profile, inputs]) => inputs.map(input =>
    [profile, input.name, input.product, input.kind, input.preserveB2b])), 'real full-height CLI/Discord products');
  t.diagnostic(`compact 4L P7 CLI/Discord chance matched with ${await verifyCompactChanceRunnerParity(cliPath)} worker(s)`);
  await verifyCompactReplayRunnerParity(cliPath);
  t.diagnostic('compact 1L /path and /pc path use the same CLI replay reducer and canonical witness');
  assert.equal(await realpath(cliPath), cliPath);
  assert.equal(createHash('sha256').update(await readFile(cliPath)).digest('hex'), digest,
    'the built binary must not change during exact-source readback');
  t.diagnostic(`source=${sourceCommit} cli_sha256=${digest} cases=${completed.length} release_authority=false`);
});
