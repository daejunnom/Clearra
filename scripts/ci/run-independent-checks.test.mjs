import assert from 'node:assert/strict';
import test from 'node:test';
import { parseIndependentChecks, runIndependentChecks } from './run-independent-checks.mjs';

test('argv groups preserve Cargo flags without a shell or eval', () => {
  assert.deepEqual(parseIndependentChecks(['--', 'cargo', 'fmt', '--check', ':::', 'node', '--test', 'a.test.mjs']),
    [['cargo', 'fmt', '--check'], ['node', '--test', 'a.test.mjs']]);
  for (const args of [[], [':::'], ['cargo', ':::'], [':::', 'node']]) {
    assert.throws(() => parseIndependentChecks(args), /nonempty argv/);
  }
});

test('failed and unlaunchable checks cannot stop independent checks or become a pass', async () => {
  const attempts = [];
  const messages = [];
  const result = await runIndependentChecks([['fail'], ['missing'], ['pass']], {
    execute: async argv => {
      attempts.push(argv[0]);
      if (argv[0] === 'missing') throw new Error('launch failed');
      return { code: argv[0] === 'pass' ? 0 : 2 };
    }, report: message => messages.push(message)
  });
  assert.deepEqual(attempts, ['fail', 'missing', 'pass']);
  assert.deepEqual(result.outcomes.map(outcome => outcome.status), ['failed', 'failed', 'passed']);
  assert.equal(result.exitCode, 1);
  assert.ok(messages.some(message => message.includes('attempted=3 planned=3 failed=2')));
});

test('explicit cancellation does not authorize starting subsequent checks', async () => {
  for (const cancellation of [{ code: 130 }, { code: null, signal: 'SIGTERM' }, { code: 0, cancelled: true }]) {
    const attempts = [];
    const result = await runIndependentChecks([['cancel'], ['must-not-run']], {
      execute: async argv => { attempts.push(argv[0]); return cancellation; }, report: () => {}
    });
    assert.deepEqual(attempts, ['cancel']);
    assert.equal(result.exitCode, 130);
  }
});

test('only all-passed checks earn a successful diagnostic exit', async () => {
  const result = await runIndependentChecks([['a'], ['b']], {
    execute: async () => ({ code: 0 }), report: () => {}
  });
  assert.equal(result.exitCode, 0);
  assert.equal(result.outcomes.length, 2);
});

test('a crashing test is a failure, not an instruction to cancel independent tests', async () => {
  const attempts = [];
  const result = await runIndependentChecks([['crash'], ['next']], {
    execute: async argv => {
      attempts.push(argv[0]);
      return argv[0] === 'crash' ? { code: null, signal: 'SIGABRT' } : { code: 0 };
    }, report: () => {}
  });
  assert.deepEqual(attempts, ['crash', 'next']);
  assert.equal(result.exitCode, 1);
});

test('real child exits are retained while a later child can still run', async () => {
  const result = await runIndependentChecks([
    [process.execPath, '-e', 'process.exit(7)'],
    [process.execPath, '-e', 'process.exit(0)']
  ], { report: () => {} });
  assert.deepEqual(result.outcomes.map(outcome => outcome.code), [7, 0]);
  assert.equal(result.exitCode, 1);
});
