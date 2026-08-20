import assert from "node:assert/strict";
import test from "node:test";

import { decodeCtk3, encodeCtk3 } from "ctk3";

import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSearchPreviewDocument } from "../src/viewer/search-preview.mjs";

test("slash input preview emits CTK3 and preserves CTK3 piece colors", () => {
  const source = encodeCtk3({
    width: 10,
    pages: [{
      height: 1,
      cells: ["I", "O", "T", "S", "Z", "J", "L", "G", null, null],
    }],
  });
  const preview = buildSearchPreviewDocument(findSlashCommand("path"), [
    { name: "field", value: source },
    { name: "next", value: "I" },
  ]);

  assert.equal(preview.format, "ctk3");
  assert.deepEqual(
    decodeCtk3(preview.source).pages[0].cells,
    ["I", "O", "T", "S", "Z", "J", "L", "G", null, null],
  );
  assert.equal(preview.document.pages[0].comment, undefined);
  assert.equal(decodeCtk3(preview.source).pages[0].comment, undefined);
});

test("grid preview treats lowercase pieces as their colors and occupancy markers as gray", () => {
  const preview = buildSearchPreviewDocument(findSlashCommand("path"), [
    { name: "field", value: "iotszjlX#." },
    { name: "next", value: "I" },
  ]);

  assert.deepEqual(preview.document.pages[0].cells, [
    "I", "O", "T", "S", "Z", "J", "L", "G", "G", null,
  ]);
  assert.equal(preview.document.pages[0].comment, undefined);
});

test("spin-structure compact grids preserve completed rows in the input preview", () => {
  const preview = buildSearchPreviewDocument(findSlashCommand("spin-structure"), [
    { name: "pieces", value: "IOTS" },
    { name: "field", value: "grid:##########/#_________" },
  ]);

  assert.ok(preview);
  assert.equal(preview.document.pages.length, 1);
  assert.equal(preview.document.pages[0].height, 2);
  assert.deepEqual(preview.document.pages[0].cells, [
    "G",
    ...Array(9).fill(null),
    ...Array(10).fill("G"),
  ]);
  assert.deepEqual(
    decodeCtk3(preview.source).pages[0].cells,
    preview.document.pages[0].cells,
  );
});

test("preview decoding cannot reject legacy cells accepted by the search parser", () => {
  const preview = buildSearchPreviewDocument(findSlashCommand("path"), [
    { name: "field", value: "+■#1Xc~□0_" },
    { name: "next", value: "I" },
  ]);

  assert.deepEqual(preview.document.pages[0].cells, [
    "G", "G", "G", "G", "G", null, null, null, null, null,
  ]);
});

test("cover preview represents both base and target fields in one bounded GIF document", () => {
  const preview = buildSearchPreviewDocument(findSlashCommand("cover"), [
    { name: "base", value: "GGGG......" },
    { name: "target", value: "....iiii.." },
    { name: "next", value: "I" },
  ]);

  assert.equal(preview.document.pages.length, 2);
  assert.deepEqual(preview.document.pages[0].cells, [
    "G", "G", "G", "G", null, null, null, null, null, null,
  ]);
  assert.deepEqual(preview.document.pages[1].cells, [
    "G", "G", "G", "G", "I", "I", "I", "I", null, null,
  ]);
});

test("cover preview preserves completed target rows before search-side completion clears", () => {
  const preview = buildSearchPreviewDocument(findSlashCommand("cover"), [
    { name: "base", value: "grid:__________/__________" },
    { name: "target", value: "grid:##########/##########" },
    { name: "next", value: "IOTSZ" },
  ]);

  assert.equal(preview.document.pages.length, 2);
  assert.deepEqual(
    preview.document.pages[1].cells,
    Array(20).fill("G"),
  );
  assert.deepEqual(
    decodeCtk3(preview.source).pages[1].cells,
    Array(20).fill("G"),
  );
});

test("preview preserves source comments without manufacturing board labels", () => {
  const source = encodeCtk3({
    width: 10,
    pages: [{
      height: 1,
      cells: ["T", "T", "T", "T", ...Array(6).fill(null)],
      comment: "source note",
    }],
  });
  const preview = buildSearchPreviewDocument(findSlashCommand("path"), [
    { name: "field", value: source },
    { name: "next", value: "T" },
  ]);

  assert.equal(preview.document.pages[0].comment, "source note");
  assert.equal(decodeCtk3(preview.source).pages[0].comment, "source note");
  assert.doesNotMatch(preview.source, /Input field|Base field|Target delta/u);
});

test("cover preview keeps only source-authored comments", () => {
  const base = encodeCtk3({
    width: 10,
    pages: [{
      height: 1,
      cells: ["G", ...Array(9).fill(null)],
      comment: "base note",
    }],
  });
  const target = encodeCtk3({
    width: 10,
    pages: [{
      height: 1,
      cells: [null, "I", "I", "I", "I", ...Array(5).fill(null)],
      comment: "target note",
    }],
  });
  const preview = buildSearchPreviewDocument(findSlashCommand("cover"), [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
  ]);

  assert.equal(preview.document.pages[0].comment, "base note");
  assert.equal(decodeCtk3(preview.source).pages[0].comment, "base note");
  assert.equal(preview.document.pages[1].comment, "target note");
  assert.equal(decodeCtk3(preview.source).pages[1].comment, "target note");
});
