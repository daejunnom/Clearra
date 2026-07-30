import type { Ctk3Operation, Ctk3Piece, Ctk3Rotation } from "./codec.js";

export type CtkCellCoordinate = {
  x: number;
  y: number;
};

type Offset = readonly [x: number, y: number];

const ROTATIONS: readonly Ctk3Rotation[] = [
  "spawn",
  "right",
  "reverse",
  "left",
];
const BASE_OFFSETS: Record<Ctk3Piece, readonly Offset[]> = {
  I: [
    [-1, 0],
    [0, 0],
    [1, 0],
    [2, 0],
  ],
  O: [
    [0, 0],
    [1, 0],
    [0, 1],
    [1, 1],
  ],
  T: [
    [-1, 0],
    [0, 0],
    [1, 0],
    [0, 1],
  ],
  S: [
    [-1, 0],
    [0, 0],
    [0, 1],
    [1, 1],
  ],
  Z: [
    [-1, 1],
    [0, 1],
    [0, 0],
    [1, 0],
  ],
  J: [
    [-1, 1],
    [-1, 0],
    [0, 0],
    [1, 0],
  ],
  L: [
    [1, 1],
    [-1, 0],
    [0, 0],
    [1, 0],
  ],
};

export function ctkOperationRotations(
  piece: Ctk3Piece,
): readonly Ctk3Rotation[] {
  if (piece === "O") return ["spawn"];
  if (piece === "I" || piece === "S" || piece === "Z") {
    return ["spawn", "right"];
  }
  return ROTATIONS;
}

export function operationCells(operation: Ctk3Operation): CtkCellCoordinate[] {
  return operationOffsets(operation.piece, operation.rotation).map(
    ([x, y]) => ({
      x: operation.x + x,
      y: operation.y + y,
    }),
  );
}

export function operationOffsets(
  piece: Ctk3Piece,
  rotation: Ctk3Rotation,
): Offset[] {
  if (piece === "O") return BASE_OFFSETS.O.map(([x, y]) => [x, y]);
  const turns = ROTATIONS.indexOf(rotation);
  return BASE_OFFSETS[piece].map(([sourceX, sourceY]) => {
    let x = sourceX;
    let y = sourceY;
    for (let turn = 0; turn < turns; turn += 1) {
      [x, y] = [y, -x];
    }
    return [x, y];
  });
}

export function canonicalizeCtkOperation(
  operation: Ctk3Operation,
): Ctk3Operation {
  const targetCells = operationCells(operation).sort(compareCells);
  const target = new Set(targetCells.map(cellKey));
  for (const rotation of ctkOperationRotations(operation.piece)) {
    const offsets = operationOffsets(operation.piece, rotation);
    for (const targetCell of targetCells) {
      for (const [offsetX, offsetY] of offsets) {
        const candidate: Ctk3Operation = {
          piece: operation.piece,
          rotation,
          x: targetCell.x - offsetX,
          y: targetCell.y - offsetY,
        };
        if (
          operationCells(candidate).every((cell) => target.has(cellKey(cell)))
        ) {
          return candidate;
        }
      }
    }
  }
  return { ...operation };
}

function cellKey({ x, y }: CtkCellCoordinate): string {
  return `${x},${y}`;
}

function compareCells(
  left: CtkCellCoordinate,
  right: CtkCellCoordinate,
): number {
  return left.y - right.y || left.x - right.x;
}
