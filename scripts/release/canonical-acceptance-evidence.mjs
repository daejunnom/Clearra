import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import { verifyAcceptedPagesBuild } from "./accepted-pages-build.mjs";
import { verifyAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";

export const CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA =
  "clearra.canonical-acceptance-evidence.v1";
export const RELEASE_GATE_INDEX_FILE = "clearra-release-gate-index.v1.json";
export const CANONICAL_ACCEPTANCE_EVIDENCE_FILE =
  "clearra-canonical-acceptance-evidence.v1.json";
export const RELEASE_ACCEPTANCE_SHARD_SCHEMA =
  "clearra.release-acceptance-shard.v1";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const VERSION = /^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const WORKFLOW_PATH = ".github/workflows/release-cli.yml";
const RELEASE_STAGES = Object.freeze([
  "NoProductDebt",
  "AdversarialCorrectness",
  "CSanitizer",
  "RustExactTests",
  "ProductE2E",
  "WasmBuildTest",
  "DesktopHost",
  "RenderGolden",
]);
export const RELEASE_ACCEPTANCE_SHARDS = Object.freeze(new Map([
  ["foundation-no-product-debt", Object.freeze({
    job: "release-acceptance-foundation-no-product-debt",
    stages: Object.freeze(["NoProductDebt"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard FoundationNoProductDebt -ExecutionSurface Trusted",
    toolchains: Object.freeze(["rust", "cargo", "cmake", "powershell"]),
  })],
  ["foundation-adversarial-correctness", Object.freeze({
    job: "release-acceptance-foundation-adversarial-correctness",
    stages: Object.freeze(["AdversarialCorrectness"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard FoundationAdversarialCorrectness -ExecutionSurface Trusted",
    toolchains: Object.freeze(["cmake", "powershell"]),
  })],
  ["foundation-desktop-host", Object.freeze({
    job: "release-acceptance-foundation-desktop-host",
    stages: Object.freeze(["DesktopHost"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard FoundationDesktopHost -ExecutionSurface Trusted",
    toolchains: Object.freeze(["rust", "cargo", "node", "npm", "powershell"]),
  })],
  ["sanitizer", Object.freeze({
    job: "release-acceptance-sanitizer",
    stages: Object.freeze(["CSanitizer"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Sanitizer -ExecutionSurface Trusted",
    toolchains: Object.freeze(["cmake", "powershell"]),
  })],
  ["rust", Object.freeze({
    job: "release-acceptance-rust",
    stages: Object.freeze(["RustExactTests", "ProductE2E", "RenderGolden"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Rust -ExecutionSurface Trusted",
    toolchains: Object.freeze(["rust", "cargo", "node", "cmake", "powershell"]),
  })],
  ["pages", Object.freeze({
    job: "release-acceptance-pages",
    stages: Object.freeze(["WasmBuildTest"]),
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Pages -ExecutionSurface Trusted",
    toolchains: Object.freeze([
      "rust",
      "cargo",
      "node",
      "npm",
      "wasm_bindgen",
      "cmake",
      "powershell",
    ]),
  })],
]));
const RELEASE_SHARD_FILES = Object.freeze(new Map(
  [...RELEASE_ACCEPTANCE_SHARDS.keys()].map((shard) => [
    shard,
    `clearra-release-acceptance-${shard}-shard.v1.json`,
  ]),
));
const DELEGATED_RELEASE_EVIDENCE = Object.freeze([
  Object.freeze({
    evidence: "complete_required_keeps_candidate",
    deferred_by: "NoProductDebt",
    owner_stage: "RustExactTests",
    owner_shard: "rust",
  }),
  Object.freeze({
    evidence: "renderer_png_artifact",
    deferred_by: "NoProductDebt",
    owner_stage: "RenderGolden",
    owner_shard: "rust",
  }),
  Object.freeze({
    evidence: "renderer_gif_artifact",
    deferred_by: "NoProductDebt",
    owner_stage: "RenderGolden",
    owner_shard: "rust",
  }),
  Object.freeze({
    evidence: "desktop_real_app_request",
    deferred_by: "NoProductDebt",
    owner_stage: "DesktopHost",
    owner_shard: "foundation-desktop-host",
  }),
  Object.freeze({
    evidence: "adversarial_rust_tests",
    deferred_by: "AdversarialCorrectness",
    owner_stage: "RustExactTests",
    owner_shard: "rust",
  }),
]);
const SURFACE_OWNERS = Object.freeze(new Map([
  ["desktop", Object.freeze(["NoProductDebt", "ProductE2E", "DesktopHost"])],
  ["discord", Object.freeze(["NoProductDebt", "ProductE2E"])],
  ["native", Object.freeze([
    "NoProductDebt",
    "AdversarialCorrectness",
    "CSanitizer",
    "RustExactTests",
    "ProductE2E",
    "RenderGolden",
  ])],
  ["wasm", Object.freeze([
    "NoProductDebt",
    "RustExactTests",
    "ProductE2E",
    "WasmBuildTest",
  ])],
]));
const REQUIRED_JOBS = Object.freeze(new Map([
  ["metadata", Object.freeze([
    "Require exact main and zero prior canonical success",
    "Bind the exact canonical acceptance run",
  ])],
  ["ctk3", Object.freeze([
    "Build and test CTK3 once",
    "Seal the accepted CTK3 distribution",
    "Upload accepted CTK3 distribution",
  ])],
  ["linux-cli", Object.freeze([
    "Build standalone WASM CPU CLI",
    "Upload Linux CLI artifact",
  ])],
  ["discord-bot", Object.freeze([
    "Verify accepted CTK3 distribution",
    "Verify Clearrabot contracts",
  ])],
  ["release-acceptance-foundation-no-product-debt", Object.freeze([
    "Archive the exact accepted source on Windows",
    "Verify canonical ReleaseAcceptance shard mapping",
    "Run canonical release acceptance NoProductDebt leaf",
    "Seal canonical release acceptance NoProductDebt leaf",
    "Upload canonical release acceptance NoProductDebt leaf",
  ])],
  ["release-acceptance-foundation-adversarial-correctness", Object.freeze([
    "Run canonical release acceptance AdversarialCorrectness leaf",
    "Seal canonical release acceptance AdversarialCorrectness leaf",
    "Upload canonical release acceptance AdversarialCorrectness leaf",
  ])],
  ["release-acceptance-foundation-desktop-host", Object.freeze([
    "Run canonical release acceptance DesktopHost leaf",
    "Seal canonical release acceptance DesktopHost leaf",
    "Upload canonical release acceptance DesktopHost leaf",
  ])],
  ["release-acceptance-sanitizer", Object.freeze([
    "Run canonical release acceptance sanitizer shard",
    "Seal canonical release acceptance sanitizer shard",
    "Upload canonical release acceptance sanitizer shard",
  ])],
  ["release-acceptance-rust", Object.freeze([
    "Download accepted CTK3 distribution",
    "Run canonical release acceptance rust shard",
    "Seal canonical release acceptance rust shard",
    "Upload canonical release acceptance rust shard",
  ])],
  ["release-acceptance-pages", Object.freeze([
    "Run canonical release acceptance Pages shard",
    "Stamp and verify the accepted Pages build",
    "Seal canonical release acceptance Pages shard",
    "Upload accepted Pages build",
    "Upload canonical release acceptance Pages shard",
  ])],
  ["release-acceptance", Object.freeze([
    "Download all canonical release acceptance shard evidence",
    "Produce canonical release gate evidence",
    "Upload canonical release gate evidence",
  ])],
  ["windows-cli", Object.freeze([
    "Build and exercise standalone WASM CPU CLI",
    "Upload Windows CLI artifact",
  ])],
  ["windows-gui", Object.freeze([
    "Build standalone SvelteKit and Tauri GUI",
    "Stage Windows GUI executable",
    "Upload Windows GUI artifact",
  ])],
]));
export const CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES = Object.freeze([
  ...REQUIRED_JOBS.keys(),
]);

export function createReleaseGateReports(authority, toolchains) {
  const identity = validateAuthority(authority);
  const tools = validateToolchains(toolchains);
  const gate = sealCanonicalReport({
    schema_id: "clearra.canonical-release-gate.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    workflow_path: WORKFLOW_PATH,
    job: "release-acceptance",
    task: "ReleaseAcceptance",
    command:
      "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ExecutionSurface Trusted",
    status: "passed",
    readiness_open_count: 0,
    stages: RELEASE_STAGES,
  });
  const toolchainManifest = sealCanonicalReport({
    schema_id: "clearra.release-toolchains.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    workflow_path: WORKFLOW_PATH,
    job: "release-acceptance",
    ...tools,
  });
  const surfaces = [...SURFACE_OWNERS.entries()].map(([surface, stageOwners]) =>
    sealCanonicalReport({
      schema_id: "clearra.release-surface-report.v1",
      source_commit: identity.sourceCommit,
      run_id: identity.runId,
      run_attempt: identity.runAttempt,
      workflow_path: WORKFLOW_PATH,
      job: "release-acceptance",
      surface,
      status: "passed",
      gate_report_sha256: gate.report_sha256,
      stage_owners: stageOwners,
    }));
  const index = sealCanonicalReport({
    schema_id: "clearra.release-gate-index.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    canonical_gate_sha256: gate.report_sha256,
    toolchain_manifest_sha256: toolchainManifest.report_sha256,
    surface_reports: surfaces.map((report) => ({
      surface: report.surface,
      sha256: report.report_sha256,
    })),
  });
  return Object.freeze({ gate, toolchainManifest, surfaces, index });
}

export async function writeReleaseGateReports(outputDirectory, authority, toolchains) {
  const root = resolve(outputDirectory);
  await mkdir(root, { recursive: true });
  const reports = createReleaseGateReports(authority, toolchains);
  await writeReleaseGateFiles(root, reports);
  return reports;
}

export function createReleaseAcceptanceShardEvidence(
  authority,
  shard,
  toolchains,
) {
  const identity = validateAuthority(authority);
  const contract = requireReleaseShard(shard);
  const tools = validateReleaseShardToolchains(toolchains, contract);
  return sealCanonicalReport({
    schema_id: RELEASE_ACCEPTANCE_SHARD_SCHEMA,
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    workflow_path: WORKFLOW_PATH,
    job: contract.job,
    shard,
    task: "ReleaseAcceptance",
    command: contract.command,
    status: "passed",
    stages: contract.stages,
    toolchains: tools,
  });
}

export function validateReleaseAcceptanceShardEvidence(
  report,
  authority,
  expectedShard,
) {
  verifyCanonicalReportHash(report, `${expectedShard} release shard evidence`);
  const expected = createReleaseAcceptanceShardEvidence(
    authority,
    expectedShard,
    report?.toolchains,
  );
  if (canonicalJson(report) !== canonicalJson(expected)) {
    throw new Error(`${expectedShard} release shard evidence differs from its closed contract`);
  }
  return true;
}

export async function writeReleaseAcceptanceShardEvidence(
  outputPath,
  authority,
  shard,
  toolchains,
) {
  const report = createReleaseAcceptanceShardEvidence(authority, shard, toolchains);
  const output = resolve(outputPath);
  if (output.split(/[\\/]/u).at(-1) !== RELEASE_SHARD_FILES.get(shard)) {
    throw new Error(`${shard} release shard evidence has an invalid output filename`);
  }
  await mkdir(dirname(output), { recursive: true });
  await writeCanonicalFile(output, report);
  return report;
}

export async function readReleaseAcceptanceShardEvidence(
  directory,
  authority,
) {
  const root = resolve(directory);
  const entries = await readdir(root, { withFileTypes: true });
  const expectedFiles = [...RELEASE_SHARD_FILES.values()].sort();
  const actualFiles = entries.map((entry) => entry.name).sort();
  if (
    actualFiles.join(",") !== expectedFiles.join(",") ||
    entries.some((entry) => !entry.isFile() || entry.isSymbolicLink())
  ) {
    throw new Error("release shard evidence directory must contain the exact six canonical files");
  }
  const reports = [];
  for (const [shard, filename] of RELEASE_SHARD_FILES) {
    const path = resolve(root, filename);
    const stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size <= 0) {
      throw new Error(`${shard} release shard evidence is not a non-empty regular file`);
    }
    const report = await readJson(path, `${shard} release shard evidence`);
    validateReleaseAcceptanceShardEvidence(report, authority, shard);
    reports.push(report);
  }
  return Object.freeze(reports);
}

export function createShardedReleaseGateReports(authority, shardReports) {
  const identity = validateAuthority(authority);
  const reports = validateReleaseShardReportSet(shardReports, identity);
  const pages = reports.find((report) => report.shard === "pages");
  const rust = reports.find((report) => report.shard === "rust");
  const sanitizer = reports.find((report) => report.shard === "sanitizer");
  const foundationNoProductDebt = reports.find(
    (report) => report.shard === "foundation-no-product-debt",
  );
  for (const tool of ["rust", "cargo", "node", "npm", "cmake", "powershell"]) {
    const versions = reports
      .filter((report) => Object.hasOwn(report.toolchains, tool))
      .map((report) => report.toolchains[tool]);
    if (new Set(versions).size !== 1) {
      throw new Error(`release shards disagree on the ${tool} toolchain version`);
    }
  }
  const tools = validateToolchains({
    rust: rust.toolchains.rust,
    cargo: rust.toolchains.cargo,
    node: pages.toolchains.node,
    npm: pages.toolchains.npm,
    wasm_bindgen: pages.toolchains.wasm_bindgen,
    cmake: sanitizer.toolchains.cmake,
    powershell: foundationNoProductDebt.toolchains.powershell,
  });
  const shardProjections = reports.map((report) => Object.freeze({
    shard: report.shard,
    job: report.job,
    command: report.command,
    stages: report.stages,
    sha256: report.report_sha256,
  }));
  const gate = sealCanonicalReport({
    schema_id: "clearra.canonical-release-gate.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    workflow_path: WORKFLOW_PATH,
    job: "release-acceptance",
    task: "ReleaseAcceptance",
    command: "canonical six-shard ReleaseAcceptance fan-in",
    execution_mode: "isolated-six-shard",
    status: "passed",
    readiness_open_count: 0,
    stages: RELEASE_STAGES,
    shards: shardProjections,
    delegated_evidence: DELEGATED_RELEASE_EVIDENCE,
  });
  const toolchainManifest = sealCanonicalReport({
    schema_id: "clearra.release-toolchains.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    workflow_path: WORKFLOW_PATH,
    job: "release-acceptance",
    execution_mode: "isolated-six-shard",
    ...tools,
    shard_toolchains: reports.map((report) => ({
      shard: report.shard,
      job: report.job,
      evidence_sha256: report.report_sha256,
      tools: report.toolchains,
    })),
  });
  const surfaces = [...SURFACE_OWNERS.entries()].map(([surface, stageOwners]) =>
    sealCanonicalReport({
      schema_id: "clearra.release-surface-report.v1",
      source_commit: identity.sourceCommit,
      run_id: identity.runId,
      run_attempt: identity.runAttempt,
      workflow_path: WORKFLOW_PATH,
      job: "release-acceptance",
      surface,
      status: "passed",
      gate_report_sha256: gate.report_sha256,
      stage_owners: stageOwners,
    }));
  const index = sealCanonicalReport({
    schema_id: "clearra.release-gate-index.v1",
    source_commit: identity.sourceCommit,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    canonical_gate_sha256: gate.report_sha256,
    toolchain_manifest_sha256: toolchainManifest.report_sha256,
    shard_reports: reports.map((report) => ({
      shard: report.shard,
      sha256: report.report_sha256,
    })),
    surface_reports: surfaces.map((report) => ({
      surface: report.surface,
      sha256: report.report_sha256,
    })),
  });
  return Object.freeze({
    gate,
    toolchainManifest,
    surfaces: Object.freeze(surfaces),
    index,
    shardReports: reports,
  });
}

export async function writeShardedReleaseGateReports(
  outputDirectory,
  shardEvidenceDirectory,
  authority,
) {
  const root = resolve(outputDirectory);
  const shardReports = await readReleaseAcceptanceShardEvidence(
    shardEvidenceDirectory,
    authority,
  );
  const reports = createShardedReleaseGateReports(authority, shardReports);
  await mkdir(root, { recursive: true });
  await writeReleaseGateFiles(root, reports);
  const shardRoot = resolve(root, "shards");
  await mkdir(shardRoot);
  await Promise.all(reports.shardReports.map((report) =>
    writeCanonicalFile(
      resolve(shardRoot, RELEASE_SHARD_FILES.get(report.shard)),
      report,
    )));
  return reports;
}

async function writeReleaseGateFiles(root, reports) {
  await Promise.all([
    writeCanonicalFile(resolve(root, "canonical-gate.v1.json"), reports.gate),
    writeCanonicalFile(resolve(root, "toolchains.v1.json"), reports.toolchainManifest),
    ...reports.surfaces.map((report) =>
      writeCanonicalFile(resolve(root, `surface-${report.surface}.v1.json`), report)),
    writeCanonicalFile(resolve(root, RELEASE_GATE_INDEX_FILE), reports.index),
  ]);
}

export function collectLocalToolchains(dependencies = {}) {
  const run = dependencies.run ?? runVersionCommand;
  const platform = dependencies.platform ?? process.platform;
  const npmInvocation = platform === "win32"
    ? Object.freeze([
        "cmd.exe",
        Object.freeze(["/d", "/s", "/c", "npm.cmd --version"]),
      ])
    : Object.freeze(["npm", Object.freeze(["--version"])]);
  const invocations = new Map([
    ["rust", ["rustc", ["--version"], "rustc"]],
    ["cargo", ["cargo", ["--version"], "cargo"]],
    ["node", ["node", ["--version"], "node"]],
    ["npm", [...npmInvocation, "npm"]],
    ["wasm_bindgen", ["wasm-bindgen", ["--version"], "wasm-bindgen"]],
    ["cmake", ["cmake", ["--version"], "cmake"]],
    ["powershell", [
      "powershell",
      ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"],
      "PowerShell",
    ]],
  ]);
  return collectToolchainSet([...invocations.keys()], invocations, run);
}

export function collectReleaseShardToolchains(shard, dependencies = {}) {
  const contract = requireReleaseShard(shard);
  const run = dependencies.run ?? runVersionCommand;
  const platform = dependencies.platform ?? process.platform;
  const npmInvocation = platform === "win32"
    ? ["cmd.exe", ["/d", "/s", "/c", "npm.cmd --version"], "npm"]
    : ["npm", ["--version"], "npm"];
  const invocations = new Map([
    ["rust", ["rustc", ["--version"], "rustc"]],
    ["cargo", ["cargo", ["--version"], "cargo"]],
    ["node", ["node", ["--version"], "node"]],
    ["npm", npmInvocation],
    ["wasm_bindgen", ["wasm-bindgen", ["--version"], "wasm-bindgen"]],
    ["cmake", ["cmake", ["--version"], "cmake"]],
    ["powershell", [
      "powershell",
      ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"],
      "PowerShell",
    ]],
  ]);
  return collectToolchainSet(contract.toolchains, invocations, run);
}

function collectToolchainSet(keys, invocations, run) {
  const tools = {};
  for (const key of keys) {
    const [command, arguments_, label] = invocations.get(key);
    tools[key] = firstLine(run(command, arguments_), label);
  }
  return Object.freeze(tools);
}

export async function createCanonicalAcceptanceEvidence(options) {
  const authority = validateAuthority(options);
  const repository = requirePattern(options.repository, REPOSITORY, "repository");
  const version = requirePattern(options.version, VERSION, "version");
  const basePath = requireNonEmptyString(options.basePath, "Pages base path");
  const jobs = validateReleaseJobs(
    await readJson(options.jobsPath, "workflow jobs"),
    authority,
  );
  const gateReports = await readAndValidateGateReports(
    options.gateEvidenceDirectory,
    authority,
  );
  const [ctk3Manifest, pagesIdentity, artifacts] = await Promise.all([
    verifyAcceptedCtk3Dist(
      options.ctk3Directory,
      authority.sourceCommit,
      authority.runId,
      authority.runAttempt,
    ),
    verifyAcceptedPagesBuild(options.pagesDirectory, {
      sourceCommit: authority.sourceCommit,
      acceptedRunId: authority.runId,
      acceptedRunAttempt: authority.runAttempt,
      basePath,
      version,
    }),
    collectReleaseArtifacts(options.productsDirectory, version, authority.sourceCommit),
  ]);

  const finalSourceFragments = Object.freeze({
    toolchains: {
      source_commit: authority.sourceCommit,
      manifest_sha256: gateReports.toolchainManifest.report_sha256,
      rust: gateReports.toolchainManifest.rust,
      node: gateReports.toolchainManifest.node,
      wasm_bindgen: gateReports.toolchainManifest.wasm_bindgen,
    },
    canonical_gate: {
      id: `release-acceptance-run-${authority.runId}-attempt-${authority.runAttempt}`,
      sha256: gateReports.gate.report_sha256,
      source_commit: authority.sourceCommit,
      status: "passed",
      readiness_open_count: 0,
    },
    surface_reports: gateReports.surfaces.map((report) => ({
      id: `${report.surface}-run-${authority.runId}-attempt-${authority.runAttempt}`,
      sha256: report.report_sha256,
      source_commit: authority.sourceCommit,
      surface: report.surface,
      status: "passed",
    })),
    release_artifacts: artifacts,
  });

  return sealCanonicalReport({
    schema_id: CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA,
    repository,
    release_version: version,
    pages_base_path: basePath,
    source_commit: authority.sourceCommit,
    run_id: authority.runId,
    run_attempt: authority.runAttempt,
    workflow_path: WORKFLOW_PATH,
    status: "passed",
    jobs,
    accepted_inputs: {
      ctk3_manifest_sha256: sha256Buffer(Buffer.from(canonicalJson(ctk3Manifest), "utf8")),
      pages_identity_sha256: sha256Buffer(Buffer.from(canonicalJson(pagesIdentity), "utf8")),
      gate_index_sha256: gateReports.index.report_sha256,
    },
    final_source_fragments: finalSourceFragments,
  });
}

export async function writeCanonicalAcceptanceEvidence(options) {
  const report = await createCanonicalAcceptanceEvidence(options);
  const output = resolve(options.outputPath);
  await mkdir(dirname(output), { recursive: true });
  await writeCanonicalFile(output, report);
  return report;
}

export function validateCanonicalAcceptanceEvidence(report, authorityValue) {
  const authority = validateAuthority(authorityValue);
  const repository = requirePattern(
    authorityValue?.repository,
    REPOSITORY,
    "repository",
  );
  const version = requirePattern(authorityValue?.version, VERSION, "version");
  const basePath = requireNonEmptyString(
    authorityValue?.basePath,
    "Pages base path",
  );
  verifyCanonicalReportHash(report, "canonical acceptance evidence");
  if (
    report.schema_id !== CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA ||
    report.repository !== repository ||
    report.release_version !== version ||
    report.pages_base_path !== basePath ||
    report.source_commit !== authority.sourceCommit ||
    report.run_id !== authority.runId ||
    report.run_attempt !== authority.runAttempt ||
    report.workflow_path !== WORKFLOW_PATH ||
    report.status !== "passed"
  ) {
    throw new Error("canonical acceptance evidence differs from the bound run attempt");
  }
  if (
    !Array.isArray(report.jobs) ||
    report.jobs.length !== CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.length
  ) {
    throw new Error("canonical acceptance evidence has an invalid job set");
  }
  const jobNames = report.jobs.map((job) => job?.name);
  if (
    jobNames.join(",") !== CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.join(",") ||
    report.jobs.some((job) => job?.status !== "passed" || !DECIMAL_ID.test(job?.job_id))
  ) {
    throw new Error("canonical acceptance evidence job identities are invalid");
  }
  requirePlainObject(report.accepted_inputs, "canonical accepted inputs");
  for (const key of [
    "ctk3_manifest_sha256",
    "pages_identity_sha256",
    "gate_index_sha256",
  ]) {
    if (typeof report.accepted_inputs[key] !== "string" || !/^[0-9a-f]{64}$/u.test(report.accepted_inputs[key])) {
      throw new Error(`canonical accepted input ${key} is invalid`);
    }
  }
  const fragments = report.final_source_fragments;
  requirePlainObject(fragments, "final-source fragments");
  if (
    fragments.toolchains?.source_commit !== authority.sourceCommit ||
    fragments.canonical_gate?.source_commit !== authority.sourceCommit ||
    fragments.canonical_gate?.status !== "passed" ||
    fragments.canonical_gate?.readiness_open_count !== 0
  ) {
    throw new Error("canonical acceptance final-source gate fragments are invalid");
  }
  const surfaces = fragments.surface_reports;
  if (
    !Array.isArray(surfaces) ||
    surfaces.map((entry) => entry?.surface).join(",") !== [...SURFACE_OWNERS.keys()].join(",") ||
    surfaces.some((entry) =>
      entry.source_commit !== authority.sourceCommit ||
      entry.status !== "passed" ||
      typeof entry.sha256 !== "string" ||
      !/^[0-9a-f]{64}$/u.test(entry.sha256))
  ) {
    throw new Error("canonical acceptance surface fragments are invalid");
  }
  const artifacts = fragments.release_artifacts;
  if (
    !Array.isArray(artifacts) ||
    artifacts.length !== 3 ||
    artifacts.some((entry) =>
      entry.source_commit !== authority.sourceCommit ||
      typeof entry.sha256 !== "string" ||
      !/^[0-9a-f]{64}$/u.test(entry.sha256) ||
      !Number.isSafeInteger(entry.size_bytes) ||
      entry.size_bytes <= 0)
  ) {
    throw new Error("canonical acceptance release artifact fragments are invalid");
  }
  return true;
}

export async function verifyCanonicalAcceptanceEvidence(report, options) {
  validateCanonicalAcceptanceEvidence(report, options);
  const expectedArtifacts = await collectReleaseArtifacts(
    options.productsDirectory,
    options.version,
    options.sourceCommit,
  );
  if (
    canonicalJson(report.final_source_fragments.release_artifacts) !==
    canonicalJson(expectedArtifacts)
  ) {
    throw new Error(
      "downloaded release products differ from canonical acceptance evidence",
    );
  }
  return true;
}

export function validateReleaseJobs(payload, authorityValue) {
  const authority = validateAuthority(authorityValue);
  requirePlainObject(payload, "workflow jobs");
  if (!Number.isSafeInteger(payload.total_count) || payload.total_count < REQUIRED_JOBS.size) {
    throw new Error("workflow jobs total_count is invalid");
  }
  if (!Array.isArray(payload.jobs) || payload.jobs.length !== payload.total_count) {
    throw new Error("workflow jobs must contain the complete non-truncated job list");
  }
  const evidence = [];
  for (const [name, requiredSteps] of REQUIRED_JOBS) {
    const matches = payload.jobs.filter((job) => job?.name === name);
    if (matches.length !== 1) {
      throw new Error(`workflow jobs must contain ${name} exactly once`);
    }
    const job = matches[0];
    if (
      String(job.run_id ?? "") !== authority.runId ||
      String(job.run_attempt ?? "") !== authority.runAttempt ||
      job.head_sha !== authority.sourceCommit ||
      job.status !== "completed" ||
      job.conclusion !== "success"
    ) {
      throw new Error(`${name} does not prove the exact successful acceptance attempt`);
    }
    if (!Array.isArray(job.steps)) {
      throw new Error(`${name} has no step evidence`);
    }
    for (const stepName of requiredSteps) {
      const steps = job.steps.filter((step) => step?.name === stepName);
      if (
        steps.length !== 1 ||
        steps[0].status !== "completed" ||
        steps[0].conclusion !== "success"
      ) {
        throw new Error(`${name} step ${stepName} did not pass exactly once`);
      }
    }
    evidence.push(Object.freeze({
      name,
      job_id: decimalId(job.id, `${name} job ID`),
      status: "passed",
    }));
  }
  return Object.freeze(evidence);
}

async function readAndValidateGateReports(directory, authority) {
  const root = resolve(directory);
  const rootEntries = await readdir(root, { withFileTypes: true });
  const expectedRootEntries = [
    "canonical-gate.v1.json",
    "shards",
    "surface-desktop.v1.json",
    "surface-discord.v1.json",
    "surface-native.v1.json",
    "surface-wasm.v1.json",
    "toolchains.v1.json",
    RELEASE_GATE_INDEX_FILE,
  ].sort();
  if (
    rootEntries.map((entry) => entry.name).sort().join(",") !==
      expectedRootEntries.join(",") ||
    rootEntries.some((entry) => entry.isSymbolicLink()) ||
    rootEntries.some((entry) =>
      entry.name === "shards" ? !entry.isDirectory() : !entry.isFile())
  ) {
    throw new Error("release gate evidence must contain the exact canonical file set");
  }
  const shardReports = await readReleaseAcceptanceShardEvidence(
    resolve(root, "shards"),
    authority,
  );
  const [gate, toolchainManifest, index, ...surfaces] = await Promise.all([
    readJson(resolve(root, "canonical-gate.v1.json"), "canonical gate"),
    readJson(resolve(root, "toolchains.v1.json"), "toolchain manifest"),
    readJson(resolve(root, RELEASE_GATE_INDEX_FILE), "release gate index"),
    ...[...SURFACE_OWNERS.keys()].map((surface) =>
      readJson(resolve(root, `surface-${surface}.v1.json`), `${surface} surface report`)),
  ]);
  const expected = createShardedReleaseGateReports(authority, shardReports);
  for (const [label, actual, canonical] of [
    ["canonical gate", gate, expected.gate],
    ["toolchain manifest", toolchainManifest, expected.toolchainManifest],
    ["release gate index", index, expected.index],
    ...surfaces.map((report, index_) => [
      `${report?.surface ?? "unknown"} surface report`,
      report,
      expected.surfaces[index_],
    ]),
  ]) {
    verifyCanonicalReportHash(actual, label);
    requireReportAuthority(actual, authority, label);
    if (canonicalJson(actual) !== canonicalJson(canonical)) {
      throw new Error(`${label} differs from the canonical six-shard fan-in`);
    }
  }
  return Object.freeze({
    gate,
    toolchainManifest,
    surfaces: Object.freeze(surfaces),
    index,
    shardReports,
  });
}

async function collectReleaseArtifacts(directory, version, sourceCommit) {
  const expected = Object.freeze([
    ["linux-cli", `Clearra-CLI-v${version}-linux-x86_64`],
    ["windows-cli", `Clearra-CLI-v${version}-windows-x86_64.exe`],
    ["windows-gui", `Clearra-GUI-v${version}-windows-x86_64.exe`],
  ]);
  const root = resolve(directory);
  const directoryEntries = await readdir(root, { withFileTypes: true });
  if (
    directoryEntries.length !== expected.length ||
    directoryEntries.some((entry) => !entry.isFile() || entry.isSymbolicLink())
  ) {
    throw new Error("release product directory must contain exactly three regular files");
  }
  const actualNames = directoryEntries.map((entry) => entry.name).sort();
  const expectedNames = expected.map(([, name]) => name).sort();
  if (actualNames.join("\0") !== expectedNames.join("\0")) {
    throw new Error("release product file names differ from the canonical three assets");
  }
  return Promise.all(expected.map(async ([role, name]) => {
    const path = resolve(root, name);
    const stats = await lstat(path);
    if (!stats.isFile() || stats.isSymbolicLink() || stats.size <= 0) {
      throw new Error(`${name} must be a non-empty regular file`);
    }
    return Object.freeze({
      role,
      name,
      sha256: sha256Buffer(await readFile(path)),
      size_bytes: stats.size,
      source_commit: sourceCommit,
    });
  }));
}

function validateAuthority(value) {
  const authority = {
    sourceCommit: requirePattern(value?.sourceCommit, SOURCE_COMMIT, "source commit"),
    runId: decimalId(value?.runId, "run ID"),
    runAttempt: decimalId(value?.runAttempt, "run attempt"),
  };
  if (authority.runAttempt !== "1") {
    throw new Error("canonical acceptance evidence forbids workflow reruns");
  }
  return Object.freeze(authority);
}

function requireReleaseShard(shard) {
  const name = requireNonEmptyString(shard, "release shard").toLowerCase();
  const contract = RELEASE_ACCEPTANCE_SHARDS.get(name);
  if (!contract) {
    throw new Error(`unknown ReleaseAcceptance shard: ${name}`);
  }
  return contract;
}

function validateReleaseShardToolchains(value, contract) {
  requirePlainObject(value, "release shard toolchains");
  if (Object.keys(value).sort().join(",") !== [...contract.toolchains].sort().join(",")) {
    throw new Error("release shard toolchains differ from the closed shard contract");
  }
  const tools = {};
  for (const key of contract.toolchains) {
    tools[key] = requireNonEmptyString(value[key], `release shard toolchains.${key}`);
  }
  return Object.freeze(tools);
}

function validateReleaseShardReportSet(shardReports, authority) {
  if (!Array.isArray(shardReports) || shardReports.length !== RELEASE_ACCEPTANCE_SHARDS.size) {
    throw new Error("release gate requires the exact six shard reports");
  }
  const byShard = new Map();
  for (const report of shardReports) {
    const shard = requireNonEmptyString(report?.shard, "release shard report name");
    if (byShard.has(shard) || !RELEASE_ACCEPTANCE_SHARDS.has(shard)) {
      throw new Error("release gate shard report set is duplicate or unknown");
    }
    validateReleaseAcceptanceShardEvidence(report, authority, shard);
    byShard.set(shard, report);
  }
  if ([...byShard.keys()].sort().join(",") !== [...RELEASE_ACCEPTANCE_SHARDS.keys()].sort().join(",")) {
    throw new Error("release gate shard report set is incomplete");
  }
  const ordered = Object.freeze(
    [...RELEASE_ACCEPTANCE_SHARDS.keys()].map((shard) => byShard.get(shard)),
  );
  const ownedStages = ordered.flatMap((report) => report.stages);
  if (
    ownedStages.length !== RELEASE_STAGES.length ||
    new Set(ownedStages).size !== RELEASE_STAGES.length ||
    ownedStages.slice().sort().join(",") !== RELEASE_STAGES.slice().sort().join(",")
  ) {
    throw new Error("release gate shard stages must have complete non-duplicate ownership");
  }
  return ordered;
}

function validateToolchains(value) {
  requirePlainObject(value, "toolchains");
  const tools = {};
  for (const key of [
    "rust",
    "cargo",
    "node",
    "npm",
    "wasm_bindgen",
    "cmake",
    "powershell",
  ]) {
    tools[key] = requireNonEmptyString(value[key], `toolchains.${key}`);
  }
  return Object.freeze(tools);
}

function requireReportAuthority(report, authority, label) {
  if (
    report.source_commit !== authority.sourceCommit ||
    report.run_id !== authority.runId ||
    report.run_attempt !== authority.runAttempt
  ) {
    throw new Error(`${label} differs from the exact acceptance attempt`);
  }
}

function decimalId(value, label) {
  const text = typeof value === "number" && Number.isSafeInteger(value)
    ? String(value)
    : typeof value === "string" ? value : "";
  return requirePattern(text, DECIMAL_ID, label);
}

function requirePattern(value, expression, label) {
  if (typeof value !== "string" || !expression.test(value)) {
    throw new Error(`${label} has an invalid format`);
  }
  return value;
}

function requireNonEmptyString(value, label) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${label} must be non-empty`);
  }
  return value.trim();
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

function firstLine(value, label) {
  const line = String(value).trim().split(/\r?\n/u)[0] ?? "";
  return requireNonEmptyString(line, `${label} version`);
}

function runVersionCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`release toolchain command failed: ${command}`);
  }
  return result.stdout;
}

async function readJson(path, label) {
  try {
    return JSON.parse(await readFile(resolve(path), "utf8"));
  } catch (error) {
    throw new Error(`${label} is not readable canonical JSON: ${error.message}`);
  }
}

async function writeCanonicalFile(path, value) {
  await writeFile(path, `${canonicalJson(value)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
}

function sha256Buffer(value) {
  return createHash("sha256").update(value).digest("hex");
}

function gateCliOptions(values) {
  return {
    sourceCommit: values["source-commit"],
    runId: values["run-id"],
    runAttempt: values["run-attempt"],
  };
}

async function main() {
  const command = process.argv[2];
  const arguments_ = process.argv.slice(3);
  try {
    if (command === "shard") {
      const { values } = parseArgs({
        args: arguments_,
        options: {
          "source-commit": { type: "string" },
          "run-id": { type: "string" },
          "run-attempt": { type: "string" },
          shard: { type: "string" },
          output: { type: "string" },
        },
        strict: true,
      });
      const shard = requireNonEmptyString(values.shard, "release shard").toLowerCase();
      const contract = requireReleaseShard(shard);
      if (process.env.GITHUB_ACTIONS !== "true" || process.env.GITHUB_JOB !== contract.job) {
        throw new Error(`${shard} shard evidence must be produced by ${contract.job}`);
      }
      const report = await writeReleaseAcceptanceShardEvidence(
        values.output,
        gateCliOptions(values),
        shard,
        collectReleaseShardToolchains(shard),
      );
      console.log(`canonical_release_shard=passed shard=${shard} sha256=${report.report_sha256}`);
      return;
    }
    if (command === "gate") {
      const { values } = parseArgs({
        args: arguments_,
        options: {
          "source-commit": { type: "string" },
          "run-id": { type: "string" },
          "run-attempt": { type: "string" },
          shards: { type: "string" },
          output: { type: "string" },
        },
        strict: true,
      });
      if (process.env.GITHUB_ACTIONS !== "true" || process.env.GITHUB_JOB !== "release-acceptance") {
        throw new Error("release gate evidence must be produced by the release-acceptance job");
      }
      const reports = await writeShardedReleaseGateReports(
        values.output,
        values.shards,
        gateCliOptions(values),
      );
      console.log(`canonical_gate_evidence=passed sha256=${reports.index.report_sha256}`);
      return;
    }
    if (command === "collect") {
      const { values } = parseArgs({
        args: arguments_,
        options: {
          repository: { type: "string" },
          version: { type: "string" },
          "source-commit": { type: "string" },
          "run-id": { type: "string" },
          "run-attempt": { type: "string" },
          "base-path": { type: "string" },
          jobs: { type: "string" },
          "gate-evidence": { type: "string" },
          ctk3: { type: "string" },
          pages: { type: "string" },
          products: { type: "string" },
          output: { type: "string" },
        },
        strict: true,
      });
      const report = await writeCanonicalAcceptanceEvidence({
        repository: values.repository,
        version: values.version,
        sourceCommit: values["source-commit"],
        runId: values["run-id"],
        runAttempt: values["run-attempt"],
        basePath: values["base-path"],
        jobsPath: values.jobs,
        gateEvidenceDirectory: values["gate-evidence"],
        ctk3Directory: values.ctk3,
        pagesDirectory: values.pages,
        productsDirectory: values.products,
        outputPath: values.output,
      });
      console.log(`canonical_acceptance_evidence=passed sha256=${report.report_sha256}`);
      return;
    }
    if (command === "verify") {
      const { values } = parseArgs({
        args: arguments_,
        options: {
          report: { type: "string" },
          repository: { type: "string" },
          version: { type: "string" },
          "source-commit": { type: "string" },
          "run-id": { type: "string" },
          "run-attempt": { type: "string" },
          "base-path": { type: "string" },
          products: { type: "string" },
        },
        strict: true,
      });
      const report = await readJson(values.report, "canonical acceptance evidence");
      await verifyCanonicalAcceptanceEvidence(report, {
        ...gateCliOptions(values),
        repository: values.repository,
        version: values.version,
        basePath: values["base-path"],
        productsDirectory: values.products,
      });
      console.log(`canonical_acceptance_evidence=verified sha256=${report.report_sha256}`);
      return;
    }
    throw new Error(
      "canonical acceptance evidence command must be shard, gate, collect, or verify",
    );
  } catch (error) {
    const reason = error instanceof Error ? error.message : "unknown failure";
    console.error(
      `canonical_acceptance_evidence=failed reason=${JSON.stringify(reason)}`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
