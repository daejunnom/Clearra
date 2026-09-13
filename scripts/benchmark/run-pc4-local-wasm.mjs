// Local-only measurement of the real App/WASM + downloaded graph. Does not
// start a server, change 4194, rebuild Rust, publish, or assert release success.
import { readFile } from 'node:fs/promises';
import { resolve, join, basename } from 'node:path';
import { parseArgs } from 'node:util';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { openBenchmarkDataset } from './pc4-local-dataset.mjs';

const { values } = parseArgs({ options: {
  directory: { type: 'string' }, profile: { type: 'string' }, 'wasm-directory': { type: 'string' },
  command: { type: 'string' }, seconds: { type: 'string', default: '60' },
  'read-limit': { type: 'string', default: '100000' }, cached: { type: 'boolean' },
  'page-bytes': { type: 'string', default: '4096' }
} });
const seconds = Number(values.seconds), readLimit = Number(values['read-limit']);
if (!values.command || !Number.isSafeInteger(seconds) || seconds < 1 || seconds > 600 ||
    !Number.isSafeInteger(readLimit) || readLimit < 1 || readLimit > 1000000) throw new Error('Explicit bounded probe arguments required');
const dataset = await openBenchmarkDataset(values.directory, values.profile);
const wasmDirectory = resolve(values['wasm-directory']);
const manifest = JSON.parse(await readFile(join(wasmDirectory, 'clearra_wasm.manifest.json'), 'utf8'));
async function verified(artifact) {
  if (basename(artifact.path) !== artifact.path || !/^[a-zA-Z0-9_.-]+$/.test(artifact.path)) throw new Error('Invalid WASM artifact path');
  const bytes = await readFile(join(wasmDirectory, artifact.path));
  if (bytes.length !== artifact.bytes || createHash('sha256').update(bytes).digest('hex') !== artifact.sha256) throw new Error('WASM artifact integrity mismatch');
  return bytes;
}
const prepare = performance.now();
const bindingBytes = await verified(manifest.bindings), wasmBytes = await verified(manifest.wasm);
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
const demandDigest = createHash('sha256'), artifactCounts = {}, started = performance.now();
let job, steps = 0, reads = 0, computeMs = 0, ioMs = 0, bridgeMs = 0, terminal = null, lastReport = started, cancelled = false;
const progress = () => ({ elapsed_ms: performance.now() - started, steps, logical_reads: reads, file_reads: dataset.calls,
  file_bytes: dataset.bytes, compute_ms: computeMs, io_ms: ioMs, bridge_ms: bridgeMs, artifact_reads: artifactCounts,
  wasm_memory_bytes: raw.memory.buffer.byteLength, cache_hits: reader.cacheHits ?? 0 });
try {
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
        at = performance.now(); const bytes = await reader.read(range.artifact, range.offset, range.length); ioMs += performance.now() - at;
        reads++; artifactCounts[range.artifact.path] = (artifactCounts[range.artifact.path] ?? 0) + 1;
        demandDigest.update(`${range.artifact.path}:${range.offset}:${range.length}\n`).update(bytes);
        at = performance.now(); input(JSON.stringify({ lookup_session: range.lookup_session, request_id: range.request_id, source: 'verified-local-file', bytes: Array.from(bytes) }));
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
  console.log(JSON.stringify({ event: 'result', cached: !!values.cached, page_bytes: values.cached ? Number(values['page-bytes']) : 0, source_commit: manifest.build?.runtime_identity?.source_commit,
    wasm_sha256: manifest.wasm.sha256, dataset_revision: dataset.plan.revision, module_prepare_ms: preparationMs,
    ...progress(), cancelled_by_probe: cancelled, demand_sha256: demandDigest.digest('hex'),
    terminal: terminal ? { event: terminal.event, status: terminal.response?.status,
      diagnostics: terminal.response?.diagnostics, error: terminal.error } : null }));
} finally {
  reader.dispose?.();
  await dataset.close();
}
