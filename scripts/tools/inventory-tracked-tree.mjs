import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../", import.meta.url));
const kinds = [
  "first_party_source_and_config",
  "test_and_fixture",
  "vendor",
  "tracked_generated",
  "retained_evidence",
  "docs",
  "gitlink",
];
const scopes = new Set([
  ".cargo", ".github", "apps", "assets", "config", "core-c", "crates",
  "docs", "packages", "scripts", "tests", "tools",
]);

function git(...args) {
  return execFileSync("git", ["--no-replace-objects", ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function classify(path, type) {
  if (type === "commit") return "gitlink";
  if (path.startsWith("tools/vendor/")) return "vendor";
  if (
    /(^|\/)(?:Cargo\.lock|pnpm-lock\.yaml|package-lock\.json)$/.test(path)
    || /^(?:assets\/skins\/default\/atlas\.png|apps\/clearra-desktop\/src-tauri\/icons\/icon\.(?:ico|png))$/.test(path)
  ) return "tracked_generated";
  if (
    /(^|\/)(?:test|tests|fixtures|goldens?|snapshots|benches)\//.test(path)
    || /(?:_tests?\.rs|\.(?:test|spec)\.[^/]+|\.snap)$/.test(path)
    || path.startsWith("crates/clearra-invariant-tests/")
    || path.startsWith("scripts/architecture/")
  ) return "test_and_fixture";
  if (
    path.startsWith("docs/research/")
    || path.startsWith("docs/legal-board/")
    || path.startsWith("config/conditioned-reachability-sources/")
    || /^(?:assets\/skins\/default|apps\/clearra-desktop\/src-tauri\/icons)\/(?:import-report|provenance)\.json$/.test(path)
  ) return "retained_evidence";
  if (path.startsWith("docs/") || /\.(?:md|rst)$/.test(path)) return "docs";
  return "first_party_source_and_config";
}

function emptyCounts() {
  return { paths: 0, blob_bytes: 0 };
}

function inventory(revision) {
  const commit = git("rev-parse", "--verify", `${revision}^{commit}`).trim();
  const byClass = Object.fromEntries(kinds.map((kind) => [kind, emptyCounts()]));
  const byScope = new Map();
  const total = emptyCounts();
  const tree = git("ls-tree", "-r", "-l", "-z", commit);
  for (const entry of tree.split("\0")) {
    if (entry === "") continue;
    const tab = entry.indexOf("\t");
    const metadata = /^(\d{6}) (blob|commit) [0-9a-f]+\s+(\d+|-)$/u.exec(entry.slice(0, tab));
    if (tab < 0 || !metadata || (metadata[2] === "blob") !== (metadata[3] !== "-")) {
      throw new Error("Unexpected git ls-tree entry metadata");
    }
    const path = entry.slice(tab + 1);
    const bytes = metadata[3] === "-" ? 0 : Number(metadata[3]);
    if (!Number.isSafeInteger(bytes)) throw new Error("Tracked blob size outside safe integer range");
    const kind = classify(path, metadata[2]);
    const scope = path.includes("/") ? path.split("/", 1)[0] : "";
    const groupName = scopes.has(scope) ? scope : "root_or_other";
    if (!byScope.has(groupName)) byScope.set(groupName, new Map());
    const scopedKinds = byScope.get(groupName);
    if (!scopedKinds.has(kind)) scopedKinds.set(kind, emptyCounts());
    for (const counts of [total, byClass[kind], scopedKinds.get(kind)]) {
      counts.paths += 1;
      counts.blob_bytes += bytes;
      if (!Number.isSafeInteger(counts.blob_bytes)) {
        throw new Error("Tracked blob total outside safe integer range");
      }
    }
  }
  return {
    schema_version: 1,
    commit,
    scope: "git-tree-paths-only",
    total,
    by_class: byClass,
    by_top_level: Object.fromEntries(Array.from(byScope, ([scope, groups]) => [
      scope,
      Object.fromEntries(groups),
    ])),
  };
}

const args = process.argv.slice(2);
const revision = args[0] ?? "HEAD";
if (args.length === 1 && args[0] === "--help") {
  process.stdout.write("Usage: node scripts/tools/inventory-tracked-tree.mjs [HEAD|full-commit-sha]\n");
} else if (args.length > 1 || (revision !== "HEAD" && !/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/i.test(revision))) {
  process.stderr.write("Expected HEAD or a full commit SHA\n");
  process.exitCode = 2;
} else {
  process.stdout.write(`${JSON.stringify(inventory(revision), null, 2)}\n`);
}
