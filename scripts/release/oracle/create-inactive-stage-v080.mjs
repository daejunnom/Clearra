#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  closeSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { fileURLToPath } from "node:url";

const SCHEMA_VERSION = "clearra.oracle.inactive-stage.v080.v1";
const TEMPLATE_PATH = fileURLToPath(
  new URL("./clearra-oracle-inactive-stage-v080.template", import.meta.url),
);

const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
const RELEASE_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const ACTIVE_RELEASE_PATTERN = /^\/opt\/clearra\/releases\/[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const MAX_FILE_SIZE = 8 * 1024 * 1024 * 1024;
const MAX_ENTRY_COUNT = 1_000_000;

function fail(message) {
  throw new Error(message);
}

function exactKeys(value, keys, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  const actual = Object.keys(value);
  if (
    actual.length !== keys.length ||
    actual.some((key, index) => key !== keys[index])
  ) {
    fail(`${label} keys or key order do not match the closed schema`);
  }
}

function exactString(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    fail(`${label} is invalid`);
  }
  return value;
}

function exactInteger(value, label, { allowZero = false, maximum } = {}) {
  const minimum = allowZero ? 0 : 1;
  if (
    !Number.isSafeInteger(value) ||
    value < minimum ||
    value > (maximum ?? Number.MAX_SAFE_INTEGER)
  ) {
    fail(`${label} is invalid`);
  }
  return value;
}

function validateCounts(value, label) {
  exactKeys(value, ["files", "directories", "symlinks"], label);
  return {
    files: exactInteger(value.files, `${label}.files`, {
      allowZero: true,
      maximum: MAX_ENTRY_COUNT,
    }),
    directories: exactInteger(value.directories, `${label}.directories`, {
      allowZero: true,
      maximum: MAX_ENTRY_COUNT,
    }),
    symlinks: exactInteger(value.symlinks, `${label}.symlinks`, {
      allowZero: true,
      maximum: MAX_ENTRY_COUNT,
    }),
  };
}

function validateLayer(value, label) {
  exactKeys(value, ["sha256", "size", "counts"], label);
  return {
    sha256: exactString(value.sha256, SHA256_PATTERN, `${label}.sha256`),
    size: exactInteger(value.size, `${label}.size`, { maximum: MAX_FILE_SIZE }),
    counts: validateCounts(value.counts, `${label}.counts`),
  };
}

function validatePriorTool(value, label) {
  if (value === null) {
    return null;
  }
  exactKeys(value, ["sha256", "size"], label);
  return {
    sha256: exactString(value.sha256, SHA256_PATTERN, `${label}.sha256`),
    size: exactInteger(value.size, `${label}.size`, { maximum: MAX_FILE_SIZE }),
  };
}

function validateTool(value, label) {
  exactKeys(value, ["sha256", "size", "prior"], label);
  const tool = {
    sha256: exactString(value.sha256, SHA256_PATTERN, `${label}.sha256`),
    size: exactInteger(value.size, `${label}.size`, { maximum: MAX_FILE_SIZE }),
    prior: validatePriorTool(value.prior, `${label}.prior`),
  };
  if (tool.prior?.sha256 === tool.sha256) {
    fail(`${label}.prior must identify a distinct transition authority`);
  }
  return tool;
}

export function validateManifest(value) {
  exactKeys(
    value,
    [
      "schemaVersion",
      "sourceCommit",
      "releaseId",
      "active",
      "candidate",
      "layers",
      "tools",
    ],
    "manifest",
  );
  if (value.schemaVersion !== SCHEMA_VERSION) {
    fail("manifest.schemaVersion is invalid");
  }
  const sourceCommit = exactString(
    value.sourceCommit,
    COMMIT_PATTERN,
    "manifest.sourceCommit",
  );
  const commitPrefix = sourceCommit.slice(0, 7);
  if (value.releaseId !== `v0.8.0-${commitPrefix}`) {
    fail("manifest.releaseId must be the v0.8.0 exact-source identity");
  }
  exactString(value.releaseId, RELEASE_ID_PATTERN, "manifest.releaseId");

  exactKeys(
    value.active,
    [
      "releasePath",
      "treeSha256",
      "settingsSha256",
      "settingsSize",
      "configSha256",
      "configSize",
    ],
    "manifest.active",
  );
  const active = {
    releasePath: exactString(
      value.active.releasePath,
      ACTIVE_RELEASE_PATTERN,
      "manifest.active.releasePath",
    ),
    treeSha256: exactString(
      value.active.treeSha256,
      SHA256_PATTERN,
      "manifest.active.treeSha256",
    ),
    settingsSha256: exactString(
      value.active.settingsSha256,
      SHA256_PATTERN,
      "manifest.active.settingsSha256",
    ),
    settingsSize: exactInteger(
      value.active.settingsSize,
      "manifest.active.settingsSize",
      { maximum: MAX_FILE_SIZE },
    ),
    configSha256: exactString(
      value.active.configSha256,
      SHA256_PATTERN,
      "manifest.active.configSha256",
    ),
    configSize: exactInteger(value.active.configSize, "manifest.active.configSize", {
      maximum: MAX_FILE_SIZE,
    }),
  };
  if (active.releasePath === `/opt/clearra/releases/${value.releaseId}`) {
    fail("manifest.active.releasePath must remain distinct from the inactive candidate");
  }

  exactKeys(value.candidate, ["treeSha256", "counts"], "manifest.candidate");
  exactKeys(
    value.candidate.counts,
    ["directories", "files0644", "files0755", "symlinks"],
    "manifest.candidate.counts",
  );
  const candidate = {
    treeSha256: exactString(
      value.candidate.treeSha256,
      SHA256_PATTERN,
      "manifest.candidate.treeSha256",
    ),
    counts: {
      directories: exactInteger(
        value.candidate.counts.directories,
        "manifest.candidate.counts.directories",
        { maximum: MAX_ENTRY_COUNT },
      ),
      files0644: exactInteger(
        value.candidate.counts.files0644,
        "manifest.candidate.counts.files0644",
        { maximum: MAX_ENTRY_COUNT },
      ),
      files0755: exactInteger(
        value.candidate.counts.files0755,
        "manifest.candidate.counts.files0755",
        { maximum: MAX_ENTRY_COUNT },
      ),
      symlinks: exactInteger(
        value.candidate.counts.symlinks,
        "manifest.candidate.counts.symlinks",
        { maximum: MAX_ENTRY_COUNT },
      ),
    },
  };

  exactKeys(
    value.layers,
    ["source", "overlay", "ctk3Dist", "dependencies"],
    "manifest.layers",
  );
  const layers = {
    source: validateLayer(value.layers.source, "manifest.layers.source"),
    overlay: validateLayer(value.layers.overlay, "manifest.layers.overlay"),
    ctk3Dist: validateLayer(value.layers.ctk3Dist, "manifest.layers.ctk3Dist"),
    dependencies: validateLayer(
      value.layers.dependencies,
      "manifest.layers.dependencies",
    ),
  };
  if (
    layers.source.counts.symlinks !== 0 ||
    layers.overlay.counts.symlinks !== 0 ||
    layers.ctk3Dist.counts.symlinks !== 0 ||
    layers.dependencies.counts.symlinks !== 2 ||
    candidate.counts.symlinks !== 2
  ) {
    fail("manifest symlink counts do not match the closed two-link dependency contract");
  }
  if (
    Object.values(layers).some(
      (layer) =>
        layer.counts.files + layer.counts.directories + layer.counts.symlinks === 0,
    )
  ) {
    fail("every frozen layer must contain at least one entry");
  }

  exactKeys(value.tools, ["launcher", "digester"], "manifest.tools");
  const tools = {
    launcher: validateTool(value.tools.launcher, "manifest.tools.launcher"),
    digester: validateTool(value.tools.digester, "manifest.tools.digester"),
  };

  return {
    schemaVersion: SCHEMA_VERSION,
    sourceCommit,
    releaseId: value.releaseId,
    commitPrefix,
    active,
    candidate,
    layers,
    tools,
  };
}

export function parseCanonicalManifest(bytes) {
  const text = Buffer.isBuffer(bytes) ? bytes.toString("utf8") : String(bytes);
  if (text.includes("\r") || text.includes("\0") || !text.endsWith("\n")) {
    fail("manifest must be canonical UTF-8 JSON with LF and one final newline");
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    fail("manifest is not valid JSON");
  }
  const canonical = `${JSON.stringify(value, null, 2)}\n`;
  if (text !== canonical) {
    fail("manifest bytes are not canonical JSON");
  }
  return validateManifest(value);
}

function replacementMap(manifest) {
  const prior = (tool) => ({
    sha256: tool.prior?.sha256 ?? "absent",
    size: tool.prior?.size ?? 0,
  });
  const priorLauncher = prior(manifest.tools.launcher);
  const priorDigester = prior(manifest.tools.digester);
  return new Map([
    ["@SOURCE_COMMIT@", manifest.sourceCommit],
    ["@COMMIT_PREFIX@", manifest.commitPrefix],
    ["@RELEASE_ID@", manifest.releaseId],
    ["@EXPECTED_ACTIVE_RELEASE@", manifest.active.releasePath],
    ["@EXPECTED_ACTIVE_TREE_SHA256@", manifest.active.treeSha256],
    ["@EXPECTED_SETTINGS_SHA256@", manifest.active.settingsSha256],
    ["@EXPECTED_SETTINGS_SIZE@", String(manifest.active.settingsSize)],
    ["@EXPECTED_CONFIG_SHA256@", manifest.active.configSha256],
    ["@EXPECTED_CONFIG_SIZE@", String(manifest.active.configSize)],
    ["@EXPECTED_SOURCE_SHA256@", manifest.layers.source.sha256],
    ["@EXPECTED_OVERLAY_SHA256@", manifest.layers.overlay.sha256],
    ["@EXPECTED_DIST_SHA256@", manifest.layers.ctk3Dist.sha256],
    ["@EXPECTED_DEPENDENCIES_SHA256@", manifest.layers.dependencies.sha256],
    ["@EXPECTED_TREE_SHA256@", manifest.candidate.treeSha256],
    ["@EXPECTED_LAUNCHER_SHA256@", manifest.tools.launcher.sha256],
    ["@EXPECTED_LAUNCHER_SIZE@", String(manifest.tools.launcher.size)],
    ["@EXPECTED_PRIOR_LAUNCHER_SHA256@", priorLauncher.sha256],
    ["@EXPECTED_PRIOR_LAUNCHER_SIZE@", String(priorLauncher.size)],
    ["@EXPECTED_DIGESTER_SHA256@", manifest.tools.digester.sha256],
    ["@EXPECTED_DIGESTER_SIZE@", String(manifest.tools.digester.size)],
    ["@EXPECTED_PRIOR_DIGESTER_SHA256@", priorDigester.sha256],
    ["@EXPECTED_PRIOR_DIGESTER_SIZE@", String(priorDigester.size)],
    ["@SOURCE_FILES@", String(manifest.layers.source.counts.files)],
    ["@SOURCE_DIRECTORIES@", String(manifest.layers.source.counts.directories)],
    ["@SOURCE_SYMLINKS@", String(manifest.layers.source.counts.symlinks)],
    ["@OVERLAY_FILES@", String(manifest.layers.overlay.counts.files)],
    ["@OVERLAY_DIRECTORIES@", String(manifest.layers.overlay.counts.directories)],
    ["@OVERLAY_SYMLINKS@", String(manifest.layers.overlay.counts.symlinks)],
    ["@DIST_FILES@", String(manifest.layers.ctk3Dist.counts.files)],
    ["@DIST_DIRECTORIES@", String(manifest.layers.ctk3Dist.counts.directories)],
    ["@DIST_SYMLINKS@", String(manifest.layers.ctk3Dist.counts.symlinks)],
    ["@DEPENDENCIES_FILES@", String(manifest.layers.dependencies.counts.files)],
    [
      "@DEPENDENCIES_DIRECTORIES@",
      String(manifest.layers.dependencies.counts.directories),
    ],
    ["@DEPENDENCIES_SYMLINKS@", String(manifest.layers.dependencies.counts.symlinks)],
    ["@CANDIDATE_DIRECTORIES@", String(manifest.candidate.counts.directories)],
    ["@CANDIDATE_FILES_0644@", String(manifest.candidate.counts.files0644)],
    ["@CANDIDATE_FILES_0755@", String(manifest.candidate.counts.files0755)],
    ["@CANDIDATE_SYMLINKS@", String(manifest.candidate.counts.symlinks)],
  ]);
}

export function generateBootstrap(manifest, templateText = readFileSync(TEMPLATE_PATH, "utf8")) {
  if (
    templateText.includes("\r") ||
    templateText.includes("\0") ||
    !templateText.endsWith("\n")
  ) {
    fail("bootstrap template must contain LF text with one final newline");
  }
  let generated = templateText;
  for (const [token, replacement] of replacementMap(manifest)) {
    const matches = generated.split(token).length - 1;
    const expectedMatches =
      token === "@COMMIT_PREFIX@" ? 7 : token === "@EXPECTED_DIGESTER_SIZE@" ? 3 : 1;
    if (matches !== expectedMatches) {
      fail(`bootstrap template token cardinality drifted for ${token}`);
    }
    generated = generated.split(token).join(replacement);
  }
  const residue = generated.match(/@[A-Z0-9_]+@/gu);
  if (residue) {
    fail(`bootstrap template contains unresolved token ${residue[0]}`);
  }
  return Buffer.from(generated, "utf8");
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parseArguments(arguments_) {
  const result = { manifestPath: "", mode: "", path: "" };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--manifest") {
      if (result.manifestPath || index + 1 >= arguments_.length) fail("invalid arguments");
      result.manifestPath = arguments_[index + 1];
      index += 1;
    } else if (argument === "--audit") {
      if (result.mode) fail("invalid arguments");
      result.mode = "audit";
    } else if (argument === "--output" || argument === "--check") {
      if (result.mode || index + 1 >= arguments_.length) fail("invalid arguments");
      result.mode = argument.slice(2);
      result.path = arguments_[index + 1];
      index += 1;
    } else {
      fail("invalid arguments");
    }
  }
  if (!result.manifestPath || !result.mode || (result.mode !== "audit" && !result.path)) {
    fail("--manifest and exactly one of --audit, --output, or --check are required");
  }
  return result;
}

function attest(manifest, generated, mode) {
  process.stdout.write(
    [
      "oracle_stage_manifest=ok",
      `oracle_stage_mode=${mode}`,
      `oracle_source_commit=${manifest.sourceCommit}`,
      `oracle_release_id=${manifest.releaseId}`,
      `oracle_release_sha256=${manifest.candidate.treeSha256}`,
      `oracle_bootstrap_sha256=${sha256(generated)}`,
      `oracle_bootstrap_size=${generated.length}`,
    ].join("\n") + "\n",
  );
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const manifest = parseCanonicalManifest(readFileSync(options.manifestPath));
  const generated = generateBootstrap(manifest);
  if (options.mode === "output") {
    let descriptor;
    try {
      descriptor = openSync(options.path, "wx", 0o600);
      writeFileSync(descriptor, generated);
    } catch (error) {
      if (descriptor !== undefined) {
        try {
          closeSync(descriptor);
        } catch {}
        try {
          unlinkSync(options.path);
        } catch {}
      }
      throw error;
    }
    closeSync(descriptor);
  } else if (options.mode === "check") {
    const actual = readFileSync(options.path);
    if (!actual.equals(generated)) {
      fail("generated bootstrap bytes do not match");
    }
  }
  attest(manifest, generated, options.mode);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`oracle_stage_manifest=failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
