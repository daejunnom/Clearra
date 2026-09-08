import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import {
  RECOVERY_SERVICE, planCandidateTagRemoval, assertUnchangedBeforePatch, verifyCandidateTagRemoval,
} from "./recovery-traffic-plan.mjs";
import { removeRecoveryCandidateTag } from "./remove-recovery-candidate-tag.mjs";
import { createRecoveryTrafficClient, readRollbackAccessToken, ROLLBACK_ACCOUNT } from "./recovery-traffic-client.mjs";

const type = "TRAFFIC_TARGET_ALLOCATION_TYPE_REVISION";
function fixture() {
  const target = { project: "clearra-cloud", region: "asia-northeast1",
    priorRevision: "clearra-current-job-v075-701454b", candidateRevision: "clearra-current-job-v080-9177273",
    candidateTag: "candidate-9177273", image: "asia-northeast1-docker.pkg.dev/clearra-cloud/clearra/clearra-current-job@sha256:" + "a".repeat(64) };
  const service = { name: RECOVERY_SERVICE, uid: "fixture-uid", etag: "etag-1", generation: "17", observedGeneration: "17",
    reconciling: false, latestCreatedRevision: `${RECOVERY_SERVICE}/revisions/${target.candidateRevision}`,
    template: { serviceAccount: "runtime-fixture", containers: [{ image: target.image }], maxInstanceRequestConcurrency: 1 },
    ingress: "INGRESS_TRAFFIC_INTERNAL_ONLY", invokerIamDisabled: false,
    traffic: [
      { type, revision: target.priorRevision, percent: 100 },
      { type, revision: target.priorRevision, tag: "keep-prior" },
      { type, revision: "clearra-current-job-v070-old", tag: "keep-other" },
      { type, revision: target.candidateRevision, tag: target.candidateTag },
    ],
    trafficStatuses: [
      { type, revision: target.priorRevision, percent: 100, tag: "keep-prior", uri: "https://example.invalid/prior" },
      { type, revision: "clearra-current-job-v070-old", tag: "keep-other" },
      { type, revision: target.candidateRevision, tag: target.candidateTag },
    ] };
  const revision = { name: `${RECOVERY_SERVICE}/revisions/${target.candidateRevision}`,
    uid: "fixture-candidate", containers: [{ image: target.image }], serviceAccount: "runtime-fixture" };
  return { target, service, revision };
}
function after(f) {
  const value = structuredClone(f.service);
  value.traffic = value.traffic.filter((x) => x.tag !== f.target.candidateTag);
  value.trafficStatuses = value.trafficStatuses.filter((x) => x.tag !== f.target.candidateTag);
  value.etag = "etag-2"; value.generation = value.observedGeneration = "18";
  return value;
}
function fakeFlow({ denyValidation = false, denyWrite = false, drift = false, corruptAfter = false } = {}) {
  const f = fixture(); const calls = []; let reads = 0;
  const request = async (method, path, body, validateOnly) => {
    calls.push({ method, path, body: structuredClone(body), validateOnly });
    if (method === "GET" && path.endsWith(`/revisions/${f.target.candidateRevision}`)) return structuredClone(f.revision);
    if (method === "GET") {
      reads += 1;
      const value = reads > 2 ? after(f) : structuredClone(f.service);
      if (drift && reads === 2) value.etag = "racing-etag";
      if (corruptAfter && reads > 2) value.template.serviceAccount = "changed";
      return value;
    }
    if ((validateOnly && denyValidation) || (!validateOnly && denyWrite)) throw new Error("HTTP 403");
    return { done: true };
  };
  return { ...f, calls, request };
}

test("removes only the sealed zero-percent candidate; preserves split prior/tag semantics and unrelated tags", () => {
  const f = fixture(); const original = structuredClone(f);
  const plan = planCandidateTagRemoval(f.service, f.revision, f.target);
  assert.deepEqual(Object.keys(plan.body).sort(), ["etag", "name", "traffic"]);
  assert.deepEqual(plan.body.traffic, original.service.traffic.slice(0, 3));
  assert.deepEqual(f, original);
  verifyCandidateTagRemoval(plan, after(f), f.revision);
});

for (const [name, mutate] of [
  ["wrong project", (f) => { f.target.project = "other"; }],
  ["wrong region", (f) => { f.target.region = "us-central1"; }],
  ["wrong service", (f) => { f.service.name += "-other"; }],
  ["missing etag", (f) => { delete f.service.etag; }],
  ["missing desired traffic", (f) => { delete f.service.traffic; }],
  ["ongoing reconciliation", (f) => { f.service.reconciling = true; }],
  ["newer revision", (f) => { f.service.latestCreatedRevision = "clearra-current-job-v080-newer"; }],
  ["unbound image", (f) => { f.revision.containers[0].image += "bad"; }],
  ["latest-relative allocation", (f) => { f.service.traffic[0].type = "TRAFFIC_TARGET_ALLOCATION_TYPE_LATEST"; }],
  ["candidate receives traffic", (f) => { f.service.traffic[3].percent = 1; }],
  ["candidate has another tag", (f) => { f.service.traffic.push({ type, revision: f.target.candidateRevision, tag: "unowned" }); }],
  ["tag points elsewhere", (f) => { f.service.traffic[3].revision = f.target.priorRevision; }],
  ["ambiguous tag", (f) => { f.service.traffic.push({ ...f.service.traffic[3] }); }],
  ["drifting observed routing", (f) => { f.service.trafficStatuses[0].percent = 99; }],
  ["unknown traffic field", (f) => { f.service.traffic[0].template = {}; }],
]) {
  test(`rejects ${name} before constructing a cleanup mutation`, () => {
    const f = fixture(); mutate(f);
    assert.throws(() => planCandidateTagRemoval(f.service, f.revision, f.target));
  });
}

test("unchanged preimage requires the exact etag, template, candidate and traffic", () => {
  const f = fixture(); const plan = planCandidateTagRemoval(f.service, f.revision, f.target);
  assertUnchangedBeforePatch(plan, f.service, f.revision);
  f.service.etag = "different";
  assert.throws(() => assertUnchangedBeforePatch(plan, f.service, f.revision), /preimage changed/);
});

test("validation denial causes zero writes and no alternate API or identity retry", async () => {
  const f = fakeFlow({ denyValidation: true });
  await assert.rejects(removeRecoveryCandidateTag(f.target, f), /403/);
  assert.equal(f.calls.filter((x) => x.method === "PATCH" && x.validateOnly === false).length, 0);
  assert.equal(f.calls.length, 3);
});

test("validate-only explicitly does not restore and never sends an actual PATCH", async () => {
  const f = fakeFlow();
  assert.deepEqual(await removeRecoveryCandidateTag(f.target, { ...f, validateOnly: true }), { status: "validated-not-restored" });
  assert.equal(f.calls.filter((x) => x.method === "PATCH").length, 1);
});

test("successful validation is rechecked before one etag-bound write and independent readback", async () => {
  const f = fakeFlow();
  assert.deepEqual(await removeRecoveryCandidateTag(f.target, f), { status: "tag-removal-verified" });
  assert.deepEqual(f.calls.filter((x) => x.method === "PATCH").map((x) => x.validateOnly), [true, false]);
  for (const call of f.calls.filter((x) => x.method === "PATCH")) assert.equal(call.body.etag, "etag-1");
});

test("non-persisted validateOnly Operation is not polled and never counts as a restoration", async () => {
  for (const validateOnly of [true, false]) {
    const f = fakeFlow(); const base = f.request; let polls = 0;
    f.request = async (...args) => {
      if (args[1].includes("/operations/")) { polls += 1; throw new Error("HTTP 404: dry-run operation is not persisted"); }
      const result = await base(...args);
      return args[0] === "PATCH" && args[3] === true
        ? { name: "projects/clearra-cloud/locations/asia-northeast1/operations/validated",
          metadata: { "@type": "type.googleapis.com/google.cloud.run.v2.Service", name: RECOVERY_SERVICE } }
        : result;
    };
    assert.deepEqual(await removeRecoveryCandidateTag(f.target, { ...f, validateOnly }),
      { status: validateOnly ? "validated-not-restored" : "tag-removal-verified" });
    assert.equal(polls, 0);
    assert.equal(f.calls.filter((call) => call.method === "PATCH" && call.validateOnly === false).length, validateOnly ? 0 : 1);
  }
});

test("unbound dry-run metadata and actual mutation polling errors remain failures", async () => {
  for (const metadata of [{}, { "@type": "other", name: RECOVERY_SERVICE },
    { "@type": "type.googleapis.com/google.cloud.run.v2.Service", name: RECOVERY_SERVICE + "-foreign" }]) {
    const f = fakeFlow(); const base = f.request;
    f.request = async (...args) => args[0] === "PATCH"
      ? { name: "projects/clearra-cloud/locations/asia-northeast1/operations/validated", metadata }
      : base(...args);
    await assert.rejects(removeRecoveryCandidateTag(f.target, f), /validation operation/);
  }
  const f = fakeFlow(); const base = f.request;
  f.request = async (...args) => {
    if (args[1].includes("/operations/")) throw new Error("HTTP 404: actual operation missing");
    const result = await base(...args);
    return args[0] === "PATCH" && args[3] === false
      ? { name: "projects/clearra-cloud/locations/asia-northeast1/operations/actual", done: false } : result;
  };
  await assert.rejects(removeRecoveryCandidateTag(f.target, { ...f, pause: async () => {} }), /actual operation missing/);
});

test("state changes after validation prevent the write", async () => {
  const f = fakeFlow({ drift: true });
  await assert.rejects(removeRecoveryCandidateTag(f.target, f), /preimage changed/);
  assert.equal(f.calls.filter((x) => x.validateOnly === false).length, 0);
});

test("write denial remains failure and is not retried by this helper", async () => {
  const f = fakeFlow({ denyWrite: true });
  await assert.rejects(removeRecoveryCandidateTag(f.target, f), /403/);
  assert.equal(f.calls.filter((x) => x.validateOnly === false).length, 1);
});

test("post-write template changes cannot be reported as cleanup success", async () => {
  const f = fakeFlow({ corruptAfter: true });
  await assert.rejects(removeRecoveryCandidateTag(f.target, f), /non-traffic authority.*service_fields=template; revision_fields=none/u);
  const original = fixture(); const plan = planCandidateTagRemoval(original.service, original.revision, original.target);
  const changed = after(original); changed['unknown-sensitive-field'] = 'do-not-print-this';
  assert.throws(() => verifyCandidateTagRemoval(plan, changed, original.revision), error =>
    error.message.includes('service_fields=unclassified-field') && !error.message.includes('unknown-sensitive-field') &&
    !error.message.includes('do-not-print-this'));
});

test("already tagless readback skips all PATCH calls", async () => {
  const f = fixture(); let patches = 0;
  const result = await removeRecoveryCandidateTag(f.target, { request: async (method, path) => {
    if (method === "PATCH") patches += 1;
    return path.includes("/revisions/") ? f.revision : after(f);
  } });
  assert.deepEqual(result, { status: "already-tagless" }); assert.equal(patches, 0);
});

test("malformed or foreign operations fail before write", async () => {
  for (const operation of [{}, { error: { code: 7 }, done: true },
    { name: "projects/other/locations/asia-northeast1/operations/foreign", done: false }]) {
    const f = fakeFlow(); const base = f.request;
    f.request = async (...args) => args[0] === "PATCH" ? operation : base(...args);
    await assert.rejects(removeRecoveryCandidateTag(f.target, f), /operation/);
  }
});

test("operation polling has a hard bound", async () => {
  const f = fakeFlow(); const base = f.request; let polls = 0;
  const operation = { name: "projects/clearra-cloud/locations/asia-northeast1/operations/fixture", done: false };
  f.request = async (...args) => {
    if (args[0] === "PATCH") return args[3] === true ? { done: true } : operation;
    if (args[1].includes("/operations/")) { polls += 1; return operation; }
    return base(...args);
  };
  await assert.rejects(removeRecoveryCandidateTag(f.target, { ...f, now: () => 0, pause: async () => {} }), /polling bound/);
  assert.equal(polls, 60);
});

const token = "fixture-not-a-real-access-token";
test("credential acquisition is shell-free and never asks to impersonate another identity", () => {
  assert.equal(readRollbackAccessToken((command, args, options) => {
    assert.equal(command, "gcloud");
    assert.deepEqual(args, ["auth", "print-access-token", `--account=${ROLLBACK_ACCOUNT}`, "--quiet"]);
    assert.equal(options.shell, false); assert.equal(options.stdio[1], "pipe");
    return { status: 0, stdout: token + "\n" };
  }), token);
  assert.throws(() => readRollbackAccessToken(() => ({ status: 1, stdout: token, stderr: token })), (error) => !error.message.includes(token));
});

test("transport sends only the traffic mask, refuses redirect forwarding and never sends template", async () => {
  const f = fixture(); const plan = planCandidateTagRemoval(f.service, f.revision, f.target);
  const request = createRecoveryTrafficClient(token, { fetchImpl: async (url, init) => {
    assert.equal(url.origin, "https://run.googleapis.com");
    assert.equal(url.search, "?updateMask=traffic&validateOnly=true");
    assert.equal(init.redirect, "error");
    assert.deepEqual(JSON.parse(init.body), plan.body);
    return new Response('{"done":true}', { status: 200 });
  } });
  assert.deepEqual(await request("PATCH", RECOVERY_SERVICE, plan.body, true), { done: true });
});

test("transport rejects foreign paths and overbroad mutation bodies without network access", async () => {
  let calls = 0;
  const request = createRecoveryTrafficClient(token, { fetchImpl: async () => { calls += 1; } });
  const f = fixture(); const body = planCandidateTagRemoval(f.service, f.revision, f.target).body;
  await assert.rejects(request("GET", "https://evil.invalid"));
  await assert.rejects(request("PATCH", RECOVERY_SERVICE, { ...body, template: {} }, false));
  await assert.rejects(request("PATCH", RECOVERY_SERVICE, { ...body, traffic: [] }, false));
  await assert.rejects(request("DELETE", RECOVERY_SERVICE));
  assert.equal(calls, 0);
});

test("network failures and HTTP denial do not expose tokens or response bodies", async () => {
  for (const fetchImpl of [async () => { throw new Error(token); }, async () => new Response(token, { status: 403 })]) {
    const request = createRecoveryTrafficClient(token, { fetchImpl });
    await assert.rejects(request("GET", RECOVERY_SERVICE), (error) => !error.message.includes(token));
  }
});

test("response bodies are bounded and malformed JSON fails closed", async () => {
  for (const body of ["[1]", "{", "x".repeat(1024 * 1024 + 1)]) {
    const request = createRecoveryTrafficClient(token, { fetchImpl: async () => new Response(body) });
    await assert.rejects(request("GET", RECOVERY_SERVICE), /invalid or exceeds/);
  }
});

test("PowerShell calls the narrow helper only after existing authority checks and retains final evidence sealing", async () => {
  const text = await readFile(new URL("../invoke-discord-runtime-recovery-v080.ps1", import.meta.url), "utf8");
  assert.match(text, /if \(\$candidateTagEntryCount -eq 1\)[\s\S]*remove-recovery-candidate-tag\.mjs/u);
  assert.match(text, /--intent "\$ArtifactRoot\/prestage\/intended-candidate-authority\.json"/u);
  assert.match(text, /--prior-revision \$PriorRevision/u);
  assert.match(text, /--workflow-run-attempt \$OriginalWorkflowRunAttempt/u);
  assert.match(text, /Cloud candidate residue is not the exact immutable latest revision/u);
  assert.match(text, /--binding "cloud_candidate_residue_readback=/u);
  assert.doesNotMatch(text, /--remove-tags=/u);
});
