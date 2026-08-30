import { createHash } from "node:crypto";
import {
  lstat,
  mkdir,
  open,
  readFile,
  unlink,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { validateFinalSourceRevalidation } from "./validate-final-source-revalidation.mjs";

export const FINAL_SOURCE_ATTEMPT_SCHEMA_ID =
  "clearra.final-source-attempt-journal.v1";
export const FINAL_SOURCE_EVENT_SCHEMA_ID =
  "clearra.final-source-attempt-event.v1";

const RELEASE = "v0.8.0";
const SHA1 = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const ATTEMPT_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const SECRET_KEY = /(?:^|_)(?:secret|token|password|credential|api_key|private_key)(?:_|$)/iu;
const FORBIDDEN_PRIOR_AUTHORITY = /(?:^|[^0-9])v?0\.7\.5(?:[^0-9]|$)/iu;
const EVENT_KINDS = Object.freeze([
  "source",
  "contracts",
  "toolchains",
  "drift-audit",
  "canonical-gate",
  "surface-report",
  "release-artifact",
  "deployment-pages",
  "deployment-discord",
  "rollback-snapshot",
  "observation",
  "tag",
  "immutable-release",
]);
const REQUIRED_CARDINALITY = Object.freeze(new Map([
  ["source", 1],
  ["contracts", 1],
  ["toolchains", 1],
  ["drift-audit", 2],
  ["canonical-gate", 1],
  ["surface-report", 4],
  ["release-artifact", 3],
  ["deployment-pages", 1],
  ["deployment-discord", 1],
  ["rollback-snapshot", 1],
  ["observation", 1],
  ["tag", 1],
  ["immutable-release", 1],
]));

export async function initializeFinalSourceAttempt({
  journalPath,
  attemptId,
  sourceCommit,
}) {
  const path = requirePath(journalPath, "journalPath");
  requireAttemptId(attemptId);
  requireSourceCommit(sourceCommit);
  await ensureSafeDirectory(dirname(path));
  const releaseLock = await acquireJournalLock(path);
  try {
    const header = sealRecord({
      schema_id: FINAL_SOURCE_ATTEMPT_SCHEMA_ID,
      sequence: 0,
      attempt_id: attemptId,
      release: RELEASE,
      source_commit: sourceCommit,
      previous_sha256: null,
    });
    await writeNewFile(path, `${canonicalJson(header)}\n`);
    return header;
  } finally {
    await releaseLock();
  }
}

export async function appendFinalSourceAttemptEvent({
  journalPath,
  kind,
  payload,
}) {
  const path = requirePath(journalPath, "journalPath");
  requireEventKind(kind);
  requirePlainObject(payload, "event payload");
  rejectForbiddenMaterial(payload);
  const releaseLock = await acquireJournalLock(path);
  try {
    const records = await readVerifiedJournal(path);
    const header = records[0];
    const previous = records.at(-1);
    const event = sealRecord({
      schema_id: FINAL_SOURCE_EVENT_SCHEMA_ID,
      sequence: previous.sequence + 1,
      attempt_id: header.attempt_id,
      release: header.release,
      source_commit: header.source_commit,
      kind,
      payload,
      previous_sha256: previous.record_sha256,
    });
    const handle = await open(path, "a", 0o600);
    try {
      await handle.writeFile(`${canonicalJson(event)}\n`, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    return event;
  } finally {
    await releaseLock();
  }
}

export async function materializeFinalSourceManifest({
  journalPath,
  outputPath,
  discordCatalogSyncReport,
  productionObservationReport,
}) {
  const path = requirePath(journalPath, "journalPath");
  const target = outputPath === undefined
    ? undefined
    : requirePath(outputPath, "outputPath");
  if (target !== undefined) await ensureSafeDirectory(dirname(target));
  const releaseLock = await acquireJournalLock(path);
  try {
    const records = await readVerifiedJournal(path);
    const header = records[0];
    const events = records.slice(1);
    const grouped = groupEvents(events);
    requireCompleteEventSet(grouped);
    const manifest = {
      schema_id: "clearra.final-source-revalidation.v1",
      release: header.release,
      source: onlyPayload(grouped, "source"),
      contracts: onlyPayload(grouped, "contracts"),
      toolchains: onlyPayload(grouped, "toolchains"),
      drift_audits: sortedPayloads(grouped, "drift-audit", "phase"),
      canonical_gate: onlyPayload(grouped, "canonical-gate"),
      surface_reports: sortedPayloads(grouped, "surface-report", "surface"),
      release_artifacts: sortedPayloads(grouped, "release-artifact", "role"),
      deployment: {
        pages: onlyPayload(grouped, "deployment-pages"),
        discord: onlyPayload(grouped, "deployment-discord"),
        rollback_snapshot: onlyPayload(grouped, "rollback-snapshot"),
      },
      observation: onlyPayload(grouped, "observation"),
      tag: onlyPayload(grouped, "tag"),
      immutable_release: onlyPayload(grouped, "immutable-release"),
    };
    validateFinalSourceRevalidation(manifest, {
      expectedSourceCommit: header.source_commit,
      expectedRelease: header.release,
      discordCatalogSyncReport,
      productionObservationReport,
    });
    if (target !== undefined) {
      await writeNewFile(target, `${JSON.stringify(manifest, null, 2)}\n`);
    }
    return manifest;
  } finally {
    await releaseLock();
  }
}

export function parseFinalSourceAttemptCliArguments(args) {
  if (!Array.isArray(args) || args.length === 0) {
    throw new Error("final-source attempt command is required");
  }
  const command = args[0];
  const specifications = new Map([
    ["initialize", {
      allowed: ["--journal", "--attempt-id", "--source-commit"],
      required: ["--journal", "--attempt-id", "--source-commit"],
    }],
    ["append", {
      allowed: ["--journal", "--kind", "--payload"],
      required: ["--journal", "--kind", "--payload"],
    }],
    ["materialize", {
      allowed: [
        "--journal",
        "--output",
        "--discord-catalog-sync-report",
        "--production-observation-report",
      ],
      required: [
        "--journal",
        "--output",
        "--discord-catalog-sync-report",
        "--production-observation-report",
      ],
    }],
  ]);
  const specification = specifications.get(command);
  if (specification === undefined) {
    throw new Error(`unsupported final-source attempt command: ${String(command)}`);
  }
  const values = parseStrictNamedArguments(args.slice(1), specification);
  return { command, values };
}

async function readVerifiedJournal(journalPath) {
  const path = resolve(journalPath);
  await assertSafePathChain(dirname(path));
  await assertRegularNonLinkFile(path);
  const raw = await readFile(path, "utf8");
  if (raw.length === 0 || !raw.endsWith("\n")) {
    throw new Error("final-source attempt journal is empty or torn");
  }
  const lines = raw.slice(0, -1).split("\n");
  const records = lines.map((line, index) => {
    try {
      return JSON.parse(line);
    } catch {
      throw new Error(`final-source attempt journal line ${index + 1} is invalid JSON`);
    }
  });
  verifyHeader(records[0]);
  for (let index = 1; index < records.length; index += 1) {
    verifyEvent(records[index], records[index - 1], records[0], index);
  }
  return records;
}

function verifyHeader(header) {
  requirePlainObject(header, "attempt journal header");
  requireExactKeys(header, [
    "schema_id",
    "sequence",
    "attempt_id",
    "release",
    "source_commit",
    "previous_sha256",
    "record_sha256",
  ], "attempt journal header");
  if (header.schema_id !== FINAL_SOURCE_ATTEMPT_SCHEMA_ID ||
      header.sequence !== 0 ||
      header.release !== RELEASE ||
      header.previous_sha256 !== null) {
    throw new Error("attempt journal header identity is invalid");
  }
  requireAttemptId(header.attempt_id);
  requireSourceCommit(header.source_commit);
  verifyRecordHash(header, "attempt journal header");
}

function verifyEvent(event, previous, header, index) {
  requirePlainObject(event, `attempt journal event ${index}`);
  requireExactKeys(event, [
    "schema_id",
    "sequence",
    "attempt_id",
    "release",
    "source_commit",
    "kind",
    "payload",
    "previous_sha256",
    "record_sha256",
  ], `attempt journal event ${index}`);
  if (event.schema_id !== FINAL_SOURCE_EVENT_SCHEMA_ID ||
      event.sequence !== index ||
      event.attempt_id !== header.attempt_id ||
      event.release !== header.release ||
      event.source_commit !== header.source_commit ||
      event.previous_sha256 !== previous.record_sha256) {
    throw new Error(`attempt journal event ${index} identity or chain is invalid`);
  }
  requireEventKind(event.kind);
  requirePlainObject(event.payload, `attempt journal event ${index} payload`);
  rejectForbiddenMaterial(event.payload);
  verifyRecordHash(event, `attempt journal event ${index}`);
}

function sealRecord(record) {
  return {
    ...record,
    record_sha256: sha256(canonicalJson(record)),
  };
}

function verifyRecordHash(record, label) {
  if (typeof record.record_sha256 !== "string" || !SHA256.test(record.record_sha256)) {
    throw new Error(`${label} record_sha256 is invalid`);
  }
  const { record_sha256: actual, ...unsigned } = record;
  if (sha256(canonicalJson(unsigned)) !== actual) {
    throw new Error(`${label} hash differs from its canonical content`);
  }
}

function groupEvents(events) {
  const grouped = new Map(EVENT_KINDS.map((kind) => [kind, []]));
  for (const event of events) grouped.get(event.kind).push(event.payload);
  return grouped;
}

function requireCompleteEventSet(grouped) {
  for (const [kind, count] of REQUIRED_CARDINALITY) {
    const actual = grouped.get(kind).length;
    if (actual !== count) {
      throw new Error(`final-source attempt requires ${count} ${kind} event(s), found ${actual}`);
    }
  }
}

function onlyPayload(grouped, kind) {
  return grouped.get(kind)[0];
}

function sortedPayloads(grouped, kind, key) {
  return [...grouped.get(kind)].sort((left, right) => {
    const leftKey = String(left[key]);
    const rightKey = String(right[key]);
    return leftKey.localeCompare(rightKey, "en");
  });
}

function parseStrictNamedArguments(args, { allowed, required }) {
  const allowedSet = new Set(allowed);
  const values = {};
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (!allowedSet.has(option)) {
      throw new Error(`unsupported final-source attempt argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate final-source attempt argument: ${option}`);
    }
    const value = args[index + 1];
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
    index += 1;
  }
  for (const option of required) {
    if (!Object.hasOwn(values, option)) {
      throw new Error(`${option} is required`);
    }
  }
  return values;
}

function requireEventKind(kind) {
  if (!EVENT_KINDS.includes(kind)) {
    throw new Error(`unsupported final-source attempt event kind: ${String(kind)}`);
  }
}

function requireAttemptId(attemptId) {
  if (typeof attemptId !== "string" || !ATTEMPT_ID.test(attemptId)) {
    throw new Error("attemptId must be a bounded portable identifier");
  }
}

function requireSourceCommit(sourceCommit) {
  if (typeof sourceCommit !== "string" || !SHA1.test(sourceCommit)) {
    throw new Error("sourceCommit must be a full lowercase SHA-1 commit");
  }
}

function requirePath(value, label) {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0")) {
    throw new Error(`${label} must be a non-empty filesystem path`);
  }
  return resolve(value);
}

function rejectForbiddenMaterial(value, path = "payload") {
  if (typeof value === "string") {
    if (FORBIDDEN_PRIOR_AUTHORITY.test(value)) {
      throw new Error(`${path} reuses a v0.7.5 authority identity`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => rejectForbiddenMaterial(entry, `${path}[${index}]`));
    return;
  }
  if (value === null || typeof value !== "object") return;
  for (const [key, nested] of Object.entries(value)) {
    if (SECRET_KEY.test(key)) throw new Error(`${path}.${key} is forbidden secret material`);
    rejectForbiddenMaterial(nested, `${path}.${key}`);
  }
}

function canonicalJson(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("canonical JSON forbids non-finite numbers");
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((entry) => canonicalJson(entry)).join(",")}]`;
  }
  requirePlainObject(value, "canonical JSON value");
  const fields = Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`,
  );
  return `{${fields.join(",")}}`;
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} fields differ from the closed schema`);
  }
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

async function writeNewFile(path, contents) {
  const handle = await open(path, "wx", 0o600);
  try {
    await handle.writeFile(contents, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function acquireJournalLock(journalPath) {
  const lockPath = `${journalPath}.lock`;
  let handle;
  try {
    handle = await open(lockPath, "wx", 0o600);
  } catch (error) {
    if (error?.code === "EEXIST") {
      throw new Error("final-source attempt journal has a concurrent writer");
    }
    throw error;
  }
  try {
    await handle.writeFile(`${process.pid}\n`, "utf8");
    await handle.sync();
  } catch (error) {
    await handle.close();
    await unlink(lockPath).catch(() => undefined);
    throw error;
  }
  return async () => {
    await handle.close();
    await unlink(lockPath);
  };
}

async function ensureSafeDirectory(directory) {
  const missing = [];
  let current = resolve(directory);
  for (;;) {
    try {
      await lstat(current);
      break;
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      missing.push(current);
      const parent = dirname(current);
      if (parent === current) throw error;
      current = parent;
    }
  }
  await assertSafePathChain(current);
  for (const path of missing.reverse()) {
    try {
      await mkdir(path, { mode: 0o700 });
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
    }
    const status = await lstat(path);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${path}`);
    }
  }
}

async function assertSafePathChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const status = await lstat(current);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

async function assertRegularNonLinkFile(path) {
  const status = await lstat(path);
  if (!status.isFile() || status.isSymbolicLink()) {
    throw new Error(`release evidence journal is not a regular non-link file: ${path}`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const { command, values } = parseFinalSourceAttemptCliArguments(process.argv.slice(2));
    if (command === "initialize") {
      await initializeFinalSourceAttempt({
        journalPath: values["--journal"],
        attemptId: values["--attempt-id"],
        sourceCommit: values["--source-commit"],
      });
    } else if (command === "append") {
      const payload = JSON.parse(await readFile(resolve(values["--payload"]), "utf8"));
      await appendFinalSourceAttemptEvent({
        journalPath: values["--journal"],
        kind: values["--kind"],
        payload,
      });
    } else {
      const discordCatalogSyncReport = await readCanonicalProducerReport(
        values["--discord-catalog-sync-report"],
        "Discord command catalog sync report",
      );
      const productionObservationReport = await readCanonicalProducerReport(
        values["--production-observation-report"],
        "production observation report",
      );
      await materializeFinalSourceManifest({
        journalPath: values["--journal"],
        outputPath: values["--output"],
        discordCatalogSyncReport,
        productionObservationReport,
      });
    }
    process.stdout.write(`${FINAL_SOURCE_ATTEMPT_SCHEMA_ID}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}

async function readCanonicalProducerReport(path, label) {
  const target = resolve(requirePath(path, `${label} path`));
  await assertSafePathChain(dirname(target));
  await assertRegularNonLinkFile(target);
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical producer JSON`);
  }
  return value;
}
