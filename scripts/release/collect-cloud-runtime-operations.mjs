import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { writeOperationalLog } from "../../apps/clearra-discord-bot/src/operational-log.mjs";

const MAX_ENTRIES = 1000;
const MAX_WINDOW_MS = 24 * 60 * 60 * 1000;
const REVISION = /^clearra-current-job-v[0-9]{3}-[0-9a-f]{7,40}$/u;
const PROJECT = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/u;
const SOURCE = /^[0-9a-f]{40}$/u;

function timestamp(value, label) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/u.test(value)) {
    throw new Error(`${label} must be a UTC timestamp`);
  }
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds)) throw new Error(`${label} is invalid`);
  return new Date(milliseconds).toISOString();
}

export function diagnosticAuthority(value) {
  if (!PROJECT.test(value.projectId ?? "")) throw new Error("Cloud project ID is invalid");
  if (!REVISION.test(value.revision ?? "")) throw new Error("Cloud revision is invalid");
  if (!SOURCE.test(value.sourceCommit ?? "")) throw new Error("Diagnostic source commit is invalid");
  const since = timestamp(value.since, "Diagnostic start");
  const until = timestamp(value.until, "Diagnostic end");
  const duration = Date.parse(until) - Date.parse(since);
  if (duration <= 0 || duration > MAX_WINDOW_MS) throw new Error("Diagnostic interval must be positive and at most 24 hours");
  return { projectId: value.projectId, revision: value.revision,
    sourceCommit: value.sourceCommit, since, until };
}

export function loggingReadArguments(authority) {
  const a = diagnosticAuthority(authority);
  const filter = [
    'resource.type="cloud_run_revision"',
    'resource.labels.location="asia-northeast1"',
    'resource.labels.service_name="clearra-current-job"',
    `resource.labels.revision_name="${a.revision}"`,
    `timestamp>="${a.since}"`,
    `timestamp<="${a.until}"`,
    '(jsonPayload.event="clearra.operation" OR textPayload:"clearra.operation" OR log_id("run.googleapis.com/requests"))',
  ].join(" AND ");
  return ["logging", "read", filter, `--project=${a.projectId}`, `--limit=${MAX_ENTRIES}`,
    "--order=asc", "--quiet",
    "--format=json(timestamp,resource.labels.revision_name,jsonPayload,textPayload,httpRequest.status,httpRequest.latency)"];
}

export function sanitizeRuntimeOperations(entries, authority) {
  const a = diagnosticAuthority(authority);
  if (!Array.isArray(entries) || entries.length > MAX_ENTRIES) throw new Error("Cloud log result exceeds the bounded entry contract");
  const operations = [];
  const requests = [];
  let ignoredEntries = 0;
  for (const entry of entries) {
    if (!entry || entry.resource?.labels?.revision_name !== a.revision) {
      ignoredEntries += 1;
      continue;
    }
    let at;
    try { at = timestamp(entry.timestamp, "Cloud log timestamp"); }
    catch { ignoredEntries += 1; continue; }
    if (at < a.since || at > a.until) {
      ignoredEntries += 1;
      continue;
    }
    let accepted = false;
    let payload = entry.jsonPayload;
    if (!payload && typeof entry.textPayload === "string") {
      try { payload = JSON.parse(entry.textPayload); } catch { /* Ignore non-structured text. */ }
    }
    if (payload?.event === "clearra.operation" && payload.scope === "job") {
      let operationAt;
      try { operationAt = timestamp(payload.at, "Operation timestamp"); } catch { /* Omit malformed records. */ }
      if (operationAt) {
        const capture = (line) => {
          operations.push({ cloudTimestamp: at, ...JSON.parse(line) });
          accepted = true;
        };
        writeOperationalLog({ info: capture, error: capture }, { ...payload, at: operationAt });
      }
    }
    const status = entry.httpRequest?.status;
    const latency = entry.httpRequest?.latency;
    if (Number.isSafeInteger(status) && status >= 100 && status <= 599 &&
      typeof latency === "string" && /^\d+(?:\.\d{1,9})?s$/u.test(latency)) {
      const durationMs = Math.round(Number(latency.slice(0, -1)) * 1000);
      if (Number.isSafeInteger(durationMs) && durationMs >= 0 && durationMs <= MAX_WINDOW_MS) {
        requests.push({ at, status, durationMs });
        accepted = true;
      }
    }
    if (!accepted) ignoredEntries += 1;
  }
  return {
    schema: "clearra.cloud-runtime-operations.v1",
    sourceCommit: a.sourceCommit,
    revision: a.revision,
    since: a.since,
    until: a.until,
    fetchedEntryCount: entries.length,
    entryLimit: MAX_ENTRIES,
    possiblyTruncated: entries.length === MAX_ENTRIES,
    ignoredEntryCount: ignoredEntries,
    operations,
    requests,
  };
}

function authorityFromEnvironment(environment) {
  if (environment.GITHUB_EVENT_NAME !== "workflow_dispatch" || environment.GITHUB_REF !== "refs/heads/main") {
    throw new Error("Runtime diagnostics require a manual exact-main workflow");
  }
  const sourceCommit = environment.GITHUB_SHA;
  if (!SOURCE.test(sourceCommit ?? "") || environment.CLEARRA_DIAGNOSTIC_EXPECTED_MAIN !== sourceCommit) {
    throw new Error("Runtime diagnostic source differs from the requested main commit");
  }
  const head = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", timeout: 10_000 }).trim();
  if (head !== sourceCommit) throw new Error("Runtime diagnostic checkout differs from its workflow source");
  return diagnosticAuthority({
    sourceCommit,
    projectId: environment.GCP_PROJECT_ID,
    revision: environment.CLEARRA_DIAGNOSTIC_REVISION,
    since: environment.CLEARRA_DIAGNOSTIC_SINCE,
    until: environment.CLEARRA_DIAGNOSTIC_UNTIL,
  });
}

async function main() {
  const authority = authorityFromEnvironment(process.env);
  if (process.argv[2] === "--validate-only") return;
  if (process.argv.length !== 2) throw new Error("Runtime diagnostic arguments are invalid");
  let entries;
  try {
    const raw = execFileSync("gcloud", loggingReadArguments(authority), {
      encoding: "utf8", timeout: 45_000, maxBuffer: 4 * 1024 * 1024,
      stdio: ["ignore", "pipe", "pipe"],
    });
    entries = JSON.parse(raw);
  } catch {
    // Provider output is never printed or uploaded. In particular, error
    // objects can contain subprocess stdout/stderr and request metadata.
    throw new Error("The bounded Cloud Logging read failed; no raw logs were retained");
  }
  const report = sanitizeRuntimeOperations(entries, authority);
  const runId = process.env.GITHUB_RUN_ID;
  const attempt = process.env.GITHUB_RUN_ATTEMPT;
  if (!/^[1-9][0-9]*$/u.test(runId ?? "") || !/^[1-9][0-9]*$/u.test(attempt ?? "")) {
    throw new Error("Runtime diagnostic workflow identity is invalid");
  }
  const output = resolve(`_local/artifacts/diagnostics/cloud-runtime/${runId}-${attempt}/operations.json`);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
  process.stdout.write(`${JSON.stringify(report)}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
