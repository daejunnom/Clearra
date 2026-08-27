import { encodeCtk3 } from "ctk3";

import { extractViewerDocuments } from "./document.mjs";

// Keep preview-only decoding at least as permissive as the authoritative
// search-field decoder. Legacy cells stay accepted internally but are not
// advertised in Discord help.
const EMPTY = /^[C~□._0]$/i;
const FILLED = /^[+■#1XGIOTSZJL]$/i;

export function buildSearchPreviewDocument(command, rawOptions = []) {
  if (["finesse-score", "finesse-score-v2"].includes(command?.input)) {
    return finesseScorePreview(rawOptions);
  }
  const fields = previewFields(command?.input);
  if (fields.length === 0) return null;
  const values = new Map(
    rawOptions
      .filter((option) => typeof option?.name === "string")
      .map((option) => [option.name, option.value]),
  );
  if (fields.some((name) => !values.has(name))) return null;

  const decoded = fields.map((name) => decodeStaticField(values.get(name), name));
  let pages;
  if (decoded.length === 1) {
    pages = [decoded[0]];
  } else {
    const [base, target] = decoded;
    pages = [
      base,
      mergePages(base, target),
    ];
  }
  const document = { width: 10, pages };
  return {
    format: "ctk3",
    source: encodeCtk3(document),
    document,
  };
}

function previewFields(input) {
  if (["cover", "build-cover", "finesse-search"].includes(input)) {
    return ["base", "target"];
  }
  if (
    [
      "pc",
      "pc-v2",
      "pc-path-v2",
      "pc-chance-v2",
      "pc-allspin-exact-v1",
      "pc-allspin-pattern-v1",
      "pc-score-v2",
      "pc-tiling-v2",
      "pc-failed-v2",
      "colored",
      "spin",
      "forward-spin-v2",
      "fixed-next",
      "forward-damage-v2",
      "forward-ren-v1",
      "score-fixed-next",
      "score-fixed-next-v2",
      "pc-score-finder-v2",
      "spin-structure",
      "spin-structure-v2",
      "spin-structure-cover-v1",
      "spin-structure-guaranteed-v1",
    ].includes(input)
  ) {
    return ["field"];
  }
  return [];
}

function finesseScorePreview(rawOptions) {
  const option = rawOptions.find(({ name }) => name === "document");
  if (typeof option?.value !== "string") return null;
  const documents = extractViewerDocuments(option.value, {
    maxDocuments: 2,
    maxPages: 128,
    maxSourceChars: 6_000,
  });
  if (documents.length !== 1) return null;
  const [{ format, source, document }] = documents;
  if (
    document.width !== 10 ||
    !Array.isArray(document.pages) ||
    document.pages.length === 0 ||
    document.pages.some((page) => !page.operation)
  ) return null;
  return { format, source, document };
}

function decodeStaticField(value, name) {
  if (typeof value !== "string") {
    throw new Error(`${name} preview input must be text.`);
  }
  const documents = extractViewerDocuments(value, {
    maxDocuments: 2,
    maxPages: 2,
    maxSourceChars: 6_000,
  });
  if (documents.length === 1) {
    const document = documents[0].document;
    if (document.width !== 10 || document.pages.length !== 1) {
      throw new Error(`${name} preview requires one 10-column page.`);
    }
    const page = document.pages[0];
    if (page.operation) throw new Error(`${name} preview requires a static field.`);
    return staticPage(page);
  }
  return gridPage(value, name);
}

function gridPage(value, name) {
  const source = value.trim();
  const rows = /^grid:/i.test(source)
    ? source.slice(source.indexOf(":") + 1).split("/")
    : source.replaceAll("\r\n", "\n").split("\n");
  if (rows.length < 1 || rows.length > 24 || rows.some((row) => row.length !== 10)) {
    throw new Error(`${name} preview grid must contain 1-24 rows of 10 cells.`);
  }
  const cells = [];
  for (const row of [...rows].reverse()) {
    for (const raw of row) {
      if (EMPTY.test(raw)) cells.push(null);
      else if (FILLED.test(raw)) {
        const color = raw.toUpperCase();
        cells.push("IOTSZJL".includes(color) ? color : "G");
      } else {
        throw new Error(`${name} preview contains an unsupported cell.`);
      }
    }
  }
  return { height: rows.length, cells };
}

function staticPage(page) {
  const height = Number(page.height);
  if (!Number.isSafeInteger(height) || height < 0 || height > 24) {
    throw new Error("The preview page height is invalid.");
  }
  if (!Array.isArray(page.cells) || page.cells.length !== height * 10) {
    throw new Error("The preview page shape is invalid.");
  }
  return {
    height,
    cells: [...page.cells],
    ...(page.flags ? { flags: { ...page.flags } } : {}),
    ...(typeof page.comment === "string" && page.comment.length > 0
      ? { comment: page.comment }
      : {}),
  };
}

function mergePages(base, target) {
  const height = Math.max(base.height, target.height);
  const cells = Array(height * 10).fill(null);
  for (let index = 0; index < base.cells.length; index += 1) {
    cells[index] = base.cells[index];
  }
  for (let index = 0; index < target.cells.length; index += 1) {
    if (target.cells[index] === null) continue;
    if (cells[index] !== null) {
      throw new Error("The preview base and target fields overlap.");
    }
    cells[index] = target.cells[index];
  }
  return {
    height,
    cells,
    ...(typeof target.comment === "string" && target.comment.length > 0
      ? { comment: target.comment }
      : {}),
  };
}
