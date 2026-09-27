// Explicit local-only layout selection. Never inherit a caller's A/B flags.
export function productMemoAbbaSelection(candidate, arm) {
  if (!['compact', 'state-major'].includes(candidate)) throw new Error('candidate must be compact or state-major');
  if (!['baseline', 'treatment'].includes(arm)) throw new Error('unknown product memo A/B arm');
  return arm === 'baseline'
    ? { storage: 'reference', layout: 'flat', label: 'reference' }
    : candidate === 'compact'
      ? { storage: 'compact', layout: 'flat', label: 'compact' }
      : { storage: 'reference', layout: 'state-major', label: 'state-major' };
}

export function productMemoAbbaEnvironment(inherited, selection) {
  return { ...inherited,
    CLEARRA_STANDARD_BAG_MEMO: selection.storage,
    CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: selection.layout };
}

export function assertProductMemoSelection(summary, selection) {
  if (summary.standard_bag_memo_storage !== selection.label ||
      summary.standard_bag_product_memo_storage !== selection.label) throw new Error('product memo storage selection was not applied');
  if (summary.standard_bag_product_memo_layout !== selection.layout) throw new Error('product memo layout selection was not applied');
  if (summary.standard_bag_union_memo_storage !== selection.storage) throw new Error('union memo storage differs from the selected control');
}
