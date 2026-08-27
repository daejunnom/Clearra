import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { encodeCtk3 } from "ctk3";

import { buildDiscordDocumentUtilityResult } from "../src/clearra/document-utility-result.mjs";

test("parity validates every page while returning only the first canonical summary", () => {
  const result = buildDiscordDocumentUtilityResult({
    kind: "parity-report.v1",
    contract_id: "parity-report.v1",
    result_kind: "parity",
    payload_kind: "parity-report-page",
    pages: [parityPage(1, 2, 3), parityPage(2, 2, 5)],
  }, {}, "parity", { locale: "en" });

  assert.match(result.content, /page 1\/2/u);
  assert.match(result.content, /pending-garbage occupied cells 3/u);
  assert.match(result.content, /pruning authority: none/u);
  assert.deepEqual(result.files, []);

  assert.throws(
    () => buildDiscordDocumentUtilityResult({
      kind: "parity-report.v1",
      contract_id: "parity-report.v1",
      result_kind: "parity",
      payload_kind: "parity-report-page",
      pages: [parityPage(1, 2, 3), {
        ...parityPage(2, 2, 5),
        feasibility_claim: true,
      }],
    }, {}, "parity"),
    /Parity page 2 is invalid/u,
  );
});

test("fumen emits a complete normal document set without portfolio metadata", () => {
  const document = "v115@vhAAgH";
  const payload = fieldDocument(document, "clearra-fumen-page-0001.txt");
  const result = buildDiscordDocumentUtilityResult({
    kind: "field-document-set.v1",
    contract_id: "field-document-set.v1",
    result_kind: "fumen",
    payload_kind: "field-document-set",
    payload: {
      document_contract: "field-document.v1",
      documents: [payload],
    },
  }, {}, "fumen", { locale: "en", maxDocumentBytes: 1024 });

  assert.equal(result.files.length, 1);
  assert.equal(result.files[0].name, payload.filename);
  assert.equal(new TextDecoder().decode(result.files[0].bytes), document);

  assert.throws(
    () => buildDiscordDocumentUtilityResult({
      kind: "field-document.v1",
      contract_id: "field-document.v1",
      result_kind: "fumen",
      payload_kind: "field-document",
      payload: { ...payload, candidate_id: "candidate-0001" },
    }, {}, "fumen"),
    /portfolio or tie metadata/u,
  );
});

test("render requires matching bounded binary transport bytes and hides paths", () => {
  const bytes = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    Buffer.from("bounded-test-png"),
  ]);
  const sha256 = digest(bytes);
  const payload = {
    document_format: "ctk3",
    artifact_format: "png",
    selected_page_number: 1,
    document_page_count: 2,
    media_type: "image/png",
    filename: "clearra-render-page-0001.png",
    byte_length: bytes.length,
    sha256,
    render_exact: true,
    skin_id: "clearra-exact-v1",
    product_max_bytes: 4096,
    transport_max_bytes: 4096,
  };
  const result = buildDiscordDocumentUtilityResult({
    kind: "render-artifact.v1",
    contract_id: "render-artifact.v1",
    result_kind: "render",
    payload_kind: "render-artifact",
    payload,
  }, {
    artifact: {
      contract: "clearra.discord-render-artifact.v1",
      artifactFormat: "png",
      mediaType: "image/png",
      filename: payload.filename,
      byteLength: bytes.length,
      sha256,
      bytesBase64: bytes.toString("base64"),
      renderExact: true,
    },
  }, "render", { maxArtifactBytes: 4096 });

  assert.equal(result.files.length, 1);
  assert.deepEqual(result.files[0].bytes, bytes);
  assert.doesNotMatch(JSON.stringify(result), /(?:tmp|outputPath|generated_files)/u);

  assert.throws(
    () => buildDiscordDocumentUtilityResult({
      kind: "render-artifact.v1",
      contract_id: "render-artifact.v1",
      result_kind: "render",
      payload_kind: "render-artifact",
      payload,
    }, { artifact: { bytesBase64: bytes.toString("base64") } }, "render"),
    /does not match/u,
  );
});

test("to-gray and mirror emit one bounded canonical attachment without tie metadata", () => {
  const ctk3 = encodeCtk3({
    width: 4,
    pages: [{
      height: 1,
      cells: ["G", null, "G", null],
      comment: "identity",
      operation: { piece: "T", rotation: "right", x: 1, y: 0 },
      garbage: ["G", null, "G", null],
    }],
  });
  const cases = [
    ["to-gray", {
      format: "ctk3",
      document: ctk3,
      page_count: 1,
      canonical_sha256: digest(ctk3),
      filename: "clearra-to-gray.ctk3",
    }],
    ["mirror", fieldDocument("v115@vhAAgH", "clearra-mirror-v115.txt")],
  ];

  for (const [transform, payload] of cases) {
    const result = buildDiscordDocumentUtilityResult({
      kind: "field-document.v1",
      contract_id: "field-document.v1",
      result_kind: transform,
      payload_kind: "field-document",
      payload,
    }, {}, transform, { locale: "en", maxDocumentBytes: 4096 });
    assert.equal(result.files.length, 1);
    assert.equal(result.files[0].name, payload.filename);
    assert.equal(new TextDecoder().decode(result.files[0].bytes), payload.document);
    assert.doesNotMatch(JSON.stringify(result), /(?:candidate|alternative|outputPath|tmp)/u);
  }

  assert.throws(
    () => buildDiscordDocumentUtilityResult({
      kind: "field-document.v1",
      contract_id: "field-document.v1",
      result_kind: "mirror",
      payload_kind: "field-document",
      payload: { ...cases[1][1], tie: true },
    }, {}, "mirror"),
    /portfolio or tie metadata/u,
  );
});

function parityPage(pageNumber, totalPages, pendingGarbage) {
  return {
    document_format: "ctk3",
    page_number: pageNumber,
    total_pages: totalPages,
    coordinate_basis: "bottom-left",
    width: 10,
    height: 4,
    occupied_cell_count: 4,
    checker_black_count: 2,
    checker_white_count: 2,
    checker_delta: 0,
    four_color_counts: [1, 1, 1, 1],
    even_column_count: 2,
    odd_column_count: 2,
    column_parity_delta: 0,
    occupied_area_mod_four: 0,
    pending_garbage_occupied_cell_count: pendingGarbage,
    feasibility_claim: false,
    pruning_authority: "none",
    page_handle_available: true,
  };
}

function fieldDocument(document, filename) {
  return {
    format: "fumen",
    document,
    page_count: 1,
    canonical_sha256: digest(document),
    filename,
  };
}

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}
