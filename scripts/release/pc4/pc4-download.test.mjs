import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { Pc4StreamSha256 } from './pc4-stream-sha256.mjs';
import { downloadPc4Profile, pc4DownloadPlan } from './pc4-download.mjs';
import { PC4_READER_CONTRACT } from './qualify-upstream-generation.mjs';

test('incremental digest matches independent SHA-256 including padding and chunk boundaries', () => {
  for (const length of [0,1,55,56,63,64,65,119,120,127,128,129,1024,1_000_000]) {
    const bytes = Uint8Array.from({ length }, (_, i) => (i * 31 + 73) & 255);
    for (const step of [1,17,64,1009,65_536]) {
      const digest = new Pc4StreamSha256();
      for (let i = 0; i < length; i += step) digest.update(bytes.subarray(i, i + step));
      assert.equal(digest.hex(), createHash('sha256').update(bytes).digest('hex'), `${length}/${step}`);
      assert.throws(() => digest.update(new Uint8Array()));
      assert.throws(() => digest.hex());
    }
  }
});

function fixture() {
  const files = new Map(['field_hash_to_id.v1.bin','graph_offsets.u32.bin','graph.bin'].map((p, i) =>
    [p, Uint8Array.from({ length: 25 + i * 9 }, (_, j) => (j * 19) & 255)]));
  const descriptors = [...files].map(([path, bytes]) => ({ path, byte_length: bytes.length,
    content_identity: 'sha256:' + createHash('sha256').update(bytes).digest('hex') }));
  const generation = { schema: 'clearra.pc4.host-generation.v1', repository: 'muse918/tetris-4lpc-mdp-vstar-policy',
    revision: 'a'.repeat(40), profiles: ['srs','srs-plus','srs-x','jstris-180','no-kick'].map(profile =>
      profile !== 'jstris-180' ? { profile, status: 'unavailable' } : { profile, status: 'ready', upstream_complete: true,
        reader_contract: PC4_READER_CONTRACT, artifacts: { fields: descriptors[0], offsets: descriptors[1], graph: descriptors[2] } }) };
  const chunks = new Map(), state = { begun: 0, committed: 0, aborted: 0, requests: 0 };
  const store = { async begin() {
    state.begun++;
    return {
      async open(file) {
        chunks.set(file.path, []);
        return { async write(bytes) { chunks.get(file.path).push(...bytes); }, async close() {}, async abort() {} };
      },
      async commit(value) { state.committed++; assert.deepEqual(value, generation); },
      async abort() { state.aborted++; chunks.clear(); }
    };
  } };
  const fetcher = async (url, options) => {
    state.requests++;
    assert.equal(options.credentials, 'omit'); assert.equal(options.headers, undefined);
    assert.ok(url.includes('/' + generation.revision + '/'));
    return new Response(files.get(url.split('/').at(-1)), { status: 200 });
  };
  return { files, generation, chunks, state, store, fetcher };
}

test('only an explicit action streams exactly the three qualified files and commits after all digests', async () => {
  const f = fixture();
  await assert.rejects(downloadPc4Profile(f.generation, f.store), { code: 'pc4_download_explicit_action_required' });
  assert.equal(f.state.begun, 0);
  const result = await downloadPc4Profile(f.generation, f.store, { intent: 'explicit-download', profile: 'jstris-180', fetcher: f.fetcher });
  assert.equal(result.storedBytes, [...f.files.values()].reduce((n, bytes) => n + bytes.length, 0));
  assert.equal(f.state.requests, 3); assert.equal(f.state.committed, 1); assert.equal(f.state.aborted, 0);
  for (const [path, bytes] of f.files) assert.deepEqual(Uint8Array.from(f.chunks.get(path)), bytes);
  assert.throws(() => pc4DownloadPlan(f.generation, 'srs'), { code: 'pc4_download_profile_unavailable' });
});

test('an explicit action without an explicit profile never defaults to Jstris or starts I/O', async () => {
  const f = fixture();
  for (const profile of [undefined, null, '', 'no180', 'unknown']) {
    assert.throws(() => pc4DownloadPlan(f.generation, profile), { code: 'pc4_download_profile_required' });
    await assert.rejects(downloadPc4Profile(f.generation, f.store, {
      intent: 'explicit-download', profile, fetcher: f.fetcher,
    }), { code: 'pc4_download_profile_required' });
  }
  assert.equal(f.state.requests, 0);
  assert.equal(f.state.begun, 0);
});

test('each kick table requires its own qualification and cannot alias the Jstris artifacts', () => {
  const f = fixture();
  for (const profile of ['srs','srs-plus','srs-x','no-kick']) {
    const claimed = structuredClone(f.generation);
    const jstris = claimed.profiles.find(p => p.profile === 'jstris-180');
    claimed.profiles = claimed.profiles.map(p => p.profile === profile ? { ...jstris, profile } : p);
    assert.throws(() => pc4DownloadPlan(claimed, profile), { code: 'pc4_download_profile_unavailable' });
  }
  const duplicate = { ...f.generation, profiles: Array(5).fill(f.generation.profiles[3]) };
  assert.throws(() => pc4DownloadPlan(duplicate, 'jstris-180'), { code: 'pc4_download_generation_invalid' });
});

for (const failure of ['digest','truncated','oversized','partial','rate-limit','cancel','quota']) {
  test(`failed ${failure} install does not publish a partial generation and removes staging`, async () => {
    const f = fixture(), controller = new AbortController();
    const fetcher = async (url, options) => {
      const response = await f.fetcher(url, options), bytes = new Uint8Array(await response.arrayBuffer());
      if (failure === 'digest') bytes[0] ^= 1;
      if (failure === 'cancel') controller.abort();
      if (failure === 'quota') throw new DOMException('quota', 'QuotaExceededError');
      return new Response(failure === 'truncated' ? bytes.subarray(1) : failure === 'oversized' ? new Uint8Array(bytes.length + 1) : bytes,
        { status: failure === 'partial' ? 206 : failure === 'rate-limit' ? 429 : 200 });
    };
    await assert.rejects(downloadPc4Profile(f.generation, f.store, { intent: 'explicit-download', profile: 'jstris-180', fetcher, signal: controller.signal }));
    assert.equal(f.state.committed, 0); assert.equal(f.state.aborted, 1); assert.equal(f.chunks.size, 0);
  });
}
