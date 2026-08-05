import { encodeCtk3 } from "ctk3";

const DEFAULT_MAX_ROWS = 24;
const DEFAULT_MAX_SOURCE_CHARS = 6_000;
const GRID_ROW = /^[#_]{10}$/;
const FENCED_GRID = /^```(?:text|field)?[ \t]*\r?\n([\s\S]*?)\r?\n```$/i;

export function isStandaloneRenderField(value, options = {}) {
  return standaloneGridRows(value, options) !== null;
}

export function extractStandaloneRenderField(value, options = {}) {
  const rows = standaloneGridRows(value, options);
  if (!rows) return null;
  const cells = [];
  for (const row of [...rows].reverse()) {
    for (const cell of row) cells.push(cell === "#" ? "G" : null);
  }
  const document = {
    width: 10,
    pages: [{ height: rows.length, cells }],
  };
  return Object.freeze({
    format: "ctk3",
    source: encodeCtk3(document),
    document,
  });
}

function standaloneGridRows(value, options) {
  if (typeof value !== "string") return null;
  const maxRows = positiveInteger(
    options.maxRows,
    DEFAULT_MAX_ROWS,
    "maxRows",
  );
  const maxSourceChars = positiveInteger(
    options.maxSourceChars,
    DEFAULT_MAX_SOURCE_CHARS,
    "maxSourceChars",
  );
  if (value.length > maxSourceChars) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const fenced = trimmed.match(FENCED_GRID);
  if (trimmed.startsWith("```") && !fenced) return null;
  const source = (fenced?.[1] ?? trimmed).replaceAll("\r\n", "\n");
  const rows = source.split("\n");
  if (rows.length < 1 || rows.length > maxRows || rows.some((row) => !GRID_ROW.test(row))) {
    return null;
  }
  return rows;
}

function positiveInteger(value, fallback, name) {
  const normalized = value ?? fallback;
  if (!Number.isSafeInteger(normalized) || normalized < 1) {
    throw new Error(`Standalone render ${name} must be a positive integer.`);
  }
  return normalized;
}
