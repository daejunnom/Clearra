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

function runLines(step) {
  const run = /^        run: ([^\n]+)$/mu.exec(step);
  if (!run) return [];
  if (!/^[|>][+-]?$/u.test(run[1].trim())) return [run[1].trim()];
  const commands = [];
  for (const line of step.slice(run.index + run[0].length).split('\n')) {
    if (line.trim() && !/^ {10}/u.test(line)) break;
    if (line.trim() && !line.trimStart().startsWith('#')) commands.push(line.trim());
  }
  return commands;
}

function assertVerifierProofExecution(workflow) {
  const job = jobs(workflow)['wasm-realms'];
  assert.ok(job, 'the actual verifier proof owner is required');
  const requirements = [
    ['node scripts/tools/run-real-web-verifier-pool-smoke.mjs',
      ['product-wasm', 'conditioned-assets', 'legal-assets', 'ui-deps']],
    ['node scripts/tools/v081-accelerator-signed-browser-acceptance.mjs',
      ['product-wasm', 'conditioned-assets', 'legal-assets', 'ui-deps', 'browser-tools']],
  ];
  for (const [command, required] of requirements) {
    const matches = steps(job).filter(step => runLines(step).includes(command));
    assert.equal(matches.length, 1, `${command}: exactly one runnable E2E step is required`);
    assert.deepEqual(runLines(matches[0]), [command], `${command}: do not wrap or suppress the required execution`);
    let guard = matches[0].match(/^        if: (.*)$/mu)?.[1]?.trim();
    assert.ok(guard, `${command}: explicit prerequisites are required`);
    if (guard.startsWith('${{') && guard.endsWith('}}')) guard = guard.slice(3, -2).trim();
    const clauses = guard.split(/\s*&&\s*/u).map(clause => {
      if (clause.replaceAll(/\s/gu, '') === '!cancelled()') return '!cancelled';
      const prerequisite = /^steps\.([a-z][a-z0-9-]*)\.outcome\s*==\s*(['"])success\2$/u.exec(clause);
      assert.ok(prerequisite, `${command}: expected a conjunctive producer condition, got ${clause}`);
      return prerequisite[1];
    });
    assert.deepEqual(clauses.sort(), ['!cancelled', ...required].sort(),
      `${command}: do not drop a producer or depend on an unrelated test result`);
  }
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
  assertVerifierProofExecution(load('v081-selective-source-ci'));
});

test('required E2E ownership rejects omission, comments, duplicates and weakened prerequisites', () => {
  const workflow = load('v081-selective-source-ci');
  const command = 'node scripts/tools/v081-accelerator-signed-browser-acceptance.mjs';
  const step = steps(jobs(workflow)['wasm-realms']).find(value => runLines(value).includes(command));
  assert.ok(step);
  const mutations = [
    workflow.replace(step, ''),
    workflow.replace(step, step.replace(`run: ${command}`, `# run: ${command}`)),
    workflow.replace(step, step.replace(`run: ${command}`, `run: ${command} || true`)),
    workflow.replace(step, `${step}${step}`),
    workflow.replace(step, step.replace(" && steps.product-wasm.outcome == 'success'", '')),
    workflow.replace(step, step.replace('!cancelled()', 'true || !cancelled()')),
    workflow.replace(step, step.replace("steps.ui-deps.outcome == 'success'",
      "steps.ui-deps.outcome == 'success' && steps.realms.outcome == 'success'")),
  ];
  for (const mutated of mutations) {
    assert.notEqual(mutated, workflow, 'the counterexample must actually change the workflow');
    assert.throws(() => assertVerifierProofExecution(mutated));
  }
  const cosmetic = step.replace(/name: [^\n]+/u, 'name: Browser behavior proof')
    .replace(`run: ${command}`, `run: |\n          ${command}`)
    .replace("steps.product-wasm.outcome == 'success'", 'steps.product-wasm.outcome == "success"');
  assertVerifierProofExecution(workflow.replace(step, cosmetic));
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
