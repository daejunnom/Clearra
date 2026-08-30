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
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "../../../scripts/release/canonical-release-evidence.mjs";
export const DISCORD_CATALOG_SCHEMA_ID =
  "clearra.discord.command-catalog.v1";
export const DISCORD_CATALOG_SNAPSHOT_SCHEMA_ID =
  "clearra.discord.command-catalog-snapshot.v1";
export const DISCORD_CATALOG_SYNC_SCHEMA_ID =
  "clearra.discord.command-catalog-sync.v1";
export const DISCORD_CATALOG_RESTORE_SCHEMA_ID =
  "clearra.discord.command-catalog-restore.v1";

const DISCORD_SNOWFLAKE = /^\d{17,20}$/u;
const RESPONSE_ONLY_COMMAND_KEYS = new Set([
  "application_id",
  "guild_id",
  "id",
  "version",
]);
const WRITABLE_COMMAND_KEYS = new Set([
  "contexts",
  "default_member_permissions",
  "default_permission",
  "description",
  "description_localizations",
  "dm_permission",
  "handler",
  "integration_types",
  "name",
  "name_localizations",
  "nsfw",
  "options",
  "type",
]);
const WRITABLE_OPTION_KEYS = new Set([
  "autocomplete",
  "channel_types",
  "choices",
  "description",
  "description_localizations",
  "max_length",
  "max_value",
  "min_length",
  "min_value",
  "name",
  "name_localizations",
  "options",
  "required",
  "type",
]);
const WRITABLE_CHOICE_KEYS = new Set([
  "name",
  "name_localizations",
  "value",
]);

export function createCanonicalDiscordCatalog({
  sourceCommit,
  commands,
}) {
  const commit = requireSourceCommit(sourceCommit);
  if (!Array.isArray(commands)) {
    throw new Error("Discord canonical catalog commands are required");
  }
  const normalized = normalizeDiscordCatalog(commands);
  return Object.freeze({
    schema_id: DISCORD_CATALOG_SCHEMA_ID,
    source_commit: commit,
    command_count: normalized.length,
    catalog_sha256: canonicalSha256(normalized),
    commands: normalized,
  });
}

export function validateCanonicalDiscordCatalog(value, expectedSourceCommit) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "command_count",
    "catalog_sha256",
    "commands",
  ], "Discord canonical command catalog");
  if (value.schema_id !== DISCORD_CATALOG_SCHEMA_ID) {
    throw new Error("Discord canonical command catalog schema is invalid");
  }
  requireSourceCommit(value.source_commit, "Discord catalog source commit");
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("Discord command catalog source differs from the release source");
  }
  const normalized = normalizeDiscordCatalog(value.commands);
  if (canonicalJson(normalized) !== canonicalJson(value.commands)) {
    throw new Error("Discord canonical command catalog is not normalized");
  }
  if (value.command_count !== normalized.length) {
    throw new Error("Discord canonical command count is invalid");
  }
  requireSha256(value.catalog_sha256, "Discord canonical catalog SHA-256");
  if (canonicalSha256(normalized) !== value.catalog_sha256) {
    throw new Error("Discord canonical catalog SHA-256 differs from its commands");
  }
  return value;
}

export async function captureDiscordCatalogSnapshot({
  rest,
  applicationId,
  sourceCommit,
  observedAt = new Date().toISOString(),
  observedCommands,
}) {
  const application = requireApplicationId(applicationId);
  const commit = requireSourceCommit(sourceCommit);
  const commands = observedCommands ??
    await rest.getGlobalCommands(application);
  const normalized = normalizeDiscordCatalog(commands, {
    allowResponseMetadata: true,
  });
  const snapshot = {
    schema_id: DISCORD_CATALOG_SNAPSHOT_SCHEMA_ID,
    source_commit: commit,
    application_id: application,
    captured_at: canonicalTimestamp(observedAt, "Discord catalog capture time"),
    command_count: normalized.length,
    catalog_sha256: canonicalSha256(normalized),
    commands: normalized,
  };
  return Object.freeze({
    ...snapshot,
    snapshot_sha256: canonicalSha256(snapshot),
  });
}

export function validateDiscordCatalogSnapshot(
  value,
  { expectedSourceCommit, expectedApplicationId } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "application_id",
    "captured_at",
    "command_count",
    "catalog_sha256",
    "commands",
    "snapshot_sha256",
  ], "Discord prior command catalog snapshot");
  if (value.schema_id !== DISCORD_CATALOG_SNAPSHOT_SCHEMA_ID) {
    throw new Error("Discord prior command catalog snapshot schema is invalid");
  }
  requireSourceCommit(value.source_commit, "Discord snapshot source commit");
  requireApplicationId(value.application_id);
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("Discord prior snapshot source differs from the release source");
  }
  if (
    expectedApplicationId !== undefined &&
    value.application_id !== expectedApplicationId
  ) {
    throw new Error("Discord prior snapshot application differs from the target");
  }
  canonicalTimestamp(value.captured_at, "Discord snapshot capture time");
  const normalized = normalizeDiscordCatalog(value.commands);
  if (canonicalJson(normalized) !== canonicalJson(value.commands)) {
    throw new Error("Discord prior command snapshot is not normalized");
  }
  if (value.command_count !== normalized.length) {
    throw new Error("Discord prior command snapshot count is invalid");
  }
  requireSha256(value.catalog_sha256, "Discord prior catalog SHA-256");
  if (canonicalSha256(normalized) !== value.catalog_sha256) {
    throw new Error("Discord prior catalog SHA-256 differs from its commands");
  }
  requireSha256(value.snapshot_sha256, "Discord prior snapshot SHA-256");
  const { snapshot_sha256: actualSnapshotSha256, ...unsignedSnapshot } = value;
  if (canonicalSha256(unsignedSnapshot) !== actualSnapshotSha256) {
    throw new Error("Discord prior snapshot SHA-256 differs from its canonical content");
  }
  return value;
}

export async function synchronizeDiscordCatalogRelease({
  rest,
  applicationId,
  sourceCommit,
  catalog,
  persistPriorSnapshot,
  now = () => new Date().toISOString(),
  synchronizationOptions,
}) {
  const {
    synchronizeGlobalCommandRegistrationFromObserved,
    verifyGlobalCommandRegistration,
  } = await import("../src/discord/command-registration.mjs");
  const application = requireApplicationId(applicationId);
  const commit = requireSourceCommit(sourceCommit);
  validateCanonicalDiscordCatalog(catalog, commit);
  if (typeof persistPriorSnapshot !== "function") {
    throw new Error("Discord sync requires durable prior-snapshot persistence");
  }

  const startedAt = canonicalTimestamp(now(), "Discord sync start time");
  const current = await rest.getGlobalCommands(application);
  const priorSnapshot = await captureDiscordCatalogSnapshot({
    rest,
    applicationId: application,
    sourceCommit: commit,
    observedAt: startedAt,
    observedCommands: current,
  });
  await persistPriorSnapshot(priorSnapshot);

  const synchronization = await synchronizeGlobalCommandRegistrationFromObserved(
    rest,
    application,
    catalog.commands,
    current,
    synchronizationOptions,
  );
  const readback = await rest.getGlobalCommands(application);
  verifyGlobalCommandRegistration(catalog.commands, readback);
  const readbackCommands = normalizeDiscordCatalog(readback, {
    allowResponseMetadata: true,
  });
  const endedAt = canonicalTimestamp(now(), "Discord sync end time");
  const report = sealCanonicalReport({
    schema_id: DISCORD_CATALOG_SYNC_SCHEMA_ID,
    source_commit: commit,
    application_id: application,
    started_at: startedAt,
    ended_at: endedAt,
    status: "synchronized",
    changed: synchronization.changed,
    command_count: catalog.command_count,
    expected_catalog_sha256: catalog.catalog_sha256,
    prior_snapshot_sha256: priorSnapshot.snapshot_sha256,
    prior_catalog_sha256: priorSnapshot.catalog_sha256,
    current_before_sha256: priorSnapshot.catalog_sha256,
    current_after_sha256: canonicalSha256(readbackCommands),
  });
  validateDiscordCatalogSyncReport(report, {
    expectedSourceCommit: commit,
    expectedApplicationId: application,
    expectedCatalog: catalog,
  });
  return Object.freeze({ priorSnapshot, report });
}

export function validateDiscordCatalogSyncReport(
  value,
  {
    expectedSourceCommit,
    expectedApplicationId,
    expectedCatalog,
  } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "application_id",
    "started_at",
    "ended_at",
    "status",
    "changed",
    "command_count",
    "expected_catalog_sha256",
    "prior_snapshot_sha256",
    "prior_catalog_sha256",
    "current_before_sha256",
    "current_after_sha256",
    "report_sha256",
  ], "Discord command catalog sync report");
  if (value.schema_id !== DISCORD_CATALOG_SYNC_SCHEMA_ID) {
    throw new Error("Discord command catalog sync report schema is invalid");
  }
  verifyCanonicalReportHash(value, "Discord command catalog sync report");
  requireSourceCommit(value.source_commit, "Discord sync source commit");
  requireApplicationId(value.application_id);
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("Discord sync report source differs from the release source");
  }
  if (
    expectedApplicationId !== undefined &&
    value.application_id !== expectedApplicationId
  ) {
    throw new Error("Discord sync report application differs from the target");
  }
  const started = canonicalTimestamp(value.started_at, "Discord sync start time");
  const ended = canonicalTimestamp(value.ended_at, "Discord sync end time");
  if (Date.parse(ended) < Date.parse(started)) {
    throw new Error("Discord sync report timestamps are reversed");
  }
  if (value.status !== "synchronized" || typeof value.changed !== "boolean") {
    throw new Error("Discord command catalog sync did not complete");
  }
  if (!Number.isSafeInteger(value.command_count) || value.command_count < 1) {
    throw new Error("Discord sync command count is invalid");
  }
  for (const key of [
    "expected_catalog_sha256",
    "prior_snapshot_sha256",
    "prior_catalog_sha256",
    "current_before_sha256",
    "current_after_sha256",
  ]) {
    requireSha256(value[key], `Discord sync ${key}`);
  }
  if (value.current_before_sha256 !== value.prior_catalog_sha256) {
    throw new Error("Discord sync prior snapshot is not its exact current preimage");
  }
  if (expectedCatalog !== undefined) {
    validateCanonicalDiscordCatalog(expectedCatalog, value.source_commit);
    if (
      value.expected_catalog_sha256 !== expectedCatalog.catalog_sha256 ||
      value.command_count !== expectedCatalog.command_count
    ) {
      throw new Error("Discord sync report differs from the canonical catalog producer");
    }
  }
  return value;
}

export async function restoreDiscordCatalogRelease({
  rest,
  applicationId,
  sourceCommit,
  priorSnapshot,
  expectedCurrentDigest,
  now = () => new Date().toISOString(),
  synchronizationOptions,
}) {
  const {
    synchronizeGlobalCommandRegistrationFromObserved,
    verifyGlobalCommandRegistration,
  } = await import("../src/discord/command-registration.mjs");
  const application = requireApplicationId(applicationId);
  const commit = requireSourceCommit(sourceCommit);
  const expectedDigest = requireSha256(
    expectedCurrentDigest,
    "Discord restore expected-current digest",
  );
  validateDiscordCatalogSnapshot(priorSnapshot, {
    expectedSourceCommit: commit,
    expectedApplicationId: application,
  });

  const startedAt = canonicalTimestamp(now(), "Discord restore start time");
  const current = await rest.getGlobalCommands(application);
  const normalizedCurrent = normalizeDiscordCatalog(current, {
    allowResponseMetadata: true,
  });
  const currentDigest = canonicalSha256(normalizedCurrent);
  if (currentDigest !== expectedDigest) {
    throw new Error(
      "Discord catalog restore refused because the current digest changed",
    );
  }

  const synchronization = await synchronizeGlobalCommandRegistrationFromObserved(
    rest,
    application,
    priorSnapshot.commands,
    current,
    synchronizationOptions,
  );
  const readback = await rest.getGlobalCommands(application);
  verifyGlobalCommandRegistration(priorSnapshot.commands, readback);
  const normalizedReadback = normalizeDiscordCatalog(readback, {
    allowResponseMetadata: true,
  });
  const readbackDigest = canonicalSha256(normalizedReadback);
  if (
    readbackDigest !== priorSnapshot.catalog_sha256 ||
    canonicalJson(normalizedReadback) !== canonicalJson(priorSnapshot.commands)
  ) {
    throw new Error("Discord catalog restore readback differs from the prior snapshot");
  }
  const endedAt = canonicalTimestamp(now(), "Discord restore end time");
  const report = sealCanonicalReport({
    schema_id: DISCORD_CATALOG_RESTORE_SCHEMA_ID,
    source_commit: commit,
    application_id: application,
    started_at: startedAt,
    ended_at: endedAt,
    status: "restored",
    changed: synchronization.changed,
    command_count: priorSnapshot.command_count,
    expected_current_sha256: expectedDigest,
    prior_snapshot_sha256: priorSnapshot.snapshot_sha256,
    prior_catalog_sha256: priorSnapshot.catalog_sha256,
    current_before_sha256: currentDigest,
    current_after_sha256: readbackDigest,
  });
  validateDiscordCatalogRestoreReport(report, {
    expectedSourceCommit: commit,
    expectedApplicationId: application,
  });
  return report;
}

export function validateDiscordCatalogRestoreReport(
  value,
  { expectedSourceCommit, expectedApplicationId } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "application_id",
    "started_at",
    "ended_at",
    "status",
    "changed",
    "command_count",
    "expected_current_sha256",
    "prior_snapshot_sha256",
    "prior_catalog_sha256",
    "current_before_sha256",
    "current_after_sha256",
    "report_sha256",
  ], "Discord command catalog restore report");
  if (value.schema_id !== DISCORD_CATALOG_RESTORE_SCHEMA_ID) {
    throw new Error("Discord command catalog restore report schema is invalid");
  }
  verifyCanonicalReportHash(value, "Discord command catalog restore report");
  requireSourceCommit(value.source_commit, "Discord restore source commit");
  requireApplicationId(value.application_id);
  if (expectedSourceCommit !== undefined && value.source_commit !== expectedSourceCommit) {
    throw new Error("Discord restore report source differs from the release source");
  }
  if (expectedApplicationId !== undefined && value.application_id !== expectedApplicationId) {
    throw new Error("Discord restore report application differs from the target");
  }
  canonicalTimestamp(value.started_at, "Discord restore start time");
  canonicalTimestamp(value.ended_at, "Discord restore end time");
  if (Date.parse(value.ended_at) < Date.parse(value.started_at)) {
    throw new Error("Discord restore report timestamps are reversed");
  }
  if (value.status !== "restored" || typeof value.changed !== "boolean") {
    throw new Error("Discord command catalog restore did not complete");
  }
  if (!Number.isSafeInteger(value.command_count) || value.command_count < 1) {
    throw new Error("Discord restore command count is invalid");
  }
  for (const key of [
    "expected_current_sha256",
    "prior_snapshot_sha256",
    "prior_catalog_sha256",
    "current_before_sha256",
    "current_after_sha256",
  ]) requireSha256(value[key], `Discord restore ${key}`);
  if (
    value.current_before_sha256 !== value.expected_current_sha256 ||
    value.current_after_sha256 !== value.prior_catalog_sha256
  ) {
    throw new Error("Discord restore is not bound to its exact preimage and readback");
  }
  return value;
}

export function normalizeDiscordCatalog(commands, options = {}) {
  if (!Array.isArray(commands) || commands.length < 1 || commands.length > 100) {
    throw new Error("Discord command catalog must contain 1 through 100 commands");
  }
  rejectSecretMaterial(commands, "Discord command catalog");
  const normalized = commands.map((command, index) =>
    normalizeCommand(command, index, options));
  normalized.sort((left, right) =>
    commandKey(left).localeCompare(commandKey(right), "en"));
  const keys = normalized.map(commandKey);
  if (new Set(keys).size !== keys.length) {
    throw new Error("Discord command catalog contains duplicate command identities");
  }
  return Object.freeze(normalized.map((command) => deepFreeze(command)));
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
  const target = resolve(requireNonEmptyString(path, "output path"));
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
      throw new Error(`release evidence path uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function normalizeCommand(value, index, { allowResponseMetadata = false }) {
  requirePlainObject(value, `Discord command ${index}`);
  const output = {};
  for (const [key, nested] of Object.entries(value)) {
    if (RESPONSE_ONLY_COMMAND_KEYS.has(key)) {
      if (!allowResponseMetadata) {
        throw new Error(`Discord command ${index}.${key} is response-only metadata`);
      }
      continue;
    }
    if (!WRITABLE_COMMAND_KEYS.has(key)) {
      throw new Error(`Discord command ${index}.${key} is not an approved writable field`);
    }
    output[key] = normalizeCommandField(key, nested, `Discord command ${index}.${key}`);
  }
  requireNonEmptyString(output.name, `Discord command ${index}.name`);
  if (output.type !== undefined && !Number.isSafeInteger(output.type)) {
    throw new Error(`Discord command ${index}.type is invalid`);
  }
  return output;
}

function normalizeCommandField(key, value, path) {
  if (key === "options") {
    if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
    return value.map((entry, index) => normalizeOption(entry, `${path}[${index}]`));
  }
  return normalizeJsonValue(value, path);
}

function normalizeOption(value, path) {
  requirePlainObject(value, path);
  const output = {};
  for (const [key, nested] of Object.entries(value)) {
    if (!WRITABLE_OPTION_KEYS.has(key)) {
      throw new Error(`${path}.${key} is not an approved writable option field`);
    }
    if (key === "options") {
      if (!Array.isArray(nested)) throw new Error(`${path}.options must be an array`);
      output.options = nested.map((entry, index) =>
        normalizeOption(entry, `${path}.options[${index}]`));
    } else if (key === "choices") {
      if (!Array.isArray(nested)) throw new Error(`${path}.choices must be an array`);
      output.choices = nested.map((entry, index) =>
        normalizeChoice(entry, `${path}.choices[${index}]`));
    } else {
      output[key] = normalizeJsonValue(nested, `${path}.${key}`);
    }
  }
  requireNonEmptyString(output.name, `${path}.name`);
  if (!Number.isSafeInteger(output.type)) throw new Error(`${path}.type is invalid`);
  return output;
}

function normalizeChoice(value, path) {
  requirePlainObject(value, path);
  const output = {};
  for (const [key, nested] of Object.entries(value)) {
    if (!WRITABLE_CHOICE_KEYS.has(key)) {
      throw new Error(`${path}.${key} is not an approved writable choice field`);
    }
    output[key] = normalizeJsonValue(nested, `${path}.${key}`);
  }
  requireNonEmptyString(output.name, `${path}.name`);
  if (!Object.hasOwn(output, "value")) throw new Error(`${path}.value is missing`);
  return output;
}

function normalizeJsonValue(value, path) {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "boolean"
  ) return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error(`${path} is not finite`);
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((entry, index) =>
      normalizeJsonValue(entry, `${path}[${index}]`));
  }
  requirePlainObject(value, path);
  const output = {};
  for (const [key, nested] of Object.entries(value)) {
    output[key] = normalizeJsonValue(nested, `${path}.${key}`);
  }
  return output;
}

function commandKey(command) {
  return `${command.type ?? 1}:${command.name}`;
}

function deepFreeze(value) {
  if (Array.isArray(value)) value.forEach(deepFreeze);
  else if (value && typeof value === "object") Object.values(value).forEach(deepFreeze);
  return Object.freeze(value);
}

function requireApplicationId(value) {
  if (typeof value !== "string" || !DISCORD_SNOWFLAKE.test(value)) {
    throw new Error("Discord application ID must be a 17-20 digit snowflake");
  }
  return value;
}

function parseCliArguments(args) {
  if (!Array.isArray(args) || args.length === 0) {
    throw new Error("Discord catalog release command is required");
  }
  const command = args[0];
  const specs = new Map([
    ["canonical", {
      required: ["--source-commit", "--output"],
      allowed: ["--source-commit", "--output"],
    }],
    ["sync", {
      required: [
        "--source-commit",
        "--application-id",
        "--catalog",
        "--prior-output",
        "--output",
      ],
      allowed: [
        "--source-commit",
        "--application-id",
        "--catalog",
        "--prior-output",
        "--output",
      ],
    }],
    ["restore", {
      required: [
        "--source-commit",
        "--application-id",
        "--prior-snapshot",
        "--expected-current-digest",
        "--output",
      ],
      allowed: [
        "--source-commit",
        "--application-id",
        "--prior-snapshot",
        "--expected-current-digest",
        "--output",
      ],
    }],
  ]);
  const spec = specs.get(command);
  if (!spec) throw new Error(`unsupported Discord catalog release command: ${command}`);
  const values = {};
  for (let index = 1; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!spec.allowed.includes(option)) {
      throw new Error(`unsupported Discord catalog release argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate Discord catalog release argument: ${option}`);
    }
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
  }
  for (const required of spec.required) {
    if (!Object.hasOwn(values, required)) throw new Error(`${required} is required`);
  }
  return { command, values };
}

async function main() {
  const { command, values } = parseCliArguments(process.argv.slice(2));
  const sourceCommit = values["--source-commit"];
  if (command === "canonical") {
    const { globalCommands } = await import("../src/discord/slash-command-catalog.mjs");
    const catalog = createCanonicalDiscordCatalog({ sourceCommit, commands: globalCommands });
    await writeCanonicalJsonNew(values["--output"], catalog);
    process.stdout.write(`${DISCORD_CATALOG_SCHEMA_ID} ${catalog.catalog_sha256}\n`);
    return;
  }

  const applicationId = values["--application-id"];
  const [{ loadCommandRegistrationCredentials }, { DiscordRestClient }] =
    await Promise.all([
      import("../src/discord/command-registration.mjs"),
      import("../src/discord/rest.mjs"),
    ]);
  const credentials = loadCommandRegistrationCredentials({
    ...process.env,
    DISCORD_APPLICATION_ID: applicationId,
  });
  const rest = new DiscordRestClient(credentials.token);
  if (command === "sync") {
    const catalog = await readCanonicalJson(
      values["--catalog"],
      "Discord canonical catalog",
    );
    validateCanonicalDiscordCatalog(catalog, sourceCommit);
    const { report } = await synchronizeDiscordCatalogRelease({
      rest,
      applicationId,
      sourceCommit,
      catalog,
      async persistPriorSnapshot(snapshot) {
        await writeCanonicalJsonNew(values["--prior-output"], snapshot);
      },
    });
    await writeCanonicalJsonNew(values["--output"], report);
    process.stdout.write(`${DISCORD_CATALOG_SYNC_SCHEMA_ID} ${report.report_sha256}\n`);
    return;
  }

  const priorSnapshot = await readCanonicalJson(
    values["--prior-snapshot"],
    "Discord prior command snapshot",
  );
  const report = await restoreDiscordCatalogRelease({
    rest,
    applicationId,
    sourceCommit,
    priorSnapshot,
    expectedCurrentDigest: values["--expected-current-digest"],
  });
  await writeCanonicalJsonNew(values["--output"], report);
  process.stdout.write(`${DISCORD_CATALOG_RESTORE_SCHEMA_ID} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `discord_catalog_release=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
