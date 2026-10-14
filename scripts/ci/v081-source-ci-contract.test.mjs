import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareClearraArguments } from '../../apps/clearra-discord-bot/src/clearra/command.mjs';
import { realCliProductProjectionRequests, realCliProjectionProfiles, realPcScoreTargetCases, realSetupScoreDocument }
  from '../../apps/clearra-discord-bot/test/support/realCliProductProjectionRequests.mjs';
import { realCliFullHeightInputs, realCliFullHeightProductRequests }
  from '../../apps/clearra-discord-bot/test/support/realCliFullHeightProductRequests.mjs';
import { assertFixtureCoverage }
  from '../../apps/clearra-discord-bot/test/support/fixtureCoverage.mjs';


const scoreExecutionOverrides = ['--backend', '--no-backend-fallback', '--count', '--objective', '--max-patterns'];

test('every actual Discord product fixture obeys the existing closed command registry', () => {
  assert.deepEqual(realCliProjectionProfiles, ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);
  const policies = ['false:false', 'true:false', 'false:true', 'true:true'];
  const products = [
    ['minimum', 'pc-minimum-cover.v2'], ['score-minimum', 'pc-score-portfolio.v2'],
    ['replay', 'pc-path-family.v2'], ['setup-score', 'setup-score-ranking.v1'],
    ...[1, 2, 3, 4, 5, 6].map(height => [`score-minimum-${height}l`, 'pc-score-portfolio.v2']),
  ];
  for (const profile of realCliProjectionProfiles) {
    const requests = realCliProductProjectionRequests(profile);
    assertFixtureCoverage(requests.map(request => [request.name, request.kind, request.policy]),
      [...products.flatMap(([name, kind]) => policies.map(policy => [name, kind, policy])),
        ['build-cover', 'build-coverage-portfolio.v2', 'default']], `${profile}/compact fixture`);
    for (const request of requests) {
      const prepared = prepareClearraArguments(request.arguments, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true });
      assert.equal(request.arguments[request.arguments.indexOf('--rule') + 1], profile);
      assert.ok(prepared.includes('--include-solution-data'));
      assert.deepEqual(prepared.slice(-3), ['--format', 'json', '--include-solution-data']);
      if (request.policy !== 'default') {
        const [legal, conditioned] = request.policy.split(':');
        for (const [enabled, on, off] of [[legal, '--legal-board', '--no-legal-board'],
          [conditioned, '--conditioned-reachability', '--no-conditioned-reachability']]) {
          assert.ok(request.arguments.includes(enabled === 'true' ? on : off),
            `${request.name}/${request.policy}: policy must change the actual input`);
          assert.ok(!request.arguments.includes(enabled === 'true' ? off : on));
        }
      }
    }
    const scoreRequests = requests.filter(request => request.kind === 'pc-score-portfolio.v2');
    for (const request of scoreRequests) {
      for (const forbidden of scoreExecutionOverrides)
        assert.ok(!request.arguments.includes(forbidden), 'score product owns its execution controls');
    }
    const setupRequests = requests.filter(request => request.name === 'setup-score');
    for (const request of setupRequests) {
      assert.equal(request.arguments[request.arguments.indexOf('--document') + 1], realSetupScoreDocument);
      assert.ok(!request.arguments.includes('--backend'));
      assert.ok(!request.arguments.includes('--max-patterns'));
    }
    const build = requests.find(request => request.name === 'build-cover');
    assert.equal(build.policy, 'default');
    for (const flag of ['--legal-board', '--no-legal-board', '--conditioned-reachability',
      '--no-conditioned-reachability', '--no-tablebase']) {
      assert.ok(!build.arguments.includes(flag));
      assert.throws(() => prepareClearraArguments([...build.arguments, flag]), /does not expose/u);
    }
  }
  assert.throws(() => realCliProductProjectionRequests('unknown-profile'), /unknown/u);
});

test('semantic coverage refuses missing, duplicate and relabelled cases even with the same total', () => {
  const expected = [['minimum', 'false:false'], ['minimum', 'true:true']];
  assertFixtureCoverage([...expected].reverse(), expected, 'reordered');
  for (const invalid of [expected.slice(0, 1), [expected[0], expected[0]],
    [expected[0], ['replay', 'true:true']], [...expected, ['replay', 'true:true']]]) {
    assert.throws(() => assertFixtureCoverage(invalid, expected, 'broken coverage'));
  }
  assert.throws(() => assertFixtureCoverage([], [], 'vacuous coverage'));
  assert.throws(() => assertFixtureCoverage([expected[0]], [expected[0], expected[0]], 'broken specification'));
});

test('the 1--6L score fixtures bind initial area and exact queue instead of empty odd targets', () => {
  assert.deepEqual(realPcScoreTargetCases.map(input => input.targetLines), [1, 2, 3, 4, 5, 6]);
  for (const input of realPcScoreTargetCases) {
    const option = name => input.arguments[input.arguments.indexOf(name) + 1];
    const height = input.targetLines;
    const board = BigInt(option('--board-mask'));
    assert.equal(option('--lines'), String(height));
    assert.equal(option('--height'), String(height));
    assert.equal(option('--pieces'), String(height));
    assert.equal(option('--queue'), 'I'.repeat(height));
    assert.equal(board >> BigInt(height * 10), 0n);
    let cells = 0;
    for (let row = 0; row < height; row += 1) {
      assert.equal((board >> BigInt(row * 10)) & 0x3ffn, 0x3f0n);
      cells += 6;
    }
    assert.equal(height * 10 - cells, input.requiredPieces * 4);
  }
});

test('full-height native and Discord score inputs obey product-owned execution before adapter stripping', () => {
  assert.deepEqual([...new Set(realCliFullHeightInputs.map(input => input.height))].sort((a, b) => a - b),
    [7, 8, 12, 24]);
  const products = [
    ['', 'pc-scenario'], ['chance', 'pc-probability.v2'],
    ['failed-queue', 'pc-failed-queue.v2'], ['minimals', 'pc-minimum-cover.v2'],
    ['score', 'pc-score-summary.v2'], ['score-minimals', 'pc-score-portfolio.v2'],
    ['score-finder', 'pc-fixed-score-witness.v2'], ['path', 'pc-path-family.v2'],
  ];
  for (const profile of realCliProjectionProfiles) {
    const requests = realCliFullHeightProductRequests(profile);
    assertFixtureCoverage(requests.map(request =>
      [request.name, request.product, request.kind, request.preserveB2b]),
    realCliFullHeightInputs.flatMap(input => [
      ...products.map(([product, kind]) => [input.name, product, kind, false]),
      [input.name, 'minimals', 'pc-minimum-cover.v2', true],
      [input.name, 'path', 'pc-path-family.v2', true],
    ]), `${profile}/full-height fixture`);
    for (const request of requests) {
      const prepared = prepareClearraArguments(request.arguments, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true });
      assert.equal(request.arguments[request.arguments.indexOf('--rule') + 1], profile);
      if (['score', 'score-minimals', 'score-finder'].includes(request.product)) {
        for (const forbidden of scoreExecutionOverrides) {
          assert.ok(!request.arguments.includes(forbidden), `${request.product} owns ${forbidden}`);
          assert.ok(!prepared.includes(forbidden));
        }
      }
      if (request.product === 'score-finder') {
        for (const forbidden of ['--tablebase', '--no-tablebase', '--tb', '--no-tb', '--score-profile', '--spin-profile']) {
          assert.ok(!request.arguments.includes(forbidden), 'Finder owns score and excludes TB semantics');
          assert.ok(!prepared.includes(forbidden));
        }
      }
    }
  }
});
