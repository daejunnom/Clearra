// Explicit local-only layout selection. Never inherit a caller's A/B flags.
export function workerMemoBenchmarkCase(id = 'pc-p7p4-srs-plus') {
  const common = {
    compare_fields: ['summary.unique_solution_count', 'summary.normalized_solution_set_hash',
      'summary.count_complete', 'summary.packing_candidate_set_digest'],
  };
  if (id === 'pc-p7p4-srs-plus') return {
    ...common, id, size: 'large',
    args: ['pc', '--board-mask', '0', '--height', '4', '--pieces', '10', '--lines', '4',
      '--patterns', 'P7P4', '--count', 'unique', '--backend', 'cpu', '--no-tablebase'],
    expected: { 'summary.unique_solution_count': 456923,
      'summary.normalized_solution_set_hash': 'cts1:98ebe8726537b29f', 'summary.count_complete': true },
  };
  if (id === 'pc-existing-field-p7-srs-plus') return {
    ...common, id, size: 'small',
    args: ['pc', '--board-mask', '0x3c0f03c0f', '--height', '4', '--pieces', '6', '--lines', '4',
      '--patterns', 'P7', '--count', 'unique', '--backend', 'cpu', '--no-tablebase'],
    expected: { 'summary.unique_solution_count': 246,
      'summary.normalized_solution_set_hash': 'cts1:cb0b19c391d5003e', 'summary.count_complete': true },
  };
  throw new Error('unknown fixed worker memo benchmark case');
}

export function assertProductMemoCaseSelection(summary, selection, entry) {
  assertProductMemoSelection(summary, selection, {
    requirePromotion: selection.layout === 'adaptive' && entry.size === 'large',
  });
  if (selection.layout === 'adaptive' && entry.size === 'small' &&
      (summary.standard_bag_product_memo_layout !== 'flat' ||
       Number(summary.standard_bag_product_memo_directory_bytes) !== 0 ||
       Number(summary.standard_bag_product_memo_promotion_attempts) !== 0 ||
       Number(summary.standard_bag_product_memo_promotions) !== 0)) {
    throw new Error('small adaptive case acquired State-major directory or promotion work');
  }
}

export function productMemoAbbaSelection(candidate, arm) {
  if (!['compact', 'state-major', 'adaptive'].includes(candidate)) throw new Error('candidate must be compact, state-major or adaptive');
  if (!['baseline', 'treatment'].includes(arm)) throw new Error('unknown product memo A/B arm');
  return arm === 'baseline'
    ? { storage: 'reference', layout: 'flat', label: 'reference' }
    : candidate === 'compact'
      ? { storage: 'compact', layout: 'flat', label: 'compact' }
      : { storage: 'reference', layout: candidate, label: candidate };
}

export function productMemoAbbaEnvironment(inherited, selection) {
  return { ...inherited,
    CLEARRA_STANDARD_BAG_MEMO: selection.storage,
    CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: selection.layout };
}

export function assertProductMemoSelection(summary, selection, { requirePromotion = false } = {}) {
  if (selection.layout === 'adaptive') {
    if (summary.standard_bag_product_memo_policy !== 'adaptive') throw new Error('adaptive memo policy was not applied');
    const layout = summary.standard_bag_product_memo_layout;
    const storage = summary.standard_bag_product_memo_storage;
    if (!['flat', 'state-major', 'mixed'].includes(layout) ||
        storage !== (layout === 'flat' ? 'reference' : layout) ||
        summary.standard_bag_memo_storage !== storage) throw new Error('adaptive memo actual layout/storage is invalid');
    const counter = key => {
      const raw = summary[key];
      if ((typeof raw !== 'number' && typeof raw !== 'string') ||
          (typeof raw === 'string' && !/^\d+$/.test(raw))) throw new Error('adaptive memo promotion evidence is invalid');
      return Number(raw);
    };
    const attempts = counter('standard_bag_product_memo_promotion_attempts');
    const promotions = counter('standard_bag_product_memo_promotions');
    if (!Number.isSafeInteger(attempts) || !Number.isSafeInteger(promotions) ||
        attempts < 0 || promotions < 0 || promotions > attempts ||
        (layout === 'flat' && promotions !== 0) ||
        (layout !== 'flat' && promotions === 0) ||
        (requirePromotion && promotions === 0)) throw new Error('adaptive memo promotion evidence is invalid');
    if (summary.standard_bag_union_memo_storage !== selection.storage) throw new Error('union memo storage differs from the selected control');
    return;
  }
  if (summary.standard_bag_memo_storage !== selection.label ||
      summary.standard_bag_product_memo_storage !== selection.label) throw new Error('product memo storage selection was not applied');
  if (summary.standard_bag_product_memo_layout !== selection.layout) throw new Error('product memo layout selection was not applied');
  if (summary.standard_bag_union_memo_storage !== selection.storage) throw new Error('union memo storage differs from the selected control');
}
