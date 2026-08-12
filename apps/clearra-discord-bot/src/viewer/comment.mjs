import {
  commentGlyphAdvance,
  paintCommentLine,
} from "./comment-font.mjs";

const MAX_COMMENT_CODE_POINTS = 160;
const MAX_COMMENT_LINES = 3;
const MIN_COMMENT_WIDTH = 80;
const PANEL_PADDING = 4;
const LINE_HEIGHT = 14;
const PANEL_BACKGROUND = 10;
const PANEL_SEPARATOR = 11;
const TEXT_COLOR = 12;

/**
 * Normalizes an untrusted CTK3/Fumen page comment as bounded plain text.
 * Comments are rasterized into GIF pixels; they are never interpreted as
 * Discord, Markdown, HTML, or a GIF application/comment extension.
 */
export function normalizeViewerComment(value) {
  if (typeof value !== "string") return "";
  const normalized = value.normalize("NFC").replaceAll("\r\n", "\n").replaceAll("\r", "\n");
  const safe = [];
  let horizontalSpace = false;
  for (const character of normalized) {
    if (character === "\n") {
      while (safe.at(-1) === " ") safe.pop();
      if (safe.length > 0 && safe.at(-1) !== "\n") safe.push("\n");
      horizontalSpace = false;
      continue;
    }
    if (/\s/u.test(character)) {
      if (safe.length > 0 && safe.at(-1) !== "\n") horizontalSpace = true;
      continue;
    }
    if (/\p{C}/u.test(character)) continue;
    if (horizontalSpace) safe.push(" ");
    safe.push(character);
    horizontalSpace = false;
  }
  while (safe.at(-1) === " " || safe.at(-1) === "\n") safe.pop();
  if (safe.length <= MAX_COMMENT_CODE_POINTS) return safe.join("");
  return `${safe.slice(0, MAX_COMMENT_CODE_POINTS - 1).join("")}…`;
}

export function prepareViewerCommentPanels(pages, boardPixelWidth) {
  if (!Array.isArray(pages) || !Number.isSafeInteger(boardPixelWidth) || boardPixelWidth < 1) {
    throw new Error("Viewer comment layout input is invalid.");
  }
  const comments = pages.map((page) => normalizeViewerComment(page?.comment));
  if (comments.every((comment) => comment.length === 0)) return null;

  const width = Math.max(MIN_COMMENT_WIDTH, boardPixelWidth);
  const textWidth = width - PANEL_PADDING * 2;
  const linesByPage = comments.map((comment) => wrapComment(comment, textWidth));
  const lineCount = Math.max(1, ...linesByPage.map((lines) => lines.length));
  return Object.freeze({
    width,
    height: 1 + PANEL_PADDING * 2 + lineCount * LINE_HEIGHT,
    linesByPage: Object.freeze(
      linesByPage.map((lines) => Object.freeze([...lines])),
    ),
  });
}

export function paintViewerCommentPanel(
  pixels,
  pixelWidth,
  panelTop,
  panel,
  pageIndex,
) {
  if (!panel) return;
  fillRectangle(
    pixels,
    pixelWidth,
    0,
    panelTop,
    panel.width,
    panel.height,
    PANEL_BACKGROUND,
  );
  fillRectangle(pixels, pixelWidth, 0, panelTop, panel.width, 1, PANEL_SEPARATOR);
  const lines = panel.linesByPage[pageIndex] ?? [];
  for (let index = 0; index < lines.length; index += 1) {
    paintCommentLine(
      pixels,
      pixelWidth,
      PANEL_PADDING,
      panelTop + PANEL_PADDING + 1 + index * LINE_HEIGHT,
      lines[index],
      TEXT_COLOR,
    );
  }
}

function wrapComment(comment, maximumWidth) {
  if (!comment) return [];
  const sourceLines = comment.split("\n");
  const output = [];
  let truncated = false;
  for (let sourceIndex = 0; sourceIndex < sourceLines.length; sourceIndex += 1) {
    const characters = [...sourceLines[sourceIndex]];
    if (characters.length === 0) {
      if (output.length > 0) output.push("");
      if (output.length >= MAX_COMMENT_LINES) {
        truncated = sourceIndex < sourceLines.length - 1;
        break;
      }
      continue;
    }
    let cursor = 0;
    while (cursor < characters.length) {
      let end = cursor;
      let width = 0;
      let lastSpace = -1;
      while (end < characters.length) {
        const character = characters[end];
        const nextWidth = width + commentGlyphAdvance(character);
        if (end > cursor && nextWidth > maximumWidth) break;
        width = nextWidth;
        if (character === " ") lastSpace = end;
        end += 1;
      }
      if (end < characters.length && lastSpace >= cursor) end = lastSpace;
      if (end === cursor) end += 1;
      const line = characters.slice(cursor, end).join("").trimEnd();
      if (line) output.push(line);
      cursor = end;
      while (characters[cursor] === " ") cursor += 1;
      if (output.length >= MAX_COMMENT_LINES) {
        truncated = cursor < characters.length || sourceIndex < sourceLines.length - 1;
        break;
      }
    }
    if (output.length >= MAX_COMMENT_LINES) break;
  }
  if (truncated && output.length > 0) {
    output[output.length - 1] = appendEllipsis(output.at(-1), maximumWidth);
  }
  return output;
}

function appendEllipsis(value, maximumWidth) {
  const characters = [...value];
  while (
    characters.length > 0 &&
    measureText(`${characters.join("")}…`) > maximumWidth
  ) {
    characters.pop();
  }
  return `${characters.join("").trimEnd()}…`;
}

function measureText(value) {
  return [...value].reduce(
    (sum, character) => sum + commentGlyphAdvance(character),
    0,
  );
}

function fillRectangle(pixels, width, x, y, rectWidth, rectHeight, color) {
  for (let row = 0; row < rectHeight; row += 1) {
    const start = (y + row) * width + x;
    pixels.fill(color, start, start + rectWidth);
  }
}
