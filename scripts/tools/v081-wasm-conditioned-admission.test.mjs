import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dirname, '..', '..');
const artifactRoot = resolve(root, 'apps', 'clearra-web', 'static', 'wasm');
const encoder = new TextDecoder();
const commandEncoder = new TextEncoder();

test('exact WASM validates and installs every qualified conditioned profile independently', async () => {
  const manifest = JSON.parse(await readFile(
    resolve(artifactRoot, 'clearra_wasm.manifest.json'), 'utf8'
  ));
  const wasm = await readFile(resolve(artifactRoot, manifest.wasm.path));
  const bindings = await import(pathToFileURL(resolve(artifactRoot, manifest.bindings.path)).href);
  const raw = await bindings.default({ module_or_path: wasm });
  const outputText = () => {
    const pointer = raw.clearra_wasm_output_ptr() >>> 0;
    const length = raw.clearra_wasm_output_len() >>> 0;
    const text = encoder.decode(new Uint8Array(raw.memory.buffer, pointer, length));
    assert.equal(raw.clearra_wasm_output_release(), 0);
    return text;
  };
  const output = () => JSON.parse(outputText());

  // Native qualification has already proved the closed Boolean domain. This
  // checks the distinct browser ABI and signed-asset admission for all five
  // profiles without regenerating any candidate or treating it as a release.
  for (const [profile, name] of ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'].entries()) {
    assert.equal(raw.clearra_wasm_accelerator_catalog(1, profile), 0);
    const plan = output();
    assert.equal(plan.product, 'board-conditioned-reachability');
    assert.equal(plan.profile, name);
    assert.equal(plan.state, 'qualified');
    assert.ok(plan.payload_bytes > 0 && plan.payload_bytes <= 16 * 1024 * 1024);
    assert.match(plan.url, /^https:\/\/github\.com\/daejunnom\/Clearra\/releases\/download\//u);

    const response = await fetch(plan.url, { signal: AbortSignal.timeout(30_000) });
    assert.equal(response.status, 200, `${name} immutable pack GET failed: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    assert.equal(bytes.byteLength, plan.payload_bytes, name);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), plan.payload_identity, name);

    assert.equal(raw.clearra_wasm_transfer_resize(bytes.byteLength), 0);
    const pointer = raw.clearra_wasm_transfer_ptr() >>> 0;
    new Uint8Array(raw.memory.buffer, pointer, bytes.byteLength).set(bytes);
    const admission = raw.clearra_wasm_accelerator_admit(1, profile, 1);
    if (admission !== 0) {
      assert.fail(`${name} qualified WASM admission failed: ${outputText()}`);
    }
    assert.deepEqual(output(), {
      state: 'ready', product: 'board-conditioned-reachability',
      profile: name, generation: plan.generation
    });

    if (name === 'no-kick') {
      // Admission alone is not an execution proof. Run a bounded real PC
      // request while this profile is installed, then drain its terminal
      // event lease before removing the asset. The primitive/source proof
      // separately establishes which supported contexts are cache hits.
      const command = commandEncoder.encode(
        'clearra pc --lines 1 --board-mask 0x3f --height 1 --pieces 1 ' +
        '--queue I --no-hold --rule no-kick --backend cpu --workers 1 ' +
        '--conditioned-reachability'
      );
      assert.equal(raw.clearra_wasm_input_resize(command.byteLength), 0);
      const inputPointer = raw.clearra_wasm_input_ptr() >>> 0;
      new Uint8Array(raw.memory.buffer, inputPointer, command.byteLength).set(command);
      const jobId = raw.clearra_wasm_start_job();
      if (jobId === 0) assert.fail(`no-kick WASM PC job start failed: ${outputText()}`);
      let status = 0;
      for (let step = 0; step < 10_000 && status !== 1; step += 1) {
        status = raw.clearra_wasm_advance_job(jobId, 10_000);
        assert.ok(status === 0 || status === 1 || status === 4,
          `no-kick WASM PC job stopped with status ${status}`);
      }
      assert.equal(status, 1, 'no-kick WASM PC job did not complete within the bounded smoke');
      assert.equal(raw.clearra_wasm_drain_job_events(jobId), 0);
      assert.ok(outputText().length > 0, 'completed PC job emitted no terminal events');
      assert.equal(raw.clearra_wasm_product_page_release(), 0);
    }

    assert.equal(raw.clearra_wasm_accelerator_remove(1, profile), 0);
    assert.deepEqual(output(), {
      state: 'not_loaded', product: 'board-conditioned-reachability',
      profile: name, removed: true
    });
  }
});
