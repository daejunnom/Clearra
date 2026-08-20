import { lstatSync, readFileSync, rmSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  currentRuntimeIdentityForCommit,
  runtimeIdentityMatches,
} from "../src/job-service/runtime-identity.mjs";

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const RELEASE_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const PROOF_MAX_BYTES = 4096;

export function verifyOracleCandidateProof(proof, expected) {
  const sourceCommit = requiredMatch(expected?.sourceCommit, COMMIT_PATTERN, "source commit");
  const candidateUrl = canonicalCandidateUrl(expected?.candidateUrl);
  const candidateRevision = requiredMatch(
    expected?.candidateRevision,
    RELEASE_PATTERN,
    "candidate revision",
  );
  const oracleReleaseId = requiredMatch(
    expected?.oracleReleaseId,
    RELEASE_PATTERN,
    "Oracle release ID",
  );
  const oracleSettingsSha256 = requiredMatch(
    expected?.oracleSettingsSha256,
    SHA256_PATTERN,
    "Oracle settings digest",
  );
  const oracleReleaseSha256 = requiredMatch(
    expected?.oracleReleaseSha256,
    SHA256_PATTERN,
    "Oracle release tree digest",
  );
  const deploymentNonce = requiredMatch(
    expected?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const expectedRuntime = currentRuntimeIdentityForCommit(sourceCommit);
  const expectedJobUrl = `${candidateUrl}/jobs`;

  if (!proof || typeof proof !== "object" || Array.isArray(proof)) {
    throw new Error("Oracle candidate proof must be an object");
  }
  const expectedKeys = [
    "boundedJobSucceeded",
    "candidateRevision",
    "candidateUrl",
    "deploymentNonce",
    "gatewayReady",
    "jobUrl",
    "oracleReleaseId",
    "oracleReleaseSha256",
    "oracleSettingsSha256",
    "runtimeIdentity",
    "sourceCommit",
  ];
  const actualKeys = Object.keys(proof).sort();
  if (
    actualKeys.length !== expectedKeys.length ||
    actualKeys.some((key, index) => key !== expectedKeys[index])
  ) {
    throw new Error("Oracle candidate proof must contain exactly the approved fields");
  }
  if (
    proof.sourceCommit !== sourceCommit ||
    proof.candidateUrl !== candidateUrl ||
    proof.candidateRevision !== candidateRevision ||
    proof.jobUrl !== expectedJobUrl ||
    proof.oracleReleaseId !== oracleReleaseId ||
    proof.oracleReleaseSha256 !== oracleReleaseSha256 ||
    proof.oracleSettingsSha256 !== oracleSettingsSha256 ||
    proof.deploymentNonce !== deploymentNonce ||
    proof.gatewayReady !== true ||
    proof.boundedJobSucceeded !== true ||
    !runtimeIdentityMatches(proof.runtimeIdentity, expectedRuntime)
  ) {
    throw new Error("Oracle candidate proof does not match this exact deployment");
  }
  return Object.freeze({
    sourceCommit,
    candidateUrl,
    candidateRevision,
    oracleReleaseId,
    oracleReleaseSha256,
    oracleSettingsSha256,
  });
}

export function consumeOracleCandidateProof(path, expected, dependencies = {}) {
  const proofPath = (dependencies.resolvePath ?? resolve)(String(path ?? ""));
  const expectedName = `clearra-oracle-candidate-${expected?.deploymentNonce}.json`;
  if (dirname(proofPath) !== "/run/clearra-deploy" || basename(proofPath) !== expectedName) {
    throw new Error("Oracle candidate proof path is not nonce-bound to the root-only namespace");
  }
  const inspect = dependencies.lstat ?? lstatSync;
  const read = dependencies.readText ?? ((candidate) => readFileSync(candidate, "utf8"));
  const remove = dependencies.remove ?? ((candidate) => rmSync(candidate, { force: false }));
  const metadata = inspect(proofPath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.uid !== 0 ||
    (metadata.mode & 0o777) !== 0o600
  ) {
    throw new Error("Oracle candidate proof must be a root-owned mode-0600 regular file");
  }
  if (metadata.size < 2 || metadata.size > PROOF_MAX_BYTES) {
    throw new Error("Oracle candidate proof size is invalid");
  }
  const serialized = read(proofPath);
  remove(proofPath);
  let proof;
  try {
    proof = JSON.parse(serialized);
  } catch {
    throw new Error("Oracle candidate proof is not valid JSON");
  }
  return verifyOracleCandidateProof(proof, expected);
}

function canonicalCandidateUrl(value) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error("candidate URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/"
  ) {
    throw new Error("candidate URL must be a credential-free HTTPS origin");
  }
  return url.origin;
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value.trim() : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

async function main() {
  const { values } = parseArgs({
    options: {
      proof: { type: "string" },
      "source-commit": { type: "string" },
      "candidate-url": { type: "string" },
      "candidate-revision": { type: "string" },
      "oracle-release-id": { type: "string" },
      "oracle-release-sha256": { type: "string" },
      "oracle-settings-sha256": { type: "string" },
      "deployment-nonce": { type: "string" },
    },
    strict: true,
  });
  try {
    consumeOracleCandidateProof(values.proof, {
      sourceCommit: values["source-commit"],
      candidateUrl: values["candidate-url"],
      candidateRevision: values["candidate-revision"],
      oracleReleaseId: values["oracle-release-id"],
      oracleReleaseSha256: values["oracle-release-sha256"],
      oracleSettingsSha256: values["oracle-settings-sha256"],
      deploymentNonce: values["deployment-nonce"],
    });
    console.log("oracle_candidate_proof=verified");
  } catch {
    console.error("oracle_candidate_proof=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
