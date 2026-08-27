import { createHash } from "node:crypto";

import { decodeViewerDocument } from "../viewer/document.mjs";

const PARITY_CONTRACT = "parity-report.v1";
const FIELD_DOCUMENT_CONTRACT = "field-document.v1";
const FIELD_DOCUMENT_SET_CONTRACT = "field-document-set.v1";
const RENDER_CONTRACT = "render-artifact.v1";
const FUMEN_PATTERN = /^v115@[A-Za-z0-9+/?]+$/u;
const CTK3_PATTERN = /^ctk3(?:_|@|b_)\S+$/iu;
const SAFE_FILENAME_PATTERN = /^[a-z0-9][a-z0-9._-]{0,127}$/iu;
const SHA256_PATTERN = /^[a-f0-9]{64}$/u;
const PNG_SIGNATURE = Buffer.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
]);
const MAX_DOCUMENT_PAGES = 4096;
const MAX_DISCORD_DOCUMENTS = 10;
const DEFAULT_DOCUMENT_BYTES = 16 * 1024 * 1024;
const DEFAULT_ARTIFACT_BYTES = 24 * 1024 * 1024;

/**
 * Validate one typed-document utility result and build a bounded Discord
 * response plan. This is deliberately not a portfolio/tie translator: fumen
 * split is an ordered document set, and every other surface owns one canonical
 * public result.
 */
export function buildDiscordDocumentUtilityResult(
  structured,
  processResult,
  publicResultKind,
  options = {},
) {
  rejectAlternativeMetadata(structured);
  switch (publicResultKind) {
    case "parity":
      return parityResult(structured, options.locale);
    case "fumen":
      return fumenResult(structured, options);
    case "render":
      return renderResult(structured, processResult?.artifact, options);
    case "to-gray":
    case "mirror":
      return fieldTransformResult(structured, publicResultKind, options);
    default:
      throw new Error("The typed-document utility result kind is not supported.");
  }
}

function fieldTransformResult(value, transform, options) {
  requireExactKeys(value, [
    "kind",
    "contract_id",
    "result_kind",
    "payload_kind",
    "payload",
  ], `${transform} result`);
  if (
    value.kind !== FIELD_DOCUMENT_CONTRACT ||
    value.contract_id !== FIELD_DOCUMENT_CONTRACT ||
    value.result_kind !== transform ||
    value.payload_kind !== "field-document"
  ) {
    throw new Error(`The ${transform} result contract is invalid.`);
  }
  const document = validateTransformFieldDocument(value.payload);
  const bytes = new TextEncoder().encode(document.document);
  const maximumBytes = positiveLimit(options.maxDocumentBytes, DEFAULT_DOCUMENT_BYTES);
  if (bytes.byteLength > maximumBytes) {
    throw new Error(`The ${transform} document exceeds the Discord document limit.`);
  }
  const korean = isKorean(options.locale);
  const label = transform === "to-gray"
    ? (korean ? "점유 색상 회색화" : "occupied-color normalization")
    : (korean ? "좌우 반전" : "mirror transform");
  const identity = transform === "to-gray"
    ? (korean
        ? "페이지·operation·주석·garbage·크기 identity 보존"
        : "page, operation, comment, garbage, and dimension identity preserved")
    : (korean
        ? "필드·operation 조각/회전·garbage를 함께 반전"
        : "field, operation piece/rotation, and garbage mirrored together");
  return Object.freeze({
    content: korean
      ? `Clearra ${label}을(를) 완료했습니다.\n${document.page_count}페이지 ${document.format} · ${identity}`
      : `Clearra ${label} completed.\n${document.page_count}-page ${document.format} document · ${identity}`,
    files: Object.freeze([Object.freeze({
      name: document.filename,
      description: korean
        ? `Clearra canonical ${document.format} ${label} 문서`
        : `Clearra canonical ${document.format} ${label} document`,
      contentType: "text/plain; charset=utf-8",
      bytes,
    })]),
  });
}

function validateTransformFieldDocument(value) {
  requireExactKeys(value, [
    "format",
    "document",
    "page_count",
    "canonical_sha256",
    "filename",
  ], "field document");
  const formatValid = value.format === "ctk3"
    ? typeof value.document === "string" && CTK3_PATTERN.test(value.document) &&
      safeFilename(value.filename, "ctk3")
    : value.format === "fumen"
      ? typeof value.document === "string" && FUMEN_PATTERN.test(value.document) &&
        safeFilename(value.filename, "txt")
      : false;
  if (
    !formatValid ||
    !boundedInteger(value.page_count, 1, MAX_DOCUMENT_PAGES) ||
    typeof value.canonical_sha256 !== "string" ||
    !SHA256_PATTERN.test(value.canonical_sha256)
  ) {
    throw new Error("The transformed field-document payload is invalid.");
  }
  const digest = createHash("sha256").update(value.document, "utf8").digest("hex");
  const decoded = decodeViewerDocument(value.document, {
    maxPages: MAX_DOCUMENT_PAGES,
    maxSourceChars: DEFAULT_DOCUMENT_BYTES,
  });
  if (digest !== value.canonical_sha256 || decoded.pages.length !== value.page_count) {
    throw new Error("The transformed field-document identity is invalid.");
  }
  return value;
}

function parityResult(value, locale) {
  requireExactKeys(value, [
    "kind",
    "contract_id",
    "result_kind",
    "payload_kind",
    "pages",
  ], "parity result");
  if (
    value.kind !== PARITY_CONTRACT ||
    value.contract_id !== PARITY_CONTRACT ||
    value.result_kind !== "parity" ||
    value.payload_kind !== "parity-report-page" ||
    !Array.isArray(value.pages) ||
    value.pages.length < 1 ||
    value.pages.length > MAX_DOCUMENT_PAGES
  ) {
    throw new Error("The parity result contract is invalid.");
  }
  for (let index = 0; index < value.pages.length; index += 1) {
    validateParityPage(value.pages[index], index + 1, value.pages.length);
  }
  const first = value.pages[0];
  const korean = isKorean(locale);
  const content = korean
    ? [
        "Clearra field-document 패리티 관찰을(를) 완료했습니다.",
        `패리티 관찰 ${first.page_number}/${first.total_pages} 페이지`,
        `점유 셀 ${first.occupied_cell_count}, pending garbage 점유 셀 ${first.pending_garbage_occupied_cell_count}`,
        "가능성 주장: 없음 · pruning 권위: 없음",
      ].join("\n")
    : [
        "Clearra field-document parity observation completed.",
        `Parity observation page ${first.page_number}/${first.total_pages}`,
        `Occupied cells ${first.occupied_cell_count}; pending-garbage occupied cells ${first.pending_garbage_occupied_cell_count}`,
        "Feasibility claim: none · pruning authority: none",
      ].join("\n");
  return Object.freeze({ content, files: Object.freeze([]) });
}

function validateParityPage(page, expectedPage, totalPages) {
  requireExactKeys(page, [
    "document_format",
    "page_number",
    "total_pages",
    "coordinate_basis",
    "width",
    "height",
    "occupied_cell_count",
    "checker_black_count",
    "checker_white_count",
    "checker_delta",
    "four_color_counts",
    "even_column_count",
    "odd_column_count",
    "column_parity_delta",
    "occupied_area_mod_four",
    "pending_garbage_occupied_cell_count",
    "feasibility_claim",
    "pruning_authority",
    "page_handle_available",
  ], `parity page ${expectedPage}`);
  if (
    !["ctk3", "fumen"].includes(page.document_format) ||
    page.page_number !== expectedPage ||
    page.total_pages !== totalPages ||
    !boundedText(page.coordinate_basis, 1, 128) ||
    !boundedInteger(page.width, 1, 4096) ||
    !boundedInteger(page.height, 0, 4096) ||
    !nonNegativeSafeInteger(page.occupied_cell_count) ||
    !nonNegativeSafeInteger(page.checker_black_count) ||
    !nonNegativeSafeInteger(page.checker_white_count) ||
    !Number.isSafeInteger(page.checker_delta) ||
    !Array.isArray(page.four_color_counts) ||
    page.four_color_counts.length !== 4 ||
    page.four_color_counts.some((count) => !nonNegativeSafeInteger(count)) ||
    !nonNegativeSafeInteger(page.even_column_count) ||
    !nonNegativeSafeInteger(page.odd_column_count) ||
    !Number.isSafeInteger(page.column_parity_delta) ||
    !boundedInteger(page.occupied_area_mod_four, 0, 3) ||
    !nonNegativeSafeInteger(page.pending_garbage_occupied_cell_count) ||
    page.feasibility_claim !== false ||
    page.pruning_authority !== "none" ||
    typeof page.page_handle_available !== "boolean"
  ) {
    throw new Error(`Parity page ${expectedPage} is invalid.`);
  }
}

function fumenResult(value, options) {
  requireExactKeys(value, [
    "kind",
    "contract_id",
    "result_kind",
    "payload_kind",
    "payload",
  ], "fumen result");
  if (value.result_kind !== "fumen") {
    throw new Error("The Fumen result kind is invalid.");
  }
  let documents;
  if (
    value.kind === FIELD_DOCUMENT_CONTRACT &&
    value.contract_id === FIELD_DOCUMENT_CONTRACT &&
    value.payload_kind === "field-document"
  ) {
    documents = [validateFieldDocument(value.payload, false)];
  } else if (
    value.kind === FIELD_DOCUMENT_SET_CONTRACT &&
    value.contract_id === FIELD_DOCUMENT_SET_CONTRACT &&
    value.payload_kind === "field-document-set"
  ) {
    requireExactKeys(value.payload, ["document_contract", "documents"], "document set");
    if (
      value.payload.document_contract !== FIELD_DOCUMENT_CONTRACT ||
      !Array.isArray(value.payload.documents) ||
      value.payload.documents.length < 1 ||
      value.payload.documents.length > MAX_DISCORD_DOCUMENTS
    ) {
      throw new Error("The Fumen document set is invalid or exceeds Discord limits.");
    }
    documents = value.payload.documents.map((document) =>
      validateFieldDocument(document, true)
    );
  } else {
    throw new Error("The Fumen result contract is invalid.");
  }

  const maximumBytes = positiveLimit(options.maxDocumentBytes, DEFAULT_DOCUMENT_BYTES);
  let totalBytes = 0;
  const files = documents.map((document) => {
    const bytes = new TextEncoder().encode(document.document);
    totalBytes += bytes.byteLength;
    if (!Number.isSafeInteger(totalBytes) || totalBytes > maximumBytes) {
      throw new Error("The complete Fumen result exceeds the Discord document limit.");
    }
    return Object.freeze({
      name: document.filename,
      description: isKorean(options.locale)
        ? "Clearra canonical v115 Fumen 문서"
        : "Clearra canonical v115 Fumen document",
      contentType: "text/plain; charset=utf-8",
      bytes,
    });
  });
  const pages = documents.reduce((sum, document) => sum + document.page_count, 0);
  const content = isKorean(options.locale)
    ? `Clearra Fumen 문서 변환을(를) 완료했습니다.\ncanonical v115 Fumen ${documents.length}개 · 총 ${pages}페이지`
    : `Clearra Fumen document transform completed.\n${documents.length} canonical v115 Fumen document(s) · ${pages} total page(s)`;
  return Object.freeze({ content, files: Object.freeze(files) });
}

function validateFieldDocument(value, splitMember) {
  requireExactKeys(value, [
    "format",
    "document",
    "page_count",
    "canonical_sha256",
    "filename",
  ], "field document");
  if (
    value.format !== "fumen" ||
    typeof value.document !== "string" ||
    !FUMEN_PATTERN.test(value.document) ||
    !boundedInteger(value.page_count, 1, MAX_DOCUMENT_PAGES) ||
    typeof value.canonical_sha256 !== "string" ||
    !SHA256_PATTERN.test(value.canonical_sha256) ||
    !safeFilename(value.filename, "txt")
  ) {
    throw new Error("The field-document payload is invalid.");
  }
  const digest = createHash("sha256").update(value.document, "utf8").digest("hex");
  const decoded = decodeViewerDocument(value.document, {
    maxPages: MAX_DOCUMENT_PAGES,
    maxSourceChars: DEFAULT_DOCUMENT_BYTES,
  });
  if (
    digest !== value.canonical_sha256 ||
    decoded.pages.length !== value.page_count ||
    (splitMember && value.page_count !== 1)
  ) {
    throw new Error("The field-document identity is invalid.");
  }
  return value;
}

function renderResult(value, artifact, options) {
  requireExactKeys(value, [
    "kind",
    "contract_id",
    "result_kind",
    "payload_kind",
    "payload",
  ], "render result");
  requireExactKeys(value.payload, [
    "document_format",
    "artifact_format",
    "selected_page_number",
    "document_page_count",
    "media_type",
    "filename",
    "byte_length",
    "sha256",
    "render_exact",
    "skin_id",
    "product_max_bytes",
    "transport_max_bytes",
  ], "render payload");
  const payload = value.payload;
  if (
    value.kind !== RENDER_CONTRACT ||
    value.contract_id !== RENDER_CONTRACT ||
    value.result_kind !== "render" ||
    value.payload_kind !== "render-artifact" ||
    !["ctk3", "fumen"].includes(payload.document_format) ||
    !["png", "gif"].includes(payload.artifact_format) ||
    payload.media_type !== (payload.artifact_format === "png" ? "image/png" : "image/gif") ||
    !safeFilename(payload.filename, payload.artifact_format) ||
    !boundedInteger(payload.document_page_count, 1, MAX_DOCUMENT_PAGES) ||
    !nonNegativeSafeInteger(payload.byte_length) ||
    payload.byte_length < 1 ||
    typeof payload.sha256 !== "string" ||
    !SHA256_PATTERN.test(payload.sha256) ||
    payload.render_exact !== true ||
    !boundedText(payload.skin_id, 1, 128) ||
    !nonNegativeSafeInteger(payload.product_max_bytes) ||
    !nonNegativeSafeInteger(payload.transport_max_bytes) ||
    (payload.artifact_format === "png" &&
      !boundedInteger(payload.selected_page_number, 1, payload.document_page_count)) ||
    (payload.artifact_format === "gif" && payload.selected_page_number !== null)
  ) {
    throw new Error("The render payload is invalid.");
  }
  const maximumBytes = positiveLimit(options.maxArtifactBytes, DEFAULT_ARTIFACT_BYTES);
  const bytes = validateRenderTransportArtifact(artifact, payload, maximumBytes);
  const content = isKorean(options.locale)
    ? `Clearra 정확한 field-document 렌더을(를) 완료했습니다.\n정확한 Rust ${payload.artifact_format.toUpperCase()} 렌더 · ${payload.document_page_count}페이지 문서`
    : `Clearra exact field-document render completed.\nExact Rust ${payload.artifact_format.toUpperCase()} render · ${payload.document_page_count}-page document`;
  return Object.freeze({
    content,
    files: Object.freeze([Object.freeze({
      name: payload.filename,
      description: isKorean(options.locale)
        ? "Clearra exact Rust renderer 결과"
        : "Clearra exact Rust renderer artifact",
      contentType: payload.media_type,
      bytes,
    })]),
  });
}

function validateRenderTransportArtifact(value, payload, maximumBytes) {
  if (
    !isRecord(value) ||
    value.contract !== "clearra.discord-render-artifact.v1" ||
    value.artifactFormat !== payload.artifact_format ||
    value.mediaType !== payload.media_type ||
    value.filename !== payload.filename ||
    value.byteLength !== payload.byte_length ||
    value.sha256 !== payload.sha256 ||
    value.renderExact !== true ||
    value.byteLength > maximumBytes ||
    value.byteLength > payload.product_max_bytes ||
    value.byteLength > payload.transport_max_bytes ||
    typeof value.bytesBase64 !== "string"
  ) {
    throw new Error("The render transport artifact does not match its typed metadata.");
  }
  const bytes = Buffer.from(value.bytesBase64, "base64");
  const signature = payload.artifact_format === "png"
    ? bytes.length >= PNG_SIGNATURE.length &&
      bytes.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)
    : bytes.length >= 6 &&
      ["GIF87a", "GIF89a"].includes(bytes.subarray(0, 6).toString("ascii"));
  if (
    bytes.toString("base64") !== value.bytesBase64 ||
    bytes.byteLength !== value.byteLength ||
    createHash("sha256").update(bytes).digest("hex") !== value.sha256 ||
    !signature
  ) {
    throw new Error("The render transport artifact bytes are invalid.");
  }
  return bytes;
}

function rejectAlternativeMetadata(value) {
  if (Array.isArray(value)) {
    for (const item of value) rejectAlternativeMetadata(item);
    return;
  }
  if (!isRecord(value)) return;
  for (const [key, nested] of Object.entries(value)) {
    const normalized = key.toLowerCase().replaceAll("_", "-");
    if (
      normalized === "candidate" ||
      normalized === "candidates" ||
      normalized === "candidate-id" ||
      normalized === "tie" ||
      normalized === "ties" ||
      normalized === "alternative" ||
      normalized === "alternatives"
    ) {
      throw new Error("Typed-document utilities do not expose portfolio or tie metadata.");
    }
    rejectAlternativeMetadata(nested);
  }
}

function requireExactKeys(value, expected, label) {
  if (!isRecord(value)) throw new Error(`${label} must be an object.`);
  const actual = Object.keys(value).sort();
  const canonical = [...expected].sort();
  if (actual.length !== canonical.length || actual.some((key, index) => key !== canonical[index])) {
    throw new Error(`${label} contains an unexpected or missing field.`);
  }
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function safeFilename(value, extension) {
  return typeof value === "string" &&
    SAFE_FILENAME_PATTERN.test(value) &&
    value.toLowerCase().endsWith(`.${extension}`) &&
    !value.includes("..") &&
    !value.includes("/") &&
    !value.includes("\\");
}

function boundedText(value, minimum, maximum) {
  return typeof value === "string" && value.length >= minimum && value.length <= maximum;
}

function boundedInteger(value, minimum, maximum) {
  return Number.isSafeInteger(value) && value >= minimum && value <= maximum;
}

function nonNegativeSafeInteger(value) {
  return Number.isSafeInteger(value) && value >= 0;
}

function positiveLimit(value, fallback) {
  const parsed = value ?? fallback;
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("The Discord typed-document output limit is invalid.");
  }
  return parsed;
}

function isKorean(locale) {
  return String(locale ?? "en").toLowerCase().startsWith("ko");
}
