// Owns candidate-tag removal sequencing, not deployment/recovery authority.
// The caller still verifies and seals the original run/attempt/artifact evidence.
import { recoveryTrafficExitCode } from "./recovery-traffic-error.mjs";
import { parseArgs } from "node:util";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import {
  RECOVERY_SERVICE, validateRecoveryTarget, planCandidateTagRemoval,
  assertUnchangedBeforePatch, verifyCandidateTagRemoval,
} from "./recovery-traffic-plan.mjs";
import { readRollbackAccessToken, createRecoveryTrafficClient } from "./recovery-traffic-client.mjs";

export async function removeRecoveryCandidateTag(target, {
  request, validateOnly = false, pause = sleep, now = Date.now,
} = {}) {
  validateRecoveryTarget(target);
  if (typeof request !== "function" || typeof validateOnly !== "boolean") {
    throw new Error("Cloud tag cleanup requires a bounded transport and an explicit mode");
  }
  const deadline = now() + 120_000;
  const revisionPath = `${RECOVERY_SERVICE}/revisions/${target.candidateRevision}`;
  const read = async () => {
    if (now() >= deadline) throw new Error("Cloud candidate cleanup deadline exceeded");
    const service = await request("GET", RECOVERY_SERVICE);
    const revision = await request("GET", revisionPath);
    return { service, revision };
  };
  const wait = async (operation) => {
    const name = operation?.name;
    for (let attempt = 0; ; attempt += 1) {
      if (!operation || typeof operation !== "object" || Array.isArray(operation) || operation.error) {
        throw new Error("Cloud traffic operation failed; recovery remains unverified");
      }
      if (operation.done === true) return;
      if (attempt >= 60 || now() >= deadline || typeof name !== "string" ||
          !/^projects\/(?:clearra-cloud|50060711800)\/locations\/asia-northeast1\/operations\/[A-Za-z0-9_-]+$/u.test(name) ||
          operation.name !== name || (operation.done !== undefined && operation.done !== false)) {
        throw new Error("Cloud traffic operation is invalid or exceeded its polling bound");
      }
      await pause(1000);
      operation = await request("GET", name);
    }
  };
  const first = await read();
  const plan = planCandidateTagRemoval(first.service, first.revision, target);
  if (!plan.removed) return { status: "already-tagless" };

  // validateOnly is not evidence of runtime recovery and does not guarantee
  // the subsequent write will pass IAM. Denial never falls back to v1/deployer.
  await wait(await request("PATCH", plan.body.name, plan.body, true));
  const checked = await read();
  assertUnchangedBeforePatch(plan, checked.service, checked.revision);
  if (validateOnly) return { status: "validated-not-restored" };
  if (now() >= deadline) throw new Error("Cloud candidate cleanup deadline exceeded");
  await wait(await request("PATCH", plan.body.name, plan.body, false));
  const after = await read();
  verifyCandidateTagRemoval(plan, after.service, after.revision);
  return { status: "tag-removal-verified" };
}

async function main() {
  const { values } = parseArgs({ strict: true, allowPositionals: false, options: {
    project: { type: "string" }, region: { type: "string" },
    intent: { type: "string" }, "prior-revision": { type: "string" },
    "source-commit": { type: "string" }, "workflow-run-id": { type: "string" },
    "workflow-run-attempt": { type: "string" }, "deployment-nonce": { type: "string" },
    "validate-only": { type: "boolean", default: false },
  } });
  // Reuse the existing canonical intent validator; never infer candidate names
  // from GUI observations or treat an unbound REST body as recovery permission.
  const { verifyDiscordPrestageIntent } = await import("../discord-deployment-recovery.mjs");
  const intent = await verifyDiscordPrestageIntent(values.intent, {
    sourceCommit: values["source-commit"], workflowRunId: values["workflow-run-id"],
    workflowRunAttempt: values["workflow-run-attempt"], deploymentNonce: values["deployment-nonce"],
  });
  const target = {
    project: values.project, region: values.region, priorRevision: values["prior-revision"],
    candidateRevision: intent.cloud_candidate_revision,
    candidateTag: intent.cloud_candidate_tag, image: intent.cloud_image_digest,
  };
  validateRecoveryTarget(target);
  const result = await removeRecoveryCandidateTag(target, {
    request: createRecoveryTrafficClient(readRollbackAccessToken()),
    validateOnly: values["validate-only"],
  });
  process.stdout.write(`cloud_candidate_tag_cleanup=${result.status}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`cloud_candidate_tag_cleanup=failed reason=${error.message}\n`);
    process.exitCode = recoveryTrafficExitCode(error);
  });
}
