import fs from 'node:fs';
import { dirname, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import {
  Worker,
  isMainThread,
  parentPort,
  workerData,
} from 'node:worker_threads';
import {
  decodeWasmDistributedPreparationMode,
  dispatchWasmDistributedPreparation,
} from './wasm-distributed-preparation-mode.mjs';

const DISTRIBUTED_BATCH_CAPACITY = 16;
const SERIAL_EVENT_DRAIN_INTERVAL = 8;

async function runProbe() {
  const [bindingsPath, commandText, workBudgetText = '8192'] = process.argv.slice(2);
  if (!bindingsPath || !commandText) {
    throw new Error(
      'usage: node wasm-pc-distributed-environment-probe.mjs CLEARRA_WASM_BINDINGS "clearra pc ..." [WORK_BUDGET]',
    );
  }
  const workBudget = positiveInteger(workBudgetText, 'WORK_BUDGET');
  const logicalProcessors = Math.max(1, globalThis.navigator?.hardwareConcurrency ?? 1);
  const loadStarted = performance.now();
  const { api, wasmBytes } = await loadApi(bindingsPath);
  api.configureHost(logicalProcessors, 0);
  const loadElapsedMs = performance.now() - loadStarted;

  writeInput(api, commandText);
  const prepareStarted = performance.now();
  const distributedMode = api.distributedPrepare();
  requireStatus(api, distributedMode);
  const preparationMode = decodeWasmDistributedPreparationMode(distributedMode);
  const prepareElapsedMs = performance.now() - prepareStarted;
  const requestedBackend = api.distributedRequestedBackend();
  const preparationFallbackReason = api.distributedFallbackReason();
  const searchStarted = performance.now();
  const outcome = await dispatchWasmDistributedPreparation(preparationMode, {
    serial: () => runSerial(api, commandText, workBudget),
    ready: () => finishPreparedResult(api),
    distributed: () => runDistributed(
      api,
      bindingsPath,
      commandText,
      workBudget,
      logicalProcessors,
    ),
  });
  const searchElapsedMs = performance.now() - searchStarted;
  const finalEvent = outcome.events.find((event) => event.event === 'final_response') ?? null;
  const failedEvent = outcome.events.find((event) => event.event === 'failed') ?? null;
  const cancelledEvent = outcome.events.find((event) => event.event === 'cancelled') ?? null;

  console.log(JSON.stringify({
    runtime: 'node-wasm-distributed-host',
    bindings_path: resolve(bindingsPath),
    wasm_path: resolve(dirname(bindingsPath), 'clearra_wasm_bg.wasm'),
    wasm_file_bytes: wasmBytes.byteLength,
    wasm_memory_bytes: api.memory.buffer.byteLength + outcome.verifierMemoryBytes,
    logical_processors: logicalProcessors,
    load_elapsed_ms: loadElapsedMs,
    prepare_elapsed_ms: prepareElapsedMs,
    search_elapsed_ms: searchElapsedMs,
    advance_calls: outcome.advanceCalls,
    work_budget: workBudget,
    distributed_mode: preparationMode.label,
    requested_backend_code: requestedBackend,
    preparation_fallback_reason_code: preparationFallbackReason,
    command: commandText,
    final_event: compactTerminalEvent(finalEvent),
    failed_event: failedEvent,
    cancelled_event: cancelledEvent,
  }, null, 2));
}

function compactTerminalEvent(event) {
  const searchReport = event?.search_report;
  if (!searchReport) return event;
  const {
    normalized_solution_keys: _normalizedSolutionKeys,
    packing_candidate_keys: _packingCandidateKeys,
    ...compactSearchReport
  } = searchReport;
  return { ...event, search_report: compactSearchReport };
}

function runSerial(api, commandText, workBudget) {
  writeInput(api, commandText);
  const jobId = api.startJob();
  if (jobId === 0) throw new Error(readOutput(api));
  let status = 0;
  let advanceCalls = 0;
  let advancesSinceDrain = 0;
  const terminalEvents = [];
  drainSerialEvents(api, jobId, terminalEvents);
  while (status === 0 || status === 4) {
    status = api.advanceJob(jobId, workBudget);
    requireStatus(api, status);
    if (![0, 1, 2, 3, 4].includes(status)) {
      throw new Error(`invalid Clearra WASM job status ${status}`);
    }
    advanceCalls += 1;
    advancesSinceDrain += 1;
    if (
      status === 4 ||
      (status !== 0 && status !== 4) ||
      advancesSinceDrain >= SERIAL_EVENT_DRAIN_INTERVAL
    ) {
      drainSerialEvents(api, jobId, terminalEvents);
      advancesSinceDrain = 0;
    }
  }
  return {
    events: terminalEvents,
    advanceCalls,
    verifierMemoryBytes: 0,
  };
}

function drainSerialEvents(api, jobId, terminalEvents) {
  requireStatus(api, api.drainJobEvents(jobId));
  const events = JSON.parse(readOutput(api));
  if (!Array.isArray(events)) {
    throw new Error('Clearra WASM serial job returned a non-array event payload');
  }
  for (const event of events) {
    if (
      event?.event === 'final_response' ||
      event?.event === 'failed' ||
      event?.event === 'cancelled' ||
      event?.event === 'terminated'
    ) {
      terminalEvents.push(event);
    }
  }
}

function finishPreparedResult(api) {
  requireStatus(api, api.distributedFinish(1, 0));
  return {
    events: JSON.parse(readOutput(api)),
    advanceCalls: 0,
    verifierMemoryBytes: 0,
  };
}

async function runDistributed(api, bindingsPath, commandText, workBudget, logicalProcessors) {
  const workerCount = Math.max(2, api.distributedWorkerCount());
  const pool = new VerifierPool(
    bindingsPath,
    commandText,
    workerCount - 1,
    logicalProcessors,
  );
  let advanceCalls = 0;
  try {
    await pool.ready();
    for (;;) {
      const status = api.distributedProduce(workBudget, DISTRIBUTED_BATCH_CAPACITY);
      requireStatus(api, status);
      advanceCalls += 1;
      if (status === 1) {
        await pool.enqueue(readOutputBytes(api));
      } else if (status === 2) {
        break;
      } else if (status === 3) {
        throw new Error('distributed search cancelled');
      } else {
        await new Promise((resolveImmediate) => setImmediate(resolveImmediate));
      }
    }
    const partials = await pool.finish();
    for (const { partial } of partials) {
      writeTransfer(api, partial);
      requireStatus(api, api.distributedMergePartial());
    }
    requireStatus(api, api.distributedFinish(1, workerCount));
    return {
      events: JSON.parse(readOutput(api)),
      advanceCalls,
      verifierMemoryBytes: partials.reduce((sum, item) => sum + item.memoryBytes, 0),
    };
  } finally {
    await pool.close();
  }
}

class VerifierPool {
  constructor(bindingsPath, commandText, size, logicalProcessors) {
    this.clients = Array.from(
      { length: size },
      () => new VerifierClient(bindingsPath, commandText, logicalProcessors),
    );
    this.available = [];
    this.waiters = [];
    this.inFlight = new Set();
  }

  async ready() {
    await Promise.all(this.clients.map((client) => client.ready));
    this.available.push(...this.clients);
  }

  async enqueue(batch) {
    const client = await this.acquire();
    const operation = client.consume(batch);
    this.inFlight.add(operation);
    void operation.finally(() => {
      this.inFlight.delete(operation);
      const waiter = this.waiters.shift();
      if (waiter) waiter(client);
      else this.available.push(client);
    });
  }

  async finish() {
    await Promise.all([...this.inFlight]);
    return Promise.all(this.clients.map((client) => client.finish()));
  }

  async close() {
    await Promise.all(this.clients.map((client) => client.close()));
  }

  acquire() {
    const client = this.available.pop();
    return client ? Promise.resolve(client) : new Promise((resolveClient) => {
      this.waiters.push(resolveClient);
    });
  }
}

class VerifierClient {
  constructor(bindingsPath, commandText, logicalProcessors) {
    this.nextRequestId = 1;
    this.pending = new Map();
    this.worker = new Worker(new URL(import.meta.url), {
      workerData: {
        role: 'verifier', bindingsPath: resolve(bindingsPath), commandText, logicalProcessors,
      },
    });
    this.ready = new Promise((resolveReady, rejectReady) => {
      this.resolveReady = resolveReady;
      this.rejectReady = rejectReady;
    });
    this.worker.on('message', (message) => this.onMessage(message));
    this.worker.on('error', (error) => this.rejectAll(error));
  }

  consume(batch) {
    return this.request('consume', { batch }, [batch.buffer]);
  }

  finish() {
    return this.request('finish');
  }

  async close() {
    this.rejectAll(new Error('verifier worker closed'));
    await this.worker.terminate();
  }

  request(type, fields = {}, transfer = []) {
    const requestId = this.nextRequestId++;
    return new Promise((resolveRequest, rejectRequest) => {
      this.pending.set(requestId, { resolveRequest, rejectRequest });
      this.worker.postMessage({ type, requestId, ...fields }, transfer);
    });
  }

  onMessage(message) {
    if (message.type === 'ready') {
      this.resolveReady();
      return;
    }
    if (message.type === 'startup-failed') {
      this.rejectReady(new Error(message.message));
      return;
    }
    const pending = this.pending.get(message.requestId);
    if (!pending) return;
    this.pending.delete(message.requestId);
    if (message.type === 'failed') pending.rejectRequest(new Error(message.message));
    else pending.resolveRequest(message.result);
  }

  rejectAll(error) {
    this.rejectReady(error);
    for (const pending of this.pending.values()) pending.rejectRequest(error);
    this.pending.clear();
  }
}

async function runVerifierWorker() {
  if (workerData?.role !== 'verifier') throw new Error('invalid verifier worker role');
  try {
    const { api } = await loadApi(workerData.bindingsPath);
    api.configureHost(Math.max(1, workerData.logicalProcessors ?? 1), 0);
    writeInput(api, workerData.commandText);
    requireStatus(api, api.verifierStart());
    parentPort.postMessage({ type: 'ready' });
    parentPort.on('message', (message) => {
      try {
        if (message.type === 'consume') {
          writeTransfer(api, new Uint8Array(message.batch));
          const consumed = api.verifierConsume();
          requireStatus(api, consumed);
          parentPort.postMessage({
            type: 'consumed', requestId: message.requestId, result: consumed,
          });
        } else if (message.type === 'finish') {
          requireStatus(api, api.verifierFinish());
          const partial = readOutputBytes(api);
          parentPort.postMessage({
            type: 'partial', requestId: message.requestId,
            result: { partial, memoryBytes: api.memory.buffer.byteLength },
          }, [partial.buffer]);
        }
      } catch (error) {
        parentPort.postMessage({
          type: 'failed', requestId: message.requestId,
          message: error instanceof Error ? error.message : String(error),
        });
      }
    });
  } catch (error) {
    parentPort.postMessage({
      type: 'startup-failed',
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

async function loadApi(bindingsPath) {
  const wasmPath = resolve(dirname(bindingsPath), 'clearra_wasm_bg.wasm');
  const wasmBytes = fs.readFileSync(wasmPath);
  const bindings = await import(pathToFileURL(resolve(bindingsPath)).href);
  const exports = await bindings.default({ module_or_path: wasmBytes });
  return { api: bindRawAbi(exports), wasmBytes };
}

function bindRawAbi(exports) {
  const names = {
    memory: 'memory', abiVersion: 'clearra_wasm_abi_version',
    configureHost: 'clearra_wasm_configure_host', inputResize: 'clearra_wasm_input_resize',
    inputPtr: 'clearra_wasm_input_ptr', transferResize: 'clearra_wasm_transfer_resize',
    transferPtr: 'clearra_wasm_transfer_ptr', distributedPrepare: 'clearra_wasm_distributed_prepare',
    distributedWorkerCount: 'clearra_wasm_distributed_worker_count',
    distributedRequestedBackend: 'clearra_wasm_distributed_requested_backend',
    distributedFallbackReason: 'clearra_wasm_distributed_preparation_fallback_reason',
    distributedProduce: 'clearra_wasm_distributed_produce',
    distributedMergePartial: 'clearra_wasm_distributed_merge_partial',
    distributedFinish: 'clearra_wasm_distributed_finish', verifierStart: 'clearra_wasm_distributed_verifier_start',
    verifierConsume: 'clearra_wasm_distributed_verifier_consume', verifierFinish: 'clearra_wasm_distributed_verifier_finish',
    startJob: 'clearra_wasm_start_job', advanceJob: 'clearra_wasm_advance_job',
    drainJobEvents: 'clearra_wasm_drain_job_events', outputPtr: 'clearra_wasm_output_ptr',
    outputLen: 'clearra_wasm_output_len', outputRelease: 'clearra_wasm_output_release',
  };
  const api = {};
  for (const [alias, exportName] of Object.entries(names)) {
    if (exports[exportName] === undefined) throw new Error(`Clearra WASM export missing: ${exportName}`);
    api[alias] = exports[exportName];
  }
  if (api.abiVersion() !== 1) throw new Error(`unsupported Clearra WASM ABI version ${api.abiVersion()}`);
  return api;
}

function writeInput(api, text) {
  const bytes = new TextEncoder().encode(text);
  requireStatus(api, api.inputResize(bytes.byteLength));
  new Uint8Array(api.memory.buffer, api.inputPtr(), bytes.byteLength).set(bytes);
}

function writeTransfer(api, bytes) {
  requireStatus(api, api.transferResize(bytes.byteLength));
  new Uint8Array(api.memory.buffer, api.transferPtr(), bytes.byteLength).set(bytes);
}

function readOutput(api) {
  try {
    return new TextDecoder().decode(new Uint8Array(api.memory.buffer, api.outputPtr(), api.outputLen()));
  } finally {
    api.outputRelease();
  }
}

function readOutputBytes(api) {
  try {
    return new Uint8Array(api.memory.buffer, api.outputPtr(), api.outputLen()).slice();
  } finally {
    api.outputRelease();
  }
}

function requireStatus(api, status) {
  if (status === -2) {
    api.outputRelease();
    throw new Error('E_WASM_OUTPUT_NOT_RELEASED: prior ABI output was not released');
  }
  if (status < 0) throw new Error(readOutput(api));
}

function positiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

if (isMainThread) {
  await runProbe();
} else {
  await runVerifierWorker();
}
