#!/usr/bin/env node

import { lstat, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { verifyAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";
import {
  validateCanonicalAcceptanceEvidence,
} from "./canonical-acceptance-evidence.mjs";

export async function validateDiscordAcceptedInputs(options) {
  const report = await readCanonicalJson(
    options?.canonicalAcceptanceEvidencePath,
    "canonical acceptance evidence",
  );
  validateCanonicalAcceptanceEvidence(report, {
    repository: options?.repository,
    version: options?.version,
    sourceCommit: options?.sourceCommit,
    runId: options?.acceptedRunId,
    runAttempt: options?.acceptedRunAttempt,
    basePath: options?.basePath,
  });
  await verifyAcceptedCtk3Dist(
    options?.acceptedCtk3DistPath,
    options?.sourceCommit,
    options?.acceptedRunId,
    options?.acceptedRunAttempt,
  );
  return Object.freeze({
    sourceCommit: options.sourceCommit,
    acceptedRunId: options.acceptedRunId,
    acceptedRunAttempt: options.acceptedRunAttempt,
    acceptanceReportSha256: report.report_sha256,
  });
}

async function readCanonicalJson(path, label) {
  const target = resolve(String(path ?? ""));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} is not canonical JSON`);
  }
  return value;
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("accepted Discord input path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function parse(args) {
  const allowed = new Set([
    "--source-commit", "--repository", "--version", "--base-path",
    "--accepted-run-id", "--accepted-run-attempt", "--accepted-ctk3-dist",
    "--canonical-acceptance-evidence",
  ]);
  const values = {};
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0) {
      throw new Error("accepted Discord input arguments are invalid");
    }
    if (Object.hasOwn(values, option)) throw new Error(`duplicate option: ${option}`);
    values[option] = value;
  }
  for (const option of allowed) if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  return values;
}

async function main() {
  const values = parse(process.argv.slice(2));
  const result = await validateDiscordAcceptedInputs({
    sourceCommit: values["--source-commit"],
    repository: values["--repository"],
    version: values["--version"],
    basePath: values["--base-path"],
    acceptedRunId: values["--accepted-run-id"],
    acceptedRunAttempt: values["--accepted-run-attempt"],
    acceptedCtk3DistPath: values["--accepted-ctk3-dist"],
    canonicalAcceptanceEvidencePath: values["--canonical-acceptance-evidence"],
  });
  process.stdout.write(
    `discord_accepted_inputs=verified source_commit=${result.sourceCommit} run_id=${result.acceptedRunId} run_attempt=${result.acceptedRunAttempt}\n`,
  );
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `discord_accepted_inputs=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
