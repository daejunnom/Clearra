import {
  ctk3FileSource,
  decodeCtk3,
  inspectCtk3,
  isCtk3,
} from "ctk3";
import { decoder as fumenDecoder } from "tetris-fumen";

const FUMEN_PATTERN = /(?:v11(?:0|5)|[Ddm]115)@[A-Za-z0-9+/?]+/g;
const URL_PATTERN = /https?:\/\/[^\s<>()]+/g;
const CTK_START_PATTERN = /ctk3(?:b_|_|@)/gi;
const PIECES = new Set(["I", "O", "T", "S", "Z", "J", "L"]);

export function extractViewerDocuments(text, options = {}) {
  const limits = viewerLimits(options);
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
    if (
      limits.maxDocuments !== null &&
      unique.size >= limits.maxDocuments
    ) {
      break;
    }
    try {
      const source =
        candidate.format === "ctk3"
          ? normalizeCtkSource(candidate.source)
          : candidate.source;
      const document = decodeViewerDocument(source, options);
      const key = `${candidate.format}:${source}`;
      if (!unique.has(key)) unique.set(key, { ...candidate, source, document });
    } catch {
      // Messages commonly contain punctuation next to a document. Invalid
      // candidates are ignored without suppressing other valid documents.
    }
  }
  return [...unique.values()];
}

export function decodeViewerDocument(source, options = {}) {
  const limits = viewerLimits(options);
  const normalized = source.trim();
  if (isCtk3(normalized)) {
    enforceSourceLimit(normalized, limits.maxSourceChars);
    const info = inspectCtk3(normalized);
    enforcePageLimit(info.pageCount, limits.maxPages);
    return decodeCtk3(normalized);
  }

  const matchedFumen = normalized.match(FUMEN_PATTERN)?.[0];
  if (matchedFumen) {
    enforceSourceLimit(matchedFumen, limits.maxSourceChars);
  }
  const fumen = matchedFumen && /^[Ddm]115@/.test(matchedFumen)
    ? `v${matchedFumen.slice(1)}`
    : matchedFumen;
  if (!fumen) throw new Error("No Fumen or CTK3 document was found.");
  const pages = fumenDecoder.decode(fumen);
  if (pages.length === 0) throw new Error("The Fumen document has no pages.");
  enforcePageLimit(pages.length, limits.maxPages);

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

export function decodeViewerFile(data, options = {}) {
  const limits = viewerLimits(options);
  enforceFileSizeLimit(data, limits.maxFileBytes);
  const source = ctk3FileSource(data);
  enforceSourceLimit(source, limits.maxSourceChars);
  const info = inspectCtk3(source);
  enforcePageLimit(info.pageCount, limits.maxPages);
  return {
    format: "ctk3",
    source,
    document: decodeCtk3(source),
  };
}

function viewerLimits(options) {
  return {
    maxDocuments: optionalPositiveInteger(options.maxDocuments, "maxDocuments"),
    maxPages: optionalPositiveInteger(options.maxPages, "maxPages"),
    maxSourceChars: optionalPositiveInteger(
      options.maxSourceChars,
      "maxSourceChars",
    ),
    maxFileBytes: optionalPositiveInteger(
      options.maxFileBytes ?? options.maxBytes,
      "maxFileBytes",
    ),
  };
}

function optionalPositiveInteger(value, name) {
  if (value === undefined || value === null) return null;
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`Viewer option ${name} must be a positive integer.`);
  }
  return value;
}

function enforceSourceLimit(source, maximum) {
  if (maximum !== null && source.length > maximum) {
    throw new Error(`The viewer document exceeds the ${maximum}-character limit.`);
  }
}

function enforcePageLimit(pageCount, maximum) {
  if (maximum !== null && pageCount > maximum) {
    throw new Error(`The viewer document exceeds the ${maximum}-page limit.`);
  }
}

function enforceFileSizeLimit(data, maximum) {
  if (maximum === null) return;
  const size =
    typeof data === "string"
      ? new TextEncoder().encode(data).byteLength
      : data?.byteLength;
  if (!Number.isSafeInteger(size) || size < 0) {
    throw new Error("The CTK3 file data is invalid.");
  }
  if (size > maximum) {
    throw new Error(`The CTK3 file exceeds the ${maximum}-byte limit.`);
  }
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
