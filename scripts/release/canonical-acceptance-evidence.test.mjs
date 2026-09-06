import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  CANONICAL_ACCEPTANCE_EVIDENCE_FILE,
  acceptedWasmToolchainsForPages,
  collectLocalToolchains,
  collectReleaseShardToolchains,
  createCanonicalAcceptanceEvidence,
  createReleaseAcceptanceShardEvidence,
  createReleaseGateReports,
  createShardedReleaseGateReports,
  validateCanonicalAcceptanceEvidence,
  validateReleaseAcceptanceShardEvidence,
  validateReleaseJobs,
  verifyCanonicalAcceptanceEvidence,
  writeCanonicalAcceptanceEvidence,
  writeReleaseAcceptanceShardEvidence,
  writeShardedReleaseGateReports,
} from "./canonical-acceptance-evidence.mjs";
import { verifyCanonicalReportHash } from "./canonical-release-evidence.mjs";
import { stampAcceptedPagesBuild } from "./accepted-pages-build.mjs";
import { sealAcceptedWasmBuild } from "./accepted-wasm-build.mjs";
import { sealAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";
import {
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
  clearraWasmCapabilitiesSha256,
} from "../tools/clearra-wasm-build-contract.mjs";

const SOURCE_COMMIT = "7".repeat(40);
const RUN_ID = "12345";
const RUN_ATTEMPT = "1";
const VERSION = "0.8.0";
const BASE_PATH = "/Clearra";
const TOOLCHAINS = Object.freeze({
  rust: "rustc 1.91.0",
  cargo: "cargo 1.91.0",
  node: "v22.18.0",
  npm: "10.9.3",
  wasm_bindgen: "wasm-bindgen 0.2.126",
  cmake: "cmake version 3.31.0",
  powershell: "5.1.26100.4768",
});
const SHARD_TOOLCHAINS = Object.freeze({
  "foundation-no-product-debt": Object.freeze({
    rust: TOOLCHAINS.rust,
    cargo: TOOLCHAINS.cargo,
    cmake: TOOLCHAINS.cmake,
    powershell: TOOLCHAINS.powershell,
  }),
  "foundation-adversarial-correctness": Object.freeze({
    cmake: TOOLCHAINS.cmake,
    powershell: TOOLCHAINS.powershell,
  }),
  "foundation-desktop-host": Object.freeze({
    rust: TOOLCHAINS.rust,
    cargo: TOOLCHAINS.cargo,
    node: TOOLCHAINS.node,
    npm: TOOLCHAINS.npm,
    powershell: TOOLCHAINS.powershell,
  }),
  sanitizer: Object.freeze({
    cmake: TOOLCHAINS.cmake,
    powershell: TOOLCHAINS.powershell,
  }),
  rust: Object.freeze({
    rust: TOOLCHAINS.rust,
    cargo: TOOLCHAINS.cargo,
    node: TOOLCHAINS.node,
    cmake: TOOLCHAINS.cmake,
    powershell: TOOLCHAINS.powershell,
  }),
  pages: TOOLCHAINS,
});

const REQUIRED_JOB_STEPS = Object.freeze(new Map([
  ["metadata", [
    "Require exact main and zero prior canonical success",
    "Bind the exact canonical acceptance run",
  ]],
  ["ctk3", [
    "Build and test CTK3 once",
    "Seal the accepted CTK3 distribution",
    "Upload accepted CTK3 distribution",
  ]],
  ["linux-cli", [
    "Build standalone WASM CPU CLI",
    "Upload Linux CLI artifact",
  ]],
  ["discord-bot", [
    "Verify accepted CTK3 distribution",
    "Verify Clearrabot contracts",
  ]],
  ["release-acceptance-foundation-no-product-debt", [
    "Archive the exact accepted source on Windows",
    "Verify canonical ReleaseAcceptance shard mapping",
    "Run canonical release acceptance NoProductDebt leaf",
    "Seal canonical release acceptance NoProductDebt leaf",
    "Upload canonical release acceptance NoProductDebt leaf",
  ]],
  ["release-acceptance-foundation-adversarial-correctness", [
    "Run canonical release acceptance AdversarialCorrectness leaf",
    "Seal canonical release acceptance AdversarialCorrectness leaf",
    "Upload canonical release acceptance AdversarialCorrectness leaf",
  ]],
  ["release-acceptance-foundation-desktop-host", [
    "Run canonical release acceptance DesktopHost leaf",
    "Seal canonical release acceptance DesktopHost leaf",
    "Upload canonical release acceptance DesktopHost leaf",
  ]],
  ["release-acceptance-sanitizer", [
    "Run canonical release acceptance sanitizer shard",
    "Seal canonical release acceptance sanitizer shard",
    "Upload canonical release acceptance sanitizer shard",
  ]],
  ["release-acceptance-rust", [
    "Download accepted CTK3 distribution",
    "Run canonical release acceptance rust shard",
    "Seal canonical release acceptance rust shard",
    "Upload canonical release acceptance rust shard",
  ]],
  ["release-acceptance-wasm-build", [
    "Run verified WASM build producer",
    "Upload accepted WASM build",
  ]],
  ["release-acceptance-pages", [
    "Download accepted WASM build",
    "Run canonical release acceptance Pages shard",
    "Stamp and verify the accepted Pages build",
    "Seal canonical release acceptance Pages shard",
    "Upload accepted Pages build",
    "Upload canonical release acceptance Pages shard",
  ]],
  ["release-acceptance", [
    "Download all canonical release acceptance shard evidence",
    "Produce canonical release gate evidence",
    "Upload canonical release gate evidence",
  ]],
  ["windows-cli", [
    "Build and exercise standalone WASM CPU CLI",
    "Upload Windows CLI artifact",
  ]],
  ["windows-gui", [
    "Build standalone SvelteKit and Tauri GUI",
    "Stage Windows GUI executable",
    "Upload Windows GUI artifact",
  ]],
]));

function authority() {
  return {
    repository: "daejunnom/Clearra",
    version: VERSION,
    sourceCommit: SOURCE_COMMIT,
    runId: RUN_ID,
    runAttempt: RUN_ATTEMPT,
    basePath: BASE_PATH,
  };
}

function jobsPayload() {
  const jobs = [...REQUIRED_JOB_STEPS.entries()].map(([name, steps], index) => ({
    id: 9000 + index,
    run_id: Number(RUN_ID),
    run_attempt: Number(RUN_ATTEMPT),
    head_sha: SOURCE_COMMIT,
    name,
    status: "completed",
    conclusion: "success",
    steps: steps.map((stepName) => ({
      name: stepName,
      status: "completed",
      conclusion: "success",
    })),
  }));
  return { total_count: jobs.length, jobs };
}

test("release gate reports deterministically bind toolchains and four surfaces", () => {
  const reports = createReleaseGateReports(authority(), TOOLCHAINS);
  assert.equal(reports.gate.status, "passed");
  assert.equal(reports.gate.readiness_open_count, 0);
  assert.deepEqual(
    reports.surfaces.map((report) => report.surface),
    ["desktop", "discord", "native", "wasm"],
  );
  for (const report of [
    reports.gate,
    reports.toolchainManifest,
    reports.index,
    ...reports.surfaces,
  ]) {
    assert.equal(verifyCanonicalReportHash(report), report.report_sha256);
    assert.equal(report.source_commit, SOURCE_COMMIT);
    assert.equal(report.run_id, RUN_ID);
    assert.equal(report.run_attempt, RUN_ATTEMPT);
  }
});

test("six isolated shard reports preserve unique stage ownership and delegated evidence", () => {
  const shards = Object.entries(SHARD_TOOLCHAINS).map(([shard, tools]) =>
    createReleaseAcceptanceShardEvidence(authority(), shard, tools));
  const reports = createShardedReleaseGateReports(authority(), shards);
  assert.equal(reports.gate.execution_mode, "isolated-six-shard");
  assert.deepEqual(
    reports.gate.shards.map((entry) => entry.shard),
    [
      "foundation-no-product-debt",
      "foundation-adversarial-correctness",
      "foundation-desktop-host",
      "sanitizer",
      "rust",
      "pages",
    ],
  );
  assert.deepEqual(
    reports.gate.shards.flatMap((entry) => entry.stages).sort(),
    [
      "NoProductDebt",
      "AdversarialCorrectness",
      "CSanitizer",
      "RustExactTests",
      "ProductE2E",
      "WasmBuildTest",
      "DesktopHost",
      "RenderGolden",
    ].sort(),
  );
  assert.deepEqual(
    reports.gate.delegated_evidence.map((entry) => [
      entry.deferred_by,
      entry.owner_stage,
      entry.owner_shard,
    ]),
    [
      ["NoProductDebt", "RustExactTests", "rust"],
      ["NoProductDebt", "RenderGolden", "rust"],
      ["NoProductDebt", "RenderGolden", "rust"],
      ["NoProductDebt", "DesktopHost", "foundation-desktop-host"],
      ["AdversarialCorrectness", "RustExactTests", "rust"],
    ],
  );
  for (const shard of shards) {
    assert.equal(
      validateReleaseAcceptanceShardEvidence(shard, authority(), shard.shard),
      true,
    );
  }

  assert.equal(
    new Set(reports.gate.shards.flatMap((entry) => entry.stages)).size,
    reports.gate.shards.flatMap((entry) => entry.stages).length,
  );

  const duplicate = [...shards.slice(0, -1), shards[0]];
  assert.throws(
    () => createShardedReleaseGateReports(authority(), duplicate),
    /duplicate or unknown/u,
  );
  const tampered = structuredClone(shards[0]);
  tampered.stages[0] = "DesktopHost";
  assert.throws(
    () => createShardedReleaseGateReports(authority(), [tampered, ...shards.slice(1)]),
    /SHA-256 differs|closed contract/u,
  );

  const inconsistentPages = createReleaseAcceptanceShardEvidence(
    authority(),
    "pages",
    { ...SHARD_TOOLCHAINS.pages, rust: "rustc 9.99.0" },
  );
  assert.throws(
    () => createShardedReleaseGateReports(
      authority(),
      [...shards.slice(0, -1), inconsistentPages],
    ),
    /disagree on the rust toolchain/u,
  );
});

test("toolchain collection uses closed commands and keeps only first version lines", () => {
  const calls = [];
  const tools = collectLocalToolchains({
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return `${command} version\nignored detail\n`;
    },
  });
  assert.equal(Object.keys(tools).length, 7);
  assert.equal(tools.rust, "rustc version");
  assert.equal(tools.cmake, "cmake version");
  assert.equal(calls.length, 7);
  assert.deepEqual(calls[0], ["rustc", ["--version"]]);
});

test("Windows toolchain collection invokes npm through a closed command interpreter", () => {
  const calls = [];
  collectLocalToolchains({
    platform: "win32",
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return `${command} version\n`;
    },
  });

  assert.deepEqual(
    calls[3],
    ["cmd.exe", ["/d", "/s", "/c", "npm.cmd --version"]],
  );
  assert.equal(calls.some(([command]) => command === "npm.cmd"), false);
});

test("shard toolchain collection invokes only the closed shard tool set", () => {
  const calls = [];
  const tools = collectReleaseShardToolchains("sanitizer", {
    run(command, arguments_) {
      calls.push([command, arguments_]);
      return `${command} version\n`;
    },
  });
  assert.deepEqual(Object.keys(tools), ["cmake", "powershell"]);
  assert.deepEqual(calls.map(([command]) => command), ["cmake", "powershell"]);
});

test("Pages shard toolchains are inherited from the producer and checked at the consumer", () => {
  assert.deepEqual(
    acceptedWasmToolchainsForPages(TOOLCHAINS, {
      node: TOOLCHAINS.node,
      npm: TOOLCHAINS.npm,
      powershell: TOOLCHAINS.powershell,
    }),
    TOOLCHAINS,
  );
  assert.throws(
    () => acceptedWasmToolchainsForPages(TOOLCHAINS, {
      node: "v99.0.0",
      npm: TOOLCHAINS.npm,
      powershell: TOOLCHAINS.powershell,
    }),
    /disagrees with the accepted WASM node toolchain/u,
  );
});

test(
  "Windows npm version probe executes while child-process shell expansion stays disabled",
  { skip: process.platform !== "win32" },
  () => {
    assert.match(collectLocalToolchains().npm, /^\d+\.\d+\.\d+/u);
  },
);

test("release job evidence rejects duplicate jobs and any failed required step", () => {
  assert.equal(validateReleaseJobs(jobsPayload(), authority()).length, REQUIRED_JOB_STEPS.size);
  const duplicate = jobsPayload();
  duplicate.jobs.push(structuredClone(duplicate.jobs[0]));
  duplicate.total_count += 1;
  assert.throws(
    () => validateReleaseJobs(duplicate, authority()),
    /metadata exactly once/u,
  );
  const failed = jobsPayload();
  failed.jobs[4].steps[0].conclusion = "failure";
  assert.throws(
    () => validateReleaseJobs(failed, authority()),
    /did not pass exactly once/u,
  );
});

test("canonical acceptance evidence validates accepted inputs and hashes three real products", async () => {
  const fixture = await createFixture();
  const report = await createCanonicalAcceptanceEvidence(fixture.options);
  assert.equal(report.schema_id, "clearra.canonical-acceptance-evidence.v1");
  assert.equal(report.status, "passed");
  assert.equal(report.final_source_fragments.release_artifacts.length, 3);
  assert.deepEqual(
    report.final_source_fragments.surface_reports.map((entry) => entry.surface),
    ["desktop", "discord", "native", "wasm"],
  );
  assert.equal(verifyCanonicalReportHash(report), report.report_sha256);
  assert.equal(validateCanonicalAcceptanceEvidence(report, authority()), true);
  assert.equal(await verifyCanonicalAcceptanceEvidence(report, {
    ...authority(),
    productsDirectory: fixture.products,
  }), true);

  const output = join(fixture.root, CANONICAL_ACCEPTANCE_EVIDENCE_FILE);
  const written = await writeCanonicalAcceptanceEvidence({
    ...fixture.options,
    outputPath: output,
  });
  assert.equal(written.report_sha256, report.report_sha256);
  assert.equal(
    JSON.parse(await readFile(output, "utf8")).report_sha256,
    report.report_sha256,
  );

  await writeFile(
    join(fixture.products, `Clearra-GUI-v${VERSION}-windows-x86_64.exe`),
    "tampered",
  );
  await assert.rejects(
    verifyCanonicalAcceptanceEvidence(report, {
      ...authority(),
      productsDirectory: fixture.products,
    }),
    /downloaded release products differ/u,
  );
  const tampered = await createCanonicalAcceptanceEvidence(fixture.options);
  assert.notEqual(
    tampered.final_source_fragments.release_artifacts[2].sha256,
    report.final_source_fragments.release_artifacts[2].sha256,
  );

  const jobs = jobsPayload();
  jobs.jobs[0].head_sha = "8".repeat(40);
  await writeFile(fixture.jobs, JSON.stringify(jobs));
  await assert.rejects(
    createCanonicalAcceptanceEvidence(fixture.options),
    /exact successful acceptance attempt/u,
  );
});

async function createFixture() {
  const root = await mkdtemp(join(tmpdir(), "clearra-canonical-acceptance-"));
  const gate = join(root, "gate");
  const shardInput = join(root, "shard-input");
  const ctk3 = join(root, "ctk3");
  const pages = join(root, "pages");
  const products = join(root, "products");
  const jobs = join(root, "jobs.json");
  await Promise.all([
    mkdir(gate),
    mkdir(shardInput),
    mkdir(ctk3),
    mkdir(join(pages, "wasm"), { recursive: true }),
    mkdir(products),
  ]);
  for (const [shard, tools] of Object.entries(SHARD_TOOLCHAINS)) {
    await writeReleaseAcceptanceShardEvidence(
      join(shardInput, `clearra-release-acceptance-${shard}-shard.v1.json`),
      authority(),
      shard,
      tools,
    );
  }
  await writeShardedReleaseGateReports(gate, shardInput, authority());

  for (const [name, payload] of [
    ["decodeWorker.js", "decode"],
    ["index.cjs", "cjs"],
    ["index.d.ts", "types"],
    ["index.js", "esm"],
  ]) {
    await writeFile(join(ctk3, name), payload);
  }
  await sealAcceptedCtk3Dist(ctk3, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT);

  const html = `<html><script src="${BASE_PATH}/_app/start.js"></script></html>`;
  const bindings = Buffer.from("export const ready = true;", "utf8");
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  const bindingsSha256 = sha256(bindings);
  const wasmSha256 = sha256(wasm);
  const bindingsPath = `clearra_wasm.${bindingsSha256.slice(0, 24)}.js`;
  const wasmPath = `clearra_wasm_bg.${wasmSha256.slice(0, 24)}.wasm`;
  await Promise.all([
    writeFile(join(pages, "index.html"), html),
    writeFile(join(pages, "404.html"), html),
    writeFile(join(pages, "wasm", "clearra_wasm.js"), bindings),
    writeFile(join(pages, "wasm", bindingsPath), bindings),
    writeFile(join(pages, "wasm", "clearra_wasm_bg.wasm"), wasm),
    writeFile(join(pages, "wasm", wasmPath), wasm),
  ]);
  await writeFile(join(pages, "wasm", "clearra_wasm.manifest.json"), JSON.stringify({
    schema_version: 1,
    build: {
      contract_version: 2,
      source_sha256: "8".repeat(64),
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
  }));
  await sealAcceptedWasmBuild(
    join(pages, "wasm"),
    SOURCE_COMMIT,
    RUN_ID,
    RUN_ATTEMPT,
    TOOLCHAINS,
  );
  await stampAcceptedPagesBuild(pages, {
    sourceCommit: SOURCE_COMMIT,
    acceptedRunId: RUN_ID,
    acceptedRunAttempt: RUN_ATTEMPT,
    basePath: BASE_PATH,
    version: VERSION,
  });

  for (const name of [
    `Clearra-CLI-v${VERSION}-linux-x86_64`,
    `Clearra-CLI-v${VERSION}-windows-x86_64.exe`,
    `Clearra-GUI-v${VERSION}-windows-x86_64.exe`,
  ]) {
    await writeFile(join(products, name), `payload:${name}`);
  }
  await writeFile(jobs, JSON.stringify(jobsPayload()));
  return {
    root,
    products,
    jobs,
    options: {
      repository: "daejunnom/Clearra",
      version: VERSION,
      sourceCommit: SOURCE_COMMIT,
      runId: RUN_ID,
      runAttempt: RUN_ATTEMPT,
      basePath: BASE_PATH,
      jobsPath: jobs,
      gateEvidenceDirectory: gate,
      ctk3Directory: ctk3,
      pagesDirectory: pages,
      productsDirectory: products,
    },
  };
}

function sha256(payload) {
  return createHash("sha256").update(payload).digest("hex");
}
