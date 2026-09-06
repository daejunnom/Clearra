import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  ACCEPTED_WASM_BUILD_RECEIPT,
  collectAcceptedWasmProducerToolchains,
  sealAcceptedWasmBuild,
  verifyAcceptedWasmBuild,
} from "./accepted-wasm-build.mjs";
import {
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
  clearraWasmCapabilitiesSha256,
} from "../tools/clearra-wasm-build-contract.mjs";

const SOURCE_COMMIT = "a".repeat(40);
const RUN_ID = "123456";
const RUN_ATTEMPT = "1";
const TOOLCHAINS = Object.freeze({
  cargo: "cargo 1.91.0",
  cmake: "cmake version 3.31.0",
  node: "v22.18.0",
  npm: "10.9.3",
  powershell: "5.1.26100.4768",
  rust: "rustc 1.91.0",
  wasm_bindgen: "wasm-bindgen 0.2.126",
});

test("accepted WASM receipt binds the closed payload, source, run, and producer toolchains", async () => {
  const fixture = await createFixture();
  try {
    const receipt = await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    assert.equal(receipt.source_commit, SOURCE_COMMIT);
    assert.equal(receipt.run_id, RUN_ID);
    assert.equal(receipt.run_attempt, RUN_ATTEMPT);
    assert.deepEqual(receipt.toolchains, TOOLCHAINS);
    assert.match(receipt.payload_sha256, /^[0-9a-f]{64}$/u);
    assert.equal(receipt.files.some((entry) => entry.path === "clearra_wasm.manifest.json"), true);

    const verified = await verifyAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
    );
    assert.deepEqual(verified.files, receipt.files);
    assert.equal(
      JSON.parse(await readFile(join(fixture.root, ACCEPTED_WASM_BUILD_RECEIPT), "utf8"))
        .payload_sha256,
      receipt.payload_sha256,
    );
  } finally {
    await fixture.dispose();
  }
});

test("accepted WASM verification rejects tampering and unsealed extra files", async () => {
  const fixture = await createFixture();
  try {
    await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    await writeFile(join(fixture.root, "clearra_wasm.js"), "tampered", "utf8");
    await assert.rejects(
      verifyAcceptedWasmBuild(fixture.root, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /closed regular-file set|alias differs/u,
    );
  } finally {
    await fixture.dispose();
  }

  const extraFixture = await createFixture();
  try {
    await sealAcceptedWasmBuild(
      extraFixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    await writeFile(join(extraFixture.root, "unexpected.bin"), "unexpected", "utf8");
    await assert.rejects(
      verifyAcceptedWasmBuild(extraFixture.root, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /closed regular-file set/u,
    );
  } finally {
    await extraFixture.dispose();
  }
});

test("accepted WASM verification rejects cross-source and cross-attempt reuse", async () => {
  const fixture = await createFixture();
  try {
    await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    await assert.rejects(
      verifyAcceptedWasmBuild(fixture.root, "b".repeat(40), RUN_ID, RUN_ATTEMPT),
      /source commit mismatch/u,
    );
    await assert.rejects(
      verifyAcceptedWasmBuild(fixture.root, SOURCE_COMMIT, RUN_ID, "2"),
      /run attempt mismatch/u,
    );
  } finally {
    await fixture.dispose();
  }
});

test("accepted WASM sealing fails closed for a partial or mismatched product payload", async () => {
  const fixture = await createFixture();
  try {
    await rm(join(fixture.root, fixture.wasmPath));
    await assert.rejects(
      sealAcceptedWasmBuild(
        fixture.root,
        SOURCE_COMMIT,
        RUN_ID,
        RUN_ATTEMPT,
        TOOLCHAINS,
      ),
      /WASM is missing/u,
    );
  } finally {
    await fixture.dispose();
  }

  const sourceFixture = await createFixture();
  try {
    await assert.rejects(
      sealAcceptedWasmBuild(
        sourceFixture.root,
        "b".repeat(40),
        RUN_ID,
        RUN_ATTEMPT,
        TOOLCHAINS,
      ),
      /expected source identity/u,
    );
  } finally {
    await sourceFixture.dispose();
  }
});

test("producer toolchain capture uses the closed seven-command set", () => {
  const calls = [];
  const toolchains = collectAcceptedWasmProducerToolchains({
    platform: "win32",
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return `${command} version\nignored\n`;
    },
  });
  assert.deepEqual(Object.keys(toolchains).sort(), Object.keys(TOOLCHAINS).sort());
  assert.equal(calls.length, 7);
  assert.deepEqual(calls.find(([command]) => command === "cmd.exe"), [
    "cmd.exe",
    ["/d", "/s", "/c", "npm.cmd --version"],
  ]);
});

async function createFixture() {
  const root = await mkdtemp(join(tmpdir(), "clearra-accepted-wasm-"));
  const bindings = Buffer.from("export const ready = true;", "utf8");
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  const bindingsSha256 = sha256(bindings);
  const wasmSha256 = sha256(wasm);
  const bindingsPath = `clearra_wasm.${bindingsSha256.slice(0, 24)}.js`;
  const wasmPath = `clearra_wasm_bg.${wasmSha256.slice(0, 24)}.wasm`;
  const manifest = {
    schema_version: 1,
    build: {
      contract_version: 2,
      source_sha256: "c".repeat(64),
      source_file_count: 1,
      capabilities_sha256: clearraWasmCapabilitiesSha256(),
      runtime_identity: {
        source_commit: SOURCE_COMMIT,
        engine_build_id: SOURCE_COMMIT,
        contract_schema_version: CLEARRA_CONTRACT_SCHEMA_VERSION,
        supply_semantics_id: CLEARRA_SUPPLY_SEMANTICS_ID,
        artifact_schema_version: CLEARRA_ARTIFACT_SCHEMA_VERSION,
      },
    },
    bindings: {
      path: bindingsPath,
      bytes: bindings.byteLength,
      sha256: bindingsSha256,
    },
    wasm: {
      path: wasmPath,
      bytes: wasm.byteLength,
      sha256: wasmSha256,
    },
  };
  await Promise.all([
    writeFile(join(root, "clearra_wasm.js"), bindings),
    writeFile(join(root, bindingsPath), bindings),
    writeFile(join(root, "clearra_wasm_bg.wasm"), wasm),
    writeFile(join(root, wasmPath), wasm),
    writeFile(join(root, "clearra_wasm.manifest.json"), `${JSON.stringify(manifest)}\n`, "utf8"),
  ]);
  return {
    root,
    wasmPath,
    async dispose() {
      await rm(root, { recursive: true, force: true });
    },
  };
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}
