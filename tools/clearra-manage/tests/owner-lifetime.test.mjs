import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { createManagedSearchOwner } from '../../../scripts/tools/managed-search-owner.mjs';

const repository = fileURLToPath(new URL('../../../', import.meta.url));
const manager = process.env.CLEARRA_MANAGER_TEST_BINARY ?? join(repository,
  'build/cargo/default/debug', process.platform === 'win32' ? 'clearra-manage.exe' : 'clearra-manage');
const fixture = fileURLToPath(new URL('./owner-lifetime-fixture.mjs', import.meta.url));
const options = {
  skip: existsSync(manager) ? false : 'Build the current clearra-manage binary before the lifecycle fixtures',
  timeout: 10_000,
};

async function executeFixture(t, timeoutArguments, mode, cancelOnReady = false) {
  const signals = new EventEmitter();
  const owner = createManagedSearchOwner({ signalSource: signals });
  let execution;
  try {
    execution = await new Promise((resolve, reject) => {
      const child = owner.spawn(manager, [
        '--root', repository, 'runtime', 'run', '--producer', 'benchmark', '--profile', 'control',
        ...timeoutArguments, '--cancel-on-stdin-eof', '--', process.execPath, fixture, mode,
      ], { cwd: repository, windowsHide: true });
      let stdout = '', stderr = '';
      let cancelled = false;
      child.stdout.setEncoding('utf8');
      child.stderr.setEncoding('utf8');
      child.stdout.on('data', (chunk) => {
        stdout += chunk;
        if (cancelOnReady && !cancelled && stdout.includes('fixture-ready\n')) {
          cancelled = true;
          signals.emit('SIGTERM');
        }
      });
      child.stderr.on('data', (chunk) => { stderr += chunk; });
      child.once('error', reject);
      child.once('close', (code, signal) => resolve({ code, signal, stdout, stderr }));
    });
  } finally {
    owner.dispose();
  }
  const paths = [...execution.stderr.matchAll(/^clearra_runtime_receipt=([^\r\n]+)$/gmu)];
  assert.equal(paths.length, 1, execution.stderr);
  const receipt = JSON.parse(readFileSync(paths[0][1], 'utf8'));
  const outcome = receipt.outcome;
  assert.equal(outcome.cancel_on_stdin_eof, true);
  assert.equal(outcome.process_tree_stopped, true);
  assert.equal(outcome.descendant_processes_at_exit, 0);
  assert.equal(execution.signal, null);
  const terminationEvents = execution.stderr.split(/\r?\n/u).flatMap((line) => {
    try {
      const event = JSON.parse(line);
      return event.schema_id === 'clearra.runtime-termination-attempt.v1' ? [event] : [];
    } catch { return []; }
  });
  if (process.platform !== 'win32') {
    assert.equal(terminationEvents.some((event) => event.attempt.phase === 'drop-cleanup'), false,
      'a confirmed stopped Unix group must not be signalled again by Drop');
  }
  t.diagnostic(JSON.stringify({ receipt: paths[0][1], timeout_seconds: outcome.timeout_seconds,
    reason: outcome.reason, duration_ms: outcome.duration_ms, process_tree_stopped: outcome.process_tree_stopped }));
  return { ...execution, outcome };
}

test('timeout zero keeps the lease until ordinary completion', options, async (t) => {
  const result = await executeFixture(t, ['--timeout', '0'], 'normal');
  assert.equal(result.code, 0);
  assert.equal(result.outcome.timeout_seconds, 0);
  assert.equal(result.outcome.reason, 'normal');
  assert.deepEqual(result.outcome.termination_audit.attempts, []);
  assert.match(result.stdout, /fixture-complete/u);
});

test('child signal remains distinct from a supervisor termination request', {
  ...options,
  skip: options.skip || (process.platform === 'win32' ? 'Unix signal status fixture' : false),
}, async (t) => {
  const result = await executeFixture(t, ['--no-timeout'], 'self-signal');
  assert.equal(result.code, 137);
  assert.equal(result.outcome.reason, 'signal');
  assert.deepEqual(result.outcome.child_termination,
    { kind: 'unix-signal', signal: 9, core_dumped: false });
  assert.equal(result.outcome.supervisor_termination_reason, null);
  assert.deepEqual(result.outcome.termination_audit.attempts, []);
});

test('owner EOF cancels an unlimited run and prevents late success', options, async (t) => {
  const result = await executeFixture(t, ['--no-timeout'], 'cancel', true);
  assert.notEqual(result.code, 0);
  assert.equal(result.outcome.timeout_seconds, 0);
  assert.equal(result.outcome.reason, 'owner-lease-ended');
  assert.equal(result.outcome.error_code, 'E_CLEARRA_OWNER_LEASE_ENDED');
  assert.doesNotMatch(result.stdout, /unexpected-late-success/u);
});

test('inherited descendant output is drained before collector joins', options, async (t) => {
  const result = await executeFixture(t, ['--no-timeout'], 'descendant');
  assert.equal(result.outcome.timeout_seconds, 0);
  assert.equal(result.outcome.reason, 'tree-not-stopped');
  assert.match(result.stdout, /fixture-descendant-pid=\d+/u);
  assert.doesNotMatch(result.stdout, /unexpected-descendant-success/u);
});

test('explicit finite deadlines remain effective with an open owner lease', options, async (t) => {
  const result = await executeFixture(t, ['--timeout', '1'], 'cancel');
  // Preserve the OS termination code; the receipt separately identifies the
  // supervisor deadline, including SIGTERM (143) on POSIX.
  assert.notEqual(result.code, 0);
  assert.equal(result.code, result.outcome.return_code);
  assert.equal(result.outcome.timeout_seconds, 1);
  assert.equal(result.outcome.reason, 'timeout');
  assert.doesNotMatch(result.stdout, /unexpected-late-success/u);
});
