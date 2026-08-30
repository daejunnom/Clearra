import { createHash } from "node:crypto";
import {
  lstat,
  open,
  readFile,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  requireExactKeys,
  requireNonEmptyString,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateCanonicalAcceptanceEvidence,
} from "./canonical-acceptance-evidence.mjs";
import { verifyAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";
import {
  validateCanonicalDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

export const DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID =
  "clearra.discord.command-sync-authority.v1";

const DECIMAL_ID = /^[1-9][0-9]{0,19}$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const VERSION = /^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/u;
const BASE_PATH = /^\/[A-Za-z0-9._-]+$/u;

export async function createDiscordCommandSyncAuthority(options) {
  const sourceCommit = requireSourceCommit(options?.sourceCommit);
  const repository = requirePattern(options?.repository, REPOSITORY, "repository");
  const version = requirePattern(options?.version, VERSION, "release version");
  const basePath = requirePattern(options?.basePath, BASE_PATH, "Pages base path");
  const acceptedRunId = requirePattern(
    options?.acceptedRunId,
    DECIMAL_ID,
    "accepted run ID",
  );
  const acceptedRunAttempt = requirePattern(
    options?.acceptedRunAttempt,
    DECIMAL_ID,
    "accepted run attempt",
  );

  const [acceptanceInput, catalogInput, acceptedCtk3Manifest] = await Promise.all([
    readCanonicalJsonFile(
      options?.canonicalAcceptanceEvidencePath,
      "canonical acceptance evidence",
    ),
    readCanonicalJsonFile(options?.catalogPath, "Discord canonical catalog"),
    verifyAcceptedCtk3Dist(
      requireNonEmptyString(options?.acceptedCtk3DistPath, "accepted CTK3 dist path"),
      sourceCommit,
      acceptedRunId,
      acceptedRunAttempt,
    ),
  ]);

  validateCanonicalAcceptanceEvidence(acceptanceInput.value, {
    repository,
    version,
    sourceCommit,
    runId: acceptedRunId,
    runAttempt: acceptedRunAttempt,
    basePath,
  });
  validateCanonicalDiscordCatalog(catalogInput.value, sourceCommit);

  const acceptedCtk3ManifestSha256 = canonicalSha256(acceptedCtk3Manifest);
  if (
    acceptanceInput.value.accepted_inputs.ctk3_manifest_sha256 !==
      acceptedCtk3ManifestSha256
  ) {
    throw new Error(
      "accepted CTK3 manifest differs from canonical acceptance evidence",
    );
  }

  const authority = sealCanonicalReport({
    schema_id: DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID,
    source_commit: sourceCommit,
    repository,
    release_version: version,
    pages_base_path: basePath,
    accepted_run_id: acceptedRunId,
    accepted_run_attempt: acceptedRunAttempt,
    accepted_ctk3_manifest_sha256: acceptedCtk3ManifestSha256,
    canonical_acceptance_evidence_sha256: acceptanceInput.value.report_sha256,
    canonical_acceptance_evidence_file_sha256: acceptanceInput.fileSha256,
    command_catalog_sha256: catalogInput.value.catalog_sha256,
    command_catalog_file_sha256: catalogInput.fileSha256,
  });
  validateDiscordCommandSyncAuthority(authority, {
    sourceCommit,
    acceptedRunId,
    acceptedRunAttempt,
    catalog: catalogInput.value,
    catalogFileSha256: catalogInput.fileSha256,
  });
  return authority;
}

export function validateDiscordCommandSyncAuthority(
  value,
  {
    sourceCommit,
    acceptedRunId,
    acceptedRunAttempt,
    catalog,
    catalogFileSha256,
  } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "repository",
    "release_version",
    "pages_base_path",
    "accepted_run_id",
    "accepted_run_attempt",
    "accepted_ctk3_manifest_sha256",
    "canonical_acceptance_evidence_sha256",
    "canonical_acceptance_evidence_file_sha256",
    "command_catalog_sha256",
    "command_catalog_file_sha256",
    "report_sha256",
  ], "Discord command sync authority");
  if (value.schema_id !== DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID) {
    throw new Error("Discord command sync authority schema is invalid");
  }
  verifyCanonicalReportHash(value, "Discord command sync authority");
  requireSourceCommit(value.source_commit, "Discord sync authority source commit");
  requirePattern(value.repository, REPOSITORY, "Discord sync authority repository");
  requirePattern(value.release_version, VERSION, "Discord sync authority release version");
  requirePattern(value.pages_base_path, BASE_PATH, "Discord sync authority Pages base path");
  requirePattern(value.accepted_run_id, DECIMAL_ID, "Discord sync authority run ID");
  requirePattern(
    value.accepted_run_attempt,
    DECIMAL_ID,
    "Discord sync authority run attempt",
  );
  for (const [field, label] of [
    ["accepted_ctk3_manifest_sha256", "accepted CTK3 manifest SHA-256"],
    ["canonical_acceptance_evidence_sha256", "canonical acceptance evidence SHA-256"],
    ["canonical_acceptance_evidence_file_sha256", "canonical acceptance evidence file SHA-256"],
    ["command_catalog_sha256", "command catalog SHA-256"],
    ["command_catalog_file_sha256", "command catalog file SHA-256"],
  ]) {
    requireSha256(value[field], `Discord sync authority ${label}`);
  }

  if (sourceCommit !== undefined && value.source_commit !== sourceCommit) {
    throw new Error("Discord command sync authority source differs from the requested source");
  }
  if (acceptedRunId !== undefined && value.accepted_run_id !== acceptedRunId) {
    throw new Error("Discord command sync authority run ID differs from the accepted run");
  }
  if (
    acceptedRunAttempt !== undefined &&
    value.accepted_run_attempt !== acceptedRunAttempt
  ) {
    throw new Error(
      "Discord command sync authority run attempt differs from the accepted attempt",
    );
  }
  if (catalog !== undefined) {
    validateCanonicalDiscordCatalog(catalog, value.source_commit);
    if (value.command_catalog_sha256 !== catalog.catalog_sha256) {
      throw new Error("Discord command sync authority differs from the canonical catalog");
    }
  }
  if (
    catalogFileSha256 !== undefined &&
    value.command_catalog_file_sha256 !== requireSha256(
      catalogFileSha256,
      "expected command catalog file SHA-256",
    )
  ) {
    throw new Error("Discord command sync authority differs from the catalog file bytes");
  }
  return value;
}

export async function readDiscordCommandSyncAuthority(
  path,
  expectedFileSha256,
  expected = {},
) {
  const input = await readCanonicalJsonFile(path, "Discord command sync authority");
  if (
    input.fileSha256 !== requireSha256(
      expectedFileSha256,
      "Discord command sync authority file SHA-256",
    )
  ) {
    throw new Error("Discord command sync authority file SHA-256 differs");
  }
  validateDiscordCommandSyncAuthority(input.value, expected);
  return Object.freeze({
    authority: input.value,
    fileSha256: input.fileSha256,
  });
}

export async function writeDiscordCommandSyncAuthority(path, authority) {
  validateDiscordCommandSyncAuthority(authority);
  const target = resolve(requireNonEmptyString(path, "authority output path"));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(authority)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

export function parseDiscordCommandSyncAuthorityCliArguments(args) {
  if (!Array.isArray(args)) {
    throw new Error("Discord command sync authority arguments are required");
  }
  const allowed = new Set([
    "--source-commit",
    "--repository",
    "--version",
    "--base-path",
    "--accepted-run-id",
    "--accepted-run-attempt",
    "--accepted-ctk3-dist",
    "--canonical-acceptance-evidence",
    "--catalog",
    "--output",
  ]);
  const values = {};
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option)) {
      throw new Error(`unsupported Discord sync authority argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate Discord sync authority argument: ${option}`);
    }
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
  }
  for (const option of allowed) {
    if (!Object.hasOwn(values, option)) {
      throw new Error(`${option} is required`);
    }
  }
  return values;
}

async function readCanonicalJsonFile(path, label) {
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
  return Object.freeze({
    value,
    fileSha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  });
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

async function main() {
  try {
    const values = parseDiscordCommandSyncAuthorityCliArguments(
      process.argv.slice(2),
    );
    const authority = await createDiscordCommandSyncAuthority({
      sourceCommit: values["--source-commit"],
      repository: values["--repository"],
      version: values["--version"],
      basePath: values["--base-path"],
      acceptedRunId: values["--accepted-run-id"],
      acceptedRunAttempt: values["--accepted-run-attempt"],
      acceptedCtk3DistPath: values["--accepted-ctk3-dist"],
      canonicalAcceptanceEvidencePath:
        values["--canonical-acceptance-evidence"],
      catalogPath: values["--catalog"],
    });
    await writeDiscordCommandSyncAuthority(values["--output"], authority);
    process.stdout.write(
      `${DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID} ${authority.report_sha256}\n`,
    );
  } catch (error) {
    process.stderr.write(
      `discord_command_sync_authority=failed reason=${
        error instanceof Error ? error.message : String(error)
      }\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
