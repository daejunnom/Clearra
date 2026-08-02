import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArguments,
  normalizeSearchField,
} from "../src/discord/slash-command-input.mjs";

test("structured PC options use CTK3 masks directly without Fumen conversion", () => {
  const ctk3 = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["G", "G", "G", "G", ...Array(6).fill(null)],
      },
    ],
  });

  const arguments_ = buildSlashCommandArguments(findSlashCommand("path"), [
    { name: "field", value: ctk3 },
    { name: "next", value: "*!" },
    { name: "options", value: "clear=2 hold=avoid" },
  ]);

  assert.deepEqual(arguments_, [
    "sfinder",
    "path",
    "--field-mask-v1",
    "000000000000000f",
    "--patterns",
    "*!",
    "--lines",
    "2",
    "--hold",
    "false",
  ]);
  assert.equal(arguments_.some((value) => value === "--fumen"), false);
  assert.equal(arguments_.some((value) => /^v115@/.test(value)), false);
});

test("raw Fumen and an encoded Fumen URL use the same colorless field decoder", () => {
  const source = fumenEncoder.encode([{ field: Field.create("TTT_______") }]);
  const url = `https://example.test/view?fumen=${encodeURIComponent(source)}`;

  assert.deepEqual(normalizeSearchField(source), {
    format: "occupied-field",
    sourceFormat: "fumen",
    occupied: 7n,
    mask: "0000000000000007",
    height: 1,
  });
  assert.deepEqual(normalizeSearchField(url), normalizeSearchField(source));
});

test("CTK3 stays single-page and cover accepts two colorless CTK3/Fumen fields", () => {
  const source = encodeCtk3({
    width: 10,
    pages: [
      { height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] },
      { height: 1, cells: ["O", "O", "O", "O", ...Array(6).fill(null)] },
    ],
  });

  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        { name: "field", value: source },
        { name: "next", value: "I" },
      ]),
    /exactly one page/,
  );

  const base = fumenEncoder.encode([{ field: Field.create("________XX") }]);
  const target = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] }],
  });
  const arguments_ = buildSlashCommandArguments(findSlashCommand("cover"), [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
    { name: "options", value: "hold=avoid" },
  ]);
  assert.deepEqual(arguments_, [
    "sfinder",
    "cover",
    "--base-mask-v1",
    `${"0".repeat(57)}300`,
    "--target-mask-v1",
    `${"0".repeat(59)}f`,
    "--patterns",
    "I",
    "--hold",
    "false",
  ]);
});

test("compressed multi-page CTK3 is rejected from metadata before full decoding", () => {
  const source = encodeCtk3({
    width: 10,
    pages: Array.from({ length: 4096 }, () => ({ height: 0, cells: [] })),
  });
  assert.equal(source.length < 100, true);
  assert.throws(() => normalizeSearchField(source), /exactly one page/);
});

test("CTK3 colors share one occupied-field projection", () => {
  const mixed = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["G", "I", "O", "T", "S", "Z", "J", "L", null, null],
      },
    ],
  });
  const grey = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: [...Array(8).fill("G"), null, null],
      },
    ],
  });
  assert.equal(normalizeSearchField(mixed).mask, "00000000000000ff");
  assert.deepEqual(normalizeSearchField(mixed), normalizeSearchField(grey));
});

test("Fumen grey and piece colors share the same occupied-field projection", () => {
  const mixed = fumenEncoder.encode([{ field: Field.create("XIOT______") }]);
  const grey = fumenEncoder.encode([{ field: Field.create("XXXX______") }]);
  assert.equal(normalizeSearchField(mixed).mask, "000000000000000f");
  assert.equal(normalizeSearchField(grey).mask, "000000000000000f");
});

test("CTK3 static-field projection rejects operations, garbage, and Board64 overflow", () => {
  const operation = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 0,
        cells: [],
        operation: { piece: "T", rotation: "spawn", x: 4, y: 0 },
      },
    ],
  });
  assert.throws(() => normalizeSearchField(operation), /contains an operation/);

  const garbage = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 0,
        cells: [],
        garbage: ["G", ...Array(9).fill(null)],
      },
    ],
  });
  assert.throws(() => normalizeSearchField(garbage), /non-empty garbage row/);

  const cells = Array(70).fill(null);
  cells[64] = "G";
  const overflow = encodeCtk3({
    width: 10,
    pages: [{ height: 7, cells }],
  });
  assert.throws(() => normalizeSearchField(overflow), /64-bit field range/);
});

test("cover rejects overlap, empty/non-tetromino targets, and completed base rows", () => {
  const field = (cells) => encodeCtk3({
    width: 10,
    pages: [{ height: Math.ceil(cells.length / 10), cells }],
  });
  const empty = field([]);
  const one = field(["G", ...Array(9).fill(null)]);
  const four = field(["G", "G", "G", "G", ...Array(6).fill(null)]);
  const full = field(Array(10).fill("G"));
  const run = (base, target) => buildSlashCommandArguments(findSlashCommand("cover"), [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
  ]);

  assert.throws(() => run(empty, empty), /at least one occupied cell/);
  assert.throws(() => run(empty, one), /divisible by four/);
  assert.throws(() => run(four, four), /must not overlap/);
  assert.throws(() => run(full, field([
    ...Array(10).fill(null),
    "I", "I", "I", "I", ...Array(6).fill(null),
  ])), /completed row/);
});

test("legacy CTK3 query URLs keep payload punctuation intact", () => {
  const source = "ctk3@.:)aB*t&hPEXlYu:YoUl/4cH8Ga[PB0z";
  const url = `https://example.test/view?ctk=${encodeURIComponent(source)}`;
  assert.equal(normalizeSearchField(url).format, "occupied-field");
});

test("optional settings are command-specific and cannot override host policy", () => {
  const source = fumenEncoder.encode([{ field: Field.create() }]);
  const base = [
    { name: "field", value: source },
    { name: "next", value: "I" },
  ];

  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        ...base,
        { name: "options", value: "workers=64" },
      ]),
    /does not support options key 'workers'/,
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        ...base,
        { name: "options", value: "clear=4 lines=3" },
      ]),
    /may be specified only once/,
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("spin"), [
        ...base,
        { name: "options", value: "type=TSM" },
      ]),
    /TSM is unavailable/,
  );
});

test("fixed queues, remaining inventories, and verify scopes fail closed", () => {
  const source = fumenEncoder.encode([{ field: Field.create() }]);
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("cat-finder"), [
        { name: "field", value: source },
        { name: "next", value: "*!" },
      ]),
    /exact queue/,
  );
  assert.deepEqual(
    buildSlashCommandArguments(findSlashCommand("pc-setup"), [
      { name: "remaining", value: "tio" },
    ]),
    ["sfinder", "pc-setup", "TIO"],
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("verify"), [
        { name: "scope", value: "files" },
      ]),
    /Verify scope/,
  );
});

test("unsupported CTK3 width and Fumen v110 fail before execution", () => {
  const source = encodeCtk3({
    width: 12,
    pages: [{ height: 0, cells: [] }],
  });
  assert.throws(() => normalizeSearchField(source), /exactly 10 columns/);
  assert.throws(
    () => normalizeSearchField("v110@vhAAgH"),
    /v110 Fumen is not supported/,
  );
});
