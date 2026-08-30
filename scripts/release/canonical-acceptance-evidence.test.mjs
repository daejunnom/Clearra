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
  collectLocalToolchains,
  createCanonicalAcceptanceEvidence,
  createReleaseGateReports,
  validateCanonicalAcceptanceEvidence,
  validateReleaseJobs,
  verifyCanonicalAcceptanceEvidence,
  writeCanonicalAcceptanceEvidence,
  writeReleaseGateReports,
} from "./canonical-acceptance-evidence.mjs";
import { verifyCanonicalReportHash } from "./canonical-release-evidence.mjs";
import { stampAcceptedPagesBuild } from "./accepted-pages-build.mjs";
import { sealAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";

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
  ["release-acceptance", [
    "Run canonical release acceptance",
    "Stamp and verify the accepted Pages build",
    "Upload accepted Pages build",
  ]],
  ["windows-products", [
    "Build and exercise standalone WASM CPU CLI",
    "Build standalone SvelteKit and Tauri GUI",
    "Stage Windows executables",
    "Upload Windows product artifacts",
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

test("release job evidence rejects duplicate jobs and any failed required step", () => {
  assert.equal(validateReleaseJobs(jobsPayload(), authority()).length, 6);
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
  const ctk3 = join(root, "ctk3");
  const pages = join(root, "pages");
  const products = join(root, "products");
  const jobs = join(root, "jobs.json");
  await Promise.all([
    mkdir(gate),
    mkdir(ctk3),
    mkdir(join(pages, "wasm"), { recursive: true }),
    mkdir(products),
  ]);
  await writeReleaseGateReports(gate, authority(), TOOLCHAINS);

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
  await Promise.all([
    writeFile(join(pages, "index.html"), html),
    writeFile(join(pages, "404.html"), html),
    writeFile(join(pages, "wasm", "clearra_wasm.js"), bindings),
    writeFile(join(pages, "wasm", "clearra_wasm_bg.wasm"), wasm),
  ]);
  await writeFile(join(pages, "wasm", "clearra_wasm.manifest.json"), JSON.stringify({
    build: {
      runtime_identity: {
        source_commit: SOURCE_COMMIT,
        engine_build_id: SOURCE_COMMIT,
        contract_schema_version: "clearra.search.contract.v2",
        supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
        artifact_schema_version: "clearra.solution-data.v1",
      },
    },
    bindings: {
      path: "clearra_wasm.js",
      bytes: bindings.byteLength,
      sha256: sha256(bindings),
    },
    wasm: {
      path: "clearra_wasm_bg.wasm",
      bytes: wasm.byteLength,
      sha256: sha256(wasm),
    },
  }));
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
