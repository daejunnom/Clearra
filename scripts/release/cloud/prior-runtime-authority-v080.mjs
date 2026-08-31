#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const CLOUD_PRIOR_RUNTIME_AUTHORITY =
  "clearra.cloud.prior-runtime-authority.v1";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u;
const REVISION = /^clearra-current-job-v(?:075|080)-[0-9a-f]{7}$/u;
const NUMERIC_VERSION = /^[1-9][0-9]{0,19}$/u;

export function createPriorRuntimeAuthority({
  sourceCommit,
  projectId,
  service,
  serviceFileSha256,
}) {
  requirePattern(sourceCommit, SOURCE_COMMIT, "deployment source commit");
  requirePattern(projectId, PROJECT_ID, "GCP project ID");
  requirePattern(serviceFileSha256, /^[0-9a-f]{64}$/u, "service file SHA-256");
  if (service?.metadata?.name !== "clearra-current-job") {
    throw new Error("prior Cloud authority returned the wrong service");
  }
  const traffic = Array.isArray(service?.status?.traffic) ? service.status.traffic : [];
  const active = traffic.filter((entry) => Number(entry?.percent ?? 0) > 0);
  if (
    active.length !== 1 ||
    Number(active[0]?.percent) !== 100 ||
    !REVISION.test(active[0]?.revisionName ?? "")
  ) {
    throw new Error("prior Cloud authority requires one exact 100-percent revision");
  }
  const containers = service?.spec?.template?.spec?.containers;
  if (!Array.isArray(containers) || containers.length !== 1) {
    throw new Error("prior Cloud authority requires one service container");
  }
  const bindings = (Array.isArray(containers[0]?.env) ? containers[0].env : [])
    .filter((entry) => entry?.name === "CLEARRA_JOB_TOKEN");
  if (bindings.length !== 1 || Object.hasOwn(bindings[0], "value")) {
    throw new Error("prior Cloud authority requires one managed job-token binding");
  }
  const reference = bindings[0]?.valueFrom?.secretKeyRef;
  const v1 = reference?.name === "clearra-job-token" ? reference?.key : undefined;
  const v2 = reference?.secret === "clearra-job-token" ? reference?.version : undefined;
  if (Number(v1 !== undefined) + Number(v2 !== undefined) !== 1) {
    throw new Error("prior Cloud authority managed Secret identity is invalid");
  }
  const secretVersion = String(v1 ?? v2);
  requirePattern(secretVersion, NUMERIC_VERSION, "prior numeric job Secret version");
  const report = {
    schema_id: CLOUD_PRIOR_RUNTIME_AUTHORITY,
    deployment_source_commit: sourceCommit,
    project_id: projectId,
    region: "asia-northeast1",
    service: "clearra-current-job",
    prior_revision: active[0].revisionName,
    job_bearer_secret: "clearra-job-token",
    job_bearer_secret_version: secretVersion,
    observed_service_file_sha256: serviceFileSha256,
  };
  return Object.freeze({ ...report, report_sha256: sha256(canonicalJson(report)) });
}

export async function createPriorRuntimeAuthorityFromFile(options) {
  const input = await readRegularFile(options?.servicePath, "Cloud service readback");
  let service;
  try {
    service = JSON.parse(input.text);
  } catch {
    throw new Error("Cloud service readback is not JSON");
  }
  return createPriorRuntimeAuthority({
    sourceCommit: options?.sourceCommit,
    projectId: options?.projectId,
    service,
    serviceFileSha256: input.fileSha256,
  });
}

async function readRegularFile(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const bytes = await readFile(target);
  return Object.freeze({
    text: bytes.toString("utf8"),
    fileSha256: createHash("sha256").update(bytes).digest("hex"),
  });
}

async function writeNew(path, report) {
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
      throw new Error("Cloud prior authority path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) throw new Error(`${label} is invalid`);
  return value;
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function parse(args) {
  const allowed = new Set(["--source-commit", "--project", "--service-json", "--output"]);
  const values = {};
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0) {
      throw new Error("Cloud prior authority arguments are invalid");
    }
    if (Object.hasOwn(values, option)) throw new Error(`duplicate option: ${option}`);
    values[option] = value;
  }
  for (const option of allowed) if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  return values;
}

async function main() {
  const values = parse(process.argv.slice(2));
  const report = await createPriorRuntimeAuthorityFromFile({
    sourceCommit: values["--source-commit"],
    projectId: values["--project"],
    servicePath: values["--service-json"],
  });
  await writeNew(values["--output"], report);
  process.stdout.write(`${CLOUD_PRIOR_RUNTIME_AUTHORITY} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `cloud_prior_runtime_authority=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
