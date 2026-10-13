import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const load = name => readFileSync(new URL(`../../.github/workflows/${name}.yml`, import.meta.url), 'utf8').replace(/\r\n/gu, '\n');

function jobs(source) {
  const blocks = source.slice(source.indexOf('\njobs:\n')).split(/(?=\n  [a-z][a-z0-9-]+:\n)/u).slice(1);
  return Object.fromEntries(blocks.map(block => [block.match(/^\n  ([a-z0-9-]+):/u)[1], block]));
}

function steps(job) {
  return job.slice(job.indexOf('\n    steps:\n')).split(/(?=\n      - (?:uses|name):)/u).slice(1);
}

test('feedback prerequisites name an earlier producer, never unrelated success()', () => {
  for (const name of ['v081-selective-source-ci', 'pc24-source-boundary', 'management-policy']) {
    const workflow = load(name);
    assert.ok(!workflow.includes('continue-on-error:'));
    for (const [job, source] of Object.entries(jobs(workflow))) {
      const known = new Set();
      for (const step of steps(source)) {
        const guard = step.match(/^        if: (.*)$/mu)?.[1];
        if (step.includes('run:') || step.includes('uses: actions/setup-node@')) {
          assert.ok(guard?.includes('!cancelled()'), `${name}/${job}: runnable diagnostics need an explicit cancellation/prerequisite guard`);
          assert.ok(!guard.includes('success()'), `${name}/${job}: unrelated failures must not block available diagnostics`);
        }
        for (const match of (guard ?? '').matchAll(/steps\.([a-z0-9-]+)\.outcome/gu)) {
          assert.ok(known.has(match[1]), `${name}/${job}: missing/later prerequisite ${match[1]}`);
        }
        const id = step.match(/^        id: ([a-z0-9-]+)$/mu)?.[1];
        if (id) { assert.ok(!known.has(id), `${name}/${job}: duplicate ${id}`); known.add(id); }
      }
    }
  }
});

test('multi-command Rust diagnostic blocks aggregate failures instead of Bash early exit', () => {
  for (const name of ['v081-selective-source-ci', 'pc24-source-boundary', 'management-policy']) {
    const source = load(name);
    assert.ok(source.includes('node scripts/ci/run-independent-checks.mjs --'));
    assert.ok(!/run: \|\n(?: {10}cargo .*\n){2}/u.test(source), `${name}: independent Cargo commands must retain all results`);
  }
});

test('a raw WASM failure does not hide real pool and browser checks with available artifacts', () => {
  const source = jobs(load('v081-selective-source-ci'))['wasm-realms'];
  for (const step of steps(source).filter(step => /name: (?:Execute the production Web pool|Exercise actual browser)/u.test(step))) {
    const guard = step.match(/^        if: (.*)$/mu)[1];
    for (const prerequisite of ['product-wasm', 'conditioned-assets', 'legal-assets', 'ui-deps']) {
      assert.ok(guard.includes(`steps.${prerequisite}.outcome == 'success'`));
    }
    assert.ok(!guard.includes('realms.outcome'));
  }
  assert.ok(source.includes('steps.browser-tools.outcome'));
});

test('independent OS jobs do not consume each other while real product wire stays producer-bound', () => {
  const management = jobs(load('management-policy'));
  assert.ok(!management['management-policy'].includes('needs: windows-runtime'));
  const selective = jobs(load('v081-selective-source-ci'));
  assert.ok(selective['product-wire-ui'].includes('needs: wasm-abi'));
  assert.ok(selective['product-wire-ui'].includes("needs.wasm-abi.outputs.product_wire_ready == 'true'"));
  assert.ok(selective['wasm-abi'].includes("steps.real-wire.outcome == 'success' && steps.wire-upload.outcome == 'success'"));
  const native = selective['native-products'];
  assert.ok(!native.includes("steps.native-check.outcome == 'success'"));
  const fullHeight = steps(native).find(step => step.includes('name: Read back the same CLI through all full-height Discord'));
  assert.ok(fullHeight.includes("steps.compute-cli.outcome == 'success' && steps.ctk3-deps.outcome == 'success'"));
});
