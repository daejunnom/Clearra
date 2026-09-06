import assert from 'node:assert/strict';
import test from 'node:test';

import { cancelSetupPathDetail } from '../src/lib/workspace/setupPathDetailState.ts';

test('cancelling an active setup detail replaces loading with a terminal failure', () => {
  const activeKey = 'hold-empty:setup-1';
  const idleKey = 'hold-i:setup-2';
  const before = {
    [activeKey]: {
      status: 'loading',
      paths: [],
      complete: false,
      publicFailures: [],
      developerFailure: null
    },
    [idleKey]: {
      status: 'complete',
      paths: [],
      complete: true,
      publicFailures: [],
      developerFailure: null
    }
  };

  const after = cancelSetupPathDetail(before, activeKey);

  assert.deepEqual(after[activeKey], {
    status: 'failed',
    paths: [],
    complete: false,
    publicFailures: [{ code: 'request-cancelled', severity: 'error' }],
    developerFailure: null
  });
  assert.equal(after[idleKey], before[idleKey]);
  assert.equal(before[activeKey].status, 'loading');
});

test('cancelling without an active detail preserves the state object', () => {
  const before = {};
  assert.equal(cancelSetupPathDetail(before, null), before);
});
