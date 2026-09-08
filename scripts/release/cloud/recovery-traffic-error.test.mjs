import assert from "node:assert/strict";
import test from "node:test";
import { recoveryTrafficHttpError, recoveryTrafficExitCode } from "./recovery-traffic-error.mjs";
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

for (const validateOnly of [true, false]) {
  test(`observed actAs denial retains message evidence and exits 77 in ${validateOnly ? "validate" : "apply"}`, async () => {
    const error = await recoveryTrafficHttpError(Response.json({ error: {
      status: "PERMISSION_DENIED",
      message: `Permission 'iam.serviceaccounts.actAs' denied on ${token} (or it may not exist).`,
    } }, { status: 403 }), { method: "PATCH", validateOnly });
    assert.match(error.message, /body_state=parsed; error_info=absent; permission_source=message/);
    assert.match(error.message, /reason=unknown/); // Do not invent an ErrorInfo.reason.
    assert.equal(error.diagnosis, "runtime-actas-denied");
    assert.equal(recoveryTrafficExitCode(error), 77);
    assert.ok(!error.message.includes(token));
  });
}

test("structured and quoted evidence are distinguished and combined without raw content", async () => {
  const error = await recoveryTrafficHttpError(Response.json({ error: {
    message: `Permission 'iam.serviceAccounts.actAs' denied ${token}`,
    details: [errorInfo("iam.serviceAccounts.actAs")],
  } }, { status: 403 }), { method: "PATCH", validateOnly: true });
  assert.match(error.message, /error_info=present; permission_source=error-info,message/);
  assert.equal(recoveryTrafficExitCode(error), 77);
});

for (const [method, status, permission] of [
  ["GET", 403, "iam.serviceAccounts.actAs"],
  ["PATCH", 401, "iam.serviceAccounts.actAs"],
  ["PATCH", 403, "run.services.update"],
]) {
  test(`${method}/${status}/${permission} does not claim the observed runtime actAs conflict`, async () => {
    const error = await recoveryTrafficHttpError(Response.json({ error: {
      details: [errorInfo(permission)],
    } }, { status }), { method, validateOnly: method === "PATCH" ? true : undefined });
    assert.equal(error.diagnosis, "unclassified-http-failure");
    assert.equal(recoveryTrafficExitCode(error), 2);
  });
}

for (const [payload, state] of [
  [null, "empty"], ["", "empty"], ["{", "invalid-json"],
  ["[]", "missing-error-object"], [JSON.stringify({ error: {} }), "parsed"],
  ["x".repeat(16 * 1024 + 1), "oversized"],
]) {
  test(`unavailable diagnostic content says ${state}, not a generic unexplained unknown`, async () => {
    const error = await recoveryTrafficHttpError(new Response(payload, { status: 403 }), { method: "PATCH", validateOnly: true });
    assert.match(error.message, new RegExp(`body_state=${state};`));
    assert.equal(recoveryTrafficExitCode(error), 2);
  });
}

test("a body transport error is distinguished from a successfully parsed permission denial", async () => {
  const response = new Response(new ReadableStream({ start(c) { c.error(new Error(token)); } }), { status: 403 });
  const error = await recoveryTrafficHttpError(response, { method: "PATCH", validateOnly: true });
  assert.match(error.message, /body_state=read-failed/);
  assert.equal(recoveryTrafficExitCode(error), 2);
  assert.ok(!error.message.includes(token));
  assert.equal(recoveryTrafficExitCode(new Error("runtime-actas-denied")), 2);
});
