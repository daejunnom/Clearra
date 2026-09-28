import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const compiled = await build({ stdin: { contents: `export * from './packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts';
export * from './packages/clearra-ui/src/lib/workspace/recoveryFieldReferences.ts';`, resolveDir: root },
  bundle: true, write: false, platform: 'node', format: 'esm', target: 'node22' });
const api = await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../tests/fixtures/recovery-build/user-20260928.json', import.meta.url), 'utf8'));
const original = { ...api.createRecoveryBuildRequest(), height: fixture.height,
  startMask: BigInt(fixture.start_mask), middleMask: BigInt(fixture.middle_mask), resultMask: BigInt(fixture.result_mask),
  firstSupply: fixture.first_supply, secondSupply: fixture.second_supply, maxEarly: fixture.max_early,
  holdEnabled: fixture.hold_enabled, allowPieceExchange: fixture.allow_piece_exchange,
  preserveB2B: fixture.preserve_b2b, rule: fixture.rule, spinProfile: fixture.spin_profile };
const normalized = 0x30483f07f3f8fn;
const argument = (args, name) => args[args.indexOf(name) + 1];

test('original P7/P7 screenshot lowers only at execution, with all settings intact', () => {
  const snapshot = { ...original };
  assert.equal(api.recoveryResultFrame(original), 'shared');
  assert.equal(api.recoveryResultMaskForEngine(original), normalized);
  assert.equal(original.resultMask, normalized << 50n);
  for (const exchange of [false, true]) {
    const input = { ...original, allowPieceExchange: exchange };
    const args = api.recoveryBuildArguments(input, 11);
    assert.equal(BigInt(argument(args, '--start-mask')), original.startMask);
    assert.equal(BigInt(argument(args, '--middle-mask')), original.middleMask);
    assert.equal(BigInt(argument(args, '--result-mask')), normalized);
    for (const [name, value] of [['--first-supply', 'P7'], ['--second-supply', 'P7'],
      ['--max-early', 'auto'], ['--rule', 'srs-plus'], ['--spin-profile', 'all-spin-plus'], ['--workers', '11']]) {
      assert.equal(argument(args, name), value);
    }
    assert.ok(args.includes('--hold') && args.includes('--preserve-b2b'));
    assert.ok(args.includes(exchange ? '--allow-piece-exchange' : '--no-piece-exchange'));
    assert.deepEqual(api.recoveryBuildDesktopRequest(input, 'ko', 11).arguments, args);
  }
  assert.deepEqual(original, snapshot);
});

test('explicit and absent legacy frames retain after-middle meaning, not heuristic floor alignment', () => {
  const legacy = { ...original, resultMask: normalized, resultFrame: 'after-middle' };
  assert.equal(api.recoveryResultMaskForEngine(legacy), normalized);
  delete legacy.resultFrame;
  assert.equal(api.recoveryResultMaskForEngine(legacy), normalized);
  legacy.resultMask <<= 50n;
  assert.equal(api.recoveryResultMaskForEngine(legacy), normalized << 50n, 'an explicitly floating legacy target stays floating');
  assert.throws(() => api.recoveryResultMaskForEngine({ ...original, resultFrame: 'guess' }));
});

test('coordinate selection round-trips targets and preserves independent drafts', () => {
  const before = { ...original };
  const after = api.changeRecoveryResultFrame(original, 'after-middle');
  assert.equal(after.resultMask, normalized);
  assert.equal(after.startMask, original.startMask);
  assert.equal(after.middleMask, original.middleMask);
  assert.ok(api.recoveryFieldReferences(after, 'resultMask', true).every(ref => ref.mask === 0n));
  const shared = api.changeRecoveryResultFrame(after, 'shared');
  assert.deepEqual(shared, original);
  assert.deepEqual(original, before);
  assert.deepEqual(api.recoveryFieldReferences(shared, 'resultMask', true).map(ref => ref.mask),
    [original.startMask, original.middleMask]);
});

test('non-contiguous completed rows are removed; empty and result-completed rows remain', () => {
  const input = { ...original, height: 6, startMask: 1023n << 10n, middleMask: 1023n << 30n,
    resultMask: (1n << 20n) | (1023n << 40n) };
  assert.equal(api.recoveryResultMaskForEngine(input), (1n << 10n) | (1023n << 20n));
  assert.equal(api.changeRecoveryResultFrame(api.changeRecoveryResultFrame(input, 'after-middle'), 'shared').resultMask,
    input.resultMask);
  const gap = { ...original, height: 10, startMask: 0n, middleMask: 15n, resultMask: 0xc03n << 70n };
  assert.equal(api.recoveryResultMaskForEngine(gap), gap.resultMask, 'empty rows are not deleted');
});

test('overlapping or removed-row result cells are rejected rather than silently discarded', () => {
  for (const cell of [1n, 1n << 40n]) {
    const input = { ...original, resultMask: original.resultMask | cell };
    const before = { ...input };
    assert.ok(api.validateRecoveryBuildRequest(input).includes('overlap'));
    assert.throws(() => api.recoveryBuildArguments(input));
    assert.throws(() => api.changeRecoveryResultFrame(input, 'after-middle'));
    assert.deepEqual(input, before);
  }
});

test('frame conversion grows to preserve high cells and rejects overflow without truncation', () => {
  const input = { ...original, height: 8, startMask: 1023n, middleMask: 0n,
    resultMask: 1n << 70n, resultFrame: 'after-middle' };
  const shared = api.changeRecoveryResultFrame(input, 'shared');
  assert.equal(shared.height, 9);
  assert.equal(shared.resultMask, 1n << 80n);
  assert.equal(api.recoveryResultMaskForEngine(shared), input.resultMask);
  const overflow = { ...input, height: 24, resultMask: 1n << 230n };
  assert.throws(() => api.changeRecoveryResultFrame(overflow, 'shared'));
  assert.equal(overflow.resultMask, 1n << 230n);
});

test('all deletion subsets in a six-row frame agree with independent per-cell row ranks', () => {
  for (let removed = 0; removed < 64; removed++) {
    let first = 0n, result = 0n, expected = 0n;
    for (let y = 0; y < 6; y++) {
      if (removed & (1 << y)) first |= 1023n << BigInt(y * 10);
      else {
        const x = (y * 3 + removed) % 10;
        result |= 1n << BigInt(y * 10 + x);
        let rank = 0;
        for (let below = 0; below < y; below++) if (!(removed & (1 << below))) rank++;
        expected |= 1n << BigInt(rank * 10 + x);
      }
    }
    const input = { ...original, height: 6, startMask: first, middleMask: 0n, resultMask: result };
    assert.equal(api.recoveryResultMaskForEngine(input), expected);
    const roundtrip = api.changeRecoveryResultFrame(api.changeRecoveryResultFrame(input, 'after-middle'), 'shared');
    assert.equal(roundtrip.resultMask, result);
  }
});
