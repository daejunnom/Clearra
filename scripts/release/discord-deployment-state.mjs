#!/usr/bin/env node

import { createHash } from "node:crypto";
import { open, lstat, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const DISCORD_DEPLOYMENT_STATE_SCHEMA_ID =
  "clearra.discord-deployment-state.v1";

const SHA = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const POSITIVE_INTEGER = /^[1-9][0-9]{0,19}$/u;
const DEPLOYMENT_NONCE = /^[0-9a-f]{64}$/u;
const STAGE_BINDINGS = Object.freeze({
  prepared: Object.freeze([
    "accepted_ctk3_manifest",
    "canonical_acceptance_evidence",
    "cloud_build_authority",
    "cloud_build_readback",
    "ctk3_actions_archive",
    "dependencies_actions_archive",
    "exact_source_archive",
    "recovery_debt_clearance",
  ]),
  prestage: Object.freeze([
    "cloud_prior_authority",
    "intended_candidate_authority",
    "oracle_rollback_capture",
    "prepared_state",
  ]),
  candidate: Object.freeze([
    "cloud_candidate_authority",
    "cloud_candidate_smoke",
    "oracle_stage_manifest",
    "prestage_state",
  ]),
  promoted: Object.freeze([
    "candidate_state",
    "cloud_traffic_readback",
    "oracle_candidate_attestation",
  ]),
  synchronized: Object.freeze([
    "discord_catalog",
    "discord_prior_catalog",
    "discord_sync_authority",
    "discord_sync_report",
    "oracle_end_observation",
    "pages_deployment_authority",
    "production_observation",
    "production_probe_authority",
    "production_probe_spec",
    "promoted_state",
  ]),
});

export async function sealDiscordDeploymentState(options) {
  const identity = validateIdentity(options);
  const bindings = await materializeBindings(options?.bindings, identity.stage);
  const report = {
    schema_id: DISCORD_DEPLOYMENT_STATE_SCHEMA_ID,
    stage: identity.stage,
    source_commit: identity.sourceCommit,
    workflow_run_id: identity.workflowRunId,
    workflow_run_attempt: identity.workflowRunAttempt,
    accepted_run_id: identity.acceptedRunId,
    accepted_run_attempt: identity.acceptedRunAttempt,
    deployment_nonce: identity.deploymentNonce,
    verified_after: identity.verifiedAfter,
    parent_report_sha256: parentReportSha256(bindings, identity.stage),
    bindings,
  };
  return Object.freeze({
    ...report,
    report_sha256: canonicalSha256(report),
  });
}

export async function verifyDiscordDeploymentState(reportPath, options) {
  const identity = validateIdentity(options);
  const input = await readCanonicalReport(reportPath);
  const report = input.value;
  requireExactKeys(report, [
    "schema_id",
    "stage",
    "source_commit",
    "workflow_run_id",
    "workflow_run_attempt",
    "accepted_run_id",
    "accepted_run_attempt",
    "deployment_nonce",
    "verified_after",
    "parent_report_sha256",
    "bindings",
    "report_sha256",
  ]);
  if (report.schema_id !== DISCORD_DEPLOYMENT_STATE_SCHEMA_ID) {
    throw new Error("Discord deployment state schema is invalid");
  }
  const expectedHash = report.report_sha256;
  requirePattern(expectedHash, SHA256, "Discord deployment state report SHA-256");
  const unhashed = { ...report };
  delete unhashed.report_sha256;
  if (canonicalSha256(unhashed) !== expectedHash) {
    throw new Error("Discord deployment state report SHA-256 differs");
  }
  for (const [field, expected] of [
    ["stage", identity.stage],
    ["source_commit", identity.sourceCommit],
    ["workflow_run_id", identity.workflowRunId],
    ["workflow_run_attempt", identity.workflowRunAttempt],
    ["accepted_run_id", identity.acceptedRunId],
    ["accepted_run_attempt", identity.acceptedRunAttempt],
    ["deployment_nonce", identity.deploymentNonce],
    ["verified_after", identity.verifiedAfter],
  ]) {
    if (report[field] !== expected) {
      throw new Error(`Discord deployment state ${field} differs`);
    }
  }
  const currentBindings = await materializeBindings(options?.bindings, identity.stage);
  if (canonicalJson(report.bindings) !== canonicalJson(currentBindings)) {
    throw new Error("Discord deployment state bound files differ");
  }
  if (report.parent_report_sha256 !== parentReportSha256(currentBindings, identity.stage)) {
    throw new Error("Discord deployment state parent report differs");
  }
  return Object.freeze({ report: Object.freeze(report), fileSha256: input.fileSha256 });
}

function validateIdentity(options) {
  const stage = String(options?.stage ?? "");
  if (!Object.hasOwn(STAGE_BINDINGS, stage)) {
    throw new Error("Discord deployment state stage is invalid");
  }
  const workflowRunAttempt = requirePattern(
    String(options?.workflowRunAttempt ?? ""),
    POSITIVE_INTEGER,
    "workflow run attempt",
  );
  const verifiedAfter = options?.verifiedAfter ?? null;
  if (["prepared", "prestage", "candidate"].includes(stage)) {
    if (verifiedAfter !== null) throw new Error(`${stage} state rejects verified-after authority`);
  } else if (!canonicalTimestamp(verifiedAfter)) {
    throw new Error("verified-after authority is invalid");
  }
  return Object.freeze({
    stage,
    sourceCommit: requirePattern(options?.sourceCommit, SHA, "source commit"),
    workflowRunId: requirePattern(
      String(options?.workflowRunId ?? ""),
      POSITIVE_INTEGER,
      "workflow run ID",
    ),
    workflowRunAttempt,
    acceptedRunId: requirePattern(
      String(options?.acceptedRunId ?? ""),
      POSITIVE_INTEGER,
      "accepted run ID",
    ),
    acceptedRunAttempt: requirePattern(
      String(options?.acceptedRunAttempt ?? ""),
      POSITIVE_INTEGER,
      "accepted run attempt",
    ),
    deploymentNonce: requirePattern(
      options?.deploymentNonce,
      DEPLOYMENT_NONCE,
      "deployment nonce",
    ),
    verifiedAfter,
  });
}

async function materializeBindings(bindings, stage) {
  if (!Array.isArray(bindings)) {
    throw new Error("Discord deployment state bindings are required");
  }
  const expectedNames = STAGE_BINDINGS[stage];
  const parsed = bindings.map((binding) => parseBinding(binding));
  parsed.sort((left, right) => left.name.localeCompare(right.name, "en"));
  if (
    parsed.length !== expectedNames.length ||
    parsed.some((binding, index) => binding.name !== expectedNames[index])
  ) {
    throw new Error(`Discord deployment state ${stage} bindings are not the closed set`);
  }
  const output = [];
  for (const binding of parsed) {
    const target = resolve(binding.path);
    await assertSafeDirectoryChain(dirname(target));
    const metadata = await lstat(target);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1) {
      throw new Error(`Discord deployment state binding is not a nonempty regular file: ${binding.name}`);
    }
    const bytes = await readFile(target);
    output.push(Object.freeze({
      name: binding.name,
      file_sha256: createHash("sha256").update(bytes).digest("hex"),
      size: metadata.size,
    }));
  }
  return Object.freeze(output);
}

function parseBinding(binding) {
  if (typeof binding !== "string") {
    throw new Error("Discord deployment state binding is invalid");
  }
  const separator = binding.indexOf("=");
  const name = binding.slice(0, separator);
  const path = binding.slice(separator + 1);
  if (separator < 1 || !/^[a-z][a-z0-9_]{0,63}$/u.test(name) || path.length === 0) {
    throw new Error("Discord deployment state binding must be name=path");
  }
  return Object.freeze({ name, path });
}

function parentReportSha256(bindings, stage) {
  if (stage === "prepared") return null;
  const parentName = stage === "prestage"
    ? "prepared_state"
    : stage === "candidate" ? "prestage_state"
    : stage === "promoted" ? "candidate_state" : "promoted_state";
  const parent = bindings.find((binding) => binding.name === parentName);
  if (!parent || !SHA256.test(parent.file_sha256)) {
    throw new Error("Discord deployment state parent binding is unavailable");
  }
  return parent.file_sha256;
}

async function readCanonicalReport(path) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error("Discord deployment state report must be a regular non-link file");
  }
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error("Discord deployment state report is not JSON");
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error("Discord deployment state report is not canonical JSON");
  }
  return Object.freeze({
    value,
    fileSha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

async function writeCanonicalReportNew(path, report) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(report)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Discord deployment state path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requireExactKeys(value, expected) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Discord deployment state report is not an object");
  }
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (canonicalJson(actual) !== canonicalJson(wanted)) {
    throw new Error("Discord deployment state report fields are not closed");
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function canonicalSha256(value) {
  return createHash("sha256").update(canonicalJson(value), "utf8").digest("hex");
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function parseCliArguments(args) {
  if (!Array.isArray(args) || args.length < 1 || !["seal", "verify"].includes(args[0])) {
    throw new Error("usage: discord-deployment-state.mjs (seal|verify) [closed options]");
  }
  const command = args[0];
  const allowed = new Set([
    "--stage", "--source-commit", "--workflow-run-id", "--workflow-run-attempt",
    "--accepted-run-id", "--accepted-run-attempt", "--binding", "--output", "--report",
    "--deployment-nonce",
    "--verified-after",
  ]);
  const values = { bindings: [] };
  for (let index = 1; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0) {
      throw new Error(`unsupported or empty Discord deployment state option: ${String(option)}`);
    }
    if (option === "--binding") {
      values.bindings.push(value);
    } else {
      if (Object.hasOwn(values, option)) throw new Error(`duplicate option: ${option}`);
      values[option] = value;
    }
  }
  const required = [
    "--stage", "--source-commit", "--workflow-run-id", "--workflow-run-attempt",
    "--accepted-run-id", "--accepted-run-attempt", command === "seal" ? "--output" : "--report",
    "--deployment-nonce",
  ];
  for (const option of required) {
    if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  }
  if (command === "seal" && Object.hasOwn(values, "--report")) throw new Error("seal rejects --report");
  if (command === "verify" && Object.hasOwn(values, "--output")) throw new Error("verify rejects --output");
  return Object.freeze({ command, values });
}

async function main() {
  const { command, values } = parseCliArguments(process.argv.slice(2));
  const options = {
    stage: values["--stage"],
    sourceCommit: values["--source-commit"],
    workflowRunId: values["--workflow-run-id"],
    workflowRunAttempt: values["--workflow-run-attempt"],
    acceptedRunId: values["--accepted-run-id"],
    acceptedRunAttempt: values["--accepted-run-attempt"],
    deploymentNonce: values["--deployment-nonce"],
    verifiedAfter: values["--verified-after"] ?? null,
    bindings: values.bindings,
  };
  if (command === "seal") {
    const report = await sealDiscordDeploymentState(options);
    await writeCanonicalReportNew(values["--output"], report);
    process.stdout.write(`${DISCORD_DEPLOYMENT_STATE_SCHEMA_ID} ${report.report_sha256}\n`);
    return;
  }
  const verified = await verifyDiscordDeploymentState(values["--report"], options);
  process.stdout.write(
    `${DISCORD_DEPLOYMENT_STATE_SCHEMA_ID} ${verified.report.report_sha256} ${verified.fileSha256}\n`,
  );
}

function canonicalTimestamp(value) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/u.test(value)) {
    return null;
  }
  const milliseconds = Date.parse(value);
  return Number.isFinite(milliseconds) && new Date(milliseconds).toISOString() === value
    ? value
    : null;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `discord_deployment_state=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
