import {
  defaultCtk3Flags,
  type Ctk3Color,
  type Ctk3Operation,
  type Ctk3Page,
  type Ctk3Piece,
  type Ctk3Rotation
} from './ctk3Codec';
import {
  operationCells,
  operationOffsets,
  type CtkCellCoordinate
} from './ctkOperationGeometry';

export { operationCells };
export type { CtkCellCoordinate };

const ROTATIONS: Ctk3Rotation[] = ['spawn', 'right', 'reverse', 'left'];

export function isOperationPlaceable(
  operation: Ctk3Operation,
  cells: Ctk3Color[],
  width: number,
  height: number
): boolean {
  return operationCells(operation).every(({ x, y }) => {
    if (x < 0 || x >= width || y < 0 || y >= height) return false;
    return (cells[y * width + x] ?? null) === null;
  });
}

export function mirrorCtkPage(page: Ctk3Page, width = 10): Ctk3Page {
  const cells = normalizeCells(page.cells, page.height, width);
  const mirrored = Array<Ctk3Color>(cells.length).fill(null);
  for (let y = 0; y < page.height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      mirrored[y * width + (width - x - 1)] = mirrorColor(
        cells[y * width + x]
      );
    }
  }
  return {
    ...page,
    cells: mirrored,
    operation: page.operation
      ? mirrorOperation(page.operation, width)
      : undefined,
    flags: {
      ...defaultCtk3Flags(),
      ...(page.flags ?? {}),
      mirror: false
    },
    garbage: page.garbage
      ? page.garbage
          .slice()
          .reverse()
          .map(mirrorColor)
      : undefined
  };
}

export function grayscaleCtkPage(page: Ctk3Page, width = 10): Ctk3Page {
  return {
    ...page,
    cells: normalizeCells(page.cells, page.height, width).map((color) =>
      color === null ? null : 'G'
    ),
    garbage: page.garbage?.map((color) => (color === null ? null : 'G'))
  };
}

export function createLineClearedPage(
  page: Ctk3Page,
  grayscale: boolean,
  width = 10
): Ctk3Page {
  const source = lockedPageCells(page, grayscale, width);
  const cells: Ctk3Color[] = [];
  for (let y = 0; y < page.height; y += 1) {
    const row = source.slice(y * width, (y + 1) * width);
    if (row.every((color) => color !== null)) continue;
    cells.push(
      ...row.map((color): Ctk3Color =>
        grayscale && color !== null ? 'G' : color
      )
    );
  }
  cells.push(
    ...Array<Ctk3Color>(page.height * width - cells.length).fill(null)
  );
  return {
    height: page.height,
    cells,
    comment: '',
    flags: defaultCtk3Flags(),
    garbage: page.garbage
      ? page.garbage.map((color) =>
          grayscale && color !== null ? 'G' : color
        )
      : undefined
  };
}

export function lockedPageCells(
  page: Ctk3Page,
  grayscale = false,
  width = 10
): Ctk3Color[] {
  const cells = normalizeCells(page.cells, page.height, width).map(
    (color): Ctk3Color =>
      grayscale && color !== null ? 'G' : color
  );
  const operation = page.operation;
  const lock = page.flags?.lock ?? defaultCtk3Flags().lock;
  if (
    !operation ||
    !lock ||
    !isOperationPlaceable(operation, cells, width, page.height)
  ) {
    return cells;
  }
  const color: Ctk3Color =
    grayscale || page.flags?.colorize === false ? 'G' : operation.piece;
  for (const { x, y } of operationCells(operation)) {
    cells[y * width + x] = color;
  }
  return cells;
}

export function clearCtkPageField(page: Ctk3Page, width = 10): Ctk3Page {
  return {
    ...page,
    cells: Array<Ctk3Color>(page.height * width).fill(null),
    operation: undefined,
    garbage: undefined
  };
}

export function compactCtkPage(page: Ctk3Page, width = 10): Ctk3Page {
  const cells = normalizeCells(page.cells, page.height, width);
  let height = page.height;
  while (
    height > 0 &&
    cells
      .slice((height - 1) * width, height * width)
      .every((color) => color === null)
  ) {
    height -= 1;
  }
  return {
    ...page,
    height,
    cells: cells.slice(0, height * width),
    flags: {
      ...defaultCtk3Flags(),
      ...(page.flags ?? {})
    },
    operation: page.operation ? { ...page.operation } : undefined,
    garbage: page.garbage?.slice()
  };
}

function mirrorOperation(
  operation: Ctk3Operation,
  width: number
): Ctk3Operation | undefined {
  const piece = mirrorPiece(operation.piece);
  const target = new Set(
    operationCells(operation).map(({ x, y }) => `${width - x - 1},${y}`)
  );
  const preferred = mirrorRotation(operation.rotation);
  const rotations = [
    preferred,
    ...ROTATIONS.filter((rotation) => rotation !== preferred)
  ];
  for (const rotation of rotations) {
    const offsets = operationOffsets(piece, rotation);
    for (const targetKey of target) {
      const [targetX, targetY] = targetKey.split(',').map(Number);
      for (const [offsetX, offsetY] of offsets) {
        const candidate: Ctk3Operation = {
          piece,
          rotation,
          x: targetX - offsetX,
          y: targetY - offsetY
        };
        const cells = operationCells(candidate);
        if (
          cells.length === target.size &&
          cells.every(({ x, y }) => target.has(`${x},${y}`))
        ) {
          return candidate;
        }
      }
    }
  }
  return undefined;
}

function mirrorColor(color: Ctk3Color): Ctk3Color {
  if (color === 'J') return 'L';
  if (color === 'L') return 'J';
  if (color === 'S') return 'Z';
  if (color === 'Z') return 'S';
  return color;
}

function mirrorPiece(piece: Ctk3Piece): Ctk3Piece {
  return mirrorColor(piece) as Ctk3Piece;
}

function mirrorRotation(rotation: Ctk3Rotation): Ctk3Rotation {
  if (rotation === 'right') return 'left';
  if (rotation === 'left') return 'right';
  return rotation;
}

function normalizeCells(
  source: Ctk3Color[],
  height: number,
  width: number
): Ctk3Color[] {
  const cells = Array<Ctk3Color>(height * width).fill(null);
  cells.splice(
    0,
    Math.min(cells.length, source.length),
    ...source.slice(0, cells.length)
  );
  return cells;
}
