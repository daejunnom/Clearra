import type { Ctk3Color, Ctk3Piece } from './ctk3Codec';
import {
  ctkOperationRotations,
  operationOffsets,
  type CtkCellCoordinate
} from './ctkOperationGeometry';

export type CtkPaintSelection = Ctk3Color | 'auto';

export const CTK_PAINT_SHORTCUTS = [
  { key: 'A', selection: 'auto' },
  { key: 'G', selection: 'G' },
  { key: 'I', selection: 'I' },
  { key: 'O', selection: 'O' },
  { key: 'T', selection: 'T' },
  { key: 'S', selection: 'S' },
  { key: 'Z', selection: 'Z' },
  { key: 'J', selection: 'J' },
  { key: 'L', selection: 'L' },
  { key: 'E', selection: null }
] as const satisfies ReadonlyArray<{
  key: string;
  selection: CtkPaintSelection;
}>;

const PIECES: readonly Ctk3Piece[] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
const SHORTCUT_SELECTIONS = new Map<string, CtkPaintSelection>(
  CTK_PAINT_SHORTCUTS.map(({ key, selection }) => [key, selection])
);
const PIECES_BY_SHAPE = createShapeCatalog();

export function inferCtkAutoColorPiece(
  indexes: readonly number[],
  width = 10,
  collapseSplitRows = true
): Ctk3Piece | null {
  if (!Number.isInteger(width) || width <= 0 || indexes.length !== 4) {
    return null;
  }
  const uniqueIndexes = new Set(indexes);
  if (
    uniqueIndexes.size !== 4 ||
    indexes.some((index) => !Number.isInteger(index) || index < 0)
  ) {
    return null;
  }
  const cells = indexes.map((index) => ({
    x: index % width,
    y: Math.floor(index / width)
  }));
  return PIECES_BY_SHAPE.get(shapeSignature(cells, collapseSplitRows)) ?? null;
}

export function ctkPaintSelectionFromShortcut(
  key: string,
  code = ''
): CtkPaintSelection | undefined {
  if (key === 'Backspace' || key === 'Delete') return null;
  const logicalKey = key.length === 1 ? key.toUpperCase() : '';
  if (SHORTCUT_SELECTIONS.has(logicalKey)) {
    return SHORTCUT_SELECTIONS.get(logicalKey);
  }
  const physicalKey = code.startsWith('Key') ? code.slice(3).toUpperCase() : '';
  return SHORTCUT_SELECTIONS.get(physicalKey);
}

function createShapeCatalog(): Map<string, Ctk3Piece> {
  const catalog = new Map<string, Ctk3Piece>();
  for (const piece of PIECES) {
    for (const rotation of ctkOperationRotations(piece)) {
      const cells = operationOffsets(piece, rotation).map(([x, y]) => ({ x, y }));
      catalog.set(shapeSignature(cells, false), piece);
    }
  }
  return catalog;
}

function shapeSignature(
  cells: readonly CtkCellCoordinate[],
  collapseSplitRows: boolean
): string {
  const rowMap = collapseSplitRows
    ? new Map(
        [...new Set(cells.map(({ y }) => y))]
          .sort((left, right) => left - right)
          .map((y, index) => [y, index])
      )
    : null;
  const normalized = cells.map(({ x, y }) => ({
    x,
    y: rowMap?.get(y) ?? y
  }));
  const minimumX = Math.min(...normalized.map(({ x }) => x));
  const minimumY = Math.min(...normalized.map(({ y }) => y));
  return normalized
    .map(({ x, y }) => `${x - minimumX},${y - minimumY}`)
    .sort()
    .join(';');
}
