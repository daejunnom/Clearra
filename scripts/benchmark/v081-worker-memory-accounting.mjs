// Logical exit snapshots are not an allocator/OS peak. Keep the one immutable
// request owner and nested memo payload separate from summed private workers.
export const PRIVATE_WORKER_MEMORY_SCOPE = 'native-worker-exit-private-retained-payload-sum';
export const PRIVATE_WORKER_COMPONENTS = [
  'worker_piece_language_retained_bytes',
  'worker_standard_bag_retained_bytes',
  'worker_reachability_retained_bytes',
  'worker_graph_projection_retained_bytes',
  'worker_other_buildup_retained_bytes',
  'worker_evaluator_retained_bytes',
  'worker_solution_identity_retained_bytes',
  'worker_solution_coverage_retained_bytes',
  'candidate_digest_retained_bytes',
];

export function readWorkerMemoryAccounting(summary) {
  if (summary.worker_retained_accounting_scope !== PRIVATE_WORKER_MEMORY_SCOPE) {
    throw new Error('incompatible private/shared worker memory accounting scope');
  }
  const bytes = key => {
    const value = summary[key];
    if ((typeof value !== 'number' && typeof value !== 'string') || value === '') {
      throw new Error(`missing exact memory component ${key}`);
    }
    const count = Number(value);
    if (!Number.isSafeInteger(count) || count < 0) {
      throw new Error(`invalid exact memory component ${key}`);
    }
    return count;
  };
  const privateComponents = Object.fromEntries(PRIVATE_WORKER_COMPONENTS.map(key => [key, bytes(key)]));
  const privateBytes = bytes('worker_retained_bytes');
  const privateSum = Object.values(privateComponents).reduce((sum, count) => sum + count, 0);
  if (!Number.isSafeInteger(privateSum) || privateBytes !== privateSum) {
    throw new Error('private worker retained total does not match its exclusive components');
  }
  const memoBytes = bytes('standard_bag_memo_payload_bytes');
  const productBytes = bytes('standard_bag_product_memo_retained_payload_bytes');
  const unionBytes = bytes('standard_bag_union_memo_retained_payload_bytes');
  const directoryBytes = bytes('standard_bag_product_memo_directory_bytes');
  const memoWithDirectory = memoBytes + directoryBytes;
  if (!Number.isSafeInteger(productBytes + unionBytes) || productBytes + unionBytes !== memoBytes) {
    throw new Error('nested product and union memo payload do not match the memo total');
  }
  if (!Number.isSafeInteger(memoWithDirectory) || memoWithDirectory > privateComponents.worker_standard_bag_retained_bytes) {
    throw new Error('nested memo payload exceeds private StandardBag owner');
  }
  const productEntries = bytes('standard_bag_product_memo_entries');
  const unionEntries = bytes('standard_bag_union_memo_entries');
  const productCapacity = bytes('standard_bag_product_memo_capacity');
  const unionCapacity = bytes('standard_bag_union_memo_capacity');
  const activeRows = bytes('standard_bag_product_memo_active_rows');
  const allocatedRows = bytes('standard_bag_product_memo_allocated_rows');
  const rowSlots = bytes('standard_bag_product_memo_row_slots');
  if (productEntries > productCapacity || unionEntries > unionCapacity || activeRows > allocatedRows || allocatedRows > rowSlots) {
    throw new Error('invalid memo entry capacity or row directory accounting');
  }
  const sharedBytes = bytes('shared_standard_bag_request_retained_bytes');
  if (!Number.isSafeInteger(privateBytes + sharedBytes)) {
    throw new Error('private plus shared payload exceeds exact receipt integer range');
  }
  return {
    private_scope: PRIVATE_WORKER_MEMORY_SCOPE,
    private_components: privateComponents,
    private_worker_retained_bytes: privateBytes,
    shared_scope: 'one-request-owned-immutable-standard-bag-tables-not-worker-multiplied',
    shared_standard_bag_request_retained_bytes: sharedBytes,
    nested_standard_bag_memo_payload_bytes: memoBytes,
    nested_standard_bag_memo: {
      product_retained_payload_bytes: productBytes, union_retained_payload_bytes: unionBytes,
      product_directory_bytes: directoryBytes, payload_plus_directory_bytes: memoWithDirectory,
      product_entries: productEntries, product_capacity: productCapacity,
      union_entries: unionEntries, union_capacity: unionCapacity,
      active_rows: activeRows, allocated_rows: allocatedRows, row_slots: rowSlots,
      scope: 'included-once-in-private-standard-bag-owner-not-an-os-peak',
    },
    private_plus_shared_retained_bytes: privateBytes + sharedBytes,
    peak_scope: 'not-an-os-peak-or-active-accelerator-owner-proof',
  };
}
