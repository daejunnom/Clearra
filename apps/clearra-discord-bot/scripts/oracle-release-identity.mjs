const RELEASE_TAG = /^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/u;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const DEPLOYMENT_NONCE = /^[0-9a-f]{64}$/u;

export function requireOracleReleaseTag(value) {
  if (typeof value !== "string" || !RELEASE_TAG.test(value)) {
    throw new Error("Oracle product release tag is invalid");
  }
  return value;
}

export function oracleCandidateReleaseId(releaseTag, sourceCommit) {
  const tag = requireOracleReleaseTag(releaseTag);
  if (typeof sourceCommit !== "string" || !SOURCE_COMMIT.test(sourceCommit)) {
    throw new Error("Oracle source commit is invalid");
  }
  return `${tag}-${sourceCommit.slice(0, 7)}`;
}

export function oracleReleaseTagFromCandidateId(releaseId, sourceCommit) {
  if (typeof releaseId !== "string") {
    throw new Error("Oracle candidate release ID is invalid");
  }
  const suffix = `-${String(sourceCommit).slice(0, 7)}`;
  if (!SOURCE_COMMIT.test(String(sourceCommit)) || !releaseId.endsWith(suffix)) {
    throw new Error("Oracle candidate release ID differs from source commit");
  }
  const tag = releaseId.slice(0, -suffix.length);
  requireOracleReleaseTag(tag);
  if (oracleCandidateReleaseId(tag, sourceCommit) !== releaseId) {
    throw new Error("Oracle candidate release ID is not canonical");
  }
  return tag;
}

export function oracleSettingsBackupPath(releaseTag, deploymentNonce) {
  const tag = requireOracleReleaseTag(releaseTag);
  if (
    typeof deploymentNonce !== "string" ||
    !DEPLOYMENT_NONCE.test(deploymentNonce)
  ) {
    throw new Error("Oracle deployment nonce is invalid");
  }
  return `/etc/clearra-gateway/settings.pre-${tag}-${deploymentNonce}`;
}
