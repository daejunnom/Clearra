// SRP rationale: prove the real scalar WASM accelerator/solver boundary across
// independent linear memories. Node workers supply realm isolation, not a
// browser transport, an alternative solver or benchmark instrumentation.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { availableParallelism } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isMainThread, parentPort, Worker, workerData } from 'node:worker_threads';
import test from 'node:test';
import { createClearraWasmBuildContract, clearraWasmBuildContractsEqual }
  from '../../../scripts/tools/clearra-wasm-build-contract.mjs';

const wasmRoot = process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR;
const assetRoot = process.env.CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR;
const profiles = ['srs-plus', 'srs', 'srs-x', 'jstris-180', 'no-kick'];
const profileIds = { srs: 0, 'srs-plus': 1, 'srs-x': 2, 'jstris-180': 3, 'no-kick': 4 };
const maximumSlices = 4096;

class ScalarRealm {
  constructor(raw, identity) {
    this.raw = raw;
    this.identity = identity;
    this.ok(raw.clearra_wasm_configure_host(availableParallelism(), 0));
    this.ok(raw.clearra_wasm_configure_product_retention(64 * 1024 * 1024));
  }
  output() {
    const raw = this.raw;
    try {
      assert.equal(raw.clearra_wasm_output_len_exact(), 1);
      return new Uint8Array(raw.memory.buffer,
        raw.clearra_wasm_output_ptr() >>> 0, raw.clearra_wasm_output_len() >>> 0).slice();
    } finally { raw.clearra_wasm_output_release(); }
  }
  ok(status) {
    if (status < 0) throw new Error(new TextDecoder().decode(this.output()));
    return status;
  }
  mutation(name, ...args) {
    this.ok(this.raw[name](...args));
    return this.output();
  }
  input(text) {
    const bytes = new TextEncoder().encode(text);
    this.ok(this.raw.clearra_wasm_input_resize(bytes.length));
    new Uint8Array(this.raw.memory.buffer, this.raw.clearra_wasm_input_ptr() >>> 0,
      bytes.length).set(bytes);
  }
  transfer(bytes, peer) {
    if (peer) assert.ok(bytes.length <= 256 * 1024);
    this.ok(this.raw[peer ? 'clearra_wasm_accelerator_peer_transfer_resize'
      : 'clearra_wasm_transfer_resize'](bytes.length));
    new Uint8Array(this.raw.memory.buffer, this.raw.clearra_wasm_transfer_ptr() >>> 0,
      bytes.length).set(bytes);
  }
  admit(profile, bytes, peer = false) {
    this.transfer(bytes, peer);
    this.mutation(peer ? 'clearra_wasm_accelerator_peer_admit'
      : 'clearra_wasm_accelerator_admit', ...(peer ? [profile, 1024 * 1024] : [1, profile, 1]));
  }
  run(command, cancel = false) {
    this.input(command);
    const job = this.raw.clearra_wasm_start_job();
    assert.ok(job > 0, new TextDecoder().decode(job === 0 ? this.output() : new Uint8Array()));
    if (cancel) {
      // The product controller cancels and drains: cancellation releases the
      // scope immediately, so another advance would be an invalid job access.
      this.ok(this.raw.clearra_wasm_cancel_job(job));
      const events = JSON.parse(new TextDecoder().decode(this.mutation('clearra_wasm_drain_job_events', job)));
      const cancelled = events.filter(event => event.event === 'cancelled');
      assert.equal(cancelled.length, 1);
      assert.equal(cancelled[0].job_id, job);
      assert.equal(cancelled[0].scope_released, true);
      assert.ok(!events.some(event => ['failed', 'final_response'].includes(event.event)));
      return { cancelled: true };
    }
    const events = [];
    for (let slice = 0; slice < maximumSlices; slice++) {
      const status = this.ok(this.raw.clearra_wasm_advance_job(job, 8192));
      const wire = this.mutation('clearra_wasm_drain_job_events', job);
      events.push(...JSON.parse(new TextDecoder().decode(wire)));
      if ([1, 2, 3].includes(status)) {
        assert.equal(status, 1, JSON.stringify(events));
        const terminals = events.filter(event => event.event === 'final_response');
        assert.equal(terminals.length, 1);
        const final = terminals[0];
        assert.equal(final.response.status, 'success', JSON.stringify(final));
        assert.deepEqual(final.response.runtime_identity, this.identity);
        const report = final.search_report;
        assert.ok(report && Array.isArray(report.normalized_solution_keys));
        assert.ok(report.normalized_solution_keys.length > 0);
        for (const field of ['solution_count_calculated', 'solution_set_materialized',
          'solution_keys_complete', 'coverage_calculated', 'probability_calculated',
          'probability_complete', 'count_complete']) assert.equal(report[field], true, field);
        assert.equal(report.unique_solution_count, report.normalized_solution_keys.length);
        assert.equal(report.solution_keys_materialized_count, report.normalized_solution_keys.length);
        assert.equal(typeof report.normalized_solution_set_hash, 'string');
        assert.ok(report.normalized_solution_set_hash.length > 0);
        assert.equal(typeof report.total_possible_pattern_count, 'string');
        assert.ok(Number(report.total_possible_pattern_count) > 0);
        assert.ok(Number.isSafeInteger(report.covered_pattern_count));
        assert.equal(typeof report.coverage_probability, 'string');
        assert.equal(report.solution_probabilities.length, report.normalized_solution_keys.length);
        assert.equal(new Set(report.normalized_solution_keys).size, report.normalized_solution_keys.length);
        return {
          keys: report.normalized_solution_keys,
          setHash: report.normalized_solution_set_hash,
          covered: report.covered_pattern_count,
          total: report.total_possible_pattern_count,
          probability: report.coverage_probability,
          solutionProbabilities: report.solution_probabilities
        };
      }
    }
    throw new Error('the small functional fixture exceeded its finite slice bound');
  }
  handle(message) {
    const { operation, profile } = message;
    if (operation === 'admit') { this.admit(profile, message.seed, true); return true; }
    if (operation === 'run') return this.run(message.command, message.cancel);
    if (operation === 'drain') return this.mutation('clearra_wasm_accelerator_peer_drain', profile);
    if (operation === 'import') {
      this.transfer(message.wire, true);
      this.mutation('clearra_wasm_accelerator_peer_import', profile);
      return true;
    }
    if (operation === 'remove') { this.mutation('clearra_wasm_accelerator_remove', 1, profile); return true; }
    throw new Error(`unsupported smoke operation: ${operation}`);
  }
}

async function instantiate(bindings, compiled, identity) {
  const exports = await import(bindings);
  return new ScalarRealm(await exports.default({ module_or_path: compiled }), identity);
}

class PeerClient {
  constructor(bindings, compiled, identity) {
    this.worker = new Worker(new URL(import.meta.url), { workerData: { bindings, compiled, identity } });
    this.pending = new Map();
    this.failure = null;
    this.nextId = 1;
    this.worker.on('message', message => {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.value);
    });
    this.worker.on('error', error => this.fail(error));
    this.worker.on('exit', code => this.fail(new Error(`owned WASM peer exited: ${code}`)));
  }
  fail(error) {
    this.failure ??= error;
    for (const pending of this.pending.values()) pending.reject(this.failure);
    this.pending.clear();
  }
  call(operation, fields = {}) {
    if (this.failure) return Promise.reject(this.failure);
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      try { this.worker.postMessage({ id, operation, ...fields }); }
      catch (error) { this.pending.delete(id); reject(error); }
    });
  }
  async stop() { await this.worker.terminate(); }
}

function command(profile, active) {
  return `clearra pc --lines 4 --board-mask 0x3c0f03c0f --height 4 --pieces 6 ` +
    `--patterns P7 --hold empty --objective unique --count unique --solution-probabilities ` +
    `--backend cpu --workers 1 --rule ${profile} --no-tablebase --no-legal-board ` +
    (active ? '--conditioned-reachability' : '--no-conditioned-reachability');
}

async function exchange(owner, peer, profile) {
  let batches = 0;
  for (; batches < 64; batches++) {
    const query = await peer.call('drain', { profile });
    if (query.length === 0) return batches;
    assert.ok(query.length <= 256 * 1024);
    owner.transfer(query, true);
    const wire = owner.mutation('clearra_wasm_accelerator_peer_answer', profile);
    assert.ok(wire.length > 0 && wire.length <= 256 * 1024);
    await peer.call('import', { profile, wire });
  }
  throw new Error('the bounded peer queue did not drain');
}

if (!isMainThread) {
  const realm = await instantiate(workerData.bindings, workerData.compiled, workerData.identity);
  let queue = Promise.resolve();
  parentPort.on('message', message => {
    queue = queue.then(() => {
      try { parentPort.postMessage({ id: message.id, value: realm.handle(message) }); }
      catch (error) { parentPort.postMessage({ id: message.id, error: String(error.stack ?? error) }); }
    });
  });
} else {
  test('qualified relation owner and two real WASM peer realms preserve solver results and lifetime', {
    skip: wasmRoot && assetRoot ? false : 'requires explicit current-source WASM and existing signed packs',
    timeout: 180_000
  }, async context => {
    const repository = await realpath(fileURLToPath(new URL('../../../', import.meta.url)));
    const root = await realpath(wasmRoot);
    assert.equal(root, join(repository, '_local', 'artifacts', 'v081-browser-peer-smoke', 'wasm'));
    const assets = await realpath(assetRoot);
    assert.equal(assets, join(repository, '_local', 'artifacts', 'v081-peer-signed-smoke'));
    const manifest = JSON.parse(await readFile(join(root, 'clearra_wasm.manifest.json'), 'utf8'));
    assert.ok(clearraWasmBuildContractsEqual(manifest.build, await createClearraWasmBuildContract(repository)));
    const artifact = async (entry, extension) => {
      assert.match(entry.path, new RegExp(`^clearra_wasm(?:_bg)?\\.[0-9a-f]{24}\\.${extension}$`, 'u'));
      const bytes = await readFile(join(root, entry.path));
      assert.equal(bytes.length, entry.bytes);
      assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.sha256);
      return bytes;
    };
    const wasm = await artifact(manifest.wasm, 'wasm');
    await artifact(manifest.bindings, 'js');
    const bindings = pathToFileURL(join(root, manifest.bindings.path)).href;
    const compiled = await WebAssembly.compile(wasm);
    const identity = manifest.build.runtime_identity;
    const owner = await instantiate(bindings, compiled, identity);
    const peers = [new PeerClient(bindings, compiled, identity), new PeerClient(bindings, compiled, identity)];
    let totalBatches = 0;
    try {
      for (const name of profiles) {
        const startBatches = totalBatches;
        const profile = profileIds[name];
        const baseline = owner.run(command(name, false));
        const bytes = await readFile(join(assets, `conditioned-${name}.cllr`));
        const plan = JSON.parse(new TextDecoder().decode(owner.mutation('clearra_wasm_accelerator_catalog', 1, profile)));
        assert.equal(plan.state, 'qualified');
        assert.equal(plan.profile, name);
        assert.equal(bytes.length, plan.payload_bytes);
        assert.equal(createHash('sha256').update(bytes).digest('hex'), plan.payload_identity);
        owner.admit(profile, bytes);
        const seed = owner.mutation('clearra_wasm_accelerator_peer_seed', profile);
        assert.ok(seed.length > 0 && seed.length <= 256 * 1024 && seed.length < bytes.length);
        assert.deepEqual(owner.run(command(name, true)), baseline);
        for (const peer of peers) {
          await peer.call('admit', { profile, seed });
          assert.deepEqual(await peer.call('run', { command: command(name, true) }), baseline);
          totalBatches += await exchange(owner, peer, profile);
          assert.deepEqual(await peer.call('run', { command: command(name, true) }), baseline);
          totalBatches += await exchange(owner, peer, profile);
          if (name === 'srs-plus') {
            assert.deepEqual(await peer.call('run', { command: command(name, true), cancel: true }), { cancelled: true });
            assert.deepEqual(await peer.call('run', { command: command(name, true) }), baseline);
            totalBatches += await exchange(owner, peer, profile);
          }
          await peer.call('remove', { profile });
          assert.deepEqual(await peer.call('run', { command: command(name, false) }), baseline);
        }
        owner.mutation('clearra_wasm_accelerator_remove', 1, profile);
        assert.deepEqual(owner.run(command(name, false)), baseline);
        assert.ok(totalBatches > startBatches, `${name}: the real solver must exchange peer queries`);
        context.diagnostic(`${name}: ${baseline.keys.length} exact identities, ` +
          `${totalBatches - startBatches} bounded peer query/reply batches`);
      }
      assert.ok(totalBatches > 0, 'real BuildUp must exercise peer query/reply, not only admission');
    } finally { await Promise.all(peers.map(peer => peer.stop())); }
  });
}
