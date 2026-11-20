import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { compareNativeRustFamilies } from './compare-native-rust-families.mjs';

function fixture(arm, keys, { header = {}, footer = {} } = {}) {
  const hash = createHash('sha256');
  const records = [{
    kind: 'header',
    schema: 'clearra.native-rust-family.v1',
    source_commit: '1'.repeat(40),
    arm,
    case: 'full-p7p4',
    requested_workers: 8,
    available_parallelism: 8,
    backend_selected: arm === 'native-c' ? 'cpu' : 'wasm-cpu',
    reported_workers: 8,
    count_complete: true,
    resource_truncated: false,
    ...header,
  }];
  for (const key of keys) {
    const bytes = Buffer.from(key, 'utf8');
    // Same u64 LE layout as the producer, computed independently for these
    // tiny fixture lengths, including cases with a multibyte input.
    const prefix = Buffer.alloc(8);
    prefix.writeUInt32LE(bytes.length);
    hash.update(prefix).update(bytes);
    records.push({ kind: 'key', key });
  }
  records.push({
    kind: 'footer',
    count: keys.length,
    keys_sha256: hash.digest('hex'),
    solution_set_hash: 'cts1:' + '2'.repeat(16),
    ...footer,
  });
  return Buffer.from(records.map(record => JSON.stringify(record) + '\n').join(''), 'utf8');
}

function* split(bytes, stride) {
  for (let offset = 0; offset < bytes.length; offset += stride) {
    yield bytes.subarray(offset, offset + stride);
  }
}

const compare = (c, r, options) =>
  compareNativeRustFamilies(split(c, 7), split(r, 11), options);

test('compares actual memberships, not just counts or reported cts1', async () => {
  // The latter two common keys are ordered as UTF-8 rather than UTF-16.
  const native = fixture('native-c', ['a', 'c', '\ue000', '\u{10000}']);
  const rust = fixture('rust-cpu', ['b', 'c', 'd', '\ue000', '\u{10000}'], {
    header: { available_parallelism: 11, reported_workers: 7 },
  });
  const result = await compare(native, rust, { witnessLimit: 1 });
  assert.equal(result.relationship, 'different');
  assert.equal(result.evidence_scope, 'local-canonical-key-diagnostic');
  assert.equal(result.shared_count, 3);
  assert.equal(result.arms['native-c'].count, 4);
  assert.equal(result.arms['native-c'].exclusive_count, 1);
  assert.deepEqual(result.arms['native-c'].first_exclusive_keys, ['a']);
  assert.equal(result.arms['rust-cpu'].count, 5);
  assert.equal(result.arms['rust-cpu'].exclusive_count, 2);
  assert.deepEqual(result.arms['rust-cpu'].first_exclusive_keys, ['b']);
  assert.equal(result.arms['rust-cpu'].reported_workers, 7);
});

test('proves strict subsets or equality only from an exhausted merge', async () => {
  const small = ['z'];
  const large = ['a', 'z'];
  for (const [c, r, expected] of [
    [small, large, 'native-c-strict-subset'],
    [large, small, 'rust-cpu-strict-subset'],
    [large, large, 'identical'],
  ]) {
    const result = await compare(fixture('native-c', c), fixture('rust-cpu', r), { witnessLimit: 0 });
    assert.equal(result.relationship, expected);
    assert.deepEqual(result.arms['native-c'].first_exclusive_keys, []);
    assert.deepEqual(result.arms['rust-cpu'].first_exclusive_keys, []);
  }
  await assert.rejects(compare(fixture('native-c', small), fixture('rust-cpu', small), {
    witnessLimit: 65,
  }), /invalid witness limit/);
});

test('rejects mismatched source, fixture, worker request or malformed arm metadata', async () => {
  const native = fixture('native-c', ['a']);
  for (const [header, problem] of [
    [{ source_commit: '3'.repeat(40) }, /source_commit mismatch/],
    [{ case: 'existing-p7' }, /case mismatch/],
    [{ requested_workers: 7, reported_workers: 7 }, /requested_workers mismatch/],
    [{ available_parallelism: 7 }, /invalid worker metadata/],
    [{ backend_selected: 'cpu' }, /unexpected selected backend/],
    [{ arm: 'native-c' }, /unexpected capture schema or arm/],
    [{ count_complete: false }, /incomplete or resource-truncated capture/],
    [{ resource_truncated: true }, /incomplete or resource-truncated capture/],
    [{ extra: true }, /invalid capture header/],
  ]) {
    await assert.rejects(compare(native, fixture('rust-cpu', ['a'], { header })), problem);
  }
});

test('rejects corrupt, duplicate, noncanonical and oversized records', async () => {
  const native = fixture('native-c', ['a']);
  for (const [rust, problem] of [
    [fixture('rust-cpu', ['a', 'a']), /not strictly increasing/],
    [fixture('rust-cpu', ['b', 'a']), /not strictly increasing/],
    [fixture('rust-cpu', ['\ud800']), /invalid canonical key length or encoding/],
    [fixture('rust-cpu', ['a\n']), /invalid key record/],
    [fixture('rust-cpu', ['a\u0085']), /invalid key record/],
    [fixture('rust-cpu', ['a'.repeat(1025)]), /invalid canonical key length or encoding/],
    [Buffer.alloc(8193, 0x61), /line byte limit exceeded/],
    [Buffer.from([0xff, 0x0a]), /invalid UTF-8/],
    [fixture('rust-cpu', []), /invalid footer count or hash metadata/],
    [fixture('rust-cpu', ['a'], { footer: { count: 2 } }), /invalid footer count or hash metadata/],
    [fixture('rust-cpu', ['a'], { footer: { keys_sha256: '0'.repeat(64) } }), /canonical key hash mismatch/],
    [fixture('rust-cpu', ['a'], { footer: { solution_set_hash: '0'.repeat(16) } }), /invalid footer count or hash metadata/],
  ]) {
    await assert.rejects(compare(native, rust), problem);
  }
});

test('requires footer, final LF and EOF even after the opposing arm is exhausted', async () => {
  const native = fixture('native-c', ['a']);
  const rust = fixture('rust-cpu', ['a', 'b']);
  const beforeFooter = rust.subarray(0, rust.lastIndexOf(0x0a, rust.length - 2) + 1);
  await assert.rejects(compare(native, beforeFooter), /incomplete capture/);
  await assert.rejects(compare(native, rust.subarray(0, -1)), /missing final LF/);
  await assert.rejects(compare(native, Buffer.concat([rust, Buffer.from('{"kind":"key","key":"z"}\n')])),
    /record after footer/);
  await assert.rejects(compare(native, Buffer.concat([rust, Buffer.from('x')])), /missing final LF/);
});

test('closes the other capture on an early invalid record', async () => {
  let released = false;
  async function* incomingNative() {
    try {
      yield* split(fixture('native-c', ['a']), 7);
    } finally {
      released = true;
    }
  }
  await assert.rejects(compareNativeRustFamilies(incomingNative(), [Buffer.from('{}\n')]),
    /invalid capture header/);
  assert.equal(released, true);
});
