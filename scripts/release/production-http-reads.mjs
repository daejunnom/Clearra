import {
  ARTIFACT_SCHEMA_VERSION,
  CONTRACT_SCHEMA_VERSION,
  normalizeRuntimeIdentity,
  RUNTIME_IDENTITY_SCHEMA,
  SUPPLY_SEMANTICS_ID,
} from "../../apps/clearra-discord-bot/src/job-service/runtime-identity.mjs";
import {
  requireExactKeys,
  requirePlainObject,
} from "./canonical-release-evidence.mjs";

// Shared read-only HTTP primitives must not depend on either executable probe
// adapter or the observation CLI: both entrypoints use this leaf module.
const MAX_HTTP_BYTES = 2 * 1024 * 1024;
const DEFAULT_HTTP_TIMEOUT_MS = 30_000;

export function validateCloudHealth(value, sourceCommit, label) {
  requireExactKeys(value, [
    "status",
    "activeJobs",
    "workerLimit",
    "runtime",
  ], label);
  if (
    value.status !== "ok" ||
    !Number.isSafeInteger(value.activeJobs) ||
    value.activeJobs < 0 ||
    value.workerLimit !== 8
  ) {
    throw new Error(`${label} service status is invalid`);
  }
  requireExactKeys(value.runtime, [
    "schema",
    "sourceCommit",
    "engineBuildId",
    "contractSchemaVersion",
    "supplySemanticsId",
    "artifactSchemaVersion",
  ], `${label} runtime identity`);
  const runtime = normalizeRuntimeIdentity(value.runtime);
  if (
    runtime.schema !== RUNTIME_IDENTITY_SCHEMA ||
    runtime.sourceCommit !== sourceCommit ||
    runtime.engineBuildId !== sourceCommit ||
    runtime.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION ||
    runtime.supplySemanticsId !== SUPPLY_SEMANTICS_ID ||
    runtime.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION
  ) {
    throw new Error(`${label} runtime identity differs from the release source`);
  }
}

export async function fetchJsonBounded(url, label, {
  fetchImplementation = fetch, timeoutMs = DEFAULT_HTTP_TIMEOUT_MS, signal,
} = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    let response;
    try {
      response = await fetchImplementation(url, {
        method: "GET", redirect: "error", cache: "no-store",
        headers: {
          accept: "application/json", "cache-control": "no-cache, no-store, max-age=0",
          pragma: "no-cache",
        },
        signal: signal ? AbortSignal.any([signal, controller.signal]) : controller.signal,
      });
    } catch {
      throw new Error(`${label} request failed`);
    }
    if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > MAX_HTTP_BYTES) {
      throw new Error(`${label} exceeded its response bound`);
    }
    const chunks = [];
    let length = 0;
    if (response.body) {
      const reader = response.body.getReader();
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          length += value.byteLength;
          if (length > MAX_HTTP_BYTES) throw new Error(`${label} exceeded its response bound`);
          chunks.push(value);
        }
      } finally { reader.releaseLock(); }
    }
    try {
      const bytes = Buffer.concat(chunks, length);
      const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
      requirePlainObject(value, label);
      return value;
    } catch {
      throw new Error(`${label} did not return one UTF-8 JSON object`);
    }
  } catch (error) {
    if (controller.signal.aborted) throw new Error(`${label} request timed out`);
    throw error;
  } finally {
    clearTimeout(timer);
    controller.abort();
  }
}
