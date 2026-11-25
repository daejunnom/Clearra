import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import test from "node:test";

import {
  diagnosticAuthority, loggingReadArguments, sanitizeRuntimeOperations,
} from "./collect-cloud-runtime-operations.mjs";

const authority = {
  projectId: "clearra-cloud", revision: "clearra-current-job-v080-ffad392",
  sourceCommit: "a".repeat(40),
  since: "2026-11-25T18:00:00.000Z", until: "2026-11-25T21:00:00.000Z",
};
const entry = {
  timestamp: "2026-11-25T20:00:00.123456Z",
  resource: { labels: { revision_name: authority.revision } },
  jsonPayload: {
    event: "clearra.operation", at: "2026-11-25T20:00:00.120Z",
    scope: "job", kind: "search", command: "pc.path", status: "cancelled",
    durationMs: 300123, timeoutClass: "pc_reverse", timeoutMs: 839999,
    failureCode: "cancelled", failureStage: "execute",
    stageDurationsMs: { prepare: 2, execute: 300121 },
  },
};

test("runtime log query is bounded to one exact revision and at most 24 hours", () => {
  assert.deepEqual(diagnosticAuthority(authority), authority);
  const args = loggingReadArguments(authority);
  assert.deepEqual(args.slice(0, 2), ["logging", "read"]);
  assert.ok(args.includes("--limit=1000"));
  assert.ok(args.includes("--order=asc"));
  assert.match(args[2], /resource\.labels\.location="asia-northeast1"/);
  assert.match(args[2], /resource\.labels\.service_name="clearra-current-job"/);
  assert.ok(args[2].includes(`resource.labels.revision_name="${authority.revision}"`));
  assert.ok(args[2].includes(`timestamp>="${authority.since}"`));
  assert.ok(args[2].includes(`timestamp<="${authority.until}"`));
  for (const changed of [
    { revision: `${authority.revision}" OR true` },
    { projectId: "bad; command" },
    { sourceCommit: "main" },
    { since: "2026-11-25T00:00:00+09:00" },
    { until: "2026-11-26T21:00:00Z" },
    { until: authority.since },
  ]) assert.throws(() => loggingReadArguments({ ...authority, ...changed }));
});

test("operation collector preserves phase timing but strips raw input and HTTP identity", () => {
  const result = sanitizeRuntimeOperations([{
    ...entry,
    jsonPayload: {
      ...entry.jsonPayload,
      arguments: ["PRIVATE INPUT"], token: "PRIVATE TOKEN", userId: "PRIVATE USER",
      stageDurationsMs: { ...entry.jsonPayload.stageDurationsMs, "PRIVATE STAGE": 123 },
    },
  }, {
    timestamp: entry.timestamp, resource: entry.resource,
    httpRequest: { status: 499, latency: "300.123s", requestUrl: "https://PRIVATE/", remoteIp: "PRIVATE IP" },
    textPayload: "PRIVATE RAW OUTPUT",
  }], authority);
  assert.equal(result.operations.length, 1);
  assert.deepEqual(result.operations[0], {
    cloudTimestamp: "2026-11-25T20:00:00.123Z", ...entry.jsonPayload,
  });
  assert.deepEqual(result.requests, [{ at: "2026-11-25T20:00:00.123Z", status: 499, durationMs: 300123 }]);
  assert.equal(result.possiblyTruncated, false);
  assert.doesNotMatch(JSON.stringify(result), /PRIVATE|arguments|token|userId|requestUrl|remoteIp/);
});

test("collector accepts structured text and rejects unrelated or malformed metadata", () => {
  const result = sanitizeRuntimeOperations([
    { ...entry, jsonPayload: undefined, textPayload: JSON.stringify(entry.jsonPayload) },
    { ...entry, resource: { labels: { revision_name: "other-revision" } } },
    { ...entry, timestamp: "2026-11-25T17:00:00.000Z" },
    { ...entry, jsonPayload: { ...entry.jsonPayload, at: "invalid" } },
    { ...entry, jsonPayload: undefined, textPayload: "private unstructured output" },
  ], authority);
  assert.equal(result.operations.length, 1);
  assert.equal(result.ignoredEntryCount, 4);
  assert.equal(sanitizeRuntimeOperations(Array(1000).fill(entry), authority).possiblyTruncated, true);
  assert.throws(() => sanitizeRuntimeOperations(Array(1001).fill(entry), authority));
});

test("diagnostic workflow retains the protected main-only boundary without Oracle credentials or mutation", () => {
  const source = readFileSync(new URL("../../.github/workflows/cloud-cli-diagnostic.yml", import.meta.url), "utf8");
  assert.match(source, /if: github\.ref == 'refs\/heads\/main'/);
  assert.match(source, /timeout-minutes: 5/);
  assert.match(source, /environment: discord-path-confirmation/);
  assert.match(source, /service_account: \$\{\{ vars\.GCP_DEPLOY_SERVICE_ACCOUNT \}\}/);
  assert.match(source, /persist-credentials: false/);
  assert.match(source, /collect-cloud-runtime-operations\.mjs --validate-only/);
  assert.doesNotMatch(source, /secrets\.|ORACLE|ssh|update-traffic|run deploy|run jobs execute|write-all/);
  assert.ok(source.indexOf("--validate-only") < source.indexOf("google-github-actions/auth"));
});

test("read-only workflow uses the existing intended WIF diagnostic entrypoint", () => {
  const workflowUrl = new URL("../../.github/workflows/cloud-cli-diagnostic.yml", import.meta.url);
  const policy = readFileSync(new URL("./github/github-wif-bootstrap.mjs", import.meta.url), "utf8");
  const trusted = /const GITHUB_DIAGNOSTIC_WORKFLOW_REF =\s*"([^"]+)"/u.exec(policy)?.[1];
  const expected = `daejunnom/Clearra/.github/workflows/${basename(workflowUrl.pathname)}@refs/heads/main`;
  assert.equal(trusted, expected);
  assert.ok(policy.includes(`assertion.workflow_ref == '${expected}'`));
});

test("collector imports in a fresh Node process with all package dependencies forbidden", () => {
  const moduleUrl = new URL("./collect-cloud-runtime-operations.mjs", import.meta.url).href;
  const program = `
    import { registerHooks } from "node:module";
    registerHooks({ resolve(specifier, context, next) {
      if (!["node:", ".", "/", "file:"].some(prefix => specifier.startsWith(prefix))) {
        throw new Error("External dependency is unavailable");
      }
      const resolved = next(specifier, context);
      if (/[/\\\\]node_modules[/\\\\]|[/\\\\]packages[/\\\\]ctk3[/\\\\]dist[/\\\\]/.test(resolved.url)) {
        throw new Error("Built dependencies are unavailable");
      }
      return resolved;
    } });
    await import(${JSON.stringify(moduleUrl)});
  `;
  execFileSync(process.execPath, ["--input-type=module", "--eval", program], {
    encoding: "utf8", timeout: 10_000, stdio: ["ignore", "pipe", "pipe"],
  });
});
