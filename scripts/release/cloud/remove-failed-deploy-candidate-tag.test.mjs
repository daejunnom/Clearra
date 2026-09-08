import assert from "node:assert/strict";
import test from "node:test";
import { cleanupFailedDeployCandidate, readDeployCleanupAccessToken, DEPLOY_CLEANUP_ACCOUNT, safeFailedCandidateCleanupReason }
  from "./remove-failed-deploy-candidate-tag.mjs";
import { ROLLBACK_ACCOUNT } from "./recovery-traffic-client.mjs";
const values = { project: "clearra-cloud", region: "asia-northeast1", intent: "sealed-intent.json",
  "prior-revision": "clearra-current-job-v075-701454b", "source-commit": "a".repeat(40),
  "workflow-run-id": "42", "workflow-run-attempt": "1", "deployment-nonce": "b".repeat(64) };
test("failed candidate cleanup reports only closed cause codes, never raw exceptions", () => {
  assert.equal(safeFailedCandidateCleanupReason({name:"RecoveryTrafficHttpError",httpStatus:403,
    phase:"validate",diagnosis:"runtime-actas-denied",message:"private response"}), "http-403-validate-runtime-actas-denied");
  assert.equal(safeFailedCandidateCleanupReason(new Error("Cloud recovery preimage changed after validateOnly; no mutation attempted")), "preimage-changed-before-apply");
  for (const value of [new Error("Bearer private-value"),{name:"RecoveryTrafficHttpError",httpStatus:403,phase:"secret-value"},null]) {
    assert.equal(safeFailedCandidateCleanupReason(value), "prior-traffic-or-exact-candidate-unverified");
  }
});
test("primary failure cleanup binds intent before using only the already approved deployer", async () => {
  const order = [];
  const result = await cleanupFailedDeployCandidate(values, {
    verify: async (path, binding) => {
      order.push("verify"); assert.equal(path, values.intent);
      assert.equal(binding.workflowRunId, "42"); assert.equal(binding.workflowRunAttempt, "1");
      assert.equal(binding.sourceCommit, values["source-commit"]);
      assert.equal(binding.deploymentNonce, values["deployment-nonce"]);
      return {cloud_candidate_revision:"clearra-current-job-v080-aaaaaaa",
        cloud_candidate_tag:"candidate-aaaaaaa", cloud_image_digest:"exact-image"};
    },
    token: () => { order.push("token"); return "not-a-real-token"; },
    client: () => { order.push("client"); return "bounded-transport"; },
    remove: async (target, options) => {
      order.push("remove"); assert.equal(target.candidateTag, "candidate-aaaaaaa");
      assert.equal(target.priorRevision, values["prior-revision"]);
      assert.deepEqual(options, {request:"bounded-transport"});
      return {status:"tag-removal-verified"};
    },
  });
  assert.deepEqual(order, ["verify","token","client","remove"]);
  assert.equal(result.status, "tag-removal-verified");
  let calls = 0;
  assert.equal(readDeployCleanupAccessToken((command, args, options) => {
    calls++; assert.equal(command, "gcloud");
    assert.deepEqual(args, ["auth","print-access-token",`--account=${DEPLOY_CLEANUP_ACCOUNT}`,"--quiet"]);
    assert.equal(options.shell, false); assert.equal(options.windowsHide, true);
    assert.deepEqual(options.stdio, ["ignore","pipe","pipe"]);
    return {status:0,stdout:"a".repeat(40)};
  }), "a".repeat(40));
  assert.equal(calls, 1);
  assert.notEqual(DEPLOY_CLEANUP_ACCOUNT, ROLLBACK_ACCOUNT);
});
test("invalid intent and unavailable deployer never fall back to rollback or ambient identities", async () => {
  await assert.rejects(cleanupFailedDeployCandidate(values, {
    verify: async () => { throw new Error("invalid intent"); },
    token: () => { assert.fail("credentials requested before validation"); },
  }), /invalid intent/);
  let calls = 0;
  assert.throws(() => readDeployCleanupAccessToken(() => {
    calls++; return {status:1,stderr:"sensitive detail",stdout:""};
  }), /no alternate identity/);
  assert.equal(calls, 1);
});
