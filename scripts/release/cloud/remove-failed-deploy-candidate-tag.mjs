// Primary-run failure cleanup under its already approved deployer identity.
// Not a recovery-identity fallback: this entry point runs only in promote.
import { spawnSync } from "node:child_process";
import { parseArgs } from "node:util";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { verifyDiscordPrestageIntent } from "../discord-deployment-recovery.mjs";
import { createRecoveryTrafficClient } from "./recovery-traffic-client.mjs";
import { removeRecoveryCandidateTag } from "./remove-recovery-candidate-tag.mjs";

export const DEPLOY_CLEANUP_ACCOUNT = "clearra-github-deployer@clearra-cloud.iam.gserviceaccount.com";
export function readDeployCleanupAccessToken(run = spawnSync) {
  const result = run("gcloud", ["auth", "print-access-token", `--account=${DEPLOY_CLEANUP_ACCOUNT}`, "--quiet"], {
    shell: false, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 128 * 1024, timeout: 30_000, windowsHide: true,
  });
  const token = typeof result?.stdout === "string" ? result.stdout.trim() : "";
  if (result?.error || result?.status !== 0 || token.length < 16 || token.length > 16_384 || /\s/u.test(token)) {
    throw new Error("Approved deployer cleanup token unavailable; no alternate identity is allowed");
  }
  return token;
}

export async function cleanupFailedDeployCandidate(values, {
  verify = verifyDiscordPrestageIntent,
  token = readDeployCleanupAccessToken,
  client = createRecoveryTrafficClient,
  remove = removeRecoveryCandidateTag,
} = {}) {
  // Validate the existing run/attempt/nonce-bound intent before authentication.
  const intent = await verify(values.intent, {
    sourceCommit: values["source-commit"], workflowRunId: values["workflow-run-id"],
    workflowRunAttempt: values["workflow-run-attempt"], deploymentNonce: values["deployment-nonce"],
  });
  return remove({
    project: values.project, region: values.region, priorRevision: values["prior-revision"],
    candidateRevision: intent.cloud_candidate_revision,
    candidateTag: intent.cloud_candidate_tag, image: intent.cloud_image_digest,
  }, { request: client(token()) });
}

async function main() {
  const { values } = parseArgs({ strict: true, allowPositionals: false, options: {
    project: { type: "string" }, region: { type: "string" }, intent: { type: "string" },
    "prior-revision": { type: "string" }, "source-commit": { type: "string" },
    "workflow-run-id": { type: "string" }, "workflow-run-attempt": { type: "string" },
    "deployment-nonce": { type: "string" },
  } });
  const result = await cleanupFailedDeployCandidate(values);
  process.stdout.write(`failed_deploy_candidate_cleanup=${result.status}\n`);
}
if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main().catch(() => {
    process.stderr.write("failed_deploy_candidate_cleanup=failed prior-traffic-or-exact-candidate-unverified\n");
    process.exitCode = 2;
  });
}
