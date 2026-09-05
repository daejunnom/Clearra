import assert from "node:assert/strict";
import test from "node:test";

import {
  ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
  analyzeDeploymentImpact,
  classifyDeploymentImpact,
  PRODUCTION_TAG_BASELINE_KIND,
  selectProductionBaseline,
} from "./deployment-impact.mjs";

test("Pages-only changes select Pages without Discord, GUI desktop, or CLI", () => {
  const impact = classifyDeploymentImpact([
    "apps/clearra-web/src/routes/+page.svelte",
    "apps/clearra-web/src/routes/page.test.ts",
  ]);
  assert.equal(impact.scope, "pages");
  assert.equal(impact.deployPages, true);
  assert.equal(impact.deployDiscord, false);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);
  assert.equal(impact.gateMode, "focused");
  assert.equal(impact.requiresFullGate, false);
  assert.deepEqual(impact.changedComponents, ["pages"]);
});

test("Discord-only changes do not select Pages, GUI, or CLI", () => {
  const impact = classifyDeploymentImpact([
    "apps/clearra-discord-bot/src/discord/path-command.mjs",
  ]);
  assert.equal(impact.scope, "discord");
  assert.equal(impact.deployPages, false);
  assert.equal(impact.deployDiscord, true);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);
  assert.equal(impact.gateMode, "full");
  assert.equal(impact.requiresFullGate, true);
});

test("docs and tests only select no deployment surface", () => {
  const impact = classifyDeploymentImpact([
    "docs/release.md",
    "tests/contracts/example.test.mjs",
    "apps/clearra-discord-bot/test/example.test.mjs",
  ]);
  assert.equal(impact.scope, "none");
  assert.equal(impact.deployPages, false);
  assert.equal(impact.deployDiscord, false);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);
  assert.equal(impact.gateMode, "none");
  assert.equal(impact.requiresFullGate, false);
  assert.equal(impact.carryForwardComponents.length, 8);
});

test("local watchdog and test snapshots are qualification-only no-product changes", () => {
  const impact = classifyDeploymentImpact([
    "scripts/windows/clearra-local-services-watchdog.ps1",
    "scripts/windows/install-clearra-local-services-watchdog.ps1",
    "snapshots/gui/pc-minimals.snap",
    "packages/clearra-ui/test/__snapshots__/pc-minimals.snap",
  ]);
  assert.equal(impact.scope, "none");
  assert.equal(impact.componentScope, "none");
  assert.equal(impact.gateMode, "none");
  assert.equal(impact.deployPages, false);
  assert.equal(impact.deployDiscord, false);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);

  const runtimeSnapshot = classifyDeploymentImpact([
    "apps/clearra-web/static/runtime-state.snap",
  ]);
  assert.equal(runtimeSnapshot.gateMode, "focused");
  assert.equal(runtimeSnapshot.deployPages, true);
});

test("an empty direct path set is an explicit all-surface no-op", () => {
  const impact = classifyDeploymentImpact([]);
  assert.equal(impact.scope, "none");
  assert.equal(impact.deployPages, false);
  assert.equal(impact.deployDiscord, false);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);
});

test("Discord deployment workflow changes conservatively select their runtime and infrastructure", () => {
  const impact = classifyDeploymentImpact([
    ".github/workflows/discord-deploy.yml",
    ".github/workflows/discord-deploy-recovery.yml",
  ]);
  assert.equal(impact.scope, "discord");
  assert.equal(impact.deployPages, false);
  assert.equal(impact.deployDiscord, true);
  assert.equal(impact.deployGui, false);
  assert.equal(impact.deployCli, false);
  assert.equal(impact.deployDiscordGateway, true);
  assert.equal(impact.deployHeavyCloudRuntime, true);
  assert.equal(impact.releaseInfrastructureChanged, true);
});

test("shared core and release authority changes conservatively select every surface", () => {
  for (const path of [
    "crates/clearra-app/src/lib.rs",
    "core-c/src/search.c",
    "scripts/release/canonical-acceptance-run.mjs",
    ".github/workflows/release-cli.yml",
    "package.json",
  ]) {
    const impact = classifyDeploymentImpact([path]);
    assert.equal(impact.scope, "shared", path);
    assert.equal(impact.deployPages, true, path);
    assert.equal(impact.deployDiscord, true, path);
    assert.equal(impact.deployGui, true, path);
    assert.equal(impact.deployCli, true, path);
    assert.equal(impact.gateMode, "full", path);
    assert.equal(impact.requiresFullGate, true, path);
  }
});

test("cross-product contracts and release infrastructure automatically promote to full gate", () => {
  for (const path of [
    "packages/clearra-ui/src/lib/workspace/WorkspaceShell.svelte",
    "packages/ctk3/src/operationGeometry.ts",
    ".github/workflows/pages.yml",
  ]) {
    const impact = classifyDeploymentImpact([path]);
    assert.equal(impact.gateMode, "full", path);
    assert.equal(impact.requiresFullGate, true, path);
  }
});

test("single-product performance-sensitive runtime paths automatically promote to full gate", () => {
  for (const path of [
    "apps/clearra-web/src/workers/ClearraVerifierPool.ts",
    "apps/clearra-discord-bot/src/job-service/runner.mjs",
  ]) {
    const impact = classifyDeploymentImpact([path]);
    assert.equal(impact.performanceSensitiveChanged, true, path);
    assert.equal(impact.requiresFullGate, true, path);
    assert.equal(impact.gateMode, "full", path);
  }
});

test("v0.8 fast qualification keeps only Pages and no-product scopes", () => {
  const pages = classifyDeploymentImpact([
    "apps/clearra-web/src/routes/+page.svelte",
  ]);
  assert.equal(pages.gateMode, "focused");
  assert.deepEqual(pages.changedComponents, ["pages"]);

  const gui = classifyDeploymentImpact(["apps/clearra-desktop/src/main.ts"]);
  assert.equal(gui.gateMode, "full");
  assert.deepEqual(gui.changedComponents, ["desktop_gui"]);

  const discord = classifyDeploymentImpact([
    "apps/clearra-discord-bot/src/discord/path-command.mjs",
  ]);
  assert.equal(discord.gateMode, "full");
  assert.deepEqual(discord.changedComponents, [
    "discord_gateway",
  ]);

  const cli = classifyDeploymentImpact(["apps/clearra-cli/src/main.rs"]);
  assert.equal(cli.gateMode, "full");
  assert.deepEqual(cli.changedComponents, ["cli"]);
});

test("Discord gateway and heavy Cloud impact stays distinct but promotes to full", () => {
  const gateway = classifyDeploymentImpact([
    "apps/clearra-discord-bot/src/discord/command-registration.mjs",
  ]);
  assert.equal(gateway.deployDiscord, true);
  assert.equal(gateway.deployDiscordGateway, true);
  assert.equal(gateway.deployHeavyCloudRuntime, false);
  assert.equal(gateway.gateMode, "full");

  const cloud = classifyDeploymentImpact([
    "apps/clearra-discord-bot/src/job-service/config.mjs",
  ]);
  assert.equal(cloud.deployDiscord, true);
  assert.equal(cloud.deployDiscordGateway, false);
  assert.equal(cloud.deployHeavyCloudRuntime, true);
  assert.equal(cloud.gateMode, "full");
});

test("GUI and CLI ownership stay independent while CLI core also reaches Discord", () => {
  const desktop = classifyDeploymentImpact(["apps/clearra-desktop/src/main.ts"]);
  assert.equal(desktop.scope, "gui");
  assert.equal(desktop.deployGui, true);
  assert.equal(desktop.deployCli, false);
  assert.equal(desktop.deployDiscord, false);

  const cli = classifyDeploymentImpact(["apps/clearra-cli/src/main.rs"]);
  assert.equal(cli.scope, "cli");
  assert.equal(cli.deployCli, true);
  assert.equal(cli.deployGui, false);
  assert.equal(cli.deployDiscord, false);

  const sharedCli = classifyDeploymentImpact(["crates/clearra-cli/src/lib.rs"]);
  assert.equal(sharedCli.scope, "discord+cli");
  assert.equal(sharedCli.deployCli, true);
  assert.equal(sharedCli.deployDiscord, true);
  assert.equal(sharedCli.deployPages, false);
});

test("Rust CTK bitmap rendering reaches every runtime consumer, including Discord's Rust CLI image", () => {
  for (const path of [
    "crates/clearra-ctk3/src/geometry.rs",
    "crates/clearra-output/src/render.rs",
    "crates/clearra-render/src/bitmap/render_board.rs",
  ]) {
    const impact = classifyDeploymentImpact([path]);
    assert.equal(impact.scope, "pages+discord+gui+cli", path);
    assert.equal(impact.deployPages, true, path);
    assert.equal(impact.deployGui, true, path);
    assert.equal(impact.deployCli, true, path);
    assert.equal(impact.deployDiscord, true, path);
  }
});

test("browser UI-only source stays off the Discord runtime", () => {
  const impact = classifyDeploymentImpact([
    "packages/clearra-ui/src/lib/workspace/WorkspaceShell.svelte",
  ]);
  assert.equal(impact.scope, "pages+gui");
  assert.equal(impact.deployPages, true);
  assert.equal(impact.deployGui, true);
  assert.equal(impact.deployCli, false);
  assert.equal(impact.deployDiscord, false);
});

test("TypeScript CTK package reaches Pages, desktop GUI, and Discord but not the Rust CLI", () => {
  const impact = classifyDeploymentImpact([
    "packages/ctk3/src/operationGeometry.ts",
  ]);
  assert.equal(impact.scope, "pages+discord+gui");
  assert.equal(impact.deployPages, true);
  assert.equal(impact.deployGui, true);
  assert.equal(impact.deployDiscord, true);
  assert.equal(impact.deployCli, false);
});

test("runtime-consumer exceptions select the exact deployment surfaces", () => {
  for (const expected of [
    {
      path: "apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin",
      scope: "pages+discord+cli",
      pages: true,
      discord: true,
      gui: false,
      cli: true,
    },
    {
      path: "crates/clearra-wasm/src/wasm_command_runtime.rs",
      scope: "pages+gui",
      pages: true,
      discord: false,
      gui: true,
      cli: false,
    },
    {
      path: "crates/clearra-wasm-abi/src/lib.rs",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: "crates/clearra-cli-command/src/web_command_parser.rs",
      scope: "pages+discord+gui+cli",
      pages: true,
      discord: true,
      gui: true,
      cli: true,
    },
    {
      path: ".github/workflows/pages.yml",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: ".github/workflows/pages-rollback.yml",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: "scripts/tools/build-clearra-wasm.mjs",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: "scripts/tools/clearra-wasm-build-contract.mjs",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: "scripts/tools/clearra-wasm-generation-retention.mjs",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
    {
      path: "scripts/tools/stage-clearra-wasm.mjs",
      scope: "pages",
      pages: true,
      discord: false,
      gui: false,
      cli: false,
    },
  ]) {
    const impact = classifyDeploymentImpact([expected.path]);
    assert.equal(impact.scope, expected.scope, expected.path);
    assert.equal(impact.deployPages, expected.pages, expected.path);
    assert.equal(impact.deployDiscord, expected.discord, expected.path);
    assert.equal(impact.deployGui, expected.gui, expected.path);
    assert.equal(impact.deployCli, expected.cli, expected.path);
  }
});

test("component deployment vector distinguishes desktop, Discord, PC4, and release infrastructure", () => {
  const tablebase = classifyDeploymentImpact([
    "apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin",
  ]);
  assert.equal(tablebase.deployPages, true);
  assert.equal(tablebase.deployDesktopGui, false);
  assert.equal(tablebase.deployDiscordGateway, false);
  assert.equal(tablebase.deployHeavyCloudRuntime, true);
  assert.equal(tablebase.deployPc4LookupService, false);
  assert.equal(tablebase.deployPc4ActivationManifest, true);
  assert.equal(tablebase.releaseInfrastructureChanged, false);

  const releaseWorkflow = classifyDeploymentImpact([".github/workflows/release-cli.yml"]);
  assert.equal(releaseWorkflow.scope, "shared");
  assert.equal(releaseWorkflow.releaseInfrastructureChanged, true);
  assert.equal(releaseWorkflow.deployPc4LookupService, true);
  assert.match(releaseWorkflow.componentScope, /release_infrastructure/u);
});

test("runtime README deletion is not mistaken for documentation-only impact", () => {
  const impact = classifyDeploymentImpact(["apps/clearra-discord-bot/README.md"]);
  assert.equal(impact.scope, "discord");
  assert.equal(impact.deployDiscord, true);
});

test("latest reachable production semver tag is the deployment baseline", () => {
  assert.equal(selectProductionBaseline(["v0.7.5", "v0.8.0", "candidate-x", "v0.7.12"]), "v0.8.0");
  assert.throws(() => selectProductionBaseline(["candidate-x"]), /no reachable/u);
});

test("a production tag on the exact source becomes a self-diff no-op baseline", () => {
  const source = "1".repeat(40);
  const prior = "2".repeat(40);
  const query = (arguments_) => {
    const key = arguments_.join(" ");
    if (key === `rev-parse ${source}^{commit}`) return source;
    if (key === `tag --merged ${source} --list v[0-9]*.[0-9]*.[0-9]*`) {
      return "v0.7.5\nv0.8.0";
    }
    if (key === "rev-list -n 1 v0.8.0") return source;
    if (key === "rev-list -n 1 v0.7.5") return prior;
    if (key === `merge-base ${source} ${source}`) return source;
    if (key === `diff --name-only --diff-filter=ACMRD ${source}..${source} --`) return "";
    throw new Error(`unexpected git query: ${key}`);
  };
  const analysis = analyzeDeploymentImpact(source, query);
  assert.equal(analysis.baselineKind, PRODUCTION_TAG_BASELINE_KIND);
  assert.equal(analysis.baselineTag, "v0.8.0");
  assert.equal(analysis.baselineCommit, source);
  assert.equal(analysis.impact.scope, "none");
});

test("a pre-tag allow-empty source intentionally uses cumulative production impact", () => {
  const source = "3".repeat(40);
  const prior = "2".repeat(40);
  const query = (arguments_) => {
    const key = arguments_.join(" ");
    if (key === `rev-parse ${source}^{commit}`) return source;
    if (key === `tag --merged ${source} --list v[0-9]*.[0-9]*.[0-9]*`) return "v0.7.5";
    if (key === "rev-list -n 1 v0.7.5") return prior;
    if (key === `merge-base ${prior} ${source}`) return prior;
    if (key === `diff --name-only --diff-filter=ACMRD ${prior}..${source} --`) {
      return "apps/clearra-discord-bot/src/bot.mjs";
    }
    throw new Error(`unexpected git query: ${key}`);
  };
  const analysis = analyzeDeploymentImpact(source, query);
  assert.equal(analysis.baselineKind, PRODUCTION_TAG_BASELINE_KIND);
  assert.equal(analysis.baselineTag, "v0.7.5");
  assert.equal(analysis.impact.scope, "discord");
  assert.equal(analysis.impact.deployDiscord, true);
  assert.equal(analysis.impact.deployPages, false);
});

test("production-tag impact rejects a non-ancestor even if tag discovery returns it", () => {
  const source = "4".repeat(40);
  const ancestor = "2".repeat(40);
  const divergent = "3".repeat(40);
  const query = (arguments_) => {
    const key = arguments_.join(" ");
    if (key === `rev-parse ${source}^{commit}`) return source;
    if (key === `tag --merged ${source} --list v[0-9]*.[0-9]*.[0-9]*`) {
      return "v0.7.4\nv9.0.0";
    }
    if (key === "rev-list -n 1 v9.0.0") return divergent;
    if (key === `merge-base ${divergent} ${source}`) return ancestor;
    throw new Error(`unexpected git query: ${key}`);
  };
  assert.throws(
    () => analyzeDeploymentImpact(source, query),
    /production tag baseline is not a source ancestor/u,
  );
});

test("accepted ledger baseline replaces the cumulative production-tag diff", () => {
  const source = "4".repeat(40);
  const ledgerSource = "3".repeat(40);
  const tagSource = "2".repeat(40);
  const baseline = {
    kind: ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
    sourceCommit: ledgerSource,
    workflowPath: ".github/workflows/fast-fix-production-finalizer.yml",
    runId: "303",
    runAttempt: "1",
    ledgerReportSha256: "a".repeat(64),
  };
  const query = (arguments_) => {
    const key = arguments_.join(" ");
    if (key === `rev-parse ${source}^{commit}`) return source;
    if (key === `rev-parse ${ledgerSource}^{commit}`) return ledgerSource;
    if (key === `merge-base ${ledgerSource} ${source}`) return ledgerSource;
    if (key === `diff --name-only --diff-filter=ACMRD ${ledgerSource}..${source} --`) {
      return "docs/follow-up.md";
    }
    if (key === `diff --name-only --diff-filter=ACMRD ${tagSource}..${source} --`) {
      throw new Error("must not use cumulative production-tag diff");
    }
    throw new Error(`unexpected git query: ${key}`);
  };
  const analysis = analyzeDeploymentImpact(source, { baseline, query });
  assert.equal(analysis.baselineKind, ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND);
  assert.equal(analysis.baselineSourceCommit, ledgerSource);
  assert.equal(analysis.baselineTag, null);
  assert.equal(analysis.baselineLedgerReportSha256, "a".repeat(64));
  assert.equal(analysis.impact.gateMode, "none");
});

test("accepted ledger baseline must be complete, attempt 1, and an ancestor", () => {
  const source = "4".repeat(40);
  const ledgerSource = "3".repeat(40);
  const base = {
    kind: ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
    sourceCommit: ledgerSource,
    workflowPath: ".github/workflows/component-ledger-bootstrap.yml",
    runId: "303",
    runAttempt: "1",
    ledgerReportSha256: "b".repeat(64),
  };
  const query = (arguments_) => {
    const key = arguments_.join(" ");
    if (key === `rev-parse ${source}^{commit}`) return source;
    if (key === `rev-parse ${ledgerSource}^{commit}`) return ledgerSource;
    if (key === `merge-base ${ledgerSource} ${source}`) return "1".repeat(40);
    throw new Error(`unexpected git query: ${key}`);
  };
  assert.throws(
    () => analyzeDeploymentImpact(source, { baseline: base, query }),
    /not a candidate ancestor/u,
  );
  assert.throws(
    () => analyzeDeploymentImpact(source, {
      baseline: { ...base, runAttempt: "2" },
      query,
    }),
    /authority is invalid/u,
  );
  const { ledgerReportSha256: ignored, ...incomplete } = base;
  void ignored;
  assert.throws(
    () => analyzeDeploymentImpact(source, { baseline: incomplete, query }),
    /fields are not exact/u,
  );
});
