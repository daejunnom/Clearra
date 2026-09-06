#!/usr/bin/env node

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { canonicalSha256 } from "./canonical-release-evidence.mjs";

const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const ACCEPTED_LEDGER_WORKFLOW = /^\.github\/workflows\/(?:component-ledger-bootstrap|fast-fix-production-finalizer)\.ya?ml$/u;
const SEMVER_TAG = /^v([0-9]+)\.([0-9]+)\.([0-9]+)$/u;
export const ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND =
  "accepted-component-ledger";
export const PRODUCTION_TAG_BASELINE_KIND = "production-tag";
export const DEPLOYMENT_COMPONENTS = Object.freeze([
  "pages",
  "desktop_gui",
  "cli",
  "discord_gateway",
  "heavy_cloud_runtime",
  "pc4_lookup_service",
  "pc4_activation_manifest",
  "release_infrastructure",
]);

export function classifyDeploymentImpact(paths) {
  if (!Array.isArray(paths)) throw new Error("changed paths are required");
  const changed = [...new Set(paths.map(normalizePath))].sort((a, b) => a.localeCompare(b, "en"));
  const productRelevantPaths = changed.filter((path) =>
    !isTestPath(path) &&
    !isDocumentationPath(path) &&
    !isLocalOnlyOperationalPath(path) &&
    !isTestSnapshotPath(path));
  const performanceSensitive = productRelevantPaths.some(isPerformanceSensitivePath);
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
    if (
      isTestPath(path) ||
      isDocumentationPath(path) ||
      isLocalOnlyOperationalPath(path) ||
      isTestSnapshotPath(path)
    ) continue;
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
      ".github/workflows/queue-pages-publication.yml",
      "scripts/release/queue-pages-publication.mjs",
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
      if (isDiscordGatewayOnlyPath(path)) {
        discordGateway = true;
      } else if (isHeavyCloudOnlyPath(path)) {
        heavyCloudRuntime = true;
      } else {
        // Shared command/protocol/config inputs are consumed by both Oracle's
        // gateway and the managed current-job service.
        discordGateway = true;
        heavyCloudRuntime = true;
      }
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
  const changedComponents = [
    pages && "pages",
    gui && "desktop_gui",
    cli && "cli",
    discordGateway && "discord_gateway",
    heavyCloudRuntime && "heavy_cloud_runtime",
    pc4LookupService && "pc4_lookup_service",
    pc4ActivationManifest && "pc4_activation_manifest",
    releaseInfrastructure && "release_infrastructure",
  ].filter(Boolean);
  // v0.8 has a closed non-mutating qualification contract only for Pages.
  // Desktop/CLI have no rolling production surface, and Discord/PC4 still
  // require their canonical deployment authorities. Keep every non-Pages
  // product change on the full gate until those component adapters exist.
  const fastEligible = changedComponents.length === 0 ||
    (changedComponents.length === 1 && changedComponents[0] === "pages");
  const requiresFullGate = releaseInfrastructure || broad ||
    performanceSensitive || !fastEligible;
  const gateMode = changedComponents.length === 0
    ? "none"
    : requiresFullGate
      ? "full"
      : "focused";
  const carryForwardComponents = DEPLOYMENT_COMPONENTS.filter(
    (component) => !changedComponents.includes(component),
  );
  return Object.freeze({
    scope,
    deployPages: pages,
    deployDiscord: discord,
    deployGui: gui,
    deployCli: cli,
    componentScope: changedComponents.join("+") || "none",
    deployDesktopGui: gui,
    deployDiscordGateway: discordGateway,
    deployHeavyCloudRuntime: heavyCloudRuntime,
    deployPc4LookupService: pc4LookupService,
    deployPc4ActivationManifest: pc4ActivationManifest,
    releaseInfrastructureChanged: releaseInfrastructure,
    performanceSensitiveChanged: performanceSensitive,
    requiresFullGate,
    gateMode,
    changedComponents: Object.freeze(changedComponents),
    carryForwardComponents: Object.freeze(carryForwardComponents),
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

export function analyzeDeploymentImpact(sourceCommit, options = {}) {
  const query = typeof options === "function" ? options : options.query ?? git;
  const requestedBaseline = typeof options === "function"
    ? undefined
    : options.baseline;
  if (!SOURCE_COMMIT.test(sourceCommit ?? "")) {
    throw new Error("deployment impact source commit is invalid");
  }
  if (query(["rev-parse", `${sourceCommit}^{commit}`]) !== sourceCommit) {
    throw new Error("deployment impact source commit does not resolve exactly");
  }
  const baseline = requestedBaseline === undefined
    ? resolveProductionTagBaseline(sourceCommit, query)
    : validateAcceptedLedgerBaseline(requestedBaseline, sourceCommit, query);
  const paths = query([
    "diff", "--name-only", "--diff-filter=ACMRD",
    `${baseline.sourceCommit}..${sourceCommit}`, "--",
  ]).split(/\r?\n/u).filter(Boolean);
  return Object.freeze({
    sourceCommit,
    baselineKind: baseline.kind,
    baselineTag: baseline.tag,
    baselineCommit: baseline.sourceCommit,
    baselineSourceCommit: baseline.sourceCommit,
    baselineWorkflowPath: baseline.workflowPath,
    baselineRunId: baseline.runId,
    baselineRunAttempt: baseline.runAttempt,
    baselineLedgerReportSha256: baseline.ledgerReportSha256,
    impact: classifyDeploymentImpact(paths),
  });
}

function resolveProductionTagBaseline(sourceCommit, query) {
  const productionTags = query([
    "tag", "--merged", sourceCommit, "--list", "v[0-9]*.[0-9]*.[0-9]*",
  ]).split(/\r?\n/u).filter(Boolean);
  const tag = selectProductionBaseline(productionTags);
  const baselineCommit = query(["rev-list", "-n", "1", tag]);
  if (!SOURCE_COMMIT.test(baselineCommit)) {
    throw new Error("production tag baseline commit is invalid");
  }
  if (query(["merge-base", baselineCommit, sourceCommit]) !== baselineCommit) {
    throw new Error("production tag baseline is not a source ancestor");
  }
  return Object.freeze({
    kind: PRODUCTION_TAG_BASELINE_KIND,
    tag,
    sourceCommit: baselineCommit,
    workflowPath: null,
    runId: null,
    runAttempt: null,
    ledgerReportSha256: null,
  });
}

function validateAcceptedLedgerBaseline(value, sourceCommit, query) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("accepted component ledger baseline is invalid");
  }
  const keys = Object.keys(value).sort();
  const expectedKeys = [
    "kind", "ledgerReportSha256", "runAttempt", "runId",
    "sourceCommit", "workflowPath",
  ].sort();
  if (JSON.stringify(keys) !== JSON.stringify(expectedKeys)) {
    throw new Error("accepted component ledger baseline fields are not exact");
  }
  if (value.kind !== ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND ||
      !SOURCE_COMMIT.test(value.sourceCommit ?? "") ||
      !ACCEPTED_LEDGER_WORKFLOW.test(value.workflowPath ?? "") ||
      !DECIMAL_ID.test(String(value.runId ?? "")) ||
      String(value.runAttempt ?? "") !== "1" ||
      !SHA256.test(value.ledgerReportSha256 ?? "")) {
    throw new Error("accepted component ledger baseline authority is invalid");
  }
  if (value.sourceCommit === sourceCommit) {
    throw new Error("accepted component ledger baseline must precede the candidate");
  }
  if (query(["rev-parse", `${value.sourceCommit}^{commit}`]) !== value.sourceCommit) {
    throw new Error("accepted component ledger baseline does not resolve exactly");
  }
  if (query(["merge-base", value.sourceCommit, sourceCommit]) !== value.sourceCommit) {
    throw new Error("accepted component ledger baseline is not a candidate ancestor");
  }
  return Object.freeze({
    kind: value.kind,
    tag: null,
    sourceCommit: value.sourceCommit,
    workflowPath: value.workflowPath,
    runId: String(value.runId),
    runAttempt: "1",
    ledgerReportSha256: value.ledgerReportSha256,
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

function isLocalOnlyOperationalPath(path) {
  return path.startsWith("scripts/windows/");
}

function isTestSnapshotPath(path) {
  return (
    path.startsWith("snapshots/") ||
    path.startsWith("test-snapshots/") ||
    path.includes("/__snapshots__/")
  );
}

function isPerformanceSensitivePath(path) {
  return (
    path.startsWith("core-c/") ||
    path.startsWith("scripts/benchmark/") ||
    path.startsWith("apps/clearra-web/src/workers/") ||
    /^apps\/clearra-discord-bot\/src\/job-service\/(?:runner|server)\.mjs$/u.test(path) ||
    /(?:^|\/)(?:benchmark|benchmarks|performance|profiling)(?:\/|[._-])/u.test(path)
  );
}

function isDiscordGatewayOnlyPath(path) {
  return (
    path.startsWith("apps/clearra-discord-bot/src/discord/") ||
    path.startsWith("apps/clearra-discord-bot/src/ingress/") ||
    path.startsWith("apps/clearra-discord-bot/src/viewer/") ||
    path.startsWith("apps/clearra-discord-bot/src/admin/") ||
    [
      "apps/clearra-discord-bot/src/bot.mjs",
      "apps/clearra-discord-bot/src/main.mjs",
      "apps/clearra-discord-bot/src/register-commands.mjs",
      "apps/clearra-discord-bot/cloudbuild-command-sync.yaml",
    ].includes(path) ||
    /^apps\/clearra-discord-bot\/scripts\/(?:capture-oracle|classify-oracle|discord-command|observe-oracle|oracle-|produce-oracle|register-commands|restore-oracle|verify-oracle)/u.test(path)
  );
}

function isHeavyCloudOnlyPath(path) {
  return (
    path.startsWith("apps/clearra-discord-bot/src/job-service/") ||
    path.startsWith("apps/clearra-discord-bot/src/cloud-run/") ||
    /^apps\/clearra-discord-bot\/(?:Dockerfile\.|cloudbuild-(?:current-)?job-service\.yaml$)/u.test(path) ||
    /^apps\/clearra-discord-bot\/scripts\/(?:prepare-cloud|run-cloud|verify-cloud)/u.test(path)
  );
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
    "scripts/tools/clearra-wasm-generation-retention.mjs",
    "scripts/tools/stage-clearra-wasm.mjs",
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
  const allowed = new Set([
    "--source-commit", "--format", "--baseline-kind",
    "--baseline-source-commit", "--baseline-workflow-path",
    "--baseline-run-id", "--baseline-run-attempt",
    "--baseline-ledger-report-sha256",
  ]);
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
  const baselineOptions = [
    "--baseline-kind", "--baseline-source-commit", "--baseline-workflow-path",
    "--baseline-run-id", "--baseline-run-attempt",
    "--baseline-ledger-report-sha256",
  ];
  const suppliedBaselineOptions = baselineOptions.filter((option) =>
    Object.hasOwn(values, option));
  if (suppliedBaselineOptions.length !== 0 &&
      suppliedBaselineOptions.length !== baselineOptions.length) {
    throw new Error("accepted component ledger baseline options must be complete");
  }
  return values;
}

function main() {
  const values = parseArguments(process.argv.slice(2));
  const sourceCommit = values["--source-commit"];
  const baseline = values["--baseline-kind"] === undefined ? undefined : {
    kind: values["--baseline-kind"],
    sourceCommit: values["--baseline-source-commit"],
    workflowPath: values["--baseline-workflow-path"],
    runId: values["--baseline-run-id"],
    runAttempt: values["--baseline-run-attempt"],
    ledgerReportSha256: values["--baseline-ledger-report-sha256"],
  };
  const analysis = analyzeDeploymentImpact(sourceCommit, { baseline });
  const impact = analysis.impact;
  const result = {
    source_commit: sourceCommit,
    baseline_kind: analysis.baselineKind,
    baseline_tag: analysis.baselineTag,
    baseline_commit: analysis.baselineCommit,
    baseline_source_commit: analysis.baselineSourceCommit,
    baseline_workflow_path: analysis.baselineWorkflowPath,
    baseline_run_id: analysis.baselineRunId,
    baseline_run_attempt: analysis.baselineRunAttempt,
    baseline_ledger_report_sha256: analysis.baselineLedgerReportSha256,
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
    performance_sensitive_changed: impact.performanceSensitiveChanged,
    requires_full_gate: impact.requiresFullGate,
    gate_mode: impact.gateMode,
    changed_components: impact.changedComponents,
    carry_forward_components: impact.carryForwardComponents,
    changed_paths_sha256: impact.changedPathsSha256,
  };
  result.impact_plan_sha256 = canonicalSha256({
    source_commit: result.source_commit,
    baseline_kind: result.baseline_kind,
    baseline_tag: result.baseline_tag,
    baseline_commit: result.baseline_commit,
    baseline_source_commit: result.baseline_source_commit,
    baseline_workflow_path: result.baseline_workflow_path,
    baseline_run_id: result.baseline_run_id,
    baseline_run_attempt: result.baseline_run_attempt,
    baseline_ledger_report_sha256: result.baseline_ledger_report_sha256,
    gate_mode: result.gate_mode,
    requires_full_gate: result.requires_full_gate,
    changed_components: result.changed_components,
    carry_forward_components: result.carry_forward_components,
    changed_paths_sha256: result.changed_paths_sha256,
  });
  if (values["--format"] === "github-output") {
    for (const [key, value] of Object.entries(result)) {
      const outputKey = Array.isArray(value) ? `${key}_json` : key;
      const outputValue = Array.isArray(value) ? JSON.stringify(value) : value;
      process.stdout.write(`${outputKey}=${outputValue}\n`);
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
