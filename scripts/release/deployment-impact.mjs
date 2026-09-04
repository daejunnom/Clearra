#!/usr/bin/env node

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const SEMVER_TAG = /^v([0-9]+)\.([0-9]+)\.([0-9]+)$/u;

export function classifyDeploymentImpact(paths) {
  if (!Array.isArray(paths)) throw new Error("changed paths are required");
  const changed = [...new Set(paths.map(normalizePath))].sort((a, b) => a.localeCompare(b, "en"));
  let pages = false;
  let discord = false;
  let gui = false;
  let cli = false;
  let discordGateway = false;
  let heavyCloudRuntime = false;
  let pc4LookupService = false;
  let pc4ActivationManifest = false;
  let releaseInfrastructure = false;
  let broad = false;
  for (const path of changed) {
    if (isTestPath(path) || isDocumentationPath(path)) continue;
    if ([
      ".github/workflows/discord-deploy.yml",
      ".github/workflows/discord-deploy-recovery.yml",
    ].includes(path)) {
      discord = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      releaseInfrastructure = true;
      continue;
    }
    if ([
      ".github/workflows/pages.yml",
      ".github/workflows/pages-rollback.yml",
    ].includes(path)) {
      pages = true;
      releaseInfrastructure = true;
      continue;
    }
    // This static browser asset is also compiled into the native CLI and is
    // copied into the Discord current-job image before that CLI is built.
    // Keep it ahead of the general Pages application rule.
    if (path.startsWith("apps/clearra-web/static/tablebase/")) {
      pages = true;
      discord = true;
      cli = true;
      heavyCloudRuntime = true;
      pc4ActivationManifest = true;
      continue;
    }
    if (path.startsWith("apps/clearra-web/")) {
      pages = true;
      continue;
    }
    if (path.startsWith("apps/clearra-discord-bot/")) {
      discord = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      continue;
    }
    if (path.startsWith("packages/ctk3/")) {
      // packages/clearra-ui re-exports CTK3 codec and operation geometry into
      // the Pages and desktop GUI bundles. The Discord job-service consumes
      // the same package directly. The standalone Rust CLI is independent.
      pages = true;
      discord = true;
      gui = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      continue;
    }
    if (path.startsWith("apps/clearra-desktop/") ||
        path.startsWith("crates/clearra-gui-host/") ||
        path.startsWith("crates/clearra-ui-schema/")) {
      gui = true;
      continue;
    }
    if (path.startsWith("packages/clearra-ui/")) {
      gui = true;
      pages = true;
      continue;
    }
    if (path.startsWith("apps/clearra-cli/")) {
      cli = true;
      continue;
    }
    // Browser WASM and its ABI export boundary are not linked into the native
    // CLI/Discord current-job image. The desktop host enables clearra-wasm
    // through its wasm-cpu-runtime feature.
    if (path.startsWith("crates/clearra-wasm-abi/")) {
      pages = true;
      continue;
    }
    if (path.startsWith("crates/clearra-wasm/")) {
      pages = true;
      gui = true;
      continue;
    }
    // The shared CLI-command crate is used by browser WASM, the desktop host, and
    // the native CLI. Discord's current-job image builds that native CLI.
    if (path.startsWith("crates/clearra-cli-command/")) {
      pages = true;
      discord = true;
      gui = true;
      cli = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      continue;
    }
    // The Rust CTK/output/bitmap renderer chain is consumed through
    // clearra-app by the browser WASM build, the desktop GUI host, and the
    // standalone CLI. The Discord current job-service also builds that CLI
    // into its Cloud Run image and exposes its exact `utility render` result,
    // so a renderer change must promote the Discord runtime as well.
    if (
      path.startsWith("crates/clearra-ctk3/") ||
      path.startsWith("crates/clearra-output/") ||
      path.startsWith("crates/clearra-render/")
    ) {
      pages = true;
      discord = true;
      gui = true;
      cli = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      continue;
    }
    if (path.startsWith("crates/clearra-cli/")) {
      cli = true;
      discord = true;
      heavyCloudRuntime = true;
      continue;
    }
    if (isDiscordReleaseAuthority(path)) {
      discord = true;
      discordGateway = true;
      heavyCloudRuntime = true;
      releaseInfrastructure = true;
      continue;
    }
    if (isPagesReleaseAuthority(path)) {
      pages = true;
      releaseInfrastructure = true;
      continue;
    }
    if (isPagesBuildTool(path)) {
      pages = true;
      continue;
    }
    if (path.startsWith(".github/workflows/") || path.startsWith("scripts/release/")) {
      releaseInfrastructure = true;
      broad = true;
      continue;
    }
    broad = true;
  }
  if (broad) {
    pages = discord = gui = cli = true;
    discordGateway = heavyCloudRuntime = pc4LookupService = pc4ActivationManifest = true;
  }
  const scope = broad
    ? "shared"
    : [pages && "pages", discord && "discord", gui && "gui", cli && "cli"].filter(Boolean).join("+") || "none";
  return Object.freeze({
    scope,
    deployPages: pages,
    deployDiscord: discord,
    deployGui: gui,
    deployCli: cli,
    componentScope: [
      pages && "pages",
      gui && "desktop_gui",
      cli && "cli",
      discordGateway && "discord_gateway",
      heavyCloudRuntime && "heavy_cloud_runtime",
      pc4LookupService && "pc4_lookup_service",
      pc4ActivationManifest && "pc4_activation_manifest",
      releaseInfrastructure && "release_infrastructure",
    ].filter(Boolean).join("+") || "none",
    deployDesktopGui: gui,
    deployDiscordGateway: discordGateway,
    deployHeavyCloudRuntime: heavyCloudRuntime,
    deployPc4LookupService: pc4LookupService,
    deployPc4ActivationManifest: pc4ActivationManifest,
    releaseInfrastructureChanged: releaseInfrastructure,
    changedPaths: Object.freeze(changed),
    changedPathsSha256: createHash("sha256").update(`${changed.join("\n")}\n`, "utf8").digest("hex"),
  });
}

export function selectProductionBaseline(tags) {
  if (!Array.isArray(tags)) throw new Error("release tags are required");
  const versions = tags.flatMap((tag) => {
    const match = String(tag).match(SEMVER_TAG);
    return match ? [{ tag: String(tag), version: match.slice(1).map(Number) }] : [];
  });
  versions.sort((left, right) => {
    for (let index = 0; index < 3; index += 1) {
      if (left.version[index] !== right.version[index]) return right.version[index] - left.version[index];
    }
    return 0;
  });
  if (versions.length === 0) throw new Error("no reachable production semver tag exists");
  return versions[0].tag;
}

export function analyzeDeploymentImpact(sourceCommit, query = git) {
  if (!SOURCE_COMMIT.test(sourceCommit ?? "")) {
    throw new Error("deployment impact source commit is invalid");
  }
  if (query(["rev-parse", `${sourceCommit}^{commit}`]) !== sourceCommit) {
    throw new Error("deployment impact source commit does not resolve exactly");
  }
  const reachableTags = query([
    "tag", "--merged", sourceCommit, "--list", "v[0-9]*.[0-9]*.[0-9]*",
  ]).split(/\r?\n/u).filter(Boolean);
  const baselineTag = selectProductionBaseline(reachableTags);
  const baselineCommit = query(["rev-list", "-n", "1", baselineTag]);
  const paths = query([
    "diff", "--name-only", "--diff-filter=ACMRD", `${baselineCommit}..${sourceCommit}`, "--",
  ]).split(/\r?\n/u).filter(Boolean);
  return Object.freeze({
    sourceCommit,
    baselineTag,
    baselineCommit,
    impact: classifyDeploymentImpact(paths),
  });
}

function isTestPath(path) {
  return (
    path.startsWith("tests/") ||
    path.endsWith(".test.mjs") ||
    path.endsWith(".test.js") ||
    path.endsWith(".test.ts") ||
    path.endsWith(".test.ps1") ||
    path.includes("/__tests__/") ||
    path.includes("/test/") ||
    path.includes("/tests/")
  );
}

function isDocumentationPath(path) {
  return path.startsWith("docs/") || (!path.includes("/") && path.endsWith(".md"));
}

function isDiscordReleaseAuthority(path) {
  return (
    path.startsWith("scripts/release/cloud/") ||
    path.startsWith("scripts/release/oracle/") ||
    /^scripts\/release\/(?:discord-|cloud-|observe-production-surfaces|production-surface-probe|materialize-production-probe)/u.test(path)
  );
}

function isPagesReleaseAuthority(path) {
  return /^scripts\/release\/(?:pages-|accepted-pages-build)/u.test(path);
}

function isPagesBuildTool(path) {
  return [
    "scripts/tools/build-clearra-wasm.mjs",
    "scripts/tools/clearra-wasm-build-contract.mjs",
  ].includes(path);
}

function normalizePath(value) {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0")) {
    throw new Error("changed path is invalid");
  }
  const path = value.replaceAll("\\", "/").replace(/^\.\//u, "");
  if (path.startsWith("/") || path.split("/").some((part) => part === ".." || part === "")) {
    throw new Error("changed path is outside the repository");
  }
  return path;
}

function git(arguments_) {
  const result = spawnSync("git", arguments_, {
    encoding: "utf8",
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) throw new Error("deployment impact git query failed");
  return result.stdout.trim();
}

function parseArguments(args) {
  const values = {};
  const allowed = new Set(["--source-commit", "--format"]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0) {
      throw new Error("deployment impact arguments are invalid");
    }
    if (Object.hasOwn(values, option)) throw new Error(`duplicate option: ${option}`);
    values[option] = value;
  }
  if (!SOURCE_COMMIT.test(values["--source-commit"] ?? "")) {
    throw new Error("deployment impact source commit is invalid");
  }
  if (!Object.hasOwn(values, "--format")) values["--format"] = "json";
  if (!["json", "github-output"].includes(values["--format"])) {
    throw new Error("deployment impact format is invalid");
  }
  return values;
}

function main() {
  const values = parseArguments(process.argv.slice(2));
  const sourceCommit = values["--source-commit"];
  const analysis = analyzeDeploymentImpact(sourceCommit);
  const impact = analysis.impact;
  const result = {
    source_commit: sourceCommit,
    baseline_tag: analysis.baselineTag,
    baseline_commit: analysis.baselineCommit,
    impact_scope: impact.scope,
    deploy_pages: impact.deployPages,
    deploy_discord: impact.deployDiscord,
    deploy_gui: impact.deployGui,
    deploy_cli: impact.deployCli,
    component_scope: impact.componentScope,
    deploy_desktop_gui: impact.deployDesktopGui,
    deploy_discord_gateway: impact.deployDiscordGateway,
    deploy_heavy_cloud_runtime: impact.deployHeavyCloudRuntime,
    deploy_pc4_lookup_service: impact.deployPc4LookupService,
    deploy_pc4_activation_manifest: impact.deployPc4ActivationManifest,
    release_infrastructure_changed: impact.releaseInfrastructureChanged,
    changed_paths_sha256: impact.changedPathsSha256,
  };
  if (values["--format"] === "github-output") {
    for (const [key, value] of Object.entries(result)) {
      process.stdout.write(`${key}=${value}\n`);
    }
  } else {
    process.stdout.write(`${JSON.stringify(result)}\n`);
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(
      `deployment_impact=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
