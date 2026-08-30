#!/usr/bin/env node

import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SOURCE_COMMIT_PATTERN = /^[0-9a-f]{40}$/u;

export function canonicalCandidateOrigin(value) {
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new Error("candidate URL must be a canonical credential-free HTTPS origin");
  }
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("candidate URL must be a canonical credential-free HTTPS origin");
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.username ||
    parsed.password ||
    parsed.search ||
    parsed.hash ||
    parsed.pathname !== "/" ||
    (value !== parsed.origin && value !== `${parsed.origin}/`)
  ) {
    throw new Error("candidate URL must be a canonical credential-free HTTPS origin");
  }
  return parsed.origin;
}

export function renderCandidateSettingsV080({ sourceCommit, candidateUrl }) {
  if (typeof sourceCommit !== "string" || !SOURCE_COMMIT_PATTERN.test(sourceCommit)) {
    throw new Error("source commit must be an exact lowercase 40-character SHA");
  }
  const candidateOrigin = canonicalCandidateOrigin(candidateUrl);
  const lines = [
    "NODE_ENV=production",
    `CLEARRA_JOB_URL=${candidateOrigin}/jobs`,
    `CLEARRA_EXPECTED_JOB_SOURCE_COMMIT=${sourceCommit}`,
    `CLEARRA_EXPECTED_ENGINE_BUILD_ID=${sourceCommit}`,
    "CLEARRA_EXPECTED_JOB_CONTRACT_REVISION=clearra.search.contract.v2",
    "CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID=clearra.supply.projected-terminal-lookahead.v1",
    "CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION=clearra.solution-data.v1",
    "CLEARRA_WORKER_AUTHORITY=remote",
    "CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=1",
    "CLEARRA_SEARCH_TIMEOUT_MS=180000",
    "CLEARRA_REVERSE_SEARCH_TIMEOUT_MS=300000",
    "CLEARRA_FORWARD_SEARCH_TIMEOUT_MS=900000",
    "CLEARRA_INTERACTION_DEADLINE_MS=840000",
  ];
  if (lines.length !== 13) {
    throw new Error("candidate settings line cardinality drifted");
  }
  return Buffer.from(`${lines.join("\n")}\n`, "utf8");
}

export function candidateSettingsAuthorityV080(options) {
  const bytes = renderCandidateSettingsV080(options);
  return Object.freeze({
    lineCount: 13,
    size: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}

function parseArguments(arguments_) {
  const options = { sourceCommit: "", candidateUrl: "", hashOnly: false };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--source-commit" || argument === "--candidate-url") {
      const key = argument === "--source-commit" ? "sourceCommit" : "candidateUrl";
      if (options[key] || index + 1 >= arguments_.length) {
        throw new Error("invalid arguments");
      }
      options[key] = arguments_[index + 1];
      index += 1;
    } else if (argument === "--hash-only") {
      if (options.hashOnly) throw new Error("invalid arguments");
      options.hashOnly = true;
    } else {
      throw new Error("invalid arguments");
    }
  }
  if (!options.sourceCommit || !options.candidateUrl || !options.hashOnly) {
    throw new Error(
      "usage: candidate-settings-v080.mjs --source-commit SHA --candidate-url HTTPS_ORIGIN --hash-only",
    );
  }
  return options;
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const authority = candidateSettingsAuthorityV080(options);
  process.stdout.write(`${authority.sha256}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`oracle_candidate_settings=failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
