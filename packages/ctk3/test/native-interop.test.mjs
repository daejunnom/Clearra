import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  decodeCtk3Exact,
  encodeCtk3,
  encodeCtk3Compact,
} from "../dist/index.js";

const fixtureUrl = new URL(
  "../../../tests/fixtures/contracts/ctk3_native_interop.v1.tsv",
  import.meta.url,
);

test("TypeScript and native Rust share canonical rev1/rev2/rev3 KATs", async () => {
  const rows = (await readFile(fixtureUrl, "utf8"))
    .trim()
    .split(/\r?\n/u)
    .slice(1)
    .map((line) => line.split("\t"));

  for (const [name, revision, canonical, compact] of rows) {
    const document = interoperabilityDocument(name);
    assert.equal(encodeCtk3(document), canonical, `${name} canonical`);
    assert.equal(encodeCtk3Compact(document), compact, `${name} compact`);
    assert.equal(decodeCtk3Exact(canonical).width, 10, `${name} decode`);
    assert.equal(payloadRevision(canonical), Number(revision), `${name} revision`);
  }
});

function interoperabilityDocument(name) {
  const E = null;
  const row = (...prefix) => [...prefix, ...Array(10 - prefix.length).fill(E)];
  if (name === "empty") {
    return { width: 10, pages: [{ height: 0, cells: [] }] };
  }
  if (name === "unicode_operation") {
    return {
      width: 10,
      pages: [
        {
          height: 1,
          cells: row("G", E, E, E, "I", "I", "I", "I"),
          comment: "주석 😀",
          operation: { piece: "I", rotation: "right", x: 4, y: 2 },
          flags: { mirror: true },
          garbage: row(E, "G"),
        },
      ],
    };
  }
  if (name === "temporal_repeat") {
    return {
      width: 10,
      pages: Array.from({ length: 8 }, () => ({
        height: 1,
        cells: row("T", "T", "T"),
        comment: "same",
      })),
    };
  }
  if (name === "temporal_moving") {
    return {
      width: 10,
      pages: Array.from({ length: 20 }, (_, index) => ({
        height: 2,
        cells: [
          ...row(...Array(index % 10).fill(E), "T"),
          ...row(...Array((index * 3) % 10).fill(E), "I"),
        ],
        comment: `p${index % 4}`,
        operation: {
          piece: "T",
          rotation: "spawn",
          x: index % 8,
          y: 3,
        },
      })),
    };
  }
  if (name === "temporal_delta") {
    return {
      width: 10,
      pages: Array.from({ length: 12 }, (_, index) => ({
        height: 2,
        cells: [
          ...row("J", "J", "J", "J"),
          ...row(...Array(index % 10).fill(E), "T"),
        ],
        comment: index % 3 === 0 ? "A" : "B",
      })),
    };
  }
  if (name === "shared_field") {
    return {
      width: 10,
      pages: Array.from({ length: 10 }, (_, index) => ({
        height: 2,
        cells: [
          ...row(...Array(10).fill("G")),
          ...row(...Array(index % 7).fill(E), index % 2 ? "S" : "Z"),
        ],
      })),
    };
  }
  throw new Error(`Unknown CTK3 interoperability fixture: ${name}`);
}

function payloadRevision(value) {
  return Buffer.from(value.slice("ctk3_".length), "base64url")[1] & 7;
}
