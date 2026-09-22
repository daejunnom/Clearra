import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyFastCorrectionEntries,
  loadOwnerManifest,
  parseRawDiff,
  selectLatestProductionTag,
  validateFastCorrectionPlan,
} from "./fast-correction-plan.mjs";

const BASE = "1".repeat(40);
const CANDIDATE = "2".repeat(40);
const OLD_OBJECT = "3".repeat(40);
const NEW_OBJECT = "4".repeat(40);
const loaded = loadOwnerManifest();

function entry(path, overrides = {}) {
  return {
    status: "M",
    path,
    oldPath: null,
    oldMode: "100644",
    newMode: "100644",
    oldObject: OLD_OBJECT,
    newObject: NEW_OBJECT,
    ...overrides,
  };
}

function classify(pathsOrEntries) {
  const entries = pathsOrEntries.map((value) => typeof value === "string" ? entry(value) : value);
  return classifyFastCorrectionEntries({
    baseCommit: BASE,
    baseTag: "v0.8.0",
    candidateCommit: CANDIDATE,
    entries,
    manifest: loaded.manifest,
    manifestSha256: loaded.sha256,
  });
}

test("Pages presentation changes select only focused Pages test, build, and deployment", () => {
  const plan = classify([
    "apps/clearra-web/src/routes/+page.svelte",
    "apps/clearra-web/test/ctkViewerQuery.contract.ts",
    "docs/gui.md",
  ]);
  assert.equal(plan.decision, "fast-eligible");
  assert.equal(plan.lane, "pages");
  assert.deepEqual(plan.affected_products, ["pages"]);
  assert.deepEqual(plan.skipped_products, ["cli", "discord", "gui"]);
  assert.deepEqual(plan.selected.tests.map(({ id }) => id), ["diff-check", "pages-workspace-contracts"]);
  assert.deepEqual(plan.selected.builds.map(({ id }) => id), ["pages-wasm-vite-build"]);
  assert.deepEqual(plan.selected.deployments.map(({ id }) => id), ["github-pages-forward"]);
  validateFastCorrectionPlan(plan);
});

test("documentation-only and release-workflow-only changes never deploy products", () => {
  const docs = classify(["README.md", "docs/test-policy.md"]);
  assert.equal(docs.decision, "fast-eligible");
  assert.equal(docs.lane, "documentation");
  assert.deepEqual(docs.selected.tests.map(({ id }) => id), ["diff-check"]);
  assert.deepEqual(docs.affected_products, []);

  const release = classify([
    ".github/workflows/finalize-release-publication.yml",
    "scripts/tools/validate-release-cli-smokes.test.mjs",
  ]);
  assert.equal(release.decision, "fast-eligible");
  assert.equal(release.lane, "release-workflow");
  assert.deepEqual(release.selected.tests.map(({ id }) => id), [
    "diff-check",
    "release-regressions",
    "release-workflow-smoke",
  ]);
  assert.deepEqual(release.selected.builds, []);
  assert.deepEqual(release.selected.deployments, []);
});

test("mixed Pages and release authority changes combine only their focused owners", () => {
  const plan = classify([
    "apps/clearra-web/src/app.html",
    ".github/workflows/finalize-release-publication.yml",
  ]);
  assert.equal(plan.lane, "pages+release-workflow");
  assert.equal(plan.deploy_pages, true);
  assert.deepEqual(plan.selected.tests.map(({ id }) => id), [
    "diff-check",
    "pages-workspace-contracts",
    "release-regressions",
    "release-workflow-smoke",
  ]);
});

test("workflow-only fixes derive exact dual-authority follow-up surfaces without a user surface switch", () => {
  const discord = classify([".github/workflows/discord-deploy.yml", "docs/test-policy.md"]);
  assert.equal(discord.decision, "fast-eligible");
  assert.deepEqual(discord.control_deployments, ["discord"]);
  assert.equal(discord.deploy_pages, false);
  assert.ok(discord.selected.deployments.some(({ id }) => id === "discord-accepted-product-redeploy"));

  const pages = classify([".github/workflows/pages.yml"]);
  assert.deepEqual(pages.control_deployments, ["pages"]);
  assert.ok(pages.selected.deployments.some(({ id }) => id === "pages-accepted-product-redeploy"));

  const both = classify([".github/workflows/discord-deploy.yml", ".github/workflows/pages.yml"]);
  assert.deepEqual(both.control_deployments, ["discord", "pages"]);
});

test("runtime Pages changes cannot be mixed with a workflow control redeploy", () => {
  const plan = classify([
    "apps/clearra-web/src/routes/+page.svelte",
    ".github/workflows/pages.yml",
  ]);
  assert.equal(plan.decision, "full-required");
  assert.ok(plan.reason_codes.includes("control-runtime-mix"));
});

test("core, native, Rust, WASM, performance, shared schemas, dependencies, and unknown paths require full canonical acceptance", () => {
  for (const path of [
    "core-c/src/search.c",
    "crates/clearra-app/src/lib.rs",
    "crates/clearra-wasm/src/lib.rs",
    "benchmarks/path-tail.mjs",
    "packages/clearra-ui/src/lib/index.ts",
    "packages/ctk3/src/index.ts",
    "apps/clearra-web/src/workers/WasmJobRunner.ts",
    "apps/clearra-web/static/tablebase/pc4.bin",
    "apps/clearra-discord-bot/src/main.mjs",
    "apps/clearra-desktop/src/main.ts",
    "apps/clearra-cli/src/main.rs",
    "apps/clearra-web/package.json",
    "Cargo.lock",
    "scripts/release/pages-deployment-authority.mjs",
    "unowned/new-runtime.bin",
  ]) {
    const plan = classify([path]);
    assert.equal(plan.decision, "full-required", path);
    assert.equal(plan.deploy_pages, false, path);
    assert.deepEqual(plan.selected, { tests: [], builds: [], deployments: [] }, path);
  }
});

test("the fast authority bundle cannot classify or approve changes to itself", () => {
  for (const path of loaded.manifest.authority_bundle) {
    const plan = classify([path]);
    assert.equal(plan.decision, "full-required", path);
    assert.ok(plan.reason_codes.includes("fast-authority-change"), path);
  }
});

test("renames, copies, submodules, symlinks, type changes, mode changes, and duplicates fail closed", () => {
  const cases = [
    entry("docs/new.md", { status: "R100", oldPath: "docs/old.md" }),
    entry("docs/copy.md", { status: "C100", oldPath: "docs/source.md" }),
    entry("docs/submodule", { oldMode: "160000", newMode: "160000" }),
    entry("docs/link.md", { oldMode: "120000", newMode: "120000" }),
    entry("docs/file.md", { status: "T", oldMode: "100644", newMode: "120000" }),
    entry("docs/file.md", { oldMode: "100644", newMode: "100755" }),
  ];
  for (const value of cases) assert.equal(classify([value]).decision, "full-required");
  const duplicate = classify([entry("docs/same.md"), entry("docs/same.md")]);
  assert.ok(duplicate.reason_codes.includes("duplicate-path"));
});

test("an exact self-diff is a sealed no-op and does not select commands", () => {
  const plan = classify([]);
  assert.equal(plan.decision, "no-op");
  assert.equal(plan.lane, "no-op");
  assert.deepEqual(plan.selected, { tests: [], builds: [], deployments: [] });
  assert.deepEqual(plan.skipped_products, ["cli", "discord", "gui", "pages"]);
});

test("raw NUL diff parsing preserves rename identity instead of flattening it", () => {
  const raw = [
    `:100644 100644 ${OLD_OBJECT} ${NEW_OBJECT} M`,
    "docs/a.md",
    `:100644 100644 ${OLD_OBJECT} ${NEW_OBJECT} R100`,
    "docs/old.md",
    "docs/new.md",
    "",
  ].join("\0");
  assert.deepEqual(parseRawDiff(raw), [
    entry("docs/a.md"),
    entry("docs/new.md", { status: "R100", oldPath: "docs/old.md" }),
  ]);
  assert.throws(() => parseRawDiff("not-terminated"), /NUL terminated/u);
});

test("plan hashes bind every selected surface and reject mutation", () => {
  const plan = classify(["apps/clearra-web/src/routes/+page.svelte"]);
  const tampered = structuredClone(plan);
  tampered.deploy_pages = false;
  assert.throws(() => validateFastCorrectionPlan(tampered), /hash mismatch/u);
});

test("latest production selection is semantic and rejects non-release tags", () => {
  assert.equal(selectLatestProductionTag(["v0.8.0", "v0.7.12", "candidate", "v1.0.0"]), "v1.0.0");
  assert.throws(() => selectLatestProductionTag(["candidate", "v0.8.0-rc.1"]), /no reachable/u);
});
