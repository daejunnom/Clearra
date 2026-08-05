import {
  CTK_BOARD_THEME,
  CTK_BOARD_WIDTH,
  CTK_COLOR_HEX,
  CTK_PALETTE_COLORS
} from '../ctkBoardTheme';
import {
  playerPieceOffsets,
  type PlayerPiece,
  type PlayerRotation
} from './playerRules';

export type PlayerRenderPhase = 'ready' | 'playing' | 'paused' | 'game-over';

export type PlayerRenderPose = {
  piece: PlayerPiece;
  rotation: PlayerRotation;
  x: number;
  y: number;
};

export type PlayerBoardFrame = {
  cells: Uint8Array;
  boardHeight: number;
  visibleRows: number;
  active: PlayerRenderPose | null;
  ghostY: number | null;
  phase: PlayerRenderPhase;
};

export type PlayerRenderAppearance = {
  ghostOpacity: number;
  gridOpacity: number;
};

export function drawPlayerFrame(
  context: CanvasRenderingContext2D,
  pixelWidth: number,
  pixelHeight: number,
  frame: PlayerBoardFrame,
  appearance: PlayerRenderAppearance
): void {
  const visibleRows = Math.max(1, Math.trunc(frame.visibleRows));
  const cellSize = Math.min(
    pixelWidth / CTK_BOARD_WIDTH,
    pixelHeight / visibleRows
  );
  const boardWidth = cellSize * CTK_BOARD_WIDTH;
  const boardHeight = cellSize * visibleRows;
  const originX = (pixelWidth - boardWidth) / 2;
  const originY = (pixelHeight - boardHeight) / 2;

  context.clearRect(0, 0, pixelWidth, pixelHeight);
  context.fillStyle = CTK_BOARD_THEME.board;
  context.fillRect(0, 0, pixelWidth, pixelHeight);
  context.fillStyle = CTK_BOARD_THEME.empty;
  context.fillRect(originX, originY, boardWidth, boardHeight);

  const maximumBoardY = Math.min(visibleRows, frame.boardHeight);
  for (let y = 0; y < maximumBoardY; y += 1) {
    const rowStart = y * CTK_BOARD_WIDTH;
    for (let x = 0; x < CTK_BOARD_WIDTH; x += 1) {
      const code = frame.cells[rowStart + x] ?? 0;
      if (code === 0) continue;
      const color = CTK_PALETTE_COLORS[code - 1];
      if (!color) continue;
      drawBlock(context, originX, originY, cellSize, visibleRows, x, y, CTK_COLOR_HEX[color]);
    }
  }

  if (frame.active && frame.ghostY !== null && frame.ghostY !== frame.active.y) {
    drawPose(
      context,
      originX,
      originY,
      cellSize,
      visibleRows,
      frame.active,
      Math.max(0, Math.min(1, appearance.ghostOpacity)),
      true,
      frame.ghostY
    );
  }
  if (frame.active) {
    drawPose(
      context,
      originX,
      originY,
      cellSize,
      visibleRows,
      frame.active,
      1,
      false,
      frame.active.y
    );
  }

  const gridOpacity = Math.max(0, Math.min(1, appearance.gridOpacity));
  if (gridOpacity > 0) {
    context.save();
    context.strokeStyle = `rgba(216, 226, 222, ${0.22 * gridOpacity})`;
    context.lineWidth = Math.max(1, cellSize * 0.035);
    context.beginPath();
    for (let x = 0; x <= CTK_BOARD_WIDTH; x += 1) {
      const position = originX + x * cellSize;
      context.moveTo(position, originY);
      context.lineTo(position, originY + boardHeight);
    }
    for (let y = 0; y <= visibleRows; y += 1) {
      const position = originY + y * cellSize;
      context.moveTo(originX, position);
      context.lineTo(originX + boardWidth, position);
    }
    context.stroke();
    context.restore();
  }
}

function drawPose(
  context: CanvasRenderingContext2D,
  originX: number,
  originY: number,
  cellSize: number,
  visibleRows: number,
  pose: PlayerRenderPose,
  opacity: number,
  outlineOnly: boolean,
  poseY: number
): void {
  const offsets = playerPieceOffsets(pose.piece, pose.rotation);
  context.save();
  context.globalAlpha = opacity;
  for (const { x: offsetX, y: offsetY } of offsets) {
    const x = pose.x + offsetX;
    const y = poseY + offsetY;
    if (x < 0 || x >= CTK_BOARD_WIDTH || y < 0 || y >= visibleRows) continue;
    const color = CTK_COLOR_HEX[pose.piece];
    if (outlineOnly) {
      const canvasY = originY + (visibleRows - y - 1) * cellSize;
      context.fillStyle = color;
      context.fillRect(
        originX + x * cellSize + cellSize * 0.12,
        canvasY + cellSize * 0.12,
        cellSize * 0.76,
        cellSize * 0.76
      );
      context.strokeStyle = color;
      context.lineWidth = Math.max(1, cellSize * 0.08);
      context.strokeRect(
        originX + x * cellSize + cellSize * 0.08,
        canvasY + cellSize * 0.08,
        cellSize * 0.84,
        cellSize * 0.84
      );
    } else {
      drawBlock(context, originX, originY, cellSize, visibleRows, x, y, color);
    }
  }
  context.restore();
}

function drawBlock(
  context: CanvasRenderingContext2D,
  originX: number,
  originY: number,
  cellSize: number,
  visibleRows: number,
  x: number,
  y: number,
  color: string
): void {
  const canvasX = originX + x * cellSize;
  const canvasY = originY + (visibleRows - y - 1) * cellSize;
  context.fillStyle = color;
  context.fillRect(canvasX, canvasY, cellSize, cellSize);
  const bevel = Math.max(1, cellSize * 0.09);
  context.fillStyle = 'rgba(255, 255, 255, .15)';
  context.fillRect(canvasX, canvasY, cellSize, bevel);
  context.fillRect(canvasX, canvasY, bevel, cellSize);
  context.fillStyle = 'rgba(20, 26, 24, .22)';
  context.fillRect(canvasX, canvasY + cellSize - bevel, cellSize, bevel);
  context.fillRect(canvasX + cellSize - bevel, canvasY, bevel, cellSize);
}
