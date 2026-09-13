import test from 'node:test';
import assert from 'node:assert/strict';
import { createRustTestEvidence } from './rust-test-evidence.mjs';

test('zero, ignored-only and missing test results do not qualify', () => {
  for (const output of ['', 'running 3 tests\n',
    'test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 171 filtered out; finished in 0.00s\n',
    'test result: FAILED. 2 passed; 1 failed; 0 ignored;\n']) {
    const evidence = createRustTestEvidence();
    evidence.observe(output);
    assert.equal(evidence.hasExecutedTests(), false);
  }
});

test('actual test results survive every stream chunk boundary and later output', () => {
  const output = 'running 2 tests\n..\ntest result: ok. 2 passed; 0 failed; 0 ignored;\n';
  for (let at = 0; at <= output.length; at++) {
    const evidence = createRustTestEvidence();
    evidence.observe(output.slice(0, at));
    evidence.observe(Buffer.from(output.slice(at)));
    evidence.observe('x'.repeat(50_000));
    assert.equal(evidence.hasExecutedTests(), true);
  }
});
