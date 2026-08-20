import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  currentRuntimeIdentityForCommit,
  runtimeIdentityMatches,
} from "../src/job-service/runtime-identity.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const REPOSITORY_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
const HEALTH_TIMEOUT_MS = 15_000;

export async function verifyAcceptedSource(options, dependencies = {}) {
  const sourceCommit = canonicalSourceCommit(options?.sourceCommit);
  const repository = canonicalRepository(options?.repository);
  const run = dependencies.run ?? runCommand;

  run("git", ["fetch", "--no-tags", "--depth=1", "origin", "main"]);
  const resolvedCommit = run("git", [
    "rev-parse",
    "--verify",
    `${sourceCommit}^{commit}`,
  ]).trim();
  const remoteMain = run("git", ["rev-parse", "--verify", "origin/main"]).trim();
  if (resolvedCommit !== sourceCommit || remoteMain !== sourceCommit) {
    throw new Error("source commit is not the exact current origin/main commit");
  }

  const acceptanceText = run("gh", [
    "api",
    "--method",
    "GET",
    `repos/${repository}/actions/workflows/release-cli.yml/runs`,
    "-f",
    "event=workflow_dispatch",
    "-f",
    "status=success",
    "-f",
    `head_sha=${sourceCommit}`,
    "-f",
    "per_page=1",
  ]);
  let acceptance;
  try {
    acceptance = JSON.parse(acceptanceText);
  } catch {
    throw new Error("canonical acceptance lookup returned invalid JSON");
  }
  if (!Array.isArray(acceptance?.workflow_runs) || acceptance.workflow_runs.length < 1) {
    throw new Error("source commit has no successful canonical acceptance run");
  }

  if (options?.activeHealthUrl !== undefined) {
    const healthUrl = canonicalHealthUrl(options.activeHealthUrl);
    const fetchImpl = dependencies.fetchImpl ?? fetch;
    const response = await fetchImpl(healthUrl, {
      method: "GET",
      headers: { accept: "application/json" },
      signal: AbortSignal.timeout(HEALTH_TIMEOUT_MS),
    });
    if (!response.ok) {
      throw new Error("active runtime health request failed");
    }
    const health = await response.json();
    if (!runtimeIdentityMatches(
      health?.runtime,
      currentRuntimeIdentityForCommit(sourceCommit),
    )) {
      throw new Error("active runtime identity does not match the accepted source");
    }
  }

  return Object.freeze({ repository, sourceCommit });
}

function canonicalSourceCommit(value) {
  const sourceCommit = typeof value === "string" ? value.trim() : "";
  if (!COMMIT_PATTERN.test(sourceCommit)) {
    throw new Error("source commit must be a full lowercase Git SHA");
  }
  return sourceCommit;
}

function canonicalRepository(value) {
  const repository = typeof value === "string" ? value.trim() : "";
  if (!REPOSITORY_PATTERN.test(repository)) {
    throw new Error("repository must use the owner/name form");
  }
  return repository;
}

function canonicalHealthUrl(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("active health URL is invalid");
  }
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) {
    throw new Error("active health URL must be credential-free HTTPS");
  }
  url.pathname = "/health";
  return url;
}

function runCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`accepted-source command failed: ${command}`);
  }
  return result.stdout;
}

async function main() {
  const { values } = parseArgs({
    options: {
      "source-commit": { type: "string" },
      repository: { type: "string" },
      "active-health-url": { type: "string" },
    },
    strict: true,
  });
  try {
    const result = await verifyAcceptedSource({
      sourceCommit: values["source-commit"],
      repository: values.repository,
      activeHealthUrl: values["active-health-url"],
    });
    console.log(`accepted_source=passed source_commit=${result.sourceCommit}`);
  } catch {
    console.error("accepted_source=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
