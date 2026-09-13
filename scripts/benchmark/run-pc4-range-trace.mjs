// Local-only transport-policy A/B. Replay an existing bounded, real WASM demand
// trace through the product HTTP reader, serving exact response bytes from the
// explicitly downloaded generation. No network, search rerun or artifact build.
import { createHash } from 'node:crypto';
import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { checkedDatasetRoot, openBenchmarkDataset } from './pc4-local-dataset.mjs';
import { createPc4RangeReader } from '../release/pc4/pc4-range-reader.mjs';
import { checkedPc4Read } from '../release/pc4/pc4-range-plan.mjs';
import { pc4SearchRangePolicy } from '../release/pc4/pc4-search-range-policy.mjs';

const { values } = parseArgs({ options: {
  directory: { type: 'string' }, profile: { type: 'string' },
  windows: { type: 'string', default: '0,512,2048,4096,16384' }, search: { type: 'boolean' }
} });
const windows = values.windows.split(',').map(Number);
if (!windows.length || windows.length > 8 || windows.some(n => !Number.isSafeInteger(n) ||
    n < 0 || n > 65536 || n && (n < 512 || (n & (n - 1))))) throw new Error('Invalid bounded window comparison');
const dataset = await openBenchmarkDataset(values.directory, values.profile);
try {
  const path = join(await checkedDatasetRoot(values.directory, values.profile), 'lookup-trace.bin');
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 4 || stat.size > 2531076) throw new Error('Invalid trace file');
  const input = await readFile(path), size = input.readUInt32LE(0);
  if (size > 131072 || 4 + size > input.length) throw new Error('Invalid trace header');
  const header = JSON.parse(input.subarray(4, 4 + size)), body = input.subarray(4 + size);
  if (header.schema !== 'clearra.pc4.lookup-trace.v1' || !Number.isSafeInteger(header.record_count) ||
      header.record_count < 1 || header.record_count > 200000 || body.length !== header.record_count * 12 ||
      header.body_sha256 !== createHash('sha256').update(body).digest('hex') ||
      JSON.stringify(header.generation) !== JSON.stringify(dataset.generation)) throw new Error('Trace identity mismatch');
  const trace = Array.from({ length: header.record_count }, (_, i) => checkedPc4Read(
    dataset.plan.files[body.readUInt32LE(i * 12)], body.readUInt32LE(i * 12 + 4), body.readUInt32LE(i * 12 + 8)));
  for (const windowBytes of windows) {
    const policy = values.search ? pc4SearchRangePolicy(dataset.generation, values.profile) : { windowBytes };
    const reader = createPc4RangeReader(dataset.generation, { ...policy, fetcher: async (url, init) => {
      const artifact = dataset.plan.files.find(a => url.endsWith(`/${a.path}`));
      if (!artifact || !url.includes(`/resolve/${dataset.plan.revision}/`)) throw new Error('Transport identity mismatch');
      const match = /^bytes=(\d+)-(\d+)$/.exec(init.headers.Range);
      if (!match) throw new Error('Transport demand mismatch');
      const offset = Number(match[1]), length = Number(match[2]) - offset + 1;
      return new Response(await dataset.read(artifact, offset, length), {
        status: 206, headers: { 'content-range': `bytes ${offset}-${offset + length - 1}/${artifact.byte_length}` }
      });
    } });
    const hash = createHash('sha256'); let completed = 0, failure = null;
    try {
      for (const { artifact, offset, length } of trace) {
        const bytes = await reader.read(artifact, offset, length);
        hash.update(`${artifact.path}:${offset}:${length}\n`).update(bytes); completed++;
      }
    } catch (error) { if (error.code !== 'pc4_online_transfer_limit') throw error; failure = error.code; }
    const digest = hash.digest('hex');
    // A full trace is evidence of equivalent bytes, not merely an I/O counter.
    // Partial traces retain their own digest but cannot claim full equivalence.
    const comparable = completed === trace.length && header.measurement?.logical_reads === trace.length;
    if (comparable && digest !== header.measurement.demand_sha256) throw new Error('Transport policy changed recorded bytes');
    console.log(JSON.stringify({ evidence: 'recorded-real-demand-local-response-transport-counts-not-network-timing',
      policy, trace_records: trace.length, completed_reads: completed, failure,
      requests: reader.requests, bytes: reader.bytes, cache_hits: reader.cacheHits,
      retained_bytes: reader.retainedBytes, ordered_demand_sha256: digest,
      complete_trace_bytes_equal: comparable ? true : null }));
    reader.dispose();
  }
} finally { await dataset.close(); }
