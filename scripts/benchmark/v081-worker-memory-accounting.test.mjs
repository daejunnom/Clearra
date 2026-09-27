import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PRIVATE_WORKER_COMPONENTS, PRIVATE_WORKER_MEMORY_SCOPE, readWorkerMemoryAccounting } from './v081-worker-memory-accounting.mjs';

function snapshot() {
  return {
    worker_retained_accounting_scope: PRIVATE_WORKER_MEMORY_SCOPE,
    ...Object.fromEntries(PRIVATE_WORKER_COMPONENTS.map(key => [key, 10])),
    worker_retained_bytes: 90,
    shared_standard_bag_request_retained_bytes: '64',
    standard_bag_memo_payload_bytes: 5,
    standard_bag_product_memo_retained_payload_bytes: 3,
    standard_bag_union_memo_retained_payload_bytes: 2,
    standard_bag_product_memo_directory_bytes: 1,
    standard_bag_product_memo_entries: 1,
    standard_bag_union_memo_entries: 1,
    standard_bag_product_memo_capacity: 2,
    standard_bag_union_memo_capacity: 2,
    standard_bag_product_memo_active_rows: 1,
    standard_bag_product_memo_allocated_rows: 2,
    standard_bag_product_memo_row_slots: 8,
  };
}

test('shared owner is counted once and nested memo is not added again', () => {
  const report = readWorkerMemoryAccounting(snapshot());
  assert.equal(report.private_worker_retained_bytes, 90);
  assert.equal(report.shared_standard_bag_request_retained_bytes, 64);
  assert.equal(report.private_plus_shared_retained_bytes, 154);
  assert.equal(report.nested_standard_bag_memo_payload_bytes, 5);
  assert.equal(report.nested_standard_bag_memo.payload_plus_directory_bytes, 6);
  assert.equal(report.nested_standard_bag_memo.product_directory_bytes, 1);
  assert.match(report.peak_scope, /not-an-os-peak/);
});

test('old mixed ownership scope and missing new component cannot be accepted', () => {
  const old = snapshot();
  old.worker_retained_accounting_scope = 'native-worker-exit-retained-payload-sum';
  assert.throws(() => readWorkerMemoryAccounting(old), /incompatible/);
  const missing = snapshot();
  delete missing.shared_standard_bag_request_retained_bytes;
  assert.throws(() => readWorkerMemoryAccounting(missing), /missing exact/);
});

test('double counted totals or memo and nonfinite byte values are rejected', () => {
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), worker_retained_bytes: 159 }), /total/);
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), standard_bag_product_memo_directory_bytes: 6 }), /nested/);
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), standard_bag_memo_payload_bytes: 11 }), /memo total/);
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), standard_bag_product_memo_retained_payload_bytes: 4 }), /memo total/);
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), standard_bag_product_memo_entries: 3 }), /capacity/);
  assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), standard_bag_product_memo_active_rows: 3 }), /directory/);
  for (const value of [null, true, '', NaN, Infinity, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => readWorkerMemoryAccounting({ ...snapshot(), worker_retained_bytes: value }), /exact memory/);
  }
});
