import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const AUDIT_SCHEMA_ID = "clearra.upstream-drift-audit.v1";
const ALLOWED_PHASES = new Set(["implementation-start", "release-freeze"]);
const USER_AGENT = "Clearra-v0.8.0-upstream-drift-audit";
const TREE_MANIFEST_FORMAT =
  "clearra.git-tree-prefix-manifest.lexical-path.v1";
const SHA1 = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;

export function parseActiveBotCommands(source) {
  if (typeof source !== "string") {
    throw new TypeError("Python source must be a string");
  }

  const lines = source.replaceAll("\r\n", "\n").split("\n");
  const commands = [];
  for (let index = 0; index < lines.length; index += 1) {
    if (!/^\s*@bot\.command(?:\s*\(|\s*$)/u.test(lines[index])) continue;

    const decorator = readDecorator(lines, index);
    let definitionIndex = decorator.endIndex + 1;
    while (definitionIndex < lines.length) {
      const line = lines[definitionIndex];
      if (/^\s*@bot\.command(?:\s*\(|\s*$)/u.test(line)) {
        throw new Error(
          `command decorator on line ${index + 1} has no function definition`,
        );
      }
      if (/^\s*@/u.test(line)) {
        const stackedDecorator = readDecorator(lines, definitionIndex);
        definitionIndex = stackedDecorator.endIndex + 1;
        continue;
      }
      const definition = /^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)\s*\(/u.exec(
        line,
      );
      if (definition) {
        const explicitName =
          /\bname\s*=\s*(["'])([^"']+)\1/u.exec(decorator.text)?.[2];
        commands.push(explicitName ?? definition[1]);
        index = definitionIndex;
        break;
      }
      if (
        line.trim() !== "" &&
        !/^\s*#/u.test(line)
      ) {
        throw new Error(
          `unexpected source between command decorator on line ${index + 1} and its function`,
        );
      }
      definitionIndex += 1;
    }
    if (definitionIndex >= lines.length) {
      throw new Error(
        `command decorator on line ${index + 1} has no function definition`,
      );
    }
  }

  const sorted = [...commands].sort();
  if (new Set(sorted).size !== sorted.length) {
    throw new Error("active bot command names must be unique");
  }
  return Object.freeze(sorted);
}

export async function auditUpstreamDrift({
  registry,
  phase,
  observedAt = new Date().toISOString(),
  resolveHead = resolveRemoteHead,
  fetchFile = fetchRepositoryFile,
  fetchTree = fetchRepositoryTree,
}) {
  validateRegistryInput(registry);
  if (!ALLOWED_PHASES.has(phase)) {
    throw new Error(
      `audit phase must be one of: ${[...ALLOWED_PHASES].join(", ")}`,
    );
  }
  assertIsoTimestamp(observedAt, "observedAt");

  const sourceById = new Map(
    registry.upstream_sources.map((source) => [source.id, source]),
  );
  const headEntries = await Promise.all(
    registry.upstream_sources.map(async (source) => [
      source.id,
      await resolveHead(source.repository),
    ]),
  );
  const heads = new Map(headEntries);
  for (const [id, head] of heads) assertSha1(head, `${id} observed HEAD`);

  const commandSources = await Promise.all(
    ["sfinder-man", "sfinderbot"].map(async (id) => {
      const source = sourceById.get(id);
      const observedHead = heads.get(id);
      const pinnedBytes = toBuffer(
        await fetchFile(source.repository, source.commit, source.path),
      );
      const observedBytes =
        observedHead === source.commit
          ? pinnedBytes
          : toBuffer(
              await fetchFile(source.repository, observedHead, source.path),
            );
      const pinnedCommands = parseActiveBotCommands(pinnedBytes.toString("utf8"));
      const observedCommands = parseActiveBotCommands(
        observedBytes.toString("utf8"),
      );
      return Object.freeze({
        id,
        repository: source.repository,
        path: source.path,
        pinnedCommit: source.commit,
        observedHead,
        pinnedBytes,
        observedBytes,
        pinnedCommands,
        observedCommands,
      });
    }),
  );
  const commandById = new Map(commandSources.map((source) => [source.id, source]));

  const solutionFinder = sourceById.get("solution-finder");
  const solutionFinderHead = heads.get("solution-finder");
  const pinnedTree = directorySnapshot(
    await fetchTree(solutionFinder.repository, solutionFinder.commit),
    solutionFinder.path,
  );
  const observedTree =
    solutionFinderHead === solutionFinder.commit
      ? pinnedTree
      : directorySnapshot(
          await fetchTree(solutionFinder.repository, solutionFinderHead),
          solutionFinder.path,
        );

  const sfinderMan = commandById.get("sfinder-man");
  const sfinderbot = commandById.get("sfinderbot");
  const inventories = [
    inventoryAudit(
      "sfinder-man",
      registryNames(registry, "sfinder-man"),
      sfinderMan.pinnedCommands,
      sfinderMan.observedCommands,
      sourceById.get("sfinder-man").expected_active_command_count,
    ),
    inventoryAudit(
      "sfinderbot-only",
      registryNames(registry, "sfinderbot"),
      difference(sfinderbot.pinnedCommands, sfinderMan.pinnedCommands),
      difference(sfinderbot.observedCommands, sfinderMan.observedCommands),
      sourceById.get("sfinderbot").expected_sfinderbot_only_command_count,
    ),
  ];

  const sources = [
    ...commandSources.map((source) => ({
      id: source.id,
      repository: source.repository,
      path: source.path,
      pinned_commit: source.pinnedCommit,
      observed_head: source.observedHead,
      head_matches_pin: source.observedHead === source.pinnedCommit,
      snapshot_kind: "file",
      pinned_sha256: sha256(source.pinnedBytes),
      observed_sha256: sha256(source.observedBytes),
      snapshot_matches_pin:
        sha256(source.pinnedBytes) === sha256(source.observedBytes),
      pinned_active_command_count: source.pinnedCommands.length,
      observed_active_command_count: source.observedCommands.length,
      pinned_active_commands_sha256: namesHash(source.pinnedCommands),
      observed_active_commands_sha256: namesHash(source.observedCommands),
    })),
    {
      id: solutionFinder.id,
      repository: solutionFinder.repository,
      path: solutionFinder.path,
      pinned_commit: solutionFinder.commit,
      observed_head: solutionFinderHead,
      head_matches_pin: solutionFinderHead === solutionFinder.commit,
      snapshot_kind: "git-tree-prefix",
      manifest_format: TREE_MANIFEST_FORMAT,
      pinned_file_count: pinnedTree.fileCount,
      observed_file_count: observedTree.fileCount,
      pinned_manifest_sha256: pinnedTree.manifestSha256,
      observed_manifest_sha256: observedTree.manifestSha256,
      snapshot_matches_pin:
        pinnedTree.fileCount === observedTree.fileCount &&
        pinnedTree.manifestSha256 === observedTree.manifestSha256,
    },
  ];

  const driftReasons = [
    ...sources.flatMap((source) => {
      const reasons = [];
      if (!source.head_matches_pin) reasons.push(`${source.id}:head-moved`);
      if (!source.snapshot_matches_pin)
        reasons.push(`${source.id}:snapshot-changed`);
      return reasons;
    }),
    ...inventories.flatMap((inventory) =>
      inventory.matches_registry
        ? []
        : [`${inventory.id}:command-inventory-mismatch`],
    ),
  ];

  const audit = {
    schema_id: AUDIT_SCHEMA_ID,
    phase,
    observed_at: observedAt,
    registry: {
      schema_id: registry.schema_id,
      target_release: registry.target_release,
      snapshot_date: registry.snapshot_date,
    },
    status: driftReasons.length === 0 ? "no-drift" : "drift-detected",
    drift_reasons: driftReasons,
    sources,
    command_inventories: inventories,
  };
  validateAuditSnapshot(audit, registry, { expectedPhase: phase });
  return Object.freeze(audit);
}

export function validateAuditSnapshot(
  audit,
  registry,
  { expectedPhase } = {},
) {
  validateRegistryInput(registry);
  if (audit?.schema_id !== AUDIT_SCHEMA_ID) {
    throw new Error(`unexpected audit schema: ${String(audit?.schema_id)}`);
  }
  if (!ALLOWED_PHASES.has(audit.phase)) {
    throw new Error(`unexpected audit phase: ${String(audit.phase)}`);
  }
  if (expectedPhase && audit.phase !== expectedPhase) {
    throw new Error(
      `audit phase mismatch: expected ${expectedPhase}, received ${audit.phase}`,
    );
  }
  assertIsoTimestamp(audit.observed_at, "audit observed_at");
  assertIsoDate(audit.registry?.snapshot_date, "audit registry snapshot_date");
  assertIsoDate(registry.snapshot_date, "current registry snapshot_date");
  if (
    audit.registry?.schema_id !== registry.schema_id ||
    audit.registry?.target_release !== registry.target_release ||
    (audit.phase === "release-freeze" &&
      audit.registry?.snapshot_date !== registry.snapshot_date) ||
    (audit.phase === "implementation-start" &&
      (audit.registry.snapshot_date > audit.observed_at.slice(0, 10) ||
        audit.registry.snapshot_date > registry.snapshot_date))
  ) {
    throw new Error("audit registry identity does not match the current registry");
  }

  const sourceRows = new Map(
    requireUniqueRows(audit.sources, "audit source").map((row) => [row.id, row]),
  );
  if (sourceRows.size !== registry.upstream_sources.length) {
    throw new Error("audit source count does not match the registry");
  }
  for (const source of registry.upstream_sources) {
    const row = sourceRows.get(source.id);
    if (!row) throw new Error(`audit source is missing: ${source.id}`);
    if (
      row.repository !== source.repository ||
      row.path !== source.path ||
      row.pinned_commit !== source.commit
    ) {
      throw new Error(`audit provenance mismatch: ${source.id}`);
    }
    assertSha1(row.observed_head, `${source.id} observed_head`);
    if (row.head_matches_pin !== (row.observed_head === row.pinned_commit)) {
      throw new Error(`${source.id} head_matches_pin is inconsistent`);
    }
    if (row.snapshot_kind === "file") {
      assertSha256(row.pinned_sha256, `${source.id} pinned_sha256`);
      assertSha256(row.observed_sha256, `${source.id} observed_sha256`);
      assertSha256(
        row.pinned_active_commands_sha256,
        `${source.id} pinned_active_commands_sha256`,
      );
      assertSha256(
        row.observed_active_commands_sha256,
        `${source.id} observed_active_commands_sha256`,
      );
      if (
        !Number.isInteger(row.pinned_active_command_count) ||
        row.pinned_active_command_count <= 0 ||
        !Number.isInteger(row.observed_active_command_count) ||
        row.observed_active_command_count <= 0
      ) {
        throw new Error(`${source.id} active command counts must be positive integers`);
      }
      if (
        row.snapshot_matches_pin !==
        (row.pinned_sha256 === row.observed_sha256)
      ) {
        throw new Error(`${source.id} snapshot_matches_pin is inconsistent`);
      }
    } else if (row.snapshot_kind === "git-tree-prefix") {
      if (row.manifest_format !== TREE_MANIFEST_FORMAT) {
        throw new Error(`${source.id} manifest format is unsupported`);
      }
      assertPositiveInteger(row.pinned_file_count, `${source.id} pinned_file_count`);
      assertPositiveInteger(
        row.observed_file_count,
        `${source.id} observed_file_count`,
      );
      assertSha256(
        row.pinned_manifest_sha256,
        `${source.id} pinned_manifest_sha256`,
      );
      assertSha256(
        row.observed_manifest_sha256,
        `${source.id} observed_manifest_sha256`,
      );
      if (
        row.snapshot_matches_pin !==
        (row.pinned_file_count === row.observed_file_count &&
          row.pinned_manifest_sha256 === row.observed_manifest_sha256)
      ) {
        throw new Error(`${source.id} snapshot_matches_pin is inconsistent`);
      }
    } else {
      throw new Error(`unexpected snapshot kind for ${source.id}`);
    }
  }

  const inventoryRows = new Map(
    requireUniqueRows(audit.command_inventories, "command inventory").map(
      (row) => [row.id, row],
    ),
  );
  for (const [id, sourceId, expectedCount] of [
    [
      "sfinder-man",
      "sfinder-man",
      sourceById(registry, "sfinder-man").expected_active_command_count,
    ],
    [
      "sfinderbot-only",
      "sfinderbot",
      sourceById(registry, "sfinderbot")
        .expected_sfinderbot_only_command_count,
    ],
  ]) {
    const row = inventoryRows.get(id);
    if (!row) throw new Error(`command inventory is missing: ${id}`);
    const expectedNames = registryNames(registry, sourceId);
    if (row.expected_count !== expectedCount) {
      throw new Error(`${id} expected count does not match the registry`);
    }
    assertSortedUniqueStrings(row.registry_names, `${id} registry_names`);
    assertSortedUniqueStrings(row.pinned_names, `${id} pinned_names`);
    assertSortedUniqueStrings(row.observed_names, `${id} observed_names`);
    if (
      row.pinned_count !== row.pinned_names.length ||
      row.observed_count !== row.observed_names.length
    ) {
      throw new Error(`${id} recorded command count is inconsistent`);
    }
    if (!sameStrings(row.registry_names, expectedNames)) {
      throw new Error(`${id} recorded registry names are stale`);
    }
    if (
      row.registry_names_sha256 !== namesHash(row.registry_names) ||
      row.pinned_names_sha256 !== namesHash(row.pinned_names) ||
      row.observed_names_sha256 !== namesHash(row.observed_names)
    ) {
      throw new Error(`${id} command inventory digest mismatch`);
    }
    const matches =
      row.pinned_count === expectedCount &&
      row.observed_count === expectedCount &&
      sameStrings(row.registry_names, row.pinned_names) &&
      sameStrings(row.registry_names, row.observed_names);
    if (row.matches_registry !== matches) {
      throw new Error(`${id} matches_registry is inconsistent`);
    }
  }

  const computedReasons = [
    ...audit.sources.flatMap((source) => [
      ...(source.head_matches_pin ? [] : [`${source.id}:head-moved`]),
      ...(source.snapshot_matches_pin
        ? []
        : [`${source.id}:snapshot-changed`]),
    ]),
    ...audit.command_inventories.flatMap((inventory) =>
      inventory.matches_registry
        ? []
        : [`${inventory.id}:command-inventory-mismatch`],
    ),
  ];
  if (!sameStrings(audit.drift_reasons, computedReasons)) {
    throw new Error("audit drift reasons are inconsistent");
  }
  const expectedStatus = computedReasons.length === 0 ? "no-drift" : "drift-detected";
  if (audit.status !== expectedStatus) {
    throw new Error("audit status is inconsistent with its evidence");
  }
  return true;
}

export function parseAuditCliArguments(args) {
  if (!Array.isArray(args)) throw new TypeError("audit CLI arguments must be an array");
  const allowed = new Set(["--phase", "--output"]);
  const values = new Map();
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (!allowed.has(option)) {
      throw new Error(`unsupported upstream drift audit argument: ${String(option)}`);
    }
    if (values.has(option)) {
      throw new Error(`duplicate upstream drift audit argument: ${option}`);
    }
    const value = args[index + 1];
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values.set(option, value);
    index += 1;
  }
  const phase = values.get("--phase");
  if (!ALLOWED_PHASES.has(phase)) {
    throw new Error(
      "--phase must be implementation-start or release-freeze",
    );
  }
  return Object.freeze({ phase, outputPath: values.get("--output") });
}

export async function writeAuditSnapshotNew({
  audit,
  registry,
  outputPath,
  expectedPhase,
}) {
  validateAuditSnapshot(audit, registry, { expectedPhase });
  if (typeof outputPath !== "string" || outputPath.length === 0 || outputPath.includes("\0")) {
    throw new Error("audit output path must be non-empty");
  }
  const path = resolve(outputPath);
  await assertSafeDirectoryChain(dirname(path));
  const handle = await open(path, "wx", 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(audit, null, 2)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  return path;
}

function inventoryAudit(id, registryNamesValue, pinnedNames, observedNames, count) {
  const registryNamesSorted = [...registryNamesValue].sort();
  const pinnedNamesSorted = [...pinnedNames].sort();
  const observedNamesSorted = [...observedNames].sort();
  return {
    id,
    expected_count: count,
    pinned_count: pinnedNamesSorted.length,
    observed_count: observedNamesSorted.length,
    registry_names_sha256: namesHash(registryNamesSorted),
    pinned_names_sha256: namesHash(pinnedNamesSorted),
    observed_names_sha256: namesHash(observedNamesSorted),
    matches_registry:
      pinnedNamesSorted.length === count &&
      observedNamesSorted.length === count &&
      sameStrings(registryNamesSorted, pinnedNamesSorted) &&
      sameStrings(registryNamesSorted, observedNamesSorted),
    registry_names: registryNamesSorted,
    pinned_names: pinnedNamesSorted,
    observed_names: observedNamesSorted,
  };
}

function readDecorator(lines, startIndex) {
  let endIndex = startIndex;
  let text = lines[startIndex].trim();
  let depth = delimiterDepth(text);
  while (depth > 0) {
    endIndex += 1;
    if (endIndex >= lines.length) {
      throw new Error(`unterminated command decorator on line ${startIndex + 1}`);
    }
    const next = lines[endIndex].trim();
    text += `\n${next}`;
    depth += delimiterDepth(next);
  }
  if (depth < 0) {
    throw new Error(`unbalanced command decorator on line ${startIndex + 1}`);
  }
  return Object.freeze({ text, endIndex });
}

function delimiterDepth(line) {
  let depth = 0;
  let quote = null;
  let escaped = false;
  for (const character of line) {
    if (escaped) {
      escaped = false;
      continue;
    }
    if (character === "\\" && quote) {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "(") {
      depth += 1;
    } else if (character === ")") {
      depth -= 1;
    }
  }
  return depth;
}

async function resolveRemoteHead(repository) {
  const result = spawnSync("git", ["ls-remote", `${repository}.git`, "HEAD"], {
    encoding: "utf8",
    shell: false,
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(
      `git ls-remote failed for ${repository}: ${result.stderr.trim() || result.error?.message || `status ${result.status}`}`,
    );
  }
  const match = /^([0-9a-f]{40})\tHEAD\s*$/u.exec(result.stdout);
  if (!match) throw new Error(`unexpected git ls-remote output for ${repository}`);
  return match[1];
}

async function fetchRepositoryFile(repository, commit, path) {
  assertSha1(commit, "repository file commit");
  const { owner, name } = githubRepository(repository);
  const encodedPath = path.split("/").map(encodeURIComponent).join("/");
  return fetchBytes(
    `https://raw.githubusercontent.com/${owner}/${name}/${commit}/${encodedPath}`,
  );
}

async function fetchRepositoryTree(repository, commit) {
  assertSha1(commit, "repository tree commit");
  const { owner, name } = githubRepository(repository);
  const response = await fetchJson(
    `https://api.github.com/repos/${owner}/${name}/git/trees/${commit}?recursive=1`,
  );
  if (response.truncated === true) {
    throw new Error(`${repository} recursive Git tree response was truncated`);
  }
  if (!Array.isArray(response.tree)) {
    throw new Error(`${repository} Git tree response has no tree array`);
  }
  return response.tree;
}

function directorySnapshot(entries, prefix) {
  const normalizedPrefix = prefix.replace(/\/+$/u, "");
  const files = entries
    .filter(
      (entry) =>
        entry?.type === "blob" &&
        typeof entry.path === "string" &&
        entry.path.startsWith(`${normalizedPrefix}/`),
    )
    .map((entry) => {
      if (
        typeof entry.mode !== "string" ||
        !SHA1.test(entry.sha) ||
        (!Number.isInteger(entry.size) && entry.size !== undefined)
      ) {
        throw new Error(`invalid Git tree entry beneath ${prefix}`);
      }
      return {
        mode: entry.mode,
        path: entry.path,
        sha: entry.sha,
        size: entry.size ?? null,
        type: entry.type,
      };
    })
    .sort((left, right) =>
      left.path < right.path ? -1 : left.path > right.path ? 1 : 0,
    );
  if (files.length === 0) {
    throw new Error(`Git tree has no files beneath ${prefix}`);
  }
  const manifest = files
    .map(
      ({ mode, path, sha, size, type }) =>
        `${mode}\0${type}\0${sha}\0${size ?? ""}\0${path}\n`,
    )
    .join("");
  return Object.freeze({
    fileCount: files.length,
    manifestSha256: sha256(manifest),
  });
}

async function fetchBytes(url) {
  const response = await fetch(url, {
    headers: { "User-Agent": USER_AGENT },
    signal: AbortSignal.timeout(30_000),
  });
  if (!response.ok) {
    throw new Error(`GET ${url} failed with HTTP ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

async function fetchJson(url) {
  const response = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": USER_AGENT,
      "X-GitHub-Api-Version": "2022-11-28",
    },
    signal: AbortSignal.timeout(30_000),
  });
  if (!response.ok) {
    throw new Error(`GET ${url} failed with HTTP ${response.status}`);
  }
  return response.json();
}

function githubRepository(repository) {
  const match = /^https:\/\/github\.com\/([^/]+)\/([^/]+?)(?:\.git)?$/u.exec(
    repository,
  );
  if (!match) throw new Error(`unsupported upstream repository: ${repository}`);
  return Object.freeze({ owner: match[1], name: match[2] });
}

function validateRegistryInput(registry) {
  if (!registry || !Array.isArray(registry.upstream_sources)) {
    throw new Error("registry has no upstream_sources array");
  }
  if (!Array.isArray(registry.upstream_command_inventory)) {
    throw new Error("registry has no upstream_command_inventory array");
  }
  requireUniqueRows(registry.upstream_sources, "upstream source");
  for (const id of ["sfinder-man", "sfinderbot", "solution-finder"]) {
    const source = sourceById(registry, id);
    assertSha1(source.commit, `${id} pinned commit`);
    githubRepository(source.repository);
    if (typeof source.path !== "string" || source.path.length === 0) {
      throw new Error(`${id} source path is missing`);
    }
  }
}

function sourceById(registry, id) {
  const source = registry.upstream_sources.find((candidate) => candidate.id === id);
  if (!source) throw new Error(`registry upstream source is missing: ${id}`);
  return source;
}

function registryNames(registry, sourceId) {
  const names = registry.upstream_command_inventory
    .filter((entry) => entry.source_id === sourceId)
    .map((entry) => entry.name)
    .sort();
  assertSortedUniqueStrings(names, `${sourceId} registry command names`);
  return names;
}

function difference(left, right) {
  const excluded = new Set(right);
  return left.filter((name) => !excluded.has(name)).sort();
}

function namesHash(names) {
  return sha256(JSON.stringify([...names].sort()));
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function toBuffer(value) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof Uint8Array) return Buffer.from(value);
  if (typeof value === "string") return Buffer.from(value, "utf8");
  throw new TypeError("upstream file fetch must return bytes or a string");
}

function requireUniqueRows(rows, label) {
  if (!Array.isArray(rows)) throw new Error(`${label} rows must be an array`);
  const ids = rows.map((row) => row?.id);
  if (ids.some((id) => typeof id !== "string" || id.length === 0)) {
    throw new Error(`${label} row has no id`);
  }
  if (new Set(ids).size !== ids.length) {
    throw new Error(`${label} ids must be unique`);
  }
  return rows;
}

function assertSortedUniqueStrings(values, label) {
  if (!Array.isArray(values) || values.some((value) => typeof value !== "string")) {
    throw new Error(`${label} must be a string array`);
  }
  const sorted = [...values].sort();
  if (new Set(values).size !== values.length || !sameStrings(values, sorted)) {
    throw new Error(`${label} must be sorted and unique`);
  }
}

function sameStrings(left, right) {
  return (
    Array.isArray(left) &&
    Array.isArray(right) &&
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function assertSha1(value, label) {
  if (typeof value !== "string" || !SHA1.test(value)) {
    throw new Error(`${label} must be a lowercase 40-character SHA-1`);
  }
}

function assertSha256(value, label) {
  if (typeof value !== "string" || !SHA256.test(value)) {
    throw new Error(`${label} must be a lowercase 64-character SHA-256`);
  }
}

function assertPositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
}

function assertIsoTimestamp(value, label) {
  if (
    typeof value !== "string" ||
    !Number.isFinite(Date.parse(value)) ||
    new Date(value).toISOString() !== value
  ) {
    throw new Error(`${label} must be a canonical ISO-8601 UTC timestamp`);
  }
}

function assertIsoDate(value, label) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}$/u.test(value) ||
    new Date(`${value}T00:00:00.000Z`).toISOString().slice(0, 10) !== value
  ) {
    throw new Error(`${label} must be a canonical ISO-8601 date`);
  }
}

async function main() {
  const { phase, outputPath } = parseAuditCliArguments(process.argv.slice(2));
  const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const registry = JSON.parse(
    await readFile(
      resolve(
        root,
        "tests/fixtures/contracts/product_capability_registry.v1.json",
      ),
      "utf8",
    ),
  );
  const audit = await auditUpstreamDrift({ registry, phase });
  if (outputPath === undefined) {
    process.stdout.write(`${JSON.stringify(audit, null, 2)}\n`);
  } else {
    await writeAuditSnapshotNew({
      audit,
      registry,
      outputPath,
      expectedPhase: phase,
    });
    process.stdout.write(`${resolve(outputPath)}\n`);
  }
  if (audit.status !== "no-drift") process.exitCode = 2;
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const status = await lstat(current);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      throw new Error(`audit output parent uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 1;
  });
}
