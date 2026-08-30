import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requireSha256,
  requireSourceCommit,
} from "./canonical-release-evidence.mjs";
import {
  PRODUCTION_OBSERVATION_SECONDS,
  PRODUCTION_PROBE_SPEC_SCHEMA_ID,
  validateProductionProbeSpec,
} from "./observe-production-surfaces.mjs";

export const PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID =
  "clearra.production-observation-probe-authority.v1";

const DISCORD_SNOWFLAKE = /^\d{17,20}$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u;
const REGION = /^[a-z]+(?:-[a-z0-9]+)+[0-9]$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const BASE_PATH = /^\/[A-Za-z0-9._-]+$/u;
const ORACLE_NONCE = /^[0-9a-f]{64}$/u;
const SHARED_ADAPTER_PATH = fileURLToPath(
  new URL("./production-surface-probe-adapter.mjs", import.meta.url),
);

export async function createProductionProbeSpec(
  authority,
  { sharedAdapterPath = SHARED_ADAPTER_PATH } = {},
) {
  validateProductionProbeAuthority(authority);
  const sharedPath = resolve(sharedAdapterPath);
  const oraclePath = resolve(authority.oracle.adapter_path);
  const sharedSha256 = await hashRegularNonLinkFile(
    sharedPath,
    "tracked Discord/Cloud/Pages probe adapter",
  );
  const oracleSha256 = await hashRegularNonLinkFile(
    oraclePath,
    "tracked Oracle probe adapter",
  );
  if (oracleSha256 !== authority.oracle.adapter_sha256) {
    throw new Error("tracked Oracle probe adapter differs from its approved SHA-256");
  }
  const catalogFileSha256 = await hashRegularNonLinkFile(
    resolve(authority.discord.catalog_path),
    "Discord canonical catalog input",
  );
  const syncReportFileSha256 = await hashRegularNonLinkFile(
    resolve(authority.discord.sync_report_path),
    "Discord sync report input",
  );
  const smokeReportFileSha256 = await hashRegularNonLinkFile(
    resolve(authority.cloud.smoke_report_path),
    "Cloud candidate smoke report input",
  );
  if (
    catalogFileSha256 !== authority.discord.catalog_file_sha256 ||
    syncReportFileSha256 !== authority.discord.sync_report_file_sha256
  ) {
    throw new Error("Discord producer evidence file SHA-256 changed");
  }
  if (smokeReportFileSha256 !== authority.cloud.smoke_report_file_sha256) {
    throw new Error("Cloud candidate smoke evidence file SHA-256 changed");
  }

  const commit = authority.source_commit;
  const probes = [
    {
      surface: "cloud",
      runtime: "node",
      path: sharedPath,
      sha256: sharedSha256,
      arguments: [
        "cloud",
        "--source-commit", commit,
        "--project-id", authority.cloud.project_id,
        "--region", authority.cloud.region,
        "--service-name", authority.cloud.service_name,
        "--revision", authority.cloud.revision,
        "--tag", authority.cloud.tag,
        "--image-digest", authority.cloud.image_digest,
        "--smoke-report", resolve(authority.cloud.smoke_report_path),
        "--smoke-report-file-sha256", smokeReportFileSha256,
      ],
      timeout_seconds: authority.cloud.timeout_seconds,
    },
    {
      surface: "discord",
      runtime: "node",
      path: sharedPath,
      sha256: sharedSha256,
      arguments: [
        "discord",
        "--source-commit", commit,
        "--application-id", authority.discord.application_id,
        "--catalog", resolve(authority.discord.catalog_path),
        "--catalog-file-sha256", catalogFileSha256,
        "--sync-report", resolve(authority.discord.sync_report_path),
        "--sync-report-file-sha256", syncReportFileSha256,
      ],
      timeout_seconds: authority.discord.timeout_seconds,
    },
    {
      surface: "oracle",
      runtime: "powershell",
      path: oraclePath,
      sha256: oracleSha256,
      arguments: [
        "-Operation", "observe-candidate",
        "-ScriptReleaseId", authority.oracle.script_release_id,
        "-ScriptReleaseSha256", authority.oracle.script_release_sha256,
        "-SourceCommit", commit,
        "-CandidateUrl", authority.oracle.candidate_url,
        "-CandidateRevision", authority.oracle.candidate_revision,
        "-OracleReleaseId", authority.oracle.oracle_release_id,
        "-OracleReleaseSha256", authority.oracle.oracle_release_sha256,
        "-OracleSettingsSha256", authority.oracle.oracle_settings_sha256,
        "-DeploymentNonce", authority.oracle.deployment_nonce,
        "-VerifiedAfter", authority.oracle.verified_after,
      ],
      timeout_seconds: authority.oracle.timeout_seconds,
    },
    {
      surface: "pages",
      runtime: "node",
      path: sharedPath,
      sha256: sharedSha256,
      arguments: [
        "pages",
        "--source-commit", commit,
        "--url", authority.pages.url,
        "--deployment-id", authority.pages.deployment_id,
        "--artifact-sha256", authority.pages.artifact_sha256,
        "--base-path", authority.pages.base_path,
        "--accepted-run-id", authority.pages.accepted_run_id,
        "--accepted-run-attempt", authority.pages.accepted_run_attempt,
      ],
      timeout_seconds: authority.pages.timeout_seconds,
    },
  ];
  const spec = {
    schema_id: PRODUCTION_PROBE_SPEC_SCHEMA_ID,
    source_commit: commit,
    interval_seconds: authority.interval_seconds,
    probes,
  };
  validateProductionProbeSpec(spec, commit);
  return Object.freeze(spec);
}

export function validateProductionProbeAuthority(value) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "interval_seconds",
    "discord",
    "cloud",
    "oracle",
    "pages",
  ], "production observation probe authority");
  if (value.schema_id !== PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID) {
    throw new Error("production observation probe authority schema is invalid");
  }
  requireSourceCommit(value.source_commit, "probe authority source commit");
  requireInterval(value.interval_seconds);
  validateDiscordAuthority(value.discord);
  validateCloudAuthority(value.cloud, value.source_commit);
  validateOracleAuthority(value.oracle, value.source_commit);
  validatePagesAuthority(value.pages);
  rejectSecretMaterial(value, "production observation probe authority");
  return value;
}

function validateDiscordAuthority(value) {
  requireExactKeys(value, [
    "application_id",
    "catalog_path",
    "catalog_file_sha256",
    "sync_report_path",
    "sync_report_file_sha256",
    "timeout_seconds",
  ], "Discord probe authority");
  requirePattern(value.application_id, DISCORD_SNOWFLAKE, "Discord application ID");
  requireAbsolutePath(value.catalog_path, "Discord canonical catalog path");
  requireSha256(value.catalog_file_sha256, "Discord canonical catalog file SHA-256");
  requireAbsolutePath(value.sync_report_path, "Discord sync report path");
  requireSha256(value.sync_report_file_sha256, "Discord sync report file SHA-256");
  requireTimeout(value.timeout_seconds, "Discord probe timeout");
}

function validateCloudAuthority(value, sourceCommit) {
  requireExactKeys(value, [
    "project_id",
    "region",
    "service_name",
    "revision",
    "tag",
    "image_digest",
    "smoke_report_path",
    "smoke_report_file_sha256",
    "timeout_seconds",
  ], "Cloud probe authority");
  requirePattern(value.project_id, PROJECT_ID, "Cloud project ID");
  requirePattern(value.region, REGION, "Cloud region");
  requirePattern(value.service_name, IDENTIFIER, "Cloud service name");
  requirePattern(value.revision, IDENTIFIER, "Cloud revision");
  requirePattern(value.tag, IDENTIFIER, "Cloud traffic tag");
  requirePattern(value.image_digest, IMAGE_DIGEST, "Cloud image digest");
  requireAbsolutePath(value.smoke_report_path, "Cloud candidate smoke report path");
  requireSha256(value.smoke_report_file_sha256, "Cloud candidate smoke report file SHA-256");
  requireTimeout(value.timeout_seconds, "Cloud probe timeout");
  if (
    !value.revision.endsWith(sourceCommit.slice(0, 7)) ||
    !value.tag.endsWith(sourceCommit.slice(0, 7))
  ) {
    throw new Error("Cloud revision/tag authority differs from the accepted source");
  }
}

function validateOracleAuthority(value, sourceCommit) {
  requireExactKeys(value, [
    "adapter_path",
    "adapter_sha256",
    "script_release_id",
    "script_release_sha256",
    "candidate_url",
    "candidate_revision",
    "oracle_release_id",
    "oracle_release_sha256",
    "oracle_settings_sha256",
    "deployment_nonce",
    "verified_after",
    "timeout_seconds",
  ], "Oracle probe authority");
  requireAbsolutePath(value.adapter_path, "Oracle adapter path");
  requireSha256(value.adapter_sha256, "Oracle adapter SHA-256");
  requirePattern(value.script_release_id, IDENTIFIER, "Oracle script release ID");
  requireSha256(value.script_release_sha256, "Oracle script release SHA-256");
  requireCredentialFreeHttpsOrigin(value.candidate_url, "Oracle candidate URL");
  requirePattern(value.candidate_revision, IDENTIFIER, "Oracle candidate revision");
  requirePattern(value.oracle_release_id, IDENTIFIER, "Oracle release ID");
  requireSha256(value.oracle_release_sha256, "Oracle release SHA-256");
  requireSha256(value.oracle_settings_sha256, "Oracle settings SHA-256");
  if (!ORACLE_NONCE.test(value.deployment_nonce ?? "")) {
    throw new Error("Oracle deployment nonce is invalid");
  }
  canonicalTimestamp(value.verified_after, "Oracle verified-after time");
  requireTimeout(value.timeout_seconds, "Oracle probe timeout");
  if (
    value.script_release_id !== value.oracle_release_id ||
    value.script_release_sha256 !== value.oracle_release_sha256 ||
    !value.script_release_id.endsWith(sourceCommit.slice(0, 7)) ||
    !value.candidate_revision.endsWith(sourceCommit.slice(0, 7))
  ) {
    throw new Error("Oracle release/revision authority differs from the accepted source");
  }
}

function validatePagesAuthority(value) {
  requireExactKeys(value, [
    "url",
    "deployment_id",
    "artifact_sha256",
    "base_path",
    "accepted_run_id",
    "accepted_run_attempt",
    "timeout_seconds",
  ], "Pages probe authority");
  const url = requireCredentialFreeHttps(value.url, "Pages URL");
  requireNonEmptyString(value.deployment_id, "Pages deployment ID");
  requireSha256(value.artifact_sha256, "Pages artifact SHA-256");
  requirePattern(value.base_path, BASE_PATH, "Pages base path");
  requirePattern(value.accepted_run_id, DECIMAL_ID, "Pages accepted run ID");
  requirePattern(value.accepted_run_attempt, DECIMAL_ID, "Pages accepted run attempt");
  requireTimeout(value.timeout_seconds, "Pages probe timeout");
  if (new URL(url).pathname !== `${value.base_path}/`) {
    throw new Error("Pages URL path differs from the exact base path");
  }
}

async function hashRegularNonLinkFile(path, label) {
  await assertSafeDirectoryChain(dirname(path));
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const bytes = await readFile(path);
  return createHash("sha256").update(bytes).digest("hex");
}

async function readCanonicalJson(path, label) {
  const target = resolve(requireNonEmptyString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return value;
}

async function writeCanonicalJsonNew(path, value) {
  const target = resolve(requireNonEmptyString(path, "probe spec output path"));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(value)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("probe authority path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function requireAbsolutePath(value, label) {
  if (typeof value !== "string" || !isAbsolute(value)) {
    throw new Error(`${label} must be absolute`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requireCredentialFreeHttps(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(`${label} must be credential-free HTTPS without query or fragment`);
  }
  return url.toString();
}

function requireCredentialFreeHttpsOrigin(value, label) {
  const normalized = requireCredentialFreeHttps(value, label);
  if (new URL(normalized).pathname !== "/") {
    throw new Error(`${label} must be an HTTPS origin`);
  }
  return normalized;
}

function requireTimeout(value, label) {
  if (!Number.isSafeInteger(value) || value < 1 || value > 60) {
    throw new Error(`${label} must be 1 through 60 seconds`);
  }
}

function requireInterval(value) {
  if (
    !Number.isSafeInteger(value) ||
    value < 1 ||
    value > PRODUCTION_OBSERVATION_SECONDS
  ) {
    throw new Error("production probe interval is invalid");
  }
}

function parseCliArguments(args) {
  const values = {};
  const allowed = new Set(["--authority", "--output"]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option)) {
      throw new Error(`unsupported probe-spec materializer argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate probe-spec materializer argument: ${option}`);
    }
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
  }
  for (const option of allowed) {
    if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  }
  return values;
}

async function main() {
  const values = parseCliArguments(process.argv.slice(2));
  const authority = await readCanonicalJson(
    values["--authority"],
    "production probe authority",
  );
  const spec = await createProductionProbeSpec(authority);
  await writeCanonicalJsonNew(values["--output"], spec);
  process.stdout.write(`${PRODUCTION_PROBE_SPEC_SCHEMA_ID}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `production_probe_spec=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
