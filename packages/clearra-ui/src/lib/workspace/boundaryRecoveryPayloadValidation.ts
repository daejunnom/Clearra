import type { ClearraProductResultPayload } from '../wasm/wasmCommandClient.ts';

export function validateBoundaryRecoveryPayload(product: ClearraProductResultPayload): string | null {
  if (product.content.payload_kind !== 'boundary-recovery') return 'invalid boundary recovery payload';
  const report = product.content.payload;
  return product.contract === 'boundary-recovery.v1' &&
    product.result_kind === 'boundary-recovery' &&
    report.knowledge_basis === 'full-fixed-queue' &&
    (report.max_early_placements === 0 || report.max_early_placements === 1) &&
    Number.isInteger(report.borrow_source_index) && report.borrow_source_index >= 0 && report.borrow_source_index < 14 &&
    /^0x[0-9a-f]{1,64}$/u.test(report.borrow_placement_mask) &&
    ['normal', 'pc-preserving-recovery', 'non-pc-recovery', 'no-path-within-declared-scope', 'incomplete'].includes(report.status) &&
    Number.isSafeInteger(report.normal_states) && report.normal_states >= 0 &&
    Number.isSafeInteger(report.recovery_states) && report.recovery_states >= 0 &&
    Array.isArray(report.steps) && report.steps.length <= 14 &&
    report.steps.every((step) =>
      Number.isInteger(step.source_queue_index) && step.source_queue_index >= 0 && step.source_queue_index < 14 &&
      /^[IJLOSTZ]$/u.test(step.piece) &&
      /^0x[0-9a-f]{1,64}$/u.test(step.placement_mask) &&
      /^0x[0-9a-f]{1,64}$/u.test(step.board_after_mask)
    ) ? null : 'invalid boundary recovery payload';
}
