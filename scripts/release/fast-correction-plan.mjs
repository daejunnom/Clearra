#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const MODE = /^(?:000000|100644|100755|120000|160000)$/u;
const STATUS = /^[ACDMRTUXB][0-9]{0,3}$/u;
const PLAN_SCHEMA = "clearra.fast-correction-plan.v1";
const MANIFEST_SCHEMA = "clearra.fast-correction-owners.v1";
const MANIFEST_PATH = "scripts/release/fast-correction-owners.v1.json";
const OWNER_IDS = new Set(["documentation", "release-workflow", "pages"]);
const RELEASE_TAG = /^v([0-9]+)\.([0-9]+)\.([0-9]+)$/u;

export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort((left, right) => left.localeCompare(right, "en"));
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

export function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export function loadOwnerManifest(path = resolve(dirname(fileURLToPath(import.meta.url)), "fast-correction-owners.v1.json")) {
  const bytes = readFileSync(path);
  let manifest;
  try {
    manifest = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("fast correction owner manifest is not valid JSON");
  }
  validateManifest(manifest);
  return Object.freeze({
    manifest: deepFreeze(manifest),
    sha256: sha256(bytes),
  });
}

export function classifyFastCorrectionEntries({ baseCommit, baseTag, candidateCommit, entries, manifest, manifestSha256 }) {
  requireCommit(baseCommit, "accepted base");
  if (!RELEASE_TAG.test(baseTag ?? "")) throw new Error("accepted base tag is invalid");
  requireCommit(candidateCommit, "candidate");
  if (!Array.isArray(entries)) throw new Error("fast correction diff entries are required");
  requireSha256(manifestSha256, "owner manifest");
  validateManifest(manifest);

  const normalized = entries.map(normalizeEntry).sort(compareEntries);
  const seen = new Set();
  const reasons = new Set();
  const owners = new Set();
  const changes = [];
  for (const entry of normalized) {
    if (seen.has(entry.path)) reasons.add("duplicate-path");
    seen.add(entry.path);
    let owner = null;
    if (entry.oldPath !== null || entry.status.startsWith("R") || entry.status.startsWith("C")) {
      reasons.add("rename-or-copy");
    } else if (entry.status.startsWith("T") || entry.status.startsWith("U") || entry.status.startsWith("X") || entry.status.startsWith("B")) {
      reasons.add("unsupported-diff-status");
    } else if (entry.oldMode === "160000" || entry.newMode === "160000") {
      reasons.add("submodule-change");
    } else if (entry.oldMode === "120000" || entry.newMode === "120000") {
      reasons.add("symlink-change");
    } else if (!regularTransition(entry.oldMode, entry.newMode)) {
      reasons.add("unsupported-file-mode");
    } else if (matchesRules(entry.path, {
      exact: manifest.authority_bundle,
      prefix: [],
      suffix: [],
    })) {
      reasons.add("fast-authority-change");
    } else if (matchesRules(entry.path, manifest.full_required)) {
      reasons.add("full-required-owner");
    } else {
      const matches = manifest.owners.filter((candidate) => matchesRules(entry.path, candidate));
      if (matches.length !== 1) {
        reasons.add(matches.length === 0 ? "unknown-owner" : "ambiguous-owner");
      } else {
        owner = matches[0].id;
        owners.add(owner);
      }
    }
    changes.push(Object.freeze({ ...entry, owner }));
  }

  const changedOwners = [...owners].sort((left, right) => left.localeCompare(right, "en"));
  const controlDeployments = [...new Set(changes.flatMap((entry) =>
    entry.owner === "release-workflow" && Object.hasOwn(manifest.workflow_deployments, entry.path)
      ? [manifest.workflow_deployments[entry.path]]
      : []
  ))].sort((left, right) => left.localeCompare(right, "en"));
  if (controlDeployments.length > 0 && changedOwners.some((owner) => owner === "pages")) {
    reasons.add("control-runtime-mix");
  }
  const reasonCodes = [...reasons].sort((left, right) => left.localeCompare(right, "en"));
  const decision = normalized.length === 0
    ? "no-op"
    : reasonCodes.length > 0
      ? "full-required"
      : "fast-eligible";
  const deployPages = decision === "fast-eligible" && owners.has("pages");
  const lane = decision === "no-op"
    ? "no-op"
    : decision === "full-required"
      ? "full-required"
      : changedOwners.filter((owner) => owner !== "documentation").join("+") || "documentation";
  const selected = decision === "fast-eligible"
    ? selectCommands(manifest, changedOwners, controlDeployments)
    : { tests: [], builds: [], deployments: [] };
  const diffPayload = changes.map(({ owner: _owner, ...entry }) => entry);
  const body = {
    schema_id: PLAN_SCHEMA,
    accepted_base_commit: baseCommit,
    accepted_base_tag: baseTag,
    candidate_commit: candidateCommit,
    owner_manifest_path: MANIFEST_PATH,
    owner_manifest_sha256: manifestSha256,
    decision,
    lane,
    changed_owners: changedOwners,
    reason_codes: reasonCodes,
    affected_products: deployPages ? ["pages"] : [],
    control_deployments: decision === "fast-eligible" ? controlDeployments : [],
    skipped_products: ["cli", "discord", "gui"].concat(deployPages ? [] : ["pages"]).sort(),
    deploy_pages: deployPages,
    changes,
    diff_sha256: sha256(`${canonicalJson(diffPayload)}\n`),
    selected,
  };
  return deepFreeze({ ...body, plan_sha256: sha256(`${canonicalJson(body)}\n`) });
}

export function validateFastCorrectionPlan(plan) {
  if (plan === null || typeof plan !== "object" || Array.isArray(plan)) {
    throw new Error("fast correction plan must be an object");
  }
  const { plan_sha256: claimed, ...body } = plan;
  requireSha256(claimed, "fast correction plan");
  const actual = sha256(`${canonicalJson(body)}\n`);
  if (actual !== claimed) throw new Error("fast correction plan hash mismatch");
  if (plan.schema_id !== PLAN_SCHEMA) throw new Error("fast correction plan schema mismatch");
  requireCommit(plan.accepted_base_commit, "accepted base");
  if (!RELEASE_TAG.test(plan.accepted_base_tag ?? "")) throw new Error("accepted base tag is invalid");
  requireCommit(plan.candidate_commit, "candidate");
  requireSha256(plan.diff_sha256, "fast correction diff");
  requireSha256(plan.owner_manifest_sha256, "owner manifest");
  if (!["no-op", "full-required", "fast-eligible"].includes(plan.decision)) {
    throw new Error("fast correction decision is invalid");
  }
  return plan;
}

export function analyzeFastCorrection({ repository, baseCommit, candidateCommit, manifestPath }, query = gitQuery) {
  requireCommit(baseCommit, "accepted base");
  requireCommit(candidateCommit, "candidate");
  const root = resolve(repository);
  if (query(root, ["rev-parse", "--verify", `${baseCommit}^{commit}`]) !== baseCommit) {
    throw new Error("accepted base does not resolve exactly");
  }
  if (query(root, ["rev-parse", "--verify", `${candidateCommit}^{commit}`]) !== candidateCommit) {
    throw new Error("candidate does not resolve exactly");
  }
  query(root, ["merge-base", "--is-ancestor", baseCommit, candidateCommit], { output: false });
  const baseTag = selectLatestProductionTag(query(root, [
    "tag", "--merged", candidateCommit, "--list", "v[0-9]*.[0-9]*.[0-9]*",
  ]).split(/\r?\n/u).filter(Boolean));
  if (query(root, ["cat-file", "-t", baseTag]) !== "tag") {
    throw new Error("latest production tag must be annotated");
  }
  if (query(root, ["rev-list", "-n", "1", baseTag]) !== baseCommit) {
    throw new Error("accepted base is not the latest reachable production tag commit");
  }
  const loaded = loadOwnerManifest(manifestPath);
  const raw = query(root, [
    "diff", "--raw", "-z", "--abbrev=40", "--find-renames=50%", "--find-copies=50%",
    `${baseCommit}..${candidateCommit}`, "--",
  ], { trim: false });
  const entries = parseRawDiff(raw);
  return classifyFastCorrectionEntries({
    baseCommit,
    baseTag,
    candidateCommit,
    entries,
    manifest: loaded.manifest,
    manifestSha256: loaded.sha256,
  });
}

export function selectLatestProductionTag(tags) {
  if (!Array.isArray(tags)) throw new Error("production tags are required");
  const versions = tags.flatMap((tag) => {
    const match = String(tag).match(RELEASE_TAG);
    return match ? [{ tag: String(tag), version: match.slice(1).map(Number) }] : [];
  });
  versions.sort((left, right) => {
    for (let index = 0; index < 3; index += 1) {
      if (left.version[index] !== right.version[index]) return right.version[index] - left.version[index];
    }
    return 0;
  });
  if (versions.length === 0) throw new Error("no reachable production release tag exists");
  return versions[0].tag;
}

export function parseRawDiff(raw) {
  if (typeof raw !== "string") throw new Error("raw git diff must be text");
  if (raw === "") return [];
  const parts = raw.split("\0");
  if (parts.at(-1) !== "") throw new Error("raw git diff is not NUL terminated");
  parts.pop();
  const entries = [];
  for (let index = 0; index < parts.length;) {
    const header = parts[index++];
    const match = header.match(/^:([0-9]{6}) ([0-9]{6}) ([0-9a-f]{40}) ([0-9a-f]{40}) ([A-Z][0-9]{0,3})$/u);
    if (!match) throw new Error("raw git diff header is invalid");
    const path = parts[index++];
    if (path === undefined) throw new Error("raw git diff path is missing");
    const status = match[5];
    const oldPath = status.startsWith("R") || status.startsWith("C") ? path : null;
    const finalPath = oldPath === null ? path : parts[index++];
    if (finalPath === undefined) throw new Error("raw git diff renamed path is missing");
    entries.push({
      status,
      path: finalPath,
      oldPath,
      oldMode: match[1],
      newMode: match[2],
      oldObject: match[3],
      newObject: match[4],
    });
  }
  return entries;
}

function selectCommands(manifest, owners, controlDeployments) {
  const selectionOwners = [
    "integrity",
    ...owners.filter((owner) => owner !== "documentation"),
    ...controlDeployments.map((surface) => `${surface}-control`),
  ];
  const result = { tests: [], builds: [], deployments: [] };
  for (const owner of selectionOwners) {
    const selection = manifest.selection[owner];
    for (const kind of Object.keys(result)) {
      for (const command of selection[kind]) {
        if (!result[kind].some((existing) => existing.id === command.id)) {
          result[kind].push(command);
        }
      }
    }
  }
  return result;
}

function normalizeEntry(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("fast correction diff entry is invalid");
  }
  const status = String(value.status ?? "");
  if (!STATUS.test(status)) throw new Error("fast correction diff status is invalid");
  const oldMode = String(value.oldMode ?? "");
  const newMode = String(value.newMode ?? "");
  if (!MODE.test(oldMode) || !MODE.test(newMode)) throw new Error("fast correction file mode is invalid");
  const oldObject = requireObjectId(value.oldObject, "old object");
  const newObject = requireObjectId(value.newObject, "new object");
  return Object.freeze({
    status,
    path: normalizePath(value.path),
    oldPath: value.oldPath === null || value.oldPath === undefined ? null : normalizePath(value.oldPath),
    oldMode,
    newMode,
    oldObject,
    newObject,
  });
}

function validateManifest(manifest) {
  if (manifest?.schema_id !== MANIFEST_SCHEMA) throw new Error("fast correction owner manifest schema mismatch");
  validateStringSet(manifest.authority_bundle, "authority bundle");
  validateRules(manifest.full_required, "full-required rules");
  if (!Array.isArray(manifest.owners) || manifest.owners.length !== OWNER_IDS.size) {
    throw new Error("fast correction owner set is not closed");
  }
  const ids = new Set();
  for (const owner of manifest.owners) {
    if (!OWNER_IDS.has(owner?.id) || ids.has(owner.id)) throw new Error("fast correction owner ID is invalid");
    ids.add(owner.id);
    validateRules(owner, `owner ${owner.id}`);
  }
  if (
    manifest.workflow_deployments === null || typeof manifest.workflow_deployments !== "object" ||
    Array.isArray(manifest.workflow_deployments) ||
    Object.values(manifest.workflow_deployments).some((value) => !["discord", "pages"].includes(value))
  ) throw new Error("fast correction workflow deployment map is invalid");
  for (const path of Object.keys(manifest.workflow_deployments)) normalizePath(path);
  const selectionIds = new Set(["integrity", "release-workflow", "pages", "discord-control", "pages-control"]);
  if (manifest.selection === null || typeof manifest.selection !== "object" || Array.isArray(manifest.selection)) {
    throw new Error("fast correction selection is invalid");
  }
  if (JSON.stringify(Object.keys(manifest.selection).sort()) !== JSON.stringify([...selectionIds].sort())) {
    throw new Error("fast correction selection owners are not closed");
  }
  const commandIds = new Set();
  for (const [owner, selection] of Object.entries(manifest.selection)) {
    if (!selectionIds.has(owner)) throw new Error("fast correction selection owner is invalid");
    if (JSON.stringify(Object.keys(selection).sort()) !== JSON.stringify(["builds", "deployments", "tests"])) {
      throw new Error("fast correction selection fields are not closed");
    }
    for (const kind of ["tests", "builds", "deployments"]) {
      if (!Array.isArray(selection[kind])) throw new Error("fast correction command set is invalid");
      for (const command of selection[kind]) {
        if (
          command === null || typeof command !== "object" || Array.isArray(command) ||
          JSON.stringify(Object.keys(command).sort()) !== JSON.stringify(["command", "id"]) ||
          !/^[a-z0-9-]+$/u.test(command.id ?? "") ||
          typeof command.command !== "string" || command.command.length === 0 ||
          commandIds.has(command.id)
        ) throw new Error("fast correction command descriptor is invalid");
        commandIds.add(command.id);
      }
    }
  }
  for (const path of manifest.authority_bundle) normalizePath(path);
  return manifest;
}

function validateRules(rules, label) {
  if (rules === null || typeof rules !== "object" || Array.isArray(rules)) throw new Error(`${label} are invalid`);
  for (const key of ["exact", "prefix", "suffix"]) validateStringSet(rules[key], `${label} ${key}`);
}

function validateStringSet(values, label) {
  if (!Array.isArray(values) || values.some((value) => typeof value !== "string" || value.length === 0) || new Set(values).size !== values.length) {
    throw new Error(`${label} must be a unique string array`);
  }
}

function matchesRules(path, rules) {
  return rules.exact.includes(path) || rules.prefix.some((prefix) => path.startsWith(prefix)) || rules.suffix.some((suffix) => path.endsWith(suffix));
}

function normalizePath(value) {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0") || value.includes("\n") || value.includes("\r")) {
    throw new Error("changed path is invalid");
  }
  const path = value.replaceAll("\\", "/");
  if (path.startsWith("/") || path.startsWith("./") || path.split("/").some((part) => part === "" || part === "." || part === "..")) {
    throw new Error("changed path is outside the repository");
  }
  return path;
}

function regularTransition(oldMode, newMode) {
  const regular = new Set(["100644", "100755"]);
  return (oldMode === "000000" && regular.has(newMode)) ||
    (newMode === "000000" && regular.has(oldMode)) ||
    (regular.has(oldMode) && regular.has(newMode) && oldMode === newMode);
}

function compareEntries(left, right) {
  return left.path.localeCompare(right.path, "en") || (left.oldPath ?? "").localeCompare(right.oldPath ?? "", "en");
}

function requireCommit(value, label) {
  if (!SOURCE_COMMIT.test(value ?? "")) throw new Error(`${label} commit is invalid`);
  return value;
}

function requireSha256(value, label) {
  if (!/^[0-9a-f]{64}$/u.test(value ?? "")) throw new Error(`${label} SHA-256 is invalid`);
  return value;
}

function requireObjectId(value, label) {
  if (!/^[0-9a-f]{40}$/u.test(value ?? "")) throw new Error(`${label} ID is invalid`);
  return value;
}

function deepFreeze(value) {
  if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) deepFreeze(child);
  }
  return value;
}

function gitQuery(repository, arguments_, { output = true, trim = true } = {}) {
  const result = spawnSync("git", ["-C", repository, ...arguments_], {
    encoding: "utf8",
    shell: false,
    stdio: ["ignore", output ? "pipe" : "ignore", "pipe"],
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`fast correction git query failed: ${arguments_[0]}`);
  }
  return output ? (trim ? result.stdout.trim() : result.stdout) : "";
}

function parseArguments(args) {
  const values = {};
  const allowed = new Set(["--repository", "--base", "--candidate", "--manifest", "--output", "--format"]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0 || Object.hasOwn(values, option)) {
      throw new Error("fast correction plan arguments are invalid");
    }
    values[option] = value;
  }
  for (const required of ["--repository", "--base", "--candidate"]) {
    if (!Object.hasOwn(values, required)) throw new Error(`missing option: ${required}`);
  }
  if (!Object.hasOwn(values, "--manifest")) values["--manifest"] = resolve(dirname(fileURLToPath(import.meta.url)), "fast-correction-owners.v1.json");
  if (!Object.hasOwn(values, "--format")) values["--format"] = "json";
  if (!["json", "github-output"].includes(values["--format"])) throw new Error("fast correction output format is invalid");
  return values;
}

function main() {
  const values = parseArguments(process.argv.slice(2));
  const plan = analyzeFastCorrection({
    repository: values["--repository"],
    baseCommit: values["--base"],
    candidateCommit: values["--candidate"],
    manifestPath: values["--manifest"],
  });
  const bytes = `${canonicalJson(plan)}\n`;
  if (values["--output"]) {
    writeFileSync(values["--output"], bytes, { encoding: "utf8", flag: "wx" });
  }
  if (values["--format"] === "github-output") {
    for (const [key, value] of [
      ["decision", plan.decision],
      ["lane", plan.lane],
      ["deploy_pages", plan.deploy_pages],
      ["plan_sha256", plan.plan_sha256],
      ["diff_sha256", plan.diff_sha256],
      ["owner_manifest_sha256", plan.owner_manifest_sha256],
    ]) process.stdout.write(`${key}=${value}\n`);
  } else {
    process.stdout.write(bytes);
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`fast_correction_plan=failed reason=${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
