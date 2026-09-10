import { canonicalSha256 } from "./canonical-release-evidence.mjs";
import { fetchJsonBounded, validateCloudHealth } from "./production-surface-probe-adapter.mjs";

// Public HTTP reads only: no gcloud, SSH, Job submission, or extra authority.
// Missing webhook coverage is reconciled once a minute during the finite window.
export function createProductionObservationGuard({ fetchJson = fetchJsonBounded } = {}) {
  const misses = new Map();
  return async ({ baselines, sequence, signal }) => {
    const cloud = baselines.get("cloud");
    const pages = baselines.get("pages");
    const sourceCommit = cloud.source_commit;
    const endpoints = [
      ["Cloud stable", new URL("health", cloud.identity.stable_url),
        (value) => validateCloudHealth(value, sourceCommit, "Cloud stable guard")],
      ["Cloud tagged", new URL("health", cloud.identity.tagged_url),
        (value) => validateCloudHealth(value, sourceCommit, "Cloud tagged guard")],
      ["Pages", new URL("clearra-build-identity.json", pages.identity.url), (value) => {
        if (canonicalSha256(value) !== pages.freshness.identity_readback_sha256) {
          throw new Error("Pages identity changed during the observation wait");
        }
      }],
    ];
    const controller = new AbortController();
    const combined = signal ? AbortSignal.any([signal, controller.signal]) : controller.signal;
    const pending = endpoints.map(async ([name, url, validate]) => {
      url.searchParams.set("source", sourceCommit);
      url.searchParams.set("guard", String(sequence));
      let value;
      try {
        value = await fetchJson(url.toString(), `${name} observation guard`, {
          timeoutMs: 10_000, signal: combined,
        });
      } catch (error) {
        combined.throwIfAborted();
        const count = (misses.get(name) ?? 0) + 1;
        misses.set(name, count);
        if (count >= 2) throw new Error(`${name} observation guard failed twice`, { cause: error });
        return;
      }
      // A returned foreign identity is definitive; do not retry it as a network miss.
      validate(value);
      misses.set(name, 0);
    });
    try { await Promise.all(pending); }
    catch (error) { controller.abort(); await Promise.allSettled(pending); throw error; }
  };
}
