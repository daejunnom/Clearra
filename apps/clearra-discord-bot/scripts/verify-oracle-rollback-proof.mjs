import { lstatSync, readFileSync, rmSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const RELEASE_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const PROOF_MAX_BYTES = 4096;

export function verifyOracleRollbackProof(proof, expected) {
  const priorRevision = requiredMatch(
    expected?.priorRevision,
    RELEASE_PATTERN,
    "prior Cloud revision",
  );
  const priorOracleReleaseId = requiredMatch(
    expected?.priorOracleReleaseId,
    RELEASE_PATTERN,
    "prior Oracle release ID",
  );
  const priorOracleSettingsSha256 = requiredMatch(
    expected?.priorOracleSettingsSha256,
    SHA256_PATTERN,
    "prior Oracle settings digest",
  );
  const priorOracleReleaseSha256 = requiredMatch(
    expected?.priorOracleReleaseSha256,
    SHA256_PATTERN,
    "prior Oracle release tree digest",
  );
  const priorRuntimeIdentitySha256 = requiredMatch(
    expected?.priorRuntimeIdentitySha256,
    SHA256_PATTERN,
    "prior runtime identity digest",
  );
  const deploymentNonce = requiredMatch(
    expected?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const priorJobUrl = canonicalJobUrl(expected?.priorJobUrl);
  if (!proof || typeof proof !== "object" || Array.isArray(proof)) {
    throw new Error("Oracle rollback proof must be an object");
  }
  const expectedKeys = [
    "boundedJobSucceeded",
    "deploymentNonce",
    "gatewayReady",
    "priorJobUrl",
    "priorOracleReleaseId",
    "priorOracleReleaseSha256",
    "priorOracleSettingsSha256",
    "priorRevision",
    "priorRuntimeIdentitySha256",
  ];
  const actualKeys = Object.keys(proof).sort();
  if (
    actualKeys.length !== expectedKeys.length ||
    actualKeys.some((key, index) => key !== expectedKeys[index])
  ) {
    throw new Error("Oracle rollback proof must contain exactly the approved fields");
  }
  if (
    proof.priorRevision !== priorRevision ||
    proof.priorOracleReleaseId !== priorOracleReleaseId ||
    proof.priorOracleReleaseSha256 !== priorOracleReleaseSha256 ||
    proof.priorOracleSettingsSha256 !== priorOracleSettingsSha256 ||
    proof.priorRuntimeIdentitySha256 !== priorRuntimeIdentitySha256 ||
    proof.priorJobUrl !== priorJobUrl ||
    proof.deploymentNonce !== deploymentNonce ||
    proof.gatewayReady !== true ||
    proof.boundedJobSucceeded !== true
  ) {
    throw new Error("Oracle rollback proof does not match the captured prior deployment");
  }
  return Object.freeze({ priorRevision, priorOracleReleaseId, priorJobUrl });
}

export function consumeOracleRollbackProof(path, expected, dependencies = {}) {
  const proofPath = (dependencies.resolvePath ?? resolve)(String(path ?? ""));
  const expectedName = `clearra-oracle-rollback-${expected?.deploymentNonce}.json`;
  if (dirname(proofPath) !== "/run/clearra-deploy" || basename(proofPath) !== expectedName) {
    throw new Error("Oracle rollback proof path is not nonce-bound to the root-only namespace");
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
    throw new Error("Oracle rollback proof must be a root-owned mode-0600 regular file");
  }
  if (metadata.size < 2 || metadata.size > PROOF_MAX_BYTES) {
    throw new Error("Oracle rollback proof size is invalid");
  }
  const serialized = read(proofPath);
  remove(proofPath);
  let proof;
  try {
    proof = JSON.parse(serialized);
  } catch {
    throw new Error("Oracle rollback proof is not valid JSON");
  }
  return verifyOracleRollbackProof(proof, expected);
}

function canonicalJobUrl(value) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error("prior Oracle job URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/jobs"
  ) {
    throw new Error("prior Oracle job URL must be a credential-free HTTPS /jobs URL");
  }
  return url.href;
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
      "prior-revision": { type: "string" },
      "prior-oracle-release-id": { type: "string" },
      "prior-oracle-release-sha256": { type: "string" },
      "prior-oracle-settings-sha256": { type: "string" },
      "prior-runtime-identity-sha256": { type: "string" },
      "prior-job-url": { type: "string" },
      "deployment-nonce": { type: "string" },
    },
    strict: true,
  });
  try {
    consumeOracleRollbackProof(values.proof, {
      priorRevision: values["prior-revision"],
      priorOracleReleaseId: values["prior-oracle-release-id"],
      priorOracleReleaseSha256: values["prior-oracle-release-sha256"],
      priorOracleSettingsSha256: values["prior-oracle-settings-sha256"],
      priorRuntimeIdentitySha256: values["prior-runtime-identity-sha256"],
      priorJobUrl: values["prior-job-url"],
      deploymentNonce: values["deployment-nonce"],
    });
    console.log("oracle_rollback_proof=verified");
  } catch {
    console.error("oracle_rollback_proof=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
