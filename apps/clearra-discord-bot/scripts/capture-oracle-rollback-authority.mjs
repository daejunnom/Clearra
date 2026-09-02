import { createHash } from "node:crypto";
import {
  closeSync,
  fsyncSync,
  linkSync,
  lstatSync,
  openSync,
  readdirSync,
  readFileSync,
  realpathSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";

import { observePriorRuntimeAuthority } from "./oracle-runtime-authority.mjs";
import { releaseTreeSha256 } from "./release-tree-digest.mjs";

const RELEASE_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const RELEASE_ROOT = "/opt/clearra/releases";
const CURRENT_LINK = "/opt/clearra/current";
const SETTINGS_PATH = "/etc/clearra-gateway/settings";
const SETTINGS_DIRECTORY = "/etc/clearra-gateway";
const FINAL_BACKUP_PATH_PATTERN =
  /^\/etc\/clearra-gateway\/settings\.pre-v0\.8\.0-[0-9a-f]{64}$/;
const BACKUP_NAME_PATTERN = /^settings\.pre-v0\.8\.0-([0-9a-f]{64})(?:\.tmp)?$/;
const MAX_STALE_BACKUPS = 16;

export function captureOracleRollbackAuthority(options, dependencies = {}) {
  const priorRevision = requiredMatch(
    options?.priorRevision,
    RELEASE_PATTERN,
    "prior Cloud revision",
  );
  const deploymentNonce = requiredMatch(
    options?.deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const priorRuntimeAuthorityKind = options?.priorRuntimeAuthorityKind;
  const resolvePath = dependencies.realpath ?? realpathSync;
  const readSettings =
    dependencies.readSettings ?? ((path) => readFileSync(path));
  const inspect = dependencies.lstat ?? lstatSync;
  const computeReleaseDigest =
    dependencies.releaseTreeSha256 ?? releaseTreeSha256;
  const run = dependencies.run ?? runCommand;
  const writeBackup = dependencies.writeBackup ?? writeSettingsBackup;
  const cleanupBackups = dependencies.cleanupBackups ?? cleanupStaleSettingsBackups;

  const priorOracleRelease = normalizePath(resolvePath(CURRENT_LINK));
  const priorOracleReleaseId = basename(priorOracleRelease);
  if (
    priorOracleRelease !== `${RELEASE_ROOT}/${priorOracleReleaseId}` ||
    !RELEASE_PATTERN.test(priorOracleReleaseId)
  ) {
    throw new Error(
      "active Oracle release is outside the immutable release root",
    );
  }
  const priorOracleReleaseSha256 = computeReleaseDigest(priorOracleRelease);
  if (!SHA256_PATTERN.test(priorOracleReleaseSha256)) {
    throw new Error("active Oracle release tree digest is invalid");
  }

  const settingsMetadata = inspect(SETTINGS_PATH);
  if (
    !settingsMetadata.isFile() ||
    settingsMetadata.isSymbolicLink() ||
    settingsMetadata.uid !== 0
  ) {
    throw new Error("active Oracle settings must be a root-owned regular file");
  }
  const settingsBytes = readSettings(SETTINGS_PATH);
  const priorOracleSettingsSha256 = createHash("sha256")
    .update(settingsBytes)
    .digest("hex");
  const settingsText = Buffer.from(settingsBytes).toString("utf8");
  const priorJobUrl = exactSetting(settingsText, "CLEARRA_JOB_URL");
  const healthUrl = canonicalHealthUrl(priorJobUrl);
  let health;
  try {
    health = JSON.parse(
      run("/usr/bin/curl", [
        "--fail",
        "--silent",
        "--show-error",
        "--max-time",
        "15",
        healthUrl,
      ]),
    );
  } catch {
    throw new Error("prior Cloud runtime health identity is unavailable");
  }
  const priorRuntimeAuthority = observePriorRuntimeAuthority({
    kind: priorRuntimeAuthorityKind,
    priorRevision,
    priorOracleReleaseId,
    health,
  });
  const priorOracleSettingsBackup = `/etc/clearra-gateway/settings.pre-v0.8.0-${deploymentNonce}`;
  cleanupBackups(priorOracleSettingsBackup);
  writeBackup(priorOracleSettingsBackup, settingsBytes);

  return Object.freeze({
    priorRevision,
    priorOracleRelease,
    priorOracleReleaseId,
    priorOracleReleaseSha256,
    priorOracleSettingsBackup,
    priorOracleSettingsSha256,
    priorRuntimeAuthorityKind: priorRuntimeAuthority.kind,
    priorRuntimeAuthoritySha256: priorRuntimeAuthority.sha256,
    priorJobUrl,
    deploymentNonce,
  });
}

function writeSettingsBackup(path, bytes) {
  const backupPath = requireSettingsBackupPath(path);
  const temporaryPath = `${backupPath}.tmp`;
  reconcileInterruptedSettingsBackup(backupPath);
  let descriptor;
  let finalCreated = false;
  let complete = false;
  try {
    descriptor = openSync(temporaryPath, "wx", 0o600);
    writeFileSync(descriptor, bytes);
    fsyncSync(descriptor);
    closeSync(descriptor);
    descriptor = undefined;
    linkSync(temporaryPath, backupPath);
    finalCreated = true;
    unlinkSync(temporaryPath);
    const metadata = lstatSync(backupPath);
    if (
      !metadata.isFile() ||
      metadata.isSymbolicLink() ||
      metadata.uid !== 0 ||
      metadata.nlink !== 1 ||
      (metadata.mode & 0o777) !== 0o600
    ) {
      throw new Error("Oracle settings backup authority is invalid");
    }
    fsyncDirectory(SETTINGS_DIRECTORY);
    complete = true;
  } finally {
    if (descriptor !== undefined) closeSync(descriptor);
    if (!complete) {
      tryUnlink(temporaryPath);
      if (finalCreated) tryUnlink(backupPath);
    }
  }
}

function cleanupStaleSettingsBackups(currentBackupPath) {
  const stale = [...new Set(readdirSync(SETTINGS_DIRECTORY, { withFileTypes: true })
    .filter((entry) => BACKUP_NAME_PATTERN.test(entry.name))
    .map((entry) => `${SETTINGS_DIRECTORY}/${
      entry.name.endsWith(".tmp") ? entry.name.slice(0, -4) : entry.name
    }`)
    .filter((path) => path !== currentBackupPath)
  )].sort();
  if (stale.length > MAX_STALE_BACKUPS) {
    throw new Error("Oracle stale settings backup inventory exceeds its bound");
  }
  for (const path of stale) {
    reconcileInterruptedSettingsBackup(path);
    const metadata = optionalLstat(path, lstatSync);
    if (metadata === null) continue;
    requireRootBackupLeaf(metadata, 1, "Oracle stale settings backup");
    unlinkSync(path);
  }
  if (stale.length > 0) fsyncDirectory(SETTINGS_DIRECTORY);
}

export function cleanupOracleRollbackBackup(deploymentNonce) {
  const nonce = requiredMatch(
    deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  const backupPath = `${SETTINGS_DIRECTORY}/settings.pre-v0.8.0-${nonce}`;
  reconcileInterruptedSettingsBackup(backupPath);
  const metadata = optionalLstat(backupPath, lstatSync);
  if (metadata === null) {
    return Object.freeze({ deploymentNonce: nonce, backupRemoved: false });
  }
  requireRootBackupLeaf(metadata, 1, "Oracle settings backup cleanup");
  unlinkSync(backupPath);
  fsyncDirectory(SETTINGS_DIRECTORY);
  return Object.freeze({ deploymentNonce: nonce, backupRemoved: true });
}

export function reconcileOracleRollbackBackupInterruption(
  deploymentNonce,
  dependencies = {},
) {
  const nonce = requiredMatch(
    deploymentNonce,
    SHA256_PATTERN,
    "deployment nonce",
  );
  return reconcileInterruptedSettingsBackup(
    `${SETTINGS_DIRECTORY}/settings.pre-v0.8.0-${nonce}`,
    dependencies,
  );
}

function reconcileInterruptedSettingsBackup(path, dependencies = {}) {
  const backupPath = requireSettingsBackupPath(path);
  const temporaryPath = `${backupPath}.tmp`;
  const inspect = dependencies.lstat ?? lstatSync;
  const remove = dependencies.unlink ?? unlinkSync;
  const syncDirectory = dependencies.fsyncDirectory ?? fsyncDirectory;
  const temporary = optionalLstat(temporaryPath, inspect);
  if (temporary === null) return false;

  const final = optionalLstat(backupPath, inspect);
  if (final === null) {
    requireRootBackupLeaf(temporary, 1, "Interrupted Oracle settings backup temporary");
    remove(temporaryPath);
    syncDirectory(SETTINGS_DIRECTORY);
    return true;
  }

  requireRootBackupLeaf(final, 2, "Interrupted Oracle settings backup final");
  requireRootBackupLeaf(temporary, 2, "Interrupted Oracle settings backup temporary");
  if (final.dev !== temporary.dev || final.ino !== temporary.ino) {
    throw new Error("Interrupted Oracle settings backup hardlink authority is invalid");
  }
  remove(temporaryPath);
  syncDirectory(SETTINGS_DIRECTORY);
  const repaired = optionalLstat(backupPath, inspect);
  if (repaired === null || repaired.dev !== final.dev || repaired.ino !== final.ino) {
    throw new Error("Interrupted Oracle settings backup repair lost its final authority");
  }
  requireRootBackupLeaf(repaired, 1, "Repaired Oracle settings backup final");
  return true;
}

function requireSettingsBackupPath(path) {
  const backupPath = String(path);
  if (!FINAL_BACKUP_PATH_PATTERN.test(backupPath)) {
    throw new Error("Oracle settings backup path is outside the approved namespace");
  }
  return backupPath;
}

function requireRootBackupLeaf(metadata, expectedLinks, label) {
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.uid !== 0 ||
    metadata.nlink !== expectedLinks ||
    (metadata.mode & 0o777) !== 0o600
  ) {
    throw new Error(`${label} authority is invalid`);
  }
}

function optionalLstat(path, inspect) {
  try {
    return inspect(path);
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

function fsyncDirectory(path) {
  const descriptor = openSync(path, "r");
  try {
    fsyncSync(descriptor);
  } finally {
    closeSync(descriptor);
  }
}

function tryUnlink(path) {
  try {
    unlinkSync(path);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
}

function exactSetting(serialized, key) {
  const values = serialized
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.startsWith(`${key}=`))
    .map((line) => line.slice(key.length + 1));
  if (values.length !== 1) {
    throw new Error(`active Oracle setting ${key} must occur exactly once`);
  }
  let url;
  try {
    url = new URL(values[0]);
  } catch {
    throw new Error("active Oracle job URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/jobs"
  ) {
    throw new Error(
      "active Oracle job URL must be a credential-free HTTPS /jobs URL",
    );
  }
  return `${url.origin}/jobs`;
}

function canonicalHealthUrl(jobUrl) {
  return `${new URL(jobUrl).origin}/health`;
}

function runCommand(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    throw new Error(`Oracle rollback capture failed: ${basename(command)}`);
  }
  return result.stdout;
}

function requiredMatch(value, pattern, label) {
  const text = typeof value === "string" ? value.trim() : "";
  if (!pattern.test(text)) throw new Error(`${label} is invalid`);
  return text;
}

function normalizePath(value) {
  return String(value).replaceAll("\\", "/");
}

async function main() {
  const { values } = parseArgs({
    options: {
      "prior-revision": { type: "string" },
      "prior-runtime-authority-kind": { type: "string" },
      "deployment-nonce": { type: "string" },
      "cleanup-deployment-nonce": { type: "string" },
    },
    strict: true,
  });
  try {
    if (typeof process.getuid !== "function" || process.getuid() !== 0) {
      throw new Error("Oracle rollback authority capture must run as root");
    }
    if (values["cleanup-deployment-nonce"]) {
      if (values["prior-revision"] || values["prior-runtime-authority-kind"] || values["deployment-nonce"]) {
        throw new Error("Oracle backup cleanup rejects capture arguments");
      }
      console.log(JSON.stringify(cleanupOracleRollbackBackup(values["cleanup-deployment-nonce"])));
      return;
    }
    const captured = captureOracleRollbackAuthority({
      priorRevision: values["prior-revision"],
      priorRuntimeAuthorityKind: values["prior-runtime-authority-kind"],
      deploymentNonce: values["deployment-nonce"],
    });
    console.log(JSON.stringify(captured));
  } catch {
    console.error("oracle_rollback_capture=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
