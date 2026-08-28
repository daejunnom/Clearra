import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  parseRollbackTar,
  validateRollbackPackageBuffer,
} from "./pages-rollback-package.mjs";

const SHA = "1".repeat(40);

function identity() {
  return {
    schema: "clearra.pages.identity.v2",
    sourceCommit: SHA,
    engineBuildId: SHA,
    contractSchemaVersion: "clearra.search.contract.v2",
    supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
    artifactSchemaVersion: "clearra.solution-data.v1",
    version: "0.7.5",
  };
}

function manifest() {
  return {
    build: {
      runtime_identity: {
        source_commit: SHA,
        engine_build_id: SHA,
        contract_schema_version: "clearra.search.contract.v2",
        supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
        artifact_schema_version: "clearra.solution-data.v1",
      },
    },
  };
}

function octal(value, length) {
  const text = value.toString(8).padStart(length - 1, "0");
  return Buffer.from(`${text}\0`, "ascii");
}

function tarHeader(path, size, type = "0") {
  const header = Buffer.alloc(512);
  const name = Buffer.from(path, "utf8");
  if (name.length > 100) {
    throw new Error("test path is too long");
  }
  name.copy(header, 0);
  octal(type === "5" ? 0o755 : 0o644, 8).copy(header, 100);
  octal(0, 8).copy(header, 108);
  octal(0, 8).copy(header, 116);
  octal(size, 12).copy(header, 124);
  octal(0, 12).copy(header, 136);
  header.fill(0x20, 148, 156);
  header[156] = type.charCodeAt(0);
  Buffer.from("ustar\0", "ascii").copy(header, 257);
  Buffer.from("00", "ascii").copy(header, 263);
  let checksum = 0;
  for (const byte of header) {
    checksum += byte;
  }
  const checksumText = checksum.toString(8).padStart(6, "0");
  Buffer.from(`${checksumText}\0 `, "ascii").copy(header, 148);
  return header;
}

function makeTar(entries) {
  const chunks = [];
  for (const entry of entries) {
    const content = entry.type === "5"
      ? Buffer.alloc(0)
      : Buffer.from(entry.content ?? "", "utf8");
    chunks.push(tarHeader(entry.path, content.length, entry.type ?? "0"));
    chunks.push(content);
    const padding = (512 - (content.length % 512)) % 512;
    if (padding > 0) {
      chunks.push(Buffer.alloc(padding));
    }
  }
  chunks.push(Buffer.alloc(1024));
  return Buffer.concat(chunks);
}

function validTar() {
  return makeTar([
    { path: "./", type: "5" },
    { path: "./wasm/", type: "5" },
    { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
    { path: "./wasm/clearra_wasm.manifest.json", content: JSON.stringify(manifest()) },
  ]);
}

test("validates the exact tar hash and both complete identity documents", () => {
  const tar = validTar();
  const expectedTarSha256 = createHash("sha256").update(tar).digest("hex");
  const result = validateRollbackPackageBuffer(tar, {
    expectedSha: SHA,
    expectedTarSha256,
  });
  assert.equal(result.actualDigest, expectedTarSha256);
  assert.equal(result.entries.get("clearra-build-identity.json").type, "0");
});

test("rejects an incorrect tar authority before reading identity", () => {
  assert.throws(
    () => validateRollbackPackageBuffer(validTar(), {
      expectedSha: SHA,
      expectedTarSha256: "2".repeat(64),
    }),
    /differs from the captured SHA-256/u,
  );
});

test("rejects traversal, links, duplicate identities, and forged identity", () => {
  const unsafeArchives = [
    makeTar([{ path: "../escape", content: "bad" }]),
    makeTar([{ path: "./linked", type: "2", content: "" }]),
    makeTar([
      { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
      { path: "./clearra-build-identity.json", content: JSON.stringify(identity()) },
    ]),
    makeTar([
      {
        path: "./clearra-build-identity.json",
        content: JSON.stringify({ ...identity(), sourceCommit: "2".repeat(40) }),
      },
      { path: "./wasm/clearra_wasm.manifest.json", content: JSON.stringify(manifest()) },
    ]),
  ];
  for (const tar of unsafeArchives) {
    if (tar === unsafeArchives[0] || tar === unsafeArchives[1] || tar === unsafeArchives[2]) {
      assert.throws(() => parseRollbackTar(tar));
      continue;
    }
    assert.throws(() => validateRollbackPackageBuffer(tar, {
      expectedSha: SHA,
      expectedTarSha256: createHash("sha256").update(tar).digest("hex"),
    }));
  }
});

test("rejects corrupted headers and data after the tar end marker", () => {
  const corrupted = Buffer.from(validTar());
  corrupted[0] ^= 1;
  assert.throws(() => parseRollbackTar(corrupted), /checksum/u);

  const trailing = Buffer.concat([validTar(), Buffer.alloc(512)]);
  trailing[trailing.length - 1] = 1;
  assert.throws(() => parseRollbackTar(trailing), /after its end marker/u);
});
