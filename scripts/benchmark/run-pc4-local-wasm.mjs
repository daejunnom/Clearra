// Local-only measurement of the real App/WASM + downloaded graph. Does not
// start a server, change 4194, rebuild Rust, publish, or assert release success.
import { lstat, open, readFile, unlink } from 'node:fs/promises';
import { resolve, join, basename } from 'node:path';
import { parseArgs } from 'node:util';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { checkedDatasetRoot, openBenchmarkDataset } from './pc4-local-dataset.mjs';
import { createPc4TraceComparison } from './pc4-trace-comparison.mjs';
import { createPc4RangeReader } from '../release/pc4/pc4-range-reader.mjs';
import { pc4SearchRangePolicy } from '../release/pc4/pc4-search-range-policy.mjs';
import { prefetchPc4LookupFrontier } from '../release/pc4/pc4-frontier-reader.mjs';

const { values } = parseArgs({ options: {
  directory: { type: 'string' }, profile: { type: 'string' }, 'wasm-directory': { type: 'string' },
  command: { type: 'string' }, seconds: { type: 'string', default: '60' },
  'read-limit': { type: 'string', default: '100000' }, cached: { type: 'boolean' },
  'page-bytes': { type: 'string', default: '4096' }, trace: { type: 'boolean' }, 'compare-trace': { type: 'boolean' },
  transport: { type: 'string', default: 'local' }, frontier: { type: 'boolean' },
  'wasm-stdin': { type: 'boolean' }, 'expected-source': { type: 'string' }
} });
const seconds = Number(values.seconds), readLimit = Number(values['read-limit']);
if (!values.command || !Number.isSafeInteger(seconds) || seconds < 1 || seconds > 600 ||
    !Number.isSafeInteger(readLimit) || readLimit < 1 || readLimit > 1000000) throw new Error('Explicit bounded probe arguments required');
if (!['local', 'http-model'].includes(values.transport) || values.transport === 'http-model' && values.cached ||
    values.frontier && values.transport !== 'http-model') throw new Error('Choose local storage OR modeled HTTP; frontier is HTTP-only');
if (!!values['wasm-directory'] === !!values['wasm-stdin'] ||
    values['wasm-stdin'] && !/^[a-f0-9]{40}$/.test(values['expected-source'] ?? '')) throw new Error('Choose a WASM directory or bounded stdin with an exact expected source');
let streamed = null;
if (values['wasm-stdin']) {
  // CI ZIP entries can be passed in memory. This avoids creating a second
  // experimental build directory just to measure an already built artifact.
  const chunks = []; let total = 0;
  for await (const chunk of process.stdin) {
    total += chunk.length;
    if (total > 64 * 1024 * 1024) throw new Error('WASM input exceeds benchmark bound');
    chunks.push(chunk);
  }
  streamed = JSON.parse(Buffer.concat(chunks).toString('utf8'));
}
const dataset = await openBenchmarkDataset(values.directory, values.profile);
const wasmDirectory = values['wasm-directory'] ? resolve(values['wasm-directory']) : null;
const manifest = streamed?.manifest ?? JSON.parse(await readFile(join(wasmDirectory, 'clearra_wasm.manifest.json'), 'utf8'));
if (values['expected-source'] && (manifest.build?.runtime_identity?.source_commit !== values['expected-source'] ||
    manifest.build?.runtime_identity?.engine_build_id !== values['expected-source'])) throw new Error('Unexpected benchmark artifact source');
async function verified(artifact) {
  if (basename(artifact.path) !== artifact.path || !/^[a-zA-Z0-9_.-]+$/.test(artifact.path)) throw new Error('Invalid WASM artifact path');
  const bytes = streamed ? Buffer.from(streamed.files[artifact.path], 'base64') : await readFile(join(wasmDirectory, artifact.path));
  if (bytes.length !== artifact.bytes || createHash('sha256').update(bytes).digest('hex') !== artifact.sha256) throw new Error('WASM artifact integrity mismatch');
  return bytes;
}
const prepare = performance.now();
const bindingBytes = await verified(manifest.bindings), wasmBytes = await verified(manifest.wasm);
streamed = null;
const bindings = await import(`data:text/javascript;base64,${bindingBytes.toString('base64')}`);
const raw = await bindings.default({ module_or_path: await WebAssembly.compile(wasmBytes) });
const decoder = new TextDecoder(), encoder = new TextEncoder();
const output = () => {
  try { return decoder.decode(new Uint8Array(raw.memory.buffer, raw.clearra_wasm_output_ptr() >>> 0, raw.clearra_wasm_output_len() >>> 0)); }
  finally { raw.clearra_wasm_output_release(); }
};
const ok = status => { if (status < 0) throw new Error(output()); return status; };
const input = text => { const bytes = encoder.encode(text); ok(raw.clearra_wasm_input_resize(bytes.length)); new Uint8Array(raw.memory.buffer, raw.clearra_wasm_input_ptr() >>> 0, bytes.length).set(bytes); };
ok(raw.clearra_wasm_configure_host(1, 0));
input(JSON.stringify(dataset.generation)); ok(raw.clearra_wasm_online_pc4_configure());
const preparationMs = performance.now() - prepare;
let reader = dataset;
if (values.cached) {
  const { createPc4LocalReader } = await import('../release/pc4/pc4-local-reader.mjs');
  reader = createPc4LocalReader(dataset.plan.files, (artifact, offset, length) => dataset.read(artifact, offset, length),
    { pageBytes: Number(values['page-bytes']), directPaths: [dataset.plan.files[2].path] });
}
if (values.transport === 'http-model') {
  // Real product transport and real WASM-produced hints, but exact local
  // responses. This measures causal request/byte counts, NOT internet latency.
  reader = createPc4RangeReader(dataset.generation, { ...pc4SearchRangePolicy(dataset.generation, values.profile),
    fetcher: async (url, init) => {
      const artifact = dataset.plan.files.find(file => url ===
        `https://huggingface.co/datasets/${dataset.generation.repository}/resolve/${dataset.plan.revision}/${file.path}`);
      const match = /^bytes=(\d+)-(\d+)$/.exec(init.headers.Range);
      if (!artifact || !match || init.credentials !== 'omit') throw new Error('Modeled transport identity mismatch');
      const offset = Number(match[1]), length = Number(match[2]) - offset + 1;
      return new Response(await dataset.read(artifact, offset, length), {
        status: 206, headers: { 'content-range': `bytes ${offset}-${offset + length - 1}/${artifact.byte_length}` }
      });
    } });
}
const demandDigest = createHash('sha256'), artifactCounts = {}, started = performance.now();
let job, steps = 0, reads = 0, computeMs = 0, ioMs = 0, bridgeMs = 0, terminal = null, lastReport = started, cancelled = false;
// One bounded local-only trace for the next HTTP planner experiment. Persist
// after timing, not during reads; never record user input text or payloads.
const trace = values.trace ? Buffer.alloc(Math.min(readLimit, 200000) * 12) : null;
let traceHandle, tracePath, traceCommitted = false, traceCount = 0;
let comparison = null;
let frontiers = 0, hintedIds = 0, maximumFrontier = 0;
const progress = () => ({ elapsed_ms: performance.now() - started, steps, logical_reads: reads, file_reads: dataset.calls,
  file_bytes: dataset.bytes, compute_ms: computeMs, io_ms: ioMs, bridge_ms: bridgeMs, artifact_reads: artifactCounts,
  wasm_memory_bytes: raw.memory.buffer.byteLength, cache_hits: reader.cacheHits ?? 0,
  ...(values.transport === 'http-model' ? { modeled_http_requests: reader.requests, modeled_http_bytes: reader.bytes,
    frontier_hints: frontiers, hinted_ids: hintedIds, maximum_frontier: maximumFrontier } : {}) });
try {
  if (values['compare-trace']) {
    const root = await checkedDatasetRoot(values.directory, values.profile);
    const path = join(root, 'lookup-trace.bin'), stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 4 || stat.size > 2531076) throw new Error('Invalid trace file');
    const input = await readFile(path);
    const size = input.readUInt32LE(0);
    if (size > 131072) throw new Error('Invalid trace header');
    const header = JSON.parse(input.subarray(4, 4 + size));
    const reference = input.subarray(4 + size);
    if (header.schema !== 'clearra.pc4.lookup-trace.v1' || header.record_count > 200000 ||
        reference.length !== header.record_count * 12 || header.body_sha256 !== createHash('sha256').update(reference).digest('hex') ||
        header.command_sha256 !== createHash('sha256').update(values.command).digest('hex') ||
        JSON.stringify(header.generation) !== JSON.stringify(dataset.generation)) throw new Error('Trace identity or bytes mismatch');
    comparison = createPc4TraceComparison(reference, dataset.plan.files);
  }
  if (trace) {
    tracePath = join(await checkedDatasetRoot(values.directory, values.profile), 'lookup-trace.bin');
    traceHandle = await open(tracePath, 'wx'); // existing evidence is reused, never overwritten
  }
  input(values.command); job = raw.clearra_wasm_start_job(); if (!job) throw new Error(output());
  for (;;) {
    if (performance.now() - started > seconds * 1000 || reads >= readLimit) {
      cancelled = true; ok(raw.clearra_wasm_cancel_job(job));
      // Cancellation retires the job immediately; drain its terminal event,
      // never advance a no-longer-active job.
      ok(raw.clearra_wasm_drain_job_events(job));
      terminal = JSON.parse(output()).findLast(e => ['final_response', 'failed', 'cancelled'].includes(e.event)) ?? null;
      break;
    }
    let at = performance.now();
    const status = ok(raw.clearra_wasm_advance_job(job, 2048)); steps++; computeMs += performance.now() - at;
    if (status === 0 || status === 4) {
      at = performance.now(); ok(raw.clearra_wasm_online_pc4_pending(job)); const range = JSON.parse(output()); bridgeMs += performance.now() - at;
      if (range) {
        at = performance.now();
        if (values.frontier) {
          if (!Array.isArray(range.lookup_frontier)) throw new Error('This WASM does not expose frontier hints; rebuild in trusted CI');
          if (range.lookup_frontier.length >= 2 && range.artifact.path === dataset.plan.files[1].path && range.length === 8 && range.offset >= 16) {
            frontiers++; hintedIds += range.lookup_frontier.length;
            maximumFrontier = Math.max(maximumFrontier, range.lookup_frontier.length);
          }
          await prefetchPc4LookupFrontier(reader, dataset.generation, range);
        }
        const bytes = await reader.read(range.artifact, range.offset, range.length); ioMs += performance.now() - at;
        reads++; artifactCounts[range.artifact.path] = (artifactCounts[range.artifact.path] ?? 0) + 1;
        demandDigest.update(`${range.artifact.path}:${range.offset}:${range.length}\n`).update(bytes);
        comparison?.observe(range.artifact, range.offset, range.length, bytes);
        if (trace && traceCount < trace.length / 12) {
          const role = dataset.plan.files.findIndex(file => file.path === range.artifact.path);
          if (role < 0 || !Number.isSafeInteger(range.offset) || range.offset > 0xffffffff) throw new Error('Trace address outside v1 format');
          trace.writeUInt32LE(role, traceCount * 12);
          trace.writeUInt32LE(range.offset, traceCount * 12 + 4);
          trace.writeUInt32LE(range.length, traceCount * 12 + 8); traceCount++;
        }
        at = performance.now(); input(JSON.stringify({ lookup_session: range.lookup_session, request_id: range.request_id,
          ...(values.transport === 'local' ? { source: 'verified-local-file' } : { status: 206,
            content_range: `bytes ${range.offset}-${range.offset + range.length - 1}/${range.artifact.byte_length}` }), bytes: Array.from(bytes) }));
        ok(raw.clearra_wasm_online_pc4_admit(job)); bridgeMs += performance.now() - at;
      }
    }
    if (status !== 0 || steps % 16 === 0) {
      ok(raw.clearra_wasm_drain_job_events(job));
      const events = JSON.parse(output());
      terminal = events.findLast(e => ['final_response', 'failed', 'cancelled'].includes(e.event)) ?? terminal;
    }
    if (performance.now() - lastReport >= 10000) { console.log(JSON.stringify({ event: 'progress', ...progress() })); lastReport = performance.now(); }
    if (terminal || ![0, 4].includes(status)) break;
  }
  const report = { event: 'result', transport: values.transport, frontier: !!values.frontier,
    evidence: values.transport === 'http-model' ? 'real-wasm-frontier-local-responses-not-internet-timing' : 'real-wasm-local-files',
    cached: !!values.cached, page_bytes: values.cached ? Number(values['page-bytes']) : 0, source_commit: manifest.build?.runtime_identity?.source_commit,
    wasm_sha256: manifest.wasm.sha256, dataset_revision: dataset.plan.revision, module_prepare_ms: preparationMs,
    ...progress(), cancelled_by_probe: cancelled, demand_sha256: demandDigest.digest('hex'),
    ...(comparison ? comparison.finish() : {}),
    terminal: terminal ? { event: terminal.event, status: terminal.response?.status,
      diagnostics: terminal.response?.diagnostics, error: terminal.error } : null };
  console.log(JSON.stringify(report));
  if (traceHandle) {
    const body = trace.subarray(0, traceCount * 12);
    const header = Buffer.from(JSON.stringify({ schema: 'clearra.pc4.lookup-trace.v1', record_count: traceCount,
      command_sha256: createHash('sha256').update(values.command).digest('hex'),
      generation: dataset.generation, measurement: report,
      body_sha256: createHash('sha256').update(body).digest('hex') }));
    const size = Buffer.alloc(4); size.writeUInt32LE(header.length);
    await traceHandle.writeFile(Buffer.concat([size, header, body])); await traceHandle.sync(); traceCommitted = true;
    console.log(JSON.stringify({ event: 'trace-saved', records: traceCount, bytes: 4 + header.length + body.length, path: tracePath }));
  }
  if (terminal?.event === 'failed') process.exitCode = 1;
} finally {
  if (traceHandle) { await traceHandle.close(); if (!traceCommitted) await unlink(tracePath); }
  reader.dispose?.();
  await dataset.close();
}
