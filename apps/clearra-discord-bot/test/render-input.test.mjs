import assert from "node:assert/strict";
import test from "node:test";

import {
  extractStandaloneRenderField,
  isStandaloneRenderField,
} from "../src/viewer/render-input.mjs";

test("standalone render fields accept plain top-first #/_ grids", () => {
  const source = [
    "#_________",
    "_________#",
  ].join("\n");

  assert.equal(isStandaloneRenderField(source), true);
  const extracted = extractStandaloneRenderField(source);
  assert.ok(extracted);
  assert.equal(extracted.format, "ctk3");
  assert.equal(extracted.document.width, 10);
  assert.equal(extracted.document.pages.length, 1);
  assert.equal(extracted.document.pages[0].height, 2);
  assert.deepEqual(extracted.document.pages[0].cells, [
    null, null, null, null, null, null, null, null, null, "G",
    "G", null, null, null, null, null, null, null, null, null,
  ]);
});

test("standalone render fields accept fenced grids, CRLF, and outer whitespace", () => {
  const fenced = [
    "  ```field",
    "##########",
    "__________",
    "```  ",
  ].join("\r\n");

  assert.equal(isStandaloneRenderField(fenced), true);
  assert.deepEqual(
    extractStandaloneRenderField(fenced).document.pages[0].cells,
    [
      ...Array(10).fill(null),
      ...Array(10).fill("G"),
    ],
  );

  const plain = "\r\n\t__________\r\n##########\r\n  ";
  assert.equal(isStandaloneRenderField(plain), true);
  assert.equal(extractStandaloneRenderField(plain).document.pages[0].height, 2);
});

test("standalone render fields allow from one through twenty-four rows", () => {
  const oneRow = "__________";
  const twentyFourRows = Array.from(
    { length: 24 },
    (_, index) => index % 2 === 0 ? "##########" : "__________",
  ).join("\n");

  assert.equal(isStandaloneRenderField(oneRow), true);
  assert.equal(extractStandaloneRenderField(oneRow).document.pages[0].height, 1);
  assert.equal(isStandaloneRenderField(twentyFourRows), true);
  assert.equal(
    extractStandaloneRenderField(twentyFourRows).document.pages[0].height,
    24,
  );
});

test("standalone render fields reject prose, malformed dimensions, and non-neutral cells", () => {
  const invalid = [
    "field: __________",
    "_________",
    "___________",
    Array(25).fill("__________").join("\n"),
    "IIII______",
    "GGGG______",
    "....______",
    "IOTSZJLG__",
  ];

  for (const source of invalid) {
    assert.equal(isStandaloneRenderField(source), false, source);
    assert.equal(extractStandaloneRenderField(source), null, source);
  }
});

test("standalone render fields reject incomplete or decorated fences", () => {
  const invalid = [
    "```field\n__________",
    "__________\n```",
    "```field\n__________\n``",
    "```field\n__________\n``` trailing prose",
    "leading prose ```field\n__________\n```",
  ];

  for (const source of invalid) {
    assert.equal(isStandaloneRenderField(source), false, source);
    assert.equal(extractStandaloneRenderField(source), null, source);
  }
});
