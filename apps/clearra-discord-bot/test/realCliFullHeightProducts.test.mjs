// Actual exact-source CLI computation and production Discord hops, not a
// manufactured result or performance acceptance. CI supplies an already built
// binary; this file never builds, downloads assets, or publishes anything.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { decodeCtk3 } from 'ctk3';
import { assertDiscordCanonicalOnlyResult, prepareClearraArguments } from '../src/clearra/command.mjs';
import { buildCtk3Result } from '../src/clearra/ctk3-result.mjs';
import { ClearraDirectExecutor } from '../src/clearra/direct-executor.mjs';
import { renderDocumentGif } from '../src/viewer/gif.mjs';
import { buildCanonicalPcPathReplayDocument, PC_PATH_REPLAY_FRAME_DELAY_MS }
  from '../src/viewer/pc-path-replay.mjs';
import { realCliProjectionProfiles } from './support/realCliProductProjectionRequests.mjs';
import { realCliFullHeightProductRequests } from './support/realCliFullHeightProductRequests.mjs';

const executable = process.env.CLEARRA_REAL_PC24_CLI;
const sourceCommit = process.env.CLEARRA_REAL_PC24_SOURCE_COMMIT;

function invoke(arguments_) {
  return execFileSync(executable, arguments_, { encoding: 'utf8', timeout: 120_000,
    maxBuffer: 1024 * 1024, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
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

test('real full-height CLI families survive Discord runner and render all physical rows', {
  skip: !executable && !sourceCommit, timeout: 1_200_000,
}, async t => {
  assert.ok(isAbsolute(executable ?? ''), 'an explicit already-built CLI path is required');
  assert.match(sourceCommit ?? '', /^[0-9a-f]{40}$/u);
  const repository = await realpath(fileURLToPath(new URL('../../..', import.meta.url)));
  const binary = process.platform === 'win32' ? 'clearra.exe' : 'clearra';
  assert.ok(['debug', 'release'].some(profile =>
    join(repository, 'build', 'cargo', 'default', profile, binary) === resolve(executable)));
  assert.equal(await realpath(executable), resolve(executable));
  const digest = createHash('sha256').update(await readFile(executable)).digest('hex');
  const executor = new ClearraDirectExecutor({ executable, processLogicalProcessors: 1,
    searchWorkersPerSession: 1, useAllLogicalProcessors: false, searchTimeoutMs: 120_000,
    interactionDeadlineMs: 150_000, maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 1024 * 1024, terminationGraceMs: 1_000 });
  let completed = 0;
  for (const profile of realCliProjectionProfiles) {
    for (const input of realCliFullHeightProductRequests(profile)) {
      const stdout = invoke(prepareClearraArguments(input.arguments, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true }));
      const raw = JSON.parse(stdout);
      const direct = assertDiscordCanonicalOnlyResult({ exitCode: 0, signal: null, stderr: '', stdout });
      const projected = JSON.parse(direct.stdout);
      verifyProduct(raw, projected, input);
      const actual = await executor.execute(input.arguments);
      assert.equal(actual.exitCode, 0, actual.stderr);
      assert.equal(actual.signal, null);
      assert.equal(actual.terminationReason, 'exit');
      assert.deepEqual(JSON.parse(actual.stdout), projected,
        `${profile}/${input.name}/${input.product}/b2b=${input.preserveB2b}: actual CLI/Discord product drift`);
      assert.equal(assertDiscordCanonicalOnlyResult(actual).stdout, actual.stdout);
      completed += 1;
    }
    t.diagnostic(`${profile}: bounded 7/8/12/24L, eight actual products, preserving-B2B/empty, complete keys and 500ms replay`);
  }
  assert.equal(completed, 200);
  assert.equal(createHash('sha256').update(await readFile(executable)).digest('hex'), digest,
    'the built binary must not change during exact-source readback');
  t.diagnostic(`source=${sourceCommit} cli_sha256=${digest} cases=${completed} release_authority=false`);
});
