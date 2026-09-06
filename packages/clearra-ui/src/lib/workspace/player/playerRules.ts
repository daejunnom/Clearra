export const PLAYER_BOARD_WIDTH = 10;
export const PLAYER_VISIBLE_ROWS = 20;
export const PLAYER_HIDDEN_ROWS = 48;
export const PLAYER_BOARD_ROWS = PLAYER_VISIBLE_ROWS + PLAYER_HIDDEN_ROWS;
export const PLAYER_BOARD_CELLS = PLAYER_BOARD_WIDTH * PLAYER_BOARD_ROWS;
export const PLAYER_FULL_ROW_MASK = (1 << PLAYER_BOARD_WIDTH) - 1;
export const PLAYER_SPAWN_Y = PLAYER_VISIBLE_ROWS - 1;

export const PLAYER_PIECES = ["I", "O", "T", "S", "Z", "J", "L"] as const;
export type PlayerPiece = (typeof PLAYER_PIECES)[number];

export const PLAYER_ROTATIONS = ["spawn", "right", "reverse", "left"] as const;
export type PlayerRotation = (typeof PLAYER_ROTATIONS)[number];
export type PlayerRotationDirection = "cw" | "ccw" | "180";

// Keep this registry aligned with the PC solver's public RuleProfile contract.
// Player input remains allocation-free because every profile is normalized into
// PRECOMPUTED_KICKS once when this module is loaded.
export const PLAYER_KICK_PROFILES = [
  "srs-plus",
  "srs",
  "srs-x",
  "jstris-180",
] as const;
export type PlayerKickProfile = (typeof PLAYER_KICK_PROFILES)[number];

export const PLAYER_CELL_ID = Object.freeze({
  empty: 0,
  G: 1,
  I: 2,
  O: 3,
  T: 4,
  S: 5,
  Z: 6,
  J: 7,
  L: 8,
} as const);

export type PlayerCellId = (typeof PLAYER_CELL_ID)[keyof typeof PLAYER_CELL_ID];
export type PlayerCtkColor = PlayerPiece | "G" | null;
export type PlayerBoardInputCell = PlayerCellId | PlayerCtkColor | undefined;

export const PLAYER_CELL_TO_CTK_COLOR = Object.freeze([
  null,
  "G",
  "I",
  "O",
  "T",
  "S",
  "Z",
  "J",
  "L",
] as const satisfies readonly PlayerCtkColor[]);

export type PlayerCellOffset = Readonly<{ x: number; y: number }>;
export type PlayerKickOffset = Readonly<{ dx: number; dy: number }>;

const SHAPES: Readonly<Record<PlayerPiece, readonly (readonly PlayerCellOffset[])[]>> =
  Object.freeze({
    I: shapeSet(
      [[0, 0], [1, 0], [2, 0], [3, 0]],
      [[0, 0], [0, 1], [0, 2], [0, 3]],
      [[0, 0], [1, 0], [2, 0], [3, 0]],
      [[0, 0], [0, 1], [0, 2], [0, 3]],
    ),
    O: shapeSet(
      [[0, 0], [1, 0], [0, 1], [1, 1]],
      [[0, 0], [1, 0], [0, 1], [1, 1]],
      [[0, 0], [1, 0], [0, 1], [1, 1]],
      [[0, 0], [1, 0], [0, 1], [1, 1]],
    ),
    T: shapeSet(
      [[0, 0], [1, 0], [2, 0], [1, 1]],
      [[0, 0], [0, 1], [0, 2], [1, 1]],
      [[0, 1], [1, 1], [2, 1], [1, 0]],
      [[1, 0], [1, 1], [1, 2], [0, 1]],
    ),
    S: shapeSet(
      [[0, 0], [1, 0], [1, 1], [2, 1]],
      [[1, 0], [0, 1], [1, 1], [0, 2]],
      [[0, 0], [1, 0], [1, 1], [2, 1]],
      [[1, 0], [0, 1], [1, 1], [0, 2]],
    ),
    Z: shapeSet(
      [[1, 0], [2, 0], [0, 1], [1, 1]],
      [[0, 0], [0, 1], [1, 1], [1, 2]],
      [[1, 0], [2, 0], [0, 1], [1, 1]],
      [[0, 0], [0, 1], [1, 1], [1, 2]],
    ),
    J: shapeSet(
      [[0, 0], [1, 0], [2, 0], [0, 1]],
      [[0, 0], [0, 1], [0, 2], [1, 2]],
      [[2, 0], [0, 1], [1, 1], [2, 1]],
      [[0, 0], [1, 0], [1, 1], [1, 2]],
    ),
    L: shapeSet(
      [[0, 0], [1, 0], [2, 0], [2, 1]],
      [[0, 0], [0, 1], [0, 2], [1, 0]],
      [[0, 0], [0, 1], [1, 1], [2, 1]],
      [[0, 2], [1, 0], [1, 1], [1, 2]],
    ),
  });

const JLSTZ_CENTERS: readonly (readonly [number, number])[] = Object.freeze([
  Object.freeze([1, 0] as const),
  Object.freeze([0, 1] as const),
  Object.freeze([1, 1] as const),
  Object.freeze([1, 1] as const),
]);

const I_CENTERS: readonly (readonly [number, number])[] = Object.freeze([
  Object.freeze([0, 0] as const),
  Object.freeze([-2, 2] as const),
  Object.freeze([0, 1] as const),
  Object.freeze([-1, 2] as const),
]);

const JLSTZ_90: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>right": kicks([[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]]),
  "reverse>right": kicks([[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]]),
  "right>spawn": kicks([[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]]),
  "right>reverse": kicks([[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]]),
  "reverse>left": kicks([[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]]),
  "spawn>left": kicks([[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]]),
  "left>reverse": kicks([[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]]),
  "left>spawn": kicks([[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]]),
});

const SRS_PLUS_I_90: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>right": kicks([[0, 0], [1, 0], [-2, 0], [-2, -1], [1, 2]]),
  "right>spawn": kicks([[0, 0], [-1, 0], [2, 0], [-1, -2], [2, 1]]),
  "right>reverse": kicks([[0, 0], [-1, 0], [2, 0], [-1, 2], [2, -1]]),
  "reverse>right": kicks([[0, 0], [-2, 0], [1, 0], [-2, 1], [1, -2]]),
  "spawn>left": kicks([[0, 0], [-1, 0], [2, 0], [2, -1], [-1, 2]]),
  "left>spawn": kicks([[0, 0], [1, 0], [-2, 0], [1, -2], [-2, 1]]),
  "left>reverse": kicks([[0, 0], [1, 0], [-2, 0], [1, 2], [-2, -1]]),
  "reverse>left": kicks([[0, 0], [2, 0], [-1, 0], [2, 1], [-1, -2]]),
});

const SRS_I_90: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>right": kicks([[0, 0], [-2, 0], [1, 0], [-2, -1], [1, 2]]),
  "left>reverse": kicks([[0, 0], [-2, 0], [1, 0], [-2, -1], [1, 2]]),
  "right>spawn": kicks([[0, 0], [2, 0], [-1, 0], [2, 1], [-1, -2]]),
  "reverse>left": kicks([[0, 0], [2, 0], [-1, 0], [2, 1], [-1, -2]]),
  "right>reverse": kicks([[0, 0], [-1, 0], [2, 0], [-1, 2], [2, -1]]),
  "spawn>left": kicks([[0, 0], [-1, 0], [2, 0], [-1, 2], [2, -1]]),
  "reverse>right": kicks([[0, 0], [1, 0], [-2, 0], [1, -2], [-2, 1]]),
  "left>spawn": kicks([[0, 0], [1, 0], [-2, 0], [1, -2], [-2, 1]]),
});

const JLSTZ_180: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>reverse": kicks([[0, 0], [0, 1], [1, 1], [-1, 1], [1, 0], [-1, 0]]),
  "reverse>spawn": kicks([[0, 0], [0, -1], [-1, -1], [1, -1], [-1, 0], [1, 0]]),
  "right>left": kicks([[0, 0], [1, 0], [1, 2], [1, 1], [0, 2], [0, 1]]),
  "left>right": kicks([[0, 0], [-1, 0], [-1, 2], [-1, 1], [0, 2], [0, 1]]),
});

const JSTRIS_180: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>reverse": kicks([[0, 0], [0, 1]]),
  "right>left": kicks([[0, 0], [1, 0]]),
  "reverse>spawn": kicks([[0, 0], [0, -1]]),
  "left>right": kicks([[0, 0], [-1, 0]]),
});

const SRS_X_JLSTZ_180: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>reverse": kicks([[0, 0], [1, 0], [2, 0], [1, -1], [2, -1], [-1, 0], [-2, 0], [-1, -1], [-2, -1], [0, 1], [3, 0], [-3, 0]]),
  "right>left": kicks([[0, 0], [0, -1], [0, -2], [-1, -1], [-1, -2], [0, 1], [0, 2], [-1, 1], [-1, 2], [1, 0], [0, -3], [0, 3]]),
  "reverse>spawn": kicks([[0, 0], [-1, 0], [-2, 0], [-1, 1], [-2, 1], [1, 0], [2, 0], [1, 1], [2, 1], [0, -1], [-3, 0], [3, 0]]),
  "left>right": kicks([[0, 0], [0, -1], [0, -2], [1, -1], [1, -2], [0, 1], [0, 2], [1, 1], [1, 2], [-1, 0], [0, -3], [0, 3]]),
});

const SRS_X_I_180: Readonly<Record<string, readonly PlayerKickOffset[]>> = Object.freeze({
  "spawn>reverse": kicks([[0, 0], [-1, 0], [-2, 0], [1, 0], [2, 0], [0, -1]]),
  "right>left": kicks([[0, 0], [0, -1], [0, -2], [0, 1], [0, 2], [-1, 0]]),
  "reverse>spawn": kicks([[0, 0], [1, 0], [2, 0], [-1, 0], [-2, 0], [0, 1]]),
  "left>right": kicks([[0, 0], [0, -1], [0, -2], [0, 1], [0, 2], [1, 0]]),
});

export function isPlayerPiece(value: unknown): value is PlayerPiece {
  return typeof value === "string" && (PLAYER_PIECES as readonly string[]).includes(value);
}

export function isPlayerKickProfile(value: unknown): value is PlayerKickProfile {
  return (
    typeof value === "string" &&
    (PLAYER_KICK_PROFILES as readonly string[]).includes(value)
  );
}

export function playerCellIdFromCtkColor(value: PlayerBoardInputCell): PlayerCellId {
  if (value === null || value === undefined || value === 0) return PLAYER_CELL_ID.empty;
  if (typeof value === "number") {
    if (Number.isInteger(value) && value >= PLAYER_CELL_ID.G && value <= PLAYER_CELL_ID.L) {
      return value as PlayerCellId;
    }
    throw new RangeError(`Player cell id must be an integer from 0 to ${PLAYER_CELL_ID.L}.`);
  }
  const color = PLAYER_CELL_ID[value];
  if (color === undefined) throw new RangeError(`Unsupported CTK color '${String(value)}'.`);
  return color;
}

export function playerCtkColorFromCellId(value: number): PlayerCtkColor {
  if (!Number.isInteger(value) || value < 0 || value >= PLAYER_CELL_TO_CTK_COLOR.length) {
    throw new RangeError(`Player cell id must be an integer from 0 to ${PLAYER_CELL_ID.L}.`);
  }
  return PLAYER_CELL_TO_CTK_COLOR[value];
}

export function playerPieceCellId(piece: PlayerPiece): PlayerCellId {
  return PLAYER_CELL_ID[piece];
}

export function playerPieceOffsets(
  piece: PlayerPiece,
  rotation: PlayerRotation,
): readonly PlayerCellOffset[] {
  return SHAPES[piece][rotationIndex(rotation)];
}

export function playerSpawnX(piece: PlayerPiece): number {
  return piece === "O" ? 4 : 3;
}

export function playerRotationTarget(
  rotation: PlayerRotation,
  direction: PlayerRotationDirection,
): PlayerRotation {
  const delta = direction === "cw" ? 1 : direction === "ccw" ? 3 : 2;
  return PLAYER_ROTATIONS[(rotationIndex(rotation) + delta) & 3];
}

export function playerKickCandidates(
  piece: PlayerPiece,
  from: PlayerRotation,
  to: PlayerRotation,
  profile: PlayerKickProfile = "srs-plus",
): readonly PlayerKickOffset[] {
  if (!isPlayerKickProfile(profile)) {
    throw new RangeError(`Unsupported player kick profile '${String(profile)}'.`);
  }
  return PRECOMPUTED_KICKS[profile][`${piece}:${from}>${to}`] ?? EMPTY_KICKS;
}

function normalizedKickCandidates(
  profile: PlayerKickProfile,
  piece: PlayerPiece,
  from: PlayerRotation,
  to: PlayerRotation,
): readonly PlayerKickOffset[] {
  const transition = `${from}>${to}`;
  if (piece === "O") {
    // TETR.IO's standard `o` is marked `disallow_kick`; `oo_kicks` belongs
    // to a separate non-standard `oo` piece. ComputeKick still performs its
    // implicit origin attempt, represented explicitly here for all SRS-X
    // rotation requests.
    return (profile === "srs-x" && (isQuarterTurn(from, to) || isHalfTurn(from, to))) ||
      (profile !== "jstris-180" && isQuarterTurn(from, to))
      ? ZERO_KICK
      : EMPTY_KICKS;
  }
  let raw: readonly PlayerKickOffset[] | undefined;
  if (isHalfTurn(from, to)) {
    raw =
      profile === "srs-plus"
        ? (piece === "I" ? JSTRIS_180 : JLSTZ_180)[transition]
        : profile === "srs-x"
          ? (piece === "I" ? SRS_X_I_180 : SRS_X_JLSTZ_180)[transition]
          : profile === "jstris-180"
            ? JSTRIS_180[transition]
            : undefined;
  } else {
    raw = (piece === "I"
      ? profile === "srs-plus"
        ? SRS_PLUS_I_90
        : SRS_I_90
      : JLSTZ_90)[transition];
  }
  if (!raw) return EMPTY_KICKS;

  const [fromX, fromY] = rotationCenter(piece, from);
  const [toX, toY] = rotationCenter(piece, to);
  return raw.map(({ dx, dy }) =>
    Object.freeze({ dx: dx + fromX - toX, dy: dy + fromY - toY }),
  );
}

export function playerPlacementFits(
  rowMasks: ArrayLike<number>,
  piece: PlayerPiece,
  rotation: PlayerRotation,
  anchorX: number,
  anchorY: number,
): boolean {
  for (const offset of playerPieceOffsets(piece, rotation)) {
    const x = anchorX + offset.x;
    const y = anchorY + offset.y;
    if (x < 0 || x >= PLAYER_BOARD_WIDTH || y < 0 || y >= PLAYER_BOARD_ROWS) return false;
    if ((rowMasks[y] & (1 << x)) !== 0) return false;
  }
  return true;
}

export function playerGhostY(
  rowMasks: ArrayLike<number>,
  piece: PlayerPiece,
  rotation: PlayerRotation,
  anchorX: number,
  anchorY: number,
): number {
  let y = anchorY;
  while (playerPlacementFits(rowMasks, piece, rotation, anchorX, y - 1)) y -= 1;
  return y;
}

function shapeSet(
  ...rotations: readonly (readonly [number, number])[][]
): readonly (readonly PlayerCellOffset[])[] {
  return Object.freeze(
    rotations.map((rotation) =>
      Object.freeze(rotation.map(([x, y]) => Object.freeze({ x, y }))),
    ),
  );
}

function kicks(values: readonly (readonly [number, number])[]): readonly PlayerKickOffset[] {
  return Object.freeze(values.map(([dx, dy]) => Object.freeze({ dx, dy })));
}

function rotationIndex(rotation: PlayerRotation): number {
  const index = PLAYER_ROTATIONS.indexOf(rotation);
  if (index < 0) throw new RangeError(`Unsupported player rotation '${String(rotation)}'.`);
  return index;
}

function rotationCenter(
  piece: PlayerPiece,
  rotation: PlayerRotation,
): readonly [number, number] {
  if (piece === "O") return O_CENTER;
  return (piece === "I" ? I_CENTERS : JLSTZ_CENTERS)[rotationIndex(rotation)];
}

function isQuarterTurn(from: PlayerRotation, to: PlayerRotation): boolean {
  const delta = (rotationIndex(to) + 4 - rotationIndex(from)) & 3;
  return delta === 1 || delta === 3;
}

function isHalfTurn(from: PlayerRotation, to: PlayerRotation): boolean {
  return ((rotationIndex(to) + 4 - rotationIndex(from)) & 3) === 2;
}

const O_CENTER = Object.freeze([0, 0] as const);
const ZERO_KICK = Object.freeze([Object.freeze({ dx: 0, dy: 0 })]);
const EMPTY_KICKS = Object.freeze([]) as readonly PlayerKickOffset[];
const PRECOMPUTED_KICKS: Readonly<
  Record<PlayerKickProfile, Readonly<Record<string, readonly PlayerKickOffset[]>>>
> = Object.freeze({
  "srs-plus": buildKickTable("srs-plus"),
  srs: buildKickTable("srs"),
  "srs-x": buildKickTable("srs-x"),
  "jstris-180": buildKickTable("jstris-180"),
});

function buildKickTable(
  profile: PlayerKickProfile,
): Readonly<Record<string, readonly PlayerKickOffset[]>> {
  const table: Record<string, readonly PlayerKickOffset[]> = {};
  for (const piece of PLAYER_PIECES) {
    for (const from of PLAYER_ROTATIONS) {
      for (const to of PLAYER_ROTATIONS) {
        if (from === to) continue;
        const sequence = normalizedKickCandidates(profile, piece, from, to);
        if (sequence.length > 0) table[`${piece}:${from}>${to}`] = Object.freeze(sequence);
      }
    }
  }
  return Object.freeze(table);
}
