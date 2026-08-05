// SRP rationale: this module has one change reason: CTK multi-page navigation window calculations.
export type CtkPageStripItem =
  | { kind: 'page'; index: number }
  | { kind: 'gap'; key: string };

export const CTK_PAGE_PREVIEW_RADIUS = 100;

export function ctkPageStripItems(
  total: number,
  current: number,
  radius = CTK_PAGE_PREVIEW_RADIUS
): CtkPageStripItem[] {
  const pageCount = Math.max(0, Math.trunc(total));
  if (pageCount === 0) return [];
  const pageIndex = Math.max(0, Math.min(pageCount - 1, Math.trunc(current)));
  const previewRadius = Math.max(0, Math.trunc(radius));
  if (pageCount <= previewRadius * 2 + 1) {
    return Array.from({ length: pageCount }, (_, index) => ({
      kind: 'page' as const,
      index
    }));
  }

  const indices = new Set<number>([0, pageCount - 1]);
  for (
    let index = Math.max(0, pageIndex - previewRadius);
    index <= Math.min(pageCount - 1, pageIndex + previewRadius);
    index += 1
  ) {
    indices.add(index);
  }
  const sorted = [...indices].sort((left, right) => left - right);
  const items: CtkPageStripItem[] = [];
  for (let index = 0; index < sorted.length; index += 1) {
    if (index > 0 && sorted[index] - sorted[index - 1] > 1) {
      items.push({ kind: 'gap', key: `${sorted[index - 1]}-${sorted[index]}` });
    }
    items.push({ kind: 'page', index: sorted[index] });
  }
  return items;
}

export function ctkPageIndexFromArrowKey(
  key: string,
  current: number,
  total: number
): number | null {
  const pageCount = Math.max(0, Math.trunc(total));
  if (pageCount === 0) return null;
  const pageIndex = Math.max(0, Math.min(pageCount - 1, Math.trunc(current)));
  if (key === 'ArrowLeft' && pageIndex > 0) return pageIndex - 1;
  if (key === 'ArrowRight' && pageIndex < pageCount - 1) return pageIndex + 1;
  return null;
}
