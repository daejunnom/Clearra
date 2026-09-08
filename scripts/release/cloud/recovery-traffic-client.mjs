// Owns the bounded Cloud Run v2 transport used by candidate tag cleanup.
// Uses only the already federated rollback identity, with no identity fallback.
import { spawnSync } from "node:child_process";
import { recoveryTrafficHttpError } from "./recovery-traffic-error.mjs";

export const ROLLBACK_ACCOUNT = "clearra-github-rollback@clearra-cloud.iam.gserviceaccount.com";
const ALLOWED_PATH = /^projects\/(?:clearra-cloud|50060711800)\/locations\/asia-northeast1\/(?:services\/clearra-current-job(?:\/revisions\/clearra-current-job-[a-z0-9-]+)?|operations\/[A-Za-z0-9_-]+)$/u;
const MAX_JSON_BYTES = 1024 * 1024;

export function readRollbackAccessToken(run = spawnSync) {
  const result = run("gcloud", ["auth", "print-access-token", `--account=${ROLLBACK_ACCOUNT}`, "--quiet"], {
    shell: false, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 128 * 1024, timeout: 30_000, windowsHide: true,
  });
  const token = typeof result?.stdout === "string" ? result.stdout.trim() : "";
  if (result?.error || result?.status !== 0 || token.length < 16 || token.length > 16_384 || /\s/u.test(token)) {
    throw new Error("Cloud rollback token acquisition failed; no alternate identity is allowed");
  }
  return token;
}

export function createRecoveryTrafficClient(token, { fetchImpl = globalThis.fetch } = {}) {
  if (typeof token !== "string" || token.length < 16 || /\s/u.test(token)) throw new Error("Invalid rollback credential");
  return async function request(method, path, body, validateOnly) {
    if (!ALLOWED_PATH.test(path) || !["GET", "PATCH"].includes(method) ||
        (method === "GET" && (body !== undefined || validateOnly !== undefined))) {
      throw new Error("Cloud recovery request is outside the closed transport scope");
    }
    const url = new URL(`https://run.googleapis.com/v2/${path}`);
    if (method === "PATCH") {
      if (!path.endsWith("/services/clearra-current-job") || typeof validateOnly !== "boolean" ||
          !body || Object.keys(body).sort().join(",") !== "etag,name,traffic" || body.name !== path ||
          typeof body.etag !== "string" || !body.etag || !Array.isArray(body.traffic) || body.traffic.length === 0) {
        throw new Error("Cloud recovery PATCH must contain only name, etag and nonempty traffic");
      }
      url.searchParams.set("updateMask", "traffic");
      url.searchParams.set("validateOnly", String(validateOnly));
    }
    let response;
    try {
      response = await fetchImpl(url, {
        method, redirect: "error", signal: AbortSignal.timeout(20_000),
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
    } catch {
      throw new Error("Cloud recovery transport failed; no request or credential was logged");
    }
    if (!response.ok) {
      throw await recoveryTrafficHttpError(response, { method, validateOnly });
    }
    const reader = response.body?.getReader();
    if (!reader) throw new Error("Cloud recovery response has no body");
    const parts = [];
    let bytes = 0;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        bytes += value.byteLength;
        if (bytes > MAX_JSON_BYTES) throw new Error("response too large");
        parts.push(Buffer.from(value));
      }
      const value = JSON.parse(Buffer.concat(parts).toString("utf8"));
      if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("not an object");
      return value;
    } catch {
      await reader.cancel().catch(() => {});
      throw new Error("Cloud recovery response is invalid or exceeds its bound");
    } finally {
      reader.releaseLock();
    }
  };
}
