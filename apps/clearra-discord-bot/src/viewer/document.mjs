import {
  decodeCtk3,
  isCtk3,
  parseCtk3File,
} from "ctk3";
import { decoder as fumenDecoder } from "tetris-fumen";

const FUMEN_PATTERN = /v11(?:0|5)@[A-Za-z0-9+/?]+/g;
const URL_PATTERN = /https?:\/\/[^\s<>()]+/g;
const CTK_START_PATTERN = /ctk3(?:b_|_|@)/gi;
const PIECES = new Set(["I", "O", "T", "S", "Z", "J", "L"]);

export function extractViewerDocuments(text) {
  const candidates = [];
  for (const match of text.matchAll(URL_PATTERN)) {
    collectUrlCandidates(match[0], candidates);
  }
  for (const match of text.matchAll(FUMEN_PATTERN)) {
    candidates.push({ format: "fumen", source: match[0] });
  }
  for (const match of text.matchAll(CTK_START_PATTERN)) {
    const source = ctkTokenAt(text, match.index ?? 0);
    if (source) candidates.push({ format: "ctk3", source });
  }

  const unique = new Map();
  for (const candidate of candidates) {
    try {
      const source =
        candidate.format === "ctk3"
          ? normalizeCtkSource(candidate.source)
          : candidate.source;
      const document = decodeViewerDocument(source);
      const key = `${candidate.format}:${source}`;
      if (!unique.has(key)) unique.set(key, { ...candidate, source, document });
    } catch {
      // Messages commonly contain punctuation next to a document. Invalid
      // candidates are ignored without suppressing other valid documents.
    }
  }
  return [...unique.values()];
}

export function decodeViewerDocument(source) {
  const normalized = source.trim();
  if (isCtk3(normalized)) return decodeCtk3(normalized);

  const fumen = normalized.match(FUMEN_PATTERN)?.[0];
  if (!fumen) throw new Error("No Fumen or CTK3 document was found.");
  const pages = fumenDecoder.decode(fumen);
  if (pages.length === 0) throw new Error("The Fumen document has no pages.");

  return {
    width: 10,
    pages: pages.map((page) => {
      const cells = [];
      let height = 0;
      for (let y = 0; y < 23; y += 1) {
        for (let x = 0; x < 10; x += 1) {
          const color = fumenColor(page.field.at(x, y));
          cells.push(color);
          if (color !== null) height = y + 1;
        }
      }
      return {
        height,
        cells: cells.slice(0, height * 10),
        ...(page.comment ? { comment: page.comment } : {}),
        ...(page.operation
          ? {
              operation: {
                piece: page.operation.type,
                rotation: page.operation.rotation,
                x: page.operation.x,
                y: page.operation.y,
              },
            }
          : {}),
        flags: {
          lock: page.flags.lock,
          mirror: page.flags.mirror,
          colorize: page.flags.colorize,
          rise: page.flags.rise,
          quiz: page.flags.quiz,
        },
      };
    }),
  };
}

export function decodeViewerFile(data) {
  const { source, document } = parseCtk3File(data);
  return {
    format: "ctk3",
    source,
    document,
  };
}

function collectUrlCandidates(rawUrl, output) {
  let url;
  try {
    url = new URL(stripTrailingPunctuation(rawUrl));
  } catch {
    return;
  }

  for (const key of ["document", "ctk", "fumen", "d"]) {
    const value = url.searchParams.get(key);
    if (value) {
      output.push({
        format: isCtk3(value) ? "ctk3" : "fumen",
        source: value,
      });
    }
  }

  const hashQuery = url.hash.includes("?")
    ? url.hash.slice(url.hash.indexOf("?") + 1)
    : "";
  if (hashQuery) {
    const hashParameters = new URLSearchParams(hashQuery);
    const value =
      hashParameters.get("document") ??
      hashParameters.get("ctk") ??
      hashParameters.get("fumen") ??
      hashParameters.get("d");
    if (value) {
      output.push({
        format: isCtk3(value) ? "ctk3" : "fumen",
        source: value,
      });
    }
  }

  const rawQuery = url.search.slice(1);
  if (/^(?:v11(?:0|5)@|ctk3(?:b_|_|@))/i.test(rawQuery)) {
    output.push({
      format: /^ctk3/i.test(rawQuery) ? "ctk3" : "fumen",
      source: safelyDecodeURIComponent(rawQuery),
    });
  }
}

function ctkTokenAt(text, start) {
  let end = start;
  while (end < text.length && !/\s/.test(text[end])) end += 1;
  return stripTrailingPunctuation(text.slice(start, end));
}

function normalizeCtkSource(source) {
  const compact = source.match(/ctk3_[A-Za-z0-9_-]+/i)?.[0];
  if (compact) return compact;
  const bundle = source.match(/ctk3b_[A-Za-z0-9_.-]+/i)?.[0];
  if (bundle) return bundle;
  return stripTrailingPunctuation(source.trim());
}

function stripTrailingPunctuation(value) {
  return value.replace(/[),;]+$/g, "");
}

function safelyDecodeURIComponent(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function fumenColor(value) {
  if (value === "_") return null;
  if (value === "X" || value === "GRAY") return "G";
  if (PIECES.has(value)) return value;
  throw new Error(`Unsupported Fumen color: ${value}`);
}
