#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  createDiscordProductionCheckpointCandidate,
  prepareDiscordProductionCheckpointInputs,
  readCanonicalFile,
  validateDiscordProductionCheckpointCandidate,
  writeCanonicalNew,
} from "./discord-production-checkpoint.mjs";
import { validateDiscordCheckpointCandidatePrerequisites } from "./discord-deployment-recovery.mjs";
import { retryEvidenceWrite, verifyEvidenceRetrySafety } from "./production-evidence-retry.mjs";

// Keep the existing CLI and import API. Library consumers must use the core
// module, never import this executing entrypoint back into a sealing command.
export * from "./discord-production-checkpoint.mjs";

function parseCli() {
  return parseArgs({
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      "workflow-run-id": { type: "string" },
      "workflow-run-attempt": { type: "string" },
      "accepted-run-id": { type: "string" },
      "accepted-run-attempt": { type: "string" },
      version: { type: "string" },
      "base-path": { type: "string" },
      "application-id": { type: "string" },
      "catalog-artifact-id": { type: "string" },
      "catalog-artifact-digest": { type: "string" },
      "canonical-acceptance-evidence": { type: "string" },
      "recovery-debt-clearance": { type: "string" },
      "catalog-recovery-authority": { type: "string" },
      "prior-catalog-snapshot": { type: "string" },
      "desired-catalog": { type: "string" },
      "sync-authority": { type: "string" },
      "sync-report": { type: "string" },
      "production-observation": { type: "string" },
      "job-list": { type: "string" },
      report: { type: "string" },
      output: { type: "string" },
    },
    strict: true,
    allowPositionals: true,
  });
}

async function main() {
  const { values, positionals } = parseCli();
  try {
    if (
      positionals.length !== 1 ||
      !["verify-inputs", "seal-candidate", "verify-candidate", "verify-prerequisites"].includes(positionals[0])
    ) {
      throw new Error(
        "Discord checkpoint operation must be verify-inputs, seal-candidate, verify-candidate, or verify-prerequisites",
      );
    }
    const identity = {
      repository: values.repository,
      sourceCommit: values["source-commit"],
      workflowRunId: values["workflow-run-id"],
      workflowRunAttempt: values["workflow-run-attempt"],
      acceptedRunId: values["accepted-run-id"],
      acceptedRunAttempt: values["accepted-run-attempt"],
    };
    if (positionals[0] === "verify-prerequisites") {
      const jobList = JSON.parse(await readFile(resolve(values["job-list"]), "utf8"));
      const proof = validateDiscordCheckpointCandidatePrerequisites(jobList, identity);
      process.stdout.write(
        `discord_checkpoint_prerequisites=verified sha256=${proof.report_sha256}\n`,
      );
    } else if (["seal-candidate", "verify-inputs"].includes(positionals[0])) {
      const options = {
        ...identity,
        version: values.version,
        basePath: values["base-path"],
        applicationId: values["application-id"],
        catalogArtifactId: values["catalog-artifact-id"],
        catalogArtifactDigest: values["catalog-artifact-digest"],
        canonicalAcceptanceEvidence: values["canonical-acceptance-evidence"],
        recoveryDebtClearance: values["recovery-debt-clearance"],
        catalogRecoveryAuthority: values["catalog-recovery-authority"],
        priorCatalogSnapshot: values["prior-catalog-snapshot"],
        desiredCatalog: values["desired-catalog"],
        syncAuthority: values["sync-authority"],
        syncReport: values["sync-report"],
        productionObservation: values["production-observation"],
      };
      if (positionals[0] === "verify-inputs") {
        const prepared = await prepareDiscordProductionCheckpointInputs(options);
        process.stdout.write(`discord_checkpoint_inputs=verified source=${prepared.source_commit}\n`);
      } else {
        options.jobList = JSON.parse(await readFile(resolve(values["job-list"]), "utf8"));
        const candidate = await createDiscordProductionCheckpointCandidate(options);
        await retryEvidenceWrite(
          () => writeCanonicalNew(values.output, candidate),
          () => verifyEvidenceRetrySafety(resolve(process.env.GITHUB_WORKSPACE ?? "."), "sync"),
          { onRetry: () => process.stdout.write("checkpoint_seal=retrying-io-after-live-readback release_status=pending\n") },
        );
        process.stdout.write(`discord_checkpoint_candidate=sealed sha256=${candidate.report_sha256}\n`);
      }
    } else {
      const input = await readCanonicalFile(values.report, "Discord checkpoint candidate");
      validateDiscordProductionCheckpointCandidate(input.value, identity);
      process.stdout.write(`discord_checkpoint_candidate=verified sha256=${input.value.report_sha256}\n`);
    }
  } catch (error) {
    process.stderr.write(
      `discord_checkpoint_candidate=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
