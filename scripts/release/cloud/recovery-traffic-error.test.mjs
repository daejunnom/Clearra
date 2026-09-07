import assert from "node:assert/strict";
import test from "node:test";
import { createRecoveryTrafficClient } from "./recovery-traffic-client.mjs";

const service = "projects/clearra-cloud/locations/asia-northeast1/services/clearra-current-job";
const token = "fixture-secret-token-do-not-log";
const body = { name: service, etag: "fixture-etag", traffic: [{ revision: "clearra-current-job-v075-701454b", percent: 100 }] };
const errorInfo = (permission, reason = "IAM_PERMISSION_DENIED") => ({
  "@type": "type.googleapis.com/google.rpc.ErrorInfo", reason,
  domain: "iam.googleapis.com", metadata: { permission, resource: token },
});

for (const [method, validateOnly, phase] of [["PATCH", true, "validate"], ["PATCH", false, "apply"], ["GET", undefined, "read"]]) {
  test(`HTTP denial reports ${phase} without changing the request or retrying`, async () => {
    let calls = 0;
    const request = createRecoveryTrafficClient(token, { fetchImpl: async (url, init) => {
      calls += 1;
      if (method === "PATCH") {
        assert.equal(url.searchParams.get("validateOnly"), String(validateOnly));
        assert.equal(url.searchParams.get("updateMask"), "traffic");
        assert.deepEqual(JSON.parse(init.body), body);
      }
      return Response.json({ error: { status: "PERMISSION_DENIED", message: token,
        details: [errorInfo("iam.serviceAccounts.actAs")] } }, { status: 403 });
    } });
    await assert.rejects(request(method, service, method === "GET" ? undefined : body, validateOnly), (error) => {
      assert.equal(error.name, "RecoveryTrafficHttpError");
      assert.equal(error.httpStatus, 403);
      assert.equal(error.phase, phase);
      assert.match(error.message, /google_status=PERMISSION_DENIED/);
      assert.match(error.message, /reported_permission=iam.serviceAccounts.actAs/);
      assert.ok(!JSON.stringify(error).includes(token) && !error.message.includes(token));
      return true;
    });
    assert.equal(calls, 1);
  });
}

test("recognizes the quoted legacy actAs spelling but never logs its account or UID", async () => {
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => Response.json({ error: {
    message: `Permission 'iam.serviceaccounts.actAs' denied on ${token} (or it may not exist).`,
  } }, { status: 403 }) });
  await assert.rejects(request("PATCH", service, body, true), (error) =>
    error.message.includes("reported_permission=iam.serviceAccounts.actAs") && !error.message.includes(token));
});

test("keeps service-update permission distinct from actAs and sorts multiple known permissions", async () => {
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => Response.json({ error: {
    status: "PERMISSION_DENIED", details: [errorInfo("run.services.update"), errorInfo("serviceusage.services.use")],
  } }, { status: 403 }) });
  await assert.rejects(request("PATCH", service, body, false), (error) =>
    error.message.includes("reported_permission=run.services.update,serviceusage.services.use") &&
    !error.message.includes("iam.serviceAccounts.actAs"));
});

test("scope errors retain only the allowlisted reason", async () => {
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => Response.json({ error: {
    status: "PERMISSION_DENIED", details: [errorInfo(token, "ACCESS_TOKEN_SCOPE_INSUFFICIENT")],
  } }, { status: 403 }) });
  await assert.rejects(request("GET", service), (error) =>
    error.message.includes("reason=ACCESS_TOKEN_SCOPE_INSUFFICIENT") &&
    error.message.includes("reported_permission=unknown") && !error.message.includes(token));
});

for (const payload of [token, "{", "[]", JSON.stringify({ error: { status: token, message: token,
  details: [errorInfo(`run.services.update${token}`, token), { "@type": token, reason: token }] } })]) {
  test("malformed or arbitrary error content remains a redacted HTTP failure", async () => {
    const request = createRecoveryTrafficClient(token, { fetchImpl: async () => new Response(payload, { status: 403 }) });
    await assert.rejects(request("GET", service), (error) =>
      error.httpStatus === 403 && error.message.includes("reported_permission=unknown") && !error.message.includes(token));
  });
}

test("oversized error bodies cancel the stream and cannot mask the original denial", async () => {
  let cancelled = false;
  const response = new Response(new ReadableStream({
    start(controller) { controller.enqueue(new Uint8Array(16 * 1024 + 1)); },
    cancel() { cancelled = true; },
  }), { status: 403 });
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => response });
  await assert.rejects(request("GET", service), (error) => error.httpStatus === 403 && error.message.includes("reason=unknown"));
  assert.equal(cancelled, true);
  assert.equal(response.body.locked, false);
});

test("body read and cancellation failures cannot leak their exceptions", async () => {
  const response = new Response(new ReadableStream({ start(controller) { controller.error(new Error(token)); } }), { status: 401 });
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => response });
  await assert.rejects(request("GET", service), (error) => error.httpStatus === 401 && !error.message.includes(token));
});

test("empty body preserves denial and success bodies retain their existing behavior", async () => {
  const denied = createRecoveryTrafficClient(token, { fetchImpl: async () => new Response(null, { status: 403 }) });
  await assert.rejects(denied("GET", service), (error) => error.httpStatus === 403);
  const success = createRecoveryTrafficClient(token, { fetchImpl: async () => Response.json({ done: true }) });
  assert.deepEqual(await success("PATCH", service, body, true), { done: true });
});
