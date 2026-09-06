#!/usr/bin/env node
// Isolated, opt-in transcript cost probe. Reads one explicitly supplied saved
// completion response; performs no search, browser action, or product mutation.
// Output contains bounded numeric summaries only, never response/field content.
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { performance } from 'node:perf_hooks';
import { build } from 'esbuild';

const { values } = parseArgs({
  options: {
    'response-json': { type: 'string' },
    iterations: { type: 'string', default: '25' }
  }
});
if (!values['response-json']) {
  throw new Error('Usage: node scripts/tools/benchmark-wasm-terminal-response.mjs --response-json <saved-response.json> [--iterations 25]');
}
const iterations = Number(values.iterations);
if (!Number.isSafeInteger(iterations) || iterations < 1 || iterations > 100) {
  throw new Error('iterations must be an integer from 1 to 100');
}
const inputPath = resolve(values['response-json']);
const inputName = basename(inputPath);
if (!/\.json$/iu.test(inputName) || /(?:^\.env|credential|service[-_ ]?account|ssh[-_ ]?key|api[-_ ]?key)/iu.test(inputName)) {
  throw new Error('Provide a saved non-secret completion response JSON file');
}
const inputStat = await stat(inputPath);
if (!inputStat.isFile() || inputStat.size > 8 * 1024 * 1024) {
  throw new Error('The saved response must be a regular JSON file of at most 8 MiB');
}
const parsed = JSON.parse(await readFile(inputPath, 'utf8'));
// Stage-AB artifacts wrap the terminal event array alongside profiling data.
// Select only the terminal response; never time/stringify the enclosing log.
const terminalEvents = Array.isArray(parsed) ? parsed
  : Array.isArray(parsed?.terminal) ? parsed.terminal : null;
const envelope = terminalEvents
  ? terminalEvents.findLast((item) => item?.event === 'final_response' || item?.event === 'failed')
  : parsed;
const response = envelope?.response ?? envelope;
if (!response || typeof response !== 'object' || typeof response.status !== 'string' || !Array.isArray(response.diagnostics)) {
  throw new Error('Input must contain a completed App response or terminal worker event');
}

const repositoryRoot = fileURLToPath(new URL('../..', import.meta.url));
const bundled = await build({
  absWorkingDir: repositoryRoot,
  entryPoints: ['packages/clearra-ui/src/lib/wasm/wasmTerminalTranscript.ts'],
  bundle: true,
  format: 'esm',
  platform: 'node',
  target: 'node22',
  logLevel: 'silent',
  write: false
});
assert.equal(bundled.outputFiles.length, 1);
const { deferWasmTerminalResponse, formatWasmTerminalLine } = await import(
  `data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].contents).toString('base64')}`
);
const expected = JSON.stringify(response, null, 2);
const cachedEntry = deferWasmTerminalResponse(response);
assert.equal(formatWasmTerminalLine(cachedEntry), expected);
let checksum = 0;
const deadline = performance.now() + 30_000;

function measure(operation) {
  for (let warmup = 0; warmup < 3; warmup += 1) checksum += operation();
  const samples = [];
  for (let index = 0; index < iterations; index += 1) {
    if (performance.now() > deadline) throw new Error('Isolated probe exceeded its 30-second budget');
    const started = performance.now();
    checksum += operation();
    samples.push(performance.now() - started);
  }
  const ordered = [...samples].sort((left, right) => left - right);
  return {
    count: samples.length,
    min_ms: ordered[0],
    median_ms: ordered[Math.floor(ordered.length / 2)],
    p95_ms: ordered[Math.min(ordered.length - 1, Math.ceil(ordered.length * 0.95) - 1)],
    max_ms: ordered.at(-1),
    total_ms: samples.reduce((sum, value) => sum + value, 0)
  };
}

const report = {
  input_bytes: inputStat.size,
  pretty_json_characters: expected.length,
  iterations,
  eager_completion_format: measure(() => JSON.stringify(response, null, 2).length),
  deferred_completion_entry: measure(() => Number(deferWasmTerminalResponse(response).response === response)),
  first_terminal_display: measure(() => formatWasmTerminalLine(deferWasmTerminalResponse(response)).length),
  cached_terminal_display: measure(() => formatWasmTerminalLine(cachedEntry).length)
};
assert.ok(Number.isFinite(checksum));
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
