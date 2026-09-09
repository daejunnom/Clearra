import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  ACCEPTED_WASM_BUILD_RECEIPT,
  collectAcceptedWasmProducerToolchains,
  rebindAcceptedWasmBuild,
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
const TRY_REUSE_SCRIPT = fileURLToPath(
  new URL("./try-reuse-accepted-wasm-build.mjs", import.meta.url),
);

test("rebinds byte-identical exact-source payload under the current run authority", async () => {
  const fixture = await createFixture();
  const destination = `${fixture.root}-rebound`;
  try {
    const previous = await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    const before = new Map(await Promise.all(previous.files.map(async (file) => [
      file.path,
      await readFile(join(fixture.root, file.path)),
    ])));
    const reuse = await rebindAcceptedWasmBuild(
      fixture.root,
      destination,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      "123457",
      "1",
    );
    assert.equal(reuse.previous_run_id, RUN_ID);
    assert.equal(reuse.current_run_id, "123457");
    assert.equal(reuse.payload_sha256, previous.payload_sha256);

    const rebound = await verifyAcceptedWasmBuild(
      destination,
      SOURCE_COMMIT,
      "123457",
      "1",
    );
    assert.equal(rebound.payload_sha256, previous.payload_sha256);
    assert.deepEqual(rebound.toolchains, TOOLCHAINS);
    for (const file of rebound.files) {
      assert.deepEqual(await readFile(join(destination, file.path)), before.get(file.path));
    }
    assert.equal(
      JSON.parse(await readFile(join(fixture.root, ACCEPTED_WASM_BUILD_RECEIPT), "utf8")).run_id,
      RUN_ID,
    );
    assert.equal(
      JSON.parse(await readFile(join(destination, ACCEPTED_WASM_BUILD_RECEIPT), "utf8")).run_id,
      "123457",
    );
  } finally {
    await rm(destination, { recursive: true, force: true });
    await fixture.dispose();
  }
});

test("WASM rebinding rejects reruns, overlap, tampering, source drift, and an existing destination", async () => {
  const fixture = await createFixture();
  const destination = `${fixture.root}-rejected`;
  try {
    await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, destination, SOURCE_COMMIT, RUN_ID, "2", "123457", "1",
      ),
      /requires first-attempt canonical runs/u,
    );
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, destination, SOURCE_COMMIT, RUN_ID, "1", "123457", "2",
      ),
      /requires first-attempt canonical runs/u,
    );
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, fixture.root, SOURCE_COMMIT, RUN_ID, "1", "123457", "1",
      ),
      /must not overlap/u,
    );
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, destination, "b".repeat(40), RUN_ID, "1", "123457", "1",
      ),
      /source commit mismatch|expected source identity/u,
    );
    await writeFile(join(fixture.root, "clearra_wasm.js"), "tampered", "utf8");
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, destination, SOURCE_COMMIT, RUN_ID, "1", "123457", "1",
      ),
      /closed regular-file set|alias differs/u,
    );
    await writeFile(destination, "occupied", "utf8");
    await assert.rejects(
      rebindAcceptedWasmBuild(
        fixture.root, destination, SOURCE_COMMIT, RUN_ID, "1", "123457", "1",
      ),
      /must not already exist/u,
    );
  } finally {
    await rm(destination, { recursive: true, force: true });
    await fixture.dispose();
  }
});

test("optional reuse CLI publishes a verified hit and turns a rejected input into a clean miss", async () => {
  const fixture = await createFixture();
  const destination = `${fixture.root}-cli-rebound`;
  const rejectedDestination = `${fixture.root}-cli-rejected`;
  const output = `${fixture.root}-github-output.txt`;
  const rejectedOutput = `${fixture.root}-github-output-rejected.txt`;
  try {
    await sealAcceptedWasmBuild(
      fixture.root,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
      TOOLCHAINS,
    );
    await Promise.all([writeFile(output, ""), writeFile(rejectedOutput, "")]);
    const hit = runTryReuse(fixture.root, destination, output, "123457");
    assert.equal(hit.status, 0, hit.stderr);
    assert.match(await readFile(output, "utf8"), /^reused=true\npayload_sha256=[0-9a-f]{64}\n$/u);
    await verifyAcceptedWasmBuild(destination, SOURCE_COMMIT, "123457", "1");

    await writeFile(join(fixture.root, "clearra_wasm.js"), "tampered", "utf8");
    const miss = runTryReuse(fixture.root, rejectedDestination, rejectedOutput, "123458");
    assert.equal(miss.status, 0, miss.stderr);
    assert.equal(await readFile(rejectedOutput, "utf8"), "reused=false\n");
    await assert.rejects(readFile(rejectedDestination), { code: "ENOENT" });
  } finally {
    await Promise.all([
      rm(destination, { recursive: true, force: true }),
      rm(rejectedDestination, { recursive: true, force: true }),
      rm(output, { force: true }),
      rm(rejectedOutput, { force: true }),
    ]);
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

test("Linux producer toolchain capture uses pwsh without a command shell", () => {
  const calls = [];
  collectAcceptedWasmProducerToolchains({
    platform: "linux",
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return `${command} version\n`;
    },
  });
  assert.deepEqual(calls.find(([command]) => command === "pwsh"), [
    "pwsh",
    ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"],
  ]);
  assert.equal(calls.some(([command]) => command === "powershell"), false);
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

function runTryReuse(source, destination, output, currentRunId) {
  return spawnSync(process.execPath, [
    TRY_REUSE_SCRIPT,
    "--source", source,
    "--destination", destination,
    "--source-commit", SOURCE_COMMIT,
    "--previous-run-id", RUN_ID,
    "--previous-run-attempt", RUN_ATTEMPT,
    "--current-run-id", currentRunId,
    "--current-run-attempt", "1",
    "--github-output", output,
  ], {
    encoding: "utf8",
    shell: false,
    windowsHide: true,
  });
}
