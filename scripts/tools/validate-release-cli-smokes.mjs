import { readFile } from "node:fs/promises";

const TERMINAL_SUPPLY_MODE = "--validate-terminal-supply-json";
const DISCORD_SCORE_MINIMALS_MODE =
  "--validate-discord-score-minimals-json";
const TERMINAL_SUPPLY_EXPECTED_COUNT = 18;
const TERMINAL_SUPPLY_EXPECTED_HASH = "cts1:8a7fc484d9b49994";
const TERMINAL_SUPPLY_INITIAL_MASK = 0x1c0701c07n;
const TERMINAL_SUPPLY_FULL_FIELD = (1n << 40n) - 1n;
const CANONICAL_PIECE_ORDER = Object.freeze([
  "I",
  "O",
  "T",
  "S",
  "Z",
  "J",
  "L",
]);
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const U64_MASK = (1n << 64n) - 1n;

const commandArguments = process.argv.slice(2);
if (commandArguments.length > 0) {
  if (commandArguments.length === 1 &&
      commandArguments[0] === DISCORD_SCORE_MINIMALS_MODE) {
    const raw = await readStandardInput();
    const structured = JSON.parse(raw);
    const {
      discordPcScoreMinimalsResultProjection,
      validDiscordPcScoreMinimalsResult,
    } = await import(new URL(
      "../../apps/clearra-discord-bot/src/discord/pc-score-minimals-result.mjs",
      import.meta.url,
    ));
    if (!validDiscordPcScoreMinimalsResult(structured)) {
      throw new Error(
        "release CLI score-minimals JSON is not a valid Discord canonical result",
      );
    }
    const projection = discordPcScoreMinimalsResultProjection(structured);
    if (projection === null) {
      throw new Error("Discord score-minimals projection is unavailable");
    }
    console.log(
      `Discord score-minimals release asset JSON passed: candidate=${projection.canonicalCandidateId}`,
    );
    process.exit(0);
  } else if (
    commandArguments.length === 3 &&
    commandArguments[0] === TERMINAL_SUPPLY_MODE &&
    commandArguments[1] === "--expected-source-commit" &&
    /^[0-9a-f]{40}$/u.test(commandArguments[2])
  ) {
    const raw = await readStandardInput();
    const evidence = validateTerminalSupplyJson(raw, commandArguments[2]);
    console.log(
      `Terminal-supply release asset JSON passed: solutions=${evidence.solutionCount} hash=${evidence.normalizedSetHash}`,
    );
    process.exit(0);
  }
  throw new Error(
    `unsupported release CLI smoke validator arguments: ${commandArguments.join(" ")}`,
  );
}

const root = new URL("../../", import.meta.url);
const packageScript = await readFile(
  new URL("scripts/tools/package-release-cli.sh", root),
  "utf8",
);
const workflow = await readFile(
  new URL(".github/workflows/release-cli.yml", root),
  "utf8",
);
const pagesWorkflow = await readFile(
  new URL(".github/workflows/pages.yml", root),
  "utf8",
);
const pagesRollbackAuthoritySource = (await readFile(
  new URL("scripts/release/pages-rollback-authority.mjs", root),
  "utf8",
)).replaceAll("\r\n", "\n");
const pagesLegacyContractSource = (await readFile(
  new URL("scripts/release/pages-legacy-contract.mjs", root),
  "utf8",
)).replaceAll("\r\n", "\n");
const pagesDeploymentAuthoritySource = (await readFile(
  new URL("scripts/release/pages-deployment-authority.mjs", root),
  "utf8",
)).replaceAll("\r\n", "\n");

const triggerSection = section(workflow, "\non:", "\npermissions:");
const workflowPermissions = section(workflow, "\npermissions:", "\nconcurrency:");
const workflowConcurrency = section(workflow, "\nconcurrency:", "\nenv:");
const workflowEnvironment = section(workflow, "\nenv:", "\njobs:");
const ctk3Job = section(workflow, "\n  ctk3:", "\n  linux-cli:");
const linuxJob = section(workflow, "\n  linux-cli:", "\n  discord-bot:");
const discordJob = section(
  workflow,
  "\n  discord-bot:",
  "\n  release-acceptance-foundation-no-product-debt:",
);
const metadataJob = section(workflow, "\n  metadata:", "\n  ctk3:");
const releaseAcceptanceFoundationNoProductDebtJob = section(
  workflow,
  "\n  release-acceptance-foundation-no-product-debt:",
  "\n  release-acceptance-foundation-adversarial-correctness:",
);
const releaseAcceptanceFoundationAdversarialCorrectnessJob = section(
  workflow,
  "\n  release-acceptance-foundation-adversarial-correctness:",
  "\n  release-acceptance-foundation-desktop-host:",
);
const releaseAcceptanceFoundationDesktopHostJob = section(
  workflow,
  "\n  release-acceptance-foundation-desktop-host:",
  "\n  release-acceptance-sanitizer:",
);
const releaseAcceptanceFoundationJob = releaseAcceptanceFoundationNoProductDebtJob;
const releaseAcceptanceSanitizerJob = section(
  workflow,
  "\n  release-acceptance-sanitizer:",
  "\n  release-acceptance-rust:",
);
const releaseAcceptanceRustJob = section(
  workflow,
  "\n  release-acceptance-rust:",
  "\n  release-acceptance-pages:",
);
const releaseAcceptancePagesJob = section(
  workflow,
  "\n  release-acceptance-pages:",
  "\n  release-acceptance:",
);
const releaseAcceptanceJob = section(
  workflow,
  "\n  release-acceptance:",
  "\n  windows-cli:",
);
const windowsCliJob = section(
  workflow,
  "\n  windows-cli:",
  "\n  windows-gui:",
);
const windowsGuiJob = section(
  workflow,
  "\n  windows-gui:",
  "\n  canonical-evidence:",
);
const canonicalEvidenceJob = section(
  workflow,
  "\n  canonical-evidence:",
  "\n  publish:",
);
const publishJob = finalJobSection(workflow, "\n  publish:");
const publishHeader = section(publishJob, "\n  publish:", "\n    steps:");
const acceptedRunStep = section(
  workflow,
  "\n      - name: Bind the exact canonical acceptance run",
  "\n\n  ctk3:",
);
const canonicalAcceptancePreflightStep = section(
  metadataJob,
  "\n      - name: Require exact main and zero prior canonical success",
  "\n      - name: Prepare approved legacy v0.7.4 annotated-tag fixture",
);
const crossRunDownloadStep = section(
  publishJob,
  "\n      - uses: actions/download-artifact@v4",
  "\n      - name: Download canonical acceptance evidence",
);
const publishReleaseStep = publishJob.slice(
  publishJob.indexOf("\n      - name: Publish GitHub Release"),
);
const linuxArchiveRegressionStep = section(
  metadataJob,
  "\n      - name: Validate independent release regressions with bounded workers",
  "\n      - name: Archive the exact accepted source on Linux",
);
const linuxLegacyTagFixtureStep = section(
  metadataJob,
  "\n      - name: Prepare approved legacy v0.7.4 annotated-tag fixture",
  "\n      - name: Validate independent release regressions with bounded workers",
);
const linuxAcceptedArchiveStep = section(
  metadataJob,
  "\n      - name: Archive the exact accepted source on Linux",
  "\n      - name: Resolve release version",
);
const windowsAcceptedArchiveStep = section(
  releaseAcceptanceFoundationJob,
  "\n      - name: Archive the exact accepted source on Windows",
  "\n      - id: release_toolchain_cache",
);
const linuxProtectedPrelude = section(
  metadataJob,
  "\n    steps:",
  "\n      - name: Resolve release version",
);
const windowsProtectedPrelude = section(
  releaseAcceptanceFoundationJob,
  "\n    steps:",
  "\n      - id: release_toolchain_cache",
);
const linuxCheckoutStep = section(
  linuxProtectedPrelude,
  "\n      - uses: actions/checkout@v4",
  "\n      - uses: actions/setup-node@v4",
);
const linuxSetupNodeStep = section(
  linuxProtectedPrelude,
  "\n      - uses: actions/setup-node@v4",
  "\n      - name: Require exact main and zero prior canonical success",
);
const windowsCheckoutStep = section(
  windowsProtectedPrelude,
  "\n      - uses: actions/checkout@v4",
  "\n      - uses: actions/setup-node@v4",
);
const windowsSetupNodeStep = section(
  windowsProtectedPrelude,
  "\n      - uses: actions/setup-node@v4",
  "\n      - name: Archive the exact accepted source on Windows",
);
const ctk3UploadStep = ctk3Job.slice(
  ctk3Job.indexOf("\n      - name: Upload accepted CTK3 distribution"),
);
const discordDownloadStep = section(
  discordJob,
  "\n      - name: Download accepted CTK3 distribution",
  "\n      - name: Install JavaScript workspace",
);
const releaseAcceptanceDownloadStep = section(
  releaseAcceptanceRustJob,
  "\n      - name: Download accepted CTK3 distribution",
  "\n      - id: release_toolchain_cache",
);
const releaseAcceptanceRunStep = section(
  releaseAcceptanceRustJob,
  "\n      - name: Run canonical release acceptance rust shard",
  "\n      - name: Seal canonical release acceptance rust shard",
);
const releaseAcceptancePagesRunStep = section(
  releaseAcceptancePagesJob,
  "\n      - name: Run canonical release acceptance Pages shard",
  "\n      - name: Stamp and verify the accepted Pages build",
);
const acceptedPagesStampStep = section(
  releaseAcceptancePagesJob,
  "\n      - name: Stamp and verify the accepted Pages build",
  "\n      - name: Seal canonical release acceptance Pages shard",
);
const acceptedPagesUploadStep = section(
  releaseAcceptancePagesJob,
  "\n      - name: Upload accepted Pages build",
  "\n      - name: Upload canonical release acceptance Pages shard",
);
const releaseGateEvidenceStep = section(
  releaseAcceptanceJob,
  "\n      - name: Produce canonical release gate evidence",
  "\n      - name: Upload canonical release gate evidence",
);
const releaseGateUploadStep = releaseAcceptanceJob.slice(
  releaseAcceptanceJob.indexOf("\n      - name: Upload canonical release gate evidence"),
);
const pagesAcceptedSourceJob = section(
  pagesWorkflow,
  "\n  accepted-source:",
  "\n  build:",
);
const publishEvidenceDownloadStep = section(
  publishJob,
  "\n      - name: Download canonical acceptance evidence",
  "\n      - name: Publish GitHub Release",
);
const pagesBuildJob = section(pagesWorkflow, "\n  build:", "\n  deploy:");
const pagesDeployJob = finalJobSection(pagesWorkflow, "\n  deploy:");
const pagesAcceptedRunStep = section(
  pagesAcceptedSourceJob,
  "\n      - name: Verify exact main and canonical acceptance identity",
  "\n      - name: Resolve sealed rollback capture report before Pages build",
);
const pagesRollbackReportResolveStep = section(
  pagesAcceptedSourceJob,
  "\n      - name: Resolve sealed rollback capture report before Pages build",
  "\n      - name: Download sealed rollback capture report before Pages build",
);
const pagesRollbackAuthorityStep = section(
  pagesAcceptedSourceJob,
  "\n      - name: Verify durable rollback capture before Pages build",
  "\n      - name: Download durable rollback capture before Pages build",
);
const pagesDownloadStep = section(
  pagesBuildJob,
  "\n      - name: Download exact accepted Pages build",
  "\n      - name: Configure Pages",
);
const pagesVerifyStep = section(
  pagesBuildJob,
  "\n      - name: Verify exact accepted Pages build",
  "\n      - name: Upload Pages artifact",
);
const pagesVerifyEnvironment = section(
  pagesVerifyStep,
  "\n        env:",
  "\n        run:",
);
const pagesUploadStep = pagesBuildJob.slice(
  pagesBuildJob.indexOf("\n      - name: Upload Pages artifact"),
);
const pagesDeployHeader = section(pagesDeployJob, "\n  deploy:", "\n    steps:");
const pagesDeployPermissions = section(
  pagesDeployHeader,
  "\n    permissions:",
  "\n    environment:",
);
const pagesAcceptedSourcePermissions = section(
  pagesAcceptedSourceJob,
  "\n    permissions:",
  "\n    outputs:",
);
const pagesLateAcceptedRunStep = section(
  pagesDeployJob,
  "\n      - name: Revalidate accepted source immediately before deployment",
  "\n      - name: Redownload durable rollback capture immediately before deployment",
);
const pagesDeploymentAuthorityStep = section(
  pagesDeployJob,
  "\n      - name: Seal deployed Pages authority from API and public readback",
  "\n      - name: Upload sealed Pages deployment authority",
);
const pagesDeploymentAuthorityUploadStep = pagesDeployJob.slice(
  pagesDeployJob.indexOf("\n      - name: Upload sealed Pages deployment authority"),
);
const productAuthorityStep = section(
  workflow,
  "\n      - name: Require the product capability and alias parser authority",
  "\n\n  release-acceptance-foundation-no-product-debt:",
);
const upstreamAuthorityStep = section(
  metadataJob,
  "\n      - name: Require the upstream drift audit authority",
  "\n      - name: Bind the exact canonical acceptance run",
);
const linuxPcTilingEnforcement = section(
  packageScript,
  '\n            if (Object.hasOwn(expected, "kind") && parsed?.kind !== expected.kind) {',
  "\n            for (const [key, value] of Object.entries(expectedSummary)) {",
);
const windowsPcTilingEnforcement = section(
  windowsCliJob,
  "\n            if ($ExpectedKind -and $parsed.kind -ne $ExpectedKind) {",
  "\n            foreach ($entry in $ExpectedSummary.GetEnumerator()) {",
);
const linuxPcTilingSmoke = section(
  packageScript,
  "\nrun_json_smoke pc-tiling",
  "\nrun_json_smoke failed-queue",
);
const windowsPcTilingSmoke = section(
  windowsCliJob,
  "\n          Invoke-ClearraJsonSmoke -Name 'pc-tiling'",
  "\n          Invoke-ClearraJsonSmoke -Name 'failed-queue'",
);

requireExactNormalizedText(
  productAuthorityStep,
  [
    "",
    "      - name: Require the product capability and alias parser authority",
    "        if: github.event_name == 'workflow_dispatch'",
    "        run: node --test tests/contracts/product_capability_registry.test.mjs",
  ].join("\n"),
  "product capability and alias parser authority step",
);
requireExactNormalizedText(
  upstreamAuthorityStep,
  [
    "",
    "      - name: Require the upstream drift audit authority",
    "        if: github.event_name == 'workflow_dispatch'",
    "        run: node --test scripts/tools/audit-upstream-drift.test.mjs",
  ].join("\n"),
  "upstream drift audit authority step",
);
requireExactNormalizedText(
  linuxArchiveRegressionStep,
  [
    "",
    "      - name: Validate independent release regressions with bounded workers",
    "        if: github.event_name == 'workflow_dispatch'",
    "        shell: bash",
    "        run: node scripts/tools/run-release-regression-tests.mjs",
  ].join("\n"),
  "bounded independent release regression step",
);
requireExactNormalizedText(
  linuxPcTilingEnforcement,
  [
    "",
    '            if (Object.hasOwn(expected, "kind") && parsed?.kind !== expected.kind) {',
    "                throw new Error(",
    "                    `Clearra CLI ${name} smoke expected kind=${JSON.stringify(expected.kind)}, ` +",
    "                    `received ${JSON.stringify(parsed?.kind)}`",
    "                );",
    "            }",
    '            if (Object.hasOwn(expected, "command_kind") &&',
    "                parsed?.contract?.command?.kind !== expected.command_kind) {",
    "                throw new Error(",
    "                    `Clearra CLI ${name} smoke expected contract.command.kind=` +",
    "                    `${JSON.stringify(expected.command_kind)}, received ` +",
    "                    `${JSON.stringify(parsed?.contract?.command?.kind)}`",
    "                );",
    "            }",
    '            if (Object.hasOwn(expected, "tiling_family_complete") &&',
    "                parsed?.contract?.pc?.tiling?.family_complete !==",
    "                    expected.tiling_family_complete) {",
    "                throw new Error(",
    "                    `Clearra CLI ${name} smoke expected contract.pc.tiling.family_complete=` +",
    "                    `${JSON.stringify(expected.tiling_family_complete)}, received ` +",
    "                    `${JSON.stringify(parsed?.contract?.pc?.tiling?.family_complete)}`",
    "                );",
    "            }",
    '            if (Object.hasOwn(expected, "tiling_family_incomplete_reason") &&',
    "                parsed?.contract?.pc?.tiling?.family_incomplete_reason !==",
    "                    expected.tiling_family_incomplete_reason) {",
    "                throw new Error(",
    "                    `Clearra CLI ${name} smoke expected ` +",
    "                    `contract.pc.tiling.family_incomplete_reason=` +",
    "                    `${JSON.stringify(expected.tiling_family_incomplete_reason)}, received ` +",
    "                    `${JSON.stringify(parsed?.contract?.pc?.tiling?.family_incomplete_reason)}`",
    "                );",
    "            }",
  ].join("\n"),
  "Linux typed pc.tiling enforcement body",
);
requireExactNormalizedText(
  windowsPcTilingEnforcement,
  [
    "",
    "            if ($ExpectedKind -and $parsed.kind -ne $ExpectedKind) {",
    '              throw "Clearra CLI $Name smoke expected kind=$ExpectedKind, received $($parsed.kind)"',
    "            }",
    "            if ($ExpectedCommandKind -and",
    "                $parsed.contract.command.kind -ne $ExpectedCommandKind) {",
    '              throw "Clearra CLI $Name smoke expected contract.command.kind=$ExpectedCommandKind, received $($parsed.contract.command.kind)"',
    "            }",
    "            if ($null -ne $ExpectedTilingFamilyComplete -and",
    "                $parsed.contract.pc.tiling.family_complete -ne $ExpectedTilingFamilyComplete) {",
    '              throw "Clearra CLI $Name smoke expected contract.pc.tiling.family_complete=$ExpectedTilingFamilyComplete, received $($parsed.contract.pc.tiling.family_complete)"',
    "            }",
    "            if ($ExpectedTilingFamilyIncompleteReason -and",
    "                $parsed.contract.pc.tiling.family_incomplete_reason -ne $ExpectedTilingFamilyIncompleteReason) {",
    '              throw "Clearra CLI $Name smoke expected contract.pc.tiling.family_incomplete_reason=$ExpectedTilingFamilyIncompleteReason, received $($parsed.contract.pc.tiling.family_incomplete_reason)"',
    "            }",
  ].join("\n"),
  "Windows typed pc.tiling enforcement body",
);

requireText(
  packageScript,
  "--features wasm-cpu-runtime,webgpu-search",
  "Linux publish features",
);
requireText(
  packageScript,
  "CLEARRA_SOURCE_COMMIT",
  "Linux source commit identity",
);
requireText(
  packageScript,
  "CLEARRA_ENGINE_BUILD_ID",
  "Linux engine build identity",
);
requireText(
  packageScript,
  'json="$("$RELEASE_BINARY" "$@")"',
  "installed Linux asset execution",
);
for (const smoke of [
  "rules",
  "rules-export-srs-x",
  "solver",
  "pc-tiling",
  "failed-queue",
  "build-probability",
  "pc-srs-x",
  "pc-score-minimals",
  "terminal-supply-p0",
]) {
  requireText(packageScript, `run_json_smoke ${smoke}`, `Linux ${smoke} smoke`);
}
for (const semanticMarker of [
  '"actual_backend":"wasm-cpu-build-probability"',
  '"probability_calculated":true',
  '"rule_profile":"srs-x"',
  '"effective_kick_model":"srs-x"',
  '"coverage_probability":"not-calculated"',
  '"probability_calculated":false',
  '"probability_complete":false',
  '"supply_probability_complete":false',
  '"resource_probability_complete":false',
]) {
  requireText(
    packageScript,
    semanticMarker,
    `Linux semantic assertion ${semanticMarker}`,
  );
}
for (const marker of [
  'embedded?.id !== "srs-x"',
  'embedded?.source_rule !== "srs-x"',
  "embedded?.entries?.length !== 80",
  "halfTurnCount !== 24",
  '{"action":"export","profile":"srs-x"}',
]) {
  requireText(packageScript, marker, `Linux SRS-X export assertion ${marker}`);
}
requireText(
  packageScript,
  '"resource_report":{"truncated":false,"truncation_reason":null,"probability_complete":false}',
  "Linux tiling non-calculation resource assertion",
);
requireExactNormalizedText(
  linuxPcTilingSmoke,
  [
    "",
    "run_json_smoke pc-tiling \\",
    '    \'{"kind":"pc-tiling-family.v1","command_kind":"pc-tiling-family.v1","tiling_family_complete":true,"tiling_family_incomplete_reason":"none","summary":{"coverage_probability":"not-calculated","probability_calculated":false,"probability_complete":false,"supply_probability_complete":false,"resource_probability_complete":false},"resource_report":{"truncated":false,"truncation_reason":null,"probability_complete":false}}\' \\',
    "    --format json pc tiling --lines 2 --queue IIOOO --no-hold \\",
    "    --backend cpu --workers 1",
  ].join("\n"),
  "Linux canonical pc.tiling smoke",
);
const linuxTerminalSmoke = section(
  packageScript,
  "\nrun_json_smoke terminal-supply-p0",
  "\nprintf 'cli_release_binary=",
);
requireTerminalSupplySmoke(linuxTerminalSmoke, "Linux");
requireText(
  packageScript,
  'node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs"',
  "Linux terminal-supply executable validator invocation",
);
requireText(
  packageScript,
  TERMINAL_SUPPLY_MODE,
  "Linux terminal-supply validator mode",
);
for (const marker of [
  DISCORD_SCORE_MINIMALS_MODE,
  '"kind":"pc-score-portfolio.v2"',
  '"score_minimals_score_equality":"score-only"',
  '"score_minimals_attack_role":"informational-only"',
  '"score_minimals_canonical_selection":"smallest-canonical-candidate-id"',
  "--board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I",
  '--ties --tie-snapshot "$score_minimals_tie_snapshot"',
  "structured?.summary?.portfolio_alternative_page",
  "Discord accepted explicit score-minimals tie metadata",
]) {
  requireText(
    packageScript,
    marker,
    `Linux Discord score-minimals cross-surface assertion ${marker}`,
  );
}
if (
  (packageScript.match(/--validate-discord-score-minimals-json/gu) ?? [])
    .length !== 2
) {
  throw new Error(
    "Linux release package must validate the canonical score-minimals result and reject the explicit tie result",
  );
}
requireText(
  packageScript,
  '--expected-source-commit "$CLEARRA_SOURCE_COMMIT"',
  "Linux terminal-supply expected source identity",
);
for (const marker of [
  "--format json --include-solution-data pc-scenario",
  "--field 0x1c0701c07 --visible-height 4 --queue STOILJZ",
  "--max-pieces 7 --exact-pieces 7",
  "--backend cpu --workers 1",
  '"unique_solution_count":18',
  '"normalized_solution_set_hash":"cts1:8a7fc484d9b49994"',
]) {
  requireText(
    linuxTerminalSmoke,
    marker,
    `Linux terminal-supply semantic assertion ${marker}`,
  );
}

requireText(
  linuxJob,
  "bash scripts/tools/package-release-cli.sh",
  "Linux packaged CLI job",
);
requireText(
  linuxJob,
  "actions/setup-node@v4",
  "Linux JSON validator Node setup",
);
requireText(linuxJob, "node-version: 22", "Linux JSON validator Node version");
requireText(
  windowsCliJob,
  "--features wasm-cpu-runtime,webgpu-search",
  "Windows publish features",
);
requireText(
  windowsCliJob,
  "$identity.source_commit -ne $env:GITHUB_SHA",
  "Windows source identity",
);
requireText(
  windowsCliJob,
  "$identity.engine_build_id -ne $env:GITHUB_SHA",
  "Windows engine identity",
);
requireText(
  windowsCliJob,
  "--expected-source-commit $env:GITHUB_SHA",
  "Windows terminal-supply expected source identity",
);
for (const smoke of [
  "rules",
  "rules-export-srs-x",
  "solver",
  "pc-tiling",
  "failed-queue",
  "build-probability",
  "pc-srs-x",
  "terminal-supply-p0",
]) {
  requireText(windowsCliJob, `-Name '${smoke}'`, `Windows ${smoke} smoke`);
}
for (const marker of [
  "$embedded.id -ne 'srs-x'",
  "$embedded.source_rule -ne 'srs-x'",
  "$embedded.entries.Count -ne 80",
  "$halfTurnCount -ne 24",
  "action = 'export'; profile = 'srs-x'",
]) {
  requireText(windowsCliJob, marker, `Windows SRS-X export assertion ${marker}`);
}
for (const semanticMarker of [
  "actual_backend = 'wasm-cpu-build-probability'",
  "probability_calculated = $true",
  "rule_profile = 'srs-x'",
  "effective_kick_model = 'srs-x'",
  "coverage_probability = 'not-calculated'",
  "probability_calculated = $false",
  "probability_complete = $false",
  "supply_probability_complete = $false",
  "resource_probability_complete = $false",
]) {
  requireText(
    windowsCliJob,
    semanticMarker,
    `Windows semantic assertion ${semanticMarker}`,
  );
}
requireText(
  windowsCliJob,
  "-ExpectedResourceReport @{ truncated = $false; truncation_reason = $null; probability_complete = $false }",
  "Windows tiling non-calculation resource assertion",
);
requireExactNormalizedText(
  windowsPcTilingSmoke,
  [
    "",
    "          Invoke-ClearraJsonSmoke -Name 'pc-tiling' `",
    "            -CommandArguments @('--format', 'json', 'pc', 'tiling', '--lines', '2', '--queue', 'IIOOO', '--no-hold', '--backend', 'cpu', '--workers', '1') `",
    "            -ExpectedKind 'pc-tiling-family.v1' `",
    "            -ExpectedCommandKind 'pc-tiling-family.v1' `",
    "            -ExpectedTilingFamilyComplete $true `",
    "            -ExpectedTilingFamilyIncompleteReason 'none' `",
    "            -ExpectedSummary @{ coverage_probability = 'not-calculated'; probability_calculated = $false; probability_complete = $false; supply_probability_complete = $false; resource_probability_complete = $false } `",
    "            -ExpectedResourceReport @{ truncated = $false; truncation_reason = $null; probability_complete = $false }",
  ].join("\n"),
  "Windows canonical pc.tiling smoke",
);
const windowsTerminalSmoke = section(
  windowsCliJob,
  "\n          Invoke-ClearraJsonSmoke -Name 'terminal-supply-p0'",
  "\n      - name: Upload Windows CLI artifact",
);
requireTerminalSupplySmoke(windowsTerminalSmoke, "Windows");
requireText(
  windowsCliJob,
  "validate-release-cli-smokes.mjs",
  "Windows terminal-supply executable validator script",
);
requireText(
  windowsCliJob,
  TERMINAL_SUPPLY_MODE,
  "Windows terminal-supply validator mode",
);
for (const marker of [
  "@('--format', 'json', '--include-solution-data', 'pc-scenario'",
  "'--field', '0x1c0701c07', '--visible-height', '4', '--queue', 'STOILJZ'",
  "'--max-pieces', '7', '--exact-pieces', '7'",
  "'--backend', 'cpu', '--workers', '1'",
  "unique_solution_count = 18",
  "normalized_solution_set_hash = 'cts1:8a7fc484d9b49994'",
]) {
  requireText(
    windowsTerminalSmoke,
    marker,
    `Windows terminal-supply semantic assertion ${marker}`,
  );
}
for (const marker of [
  "$artifactDir = Join-Path $PWD.Path",
  '$cli = Join-Path $artifactDir "Clearra-CLI-v$version-windows-x86_64.exe"',
  "Copy-Item -LiteralPath $builtCli -Destination $cli -Force",
  "$jsonLines = & $cli @CommandArguments",
]) {
  requireText(
    windowsCliJob,
    marker,
    `Windows staged release asset execution ${marker}`,
  );
}

requireExactYamlKeySet(
  workflow,
  0,
  ["name", "on", "permissions", "concurrency", "env", "jobs"],
  "release workflow top level",
);
requireExactYamlKeySet(
  workflowPermissions,
  2,
  ["contents", "actions"],
  "release workflow permissions",
);
requireExactYamlScalar(
  workflowPermissions,
  "contents",
  "write",
  "release contents permission",
  2,
);
requireExactYamlScalar(
  workflowPermissions,
  "actions",
  "read",
  "accepted-run lookup permission",
  2,
);
requireExactYamlKeySet(
  workflowConcurrency,
  2,
  ["group", "cancel-in-progress"],
  "release exact-source concurrency",
);
requireExactYamlScalar(
  workflowConcurrency,
  "group",
  "canonical-release-${{ github.sha }}",
  "release exact-source concurrency group",
  2,
);
requireExactYamlScalar(
  workflowConcurrency,
  "cancel-in-progress",
  "false",
  "release exact-source concurrency cancellation policy",
  2,
);
requireExactYamlKeySet(
  workflowEnvironment,
  2,
  ["CLEARRA_SOURCE_COMMIT", "CLEARRA_ENGINE_BUILD_ID"],
  "release workflow environment",
);
for (const key of ["CLEARRA_SOURCE_COMMIT", "CLEARRA_ENGINE_BUILD_ID"]) {
  requireExactYamlScalar(
    workflowEnvironment,
    key,
    "${{ github.sha }}",
    `release workflow ${key}`,
    2,
  );
}
for (const [name, job, runner] of [
  ["CTK3", ctk3Job, "ubuntu-latest"],
  ["Linux CLI", linuxJob, "ubuntu-latest"],
  ["Windows CLI", windowsCliJob, "windows-latest"],
  ["Windows GUI", windowsGuiJob, "windows-latest"],
]) {
  requireExactYamlKeySet(
    job,
    4,
    ["if", "needs", "runs-on", "steps"],
    `${name} job`,
  );
  requireExactYamlScalar(
    job,
    "if",
    "github.event_name == 'workflow_dispatch'",
    `${name} dispatch-only condition`,
  );
  requireExactYamlScalar(job, "needs", "metadata", `${name} dependency`);
  requireExactYamlScalar(job, "runs-on", runner, `${name} runner`);
  if (/^    continue-on-error\s*:/mu.test(job)) {
    throw new Error(`${name} job must fail closed`);
  }
}
const windowsProductCachePrefix =
  "product-v2-${{ runner.os }}-${{ hashFiles('Cargo.lock', 'apps/clearra-desktop/src-tauri/Cargo.lock', 'package-lock.json') }}";
const windowsProductCacheKey =
  `key: ${windowsProductCachePrefix}-` + "${{ github.sha }}";
const windowsProductCacheRestoreKeys = [
  "restore-keys: |",
  `            ${windowsProductCachePrefix}-`,
  `            ${windowsProductCachePrefix}`,
].join("\n");
for (const [name, job] of [
  ["Windows CLI", windowsCliJob],
  ["Windows GUI", windowsGuiJob],
]) {
  for (const marker of [windowsProductCacheKey, windowsProductCacheRestoreKeys]) {
    requireText(job, marker, `${name} lock-compatible exact-SHA cache ${marker}`);
  }
  if (
    (job.match(/\brestore-keys: \|$/gmu) ?? []).length !== 1 ||
    job.split(windowsProductCacheKey).length - 1 !== 1
  ) {
    throw new Error(`${name} must have exactly one exact-SHA product cache contract`);
  }
}
if (
  (windowsCliJob.match(/actions\/cache\/restore@v4/gu) ?? []).length !== 1 ||
  windowsCliJob.includes("actions/cache@v4") ||
  windowsCliJob.includes("actions/cache/save@v4")
) {
  throw new Error("Windows CLI must remain a single restore-only product cache reader");
}
if (
  (windowsGuiJob.match(/actions\/cache@v4/gu) ?? []).length !== 1 ||
  windowsGuiJob.includes("actions/cache/save@v4")
) {
  throw new Error("Windows GUI must remain the sole automatic product cache writer");
}
requireExactYamlKeySet(
  discordJob,
  4,
  ["if", "needs", "runs-on", "steps"],
  "Discord job",
);
requireExactYamlScalar(
  discordJob,
  "if",
  "github.event_name == 'workflow_dispatch'",
  "Discord dispatch-only condition",
);
requireExactYamlFlowSequence(
  discordJob,
  "needs",
  ["metadata", "ctk3"],
  "Discord dependency on metadata and accepted CTK3",
);
requireExactYamlScalar(discordJob, "runs-on", "ubuntu-latest", "Discord runner");
if (/^    continue-on-error\s*:/mu.test(discordJob)) {
  throw new Error("Discord job must fail closed");
}
requireExactStepSkeleton(
  ctk3Job,
  [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Install JavaScript workspace",
    "- name: Validate accepted CTK3 artifact contract",
    "- name: Build and test CTK3 once",
    "- name: Seal the accepted CTK3 distribution",
    "- name: Upload accepted CTK3 distribution",
  ],
  "accepted CTK3 owner",
);
for (const [marker, description] of [
  ["run: node --test scripts/tools/accepted-ctk3-dist.test.mjs", "artifact contract unit"],
  ["run: npm test --workspace ctk3", "single package build and test"],
  [
    'run: node scripts/tools/accepted-ctk3-dist.mjs --seal packages/ctk3/dist --source-commit "$CLEARRA_SOURCE_COMMIT" --run-id "$GITHUB_RUN_ID" --run-attempt "$GITHUB_RUN_ATTEMPT"',
    "source-and-attempt-bound artifact seal",
  ],
]) {
  requireText(ctk3Job, marker, `accepted CTK3 ${description}`);
}
requireExactYamlKeySet(
  ctk3UploadStep,
  8,
  ["uses", "with"],
  "accepted CTK3 upload step",
);
requireExactYamlScalar(
  ctk3UploadStep,
  "name",
  "ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
  "accepted CTK3 artifact name",
  10,
);
requireExactYamlScalar(
  ctk3UploadStep,
  "path",
  "packages/ctk3/dist",
  "accepted CTK3 artifact path",
  10,
);
requireExactYamlScalar(
  ctk3UploadStep,
  "if-no-files-found",
  "error",
  "accepted CTK3 missing artifact policy",
  10,
);
if ((workflow.match(/npm test --workspace ctk3/gu) ?? []).length !== 1) {
  throw new Error("CTK3 package build and test must have exactly one workflow owner");
}
requireMatch(
  triggerSection,
  /^  push:\s*\r?\n    tags:\s*\[(?:"v\*"|'v\*')\]\s*$/mu,
  "exact v* tag release trigger",
);
requireText(
  triggerSection,
  "  workflow_dispatch:",
  "canonical acceptance trigger",
);
requireExactYamlKeySet(
  section(metadataJob, "\n    outputs:", "\n    steps:"),
  6,
  ["version", "accepted_run_id", "accepted_run_attempt"],
  "metadata outputs",
);
requireExactYamlScalar(
  section(metadataJob, "\n    outputs:", "\n    steps:"),
  "accepted_run_id",
  "${{ steps.accepted_run.outputs.accepted_run_id }}",
  "bound canonical run output",
  6,
);
requireExactYamlScalar(
  section(metadataJob, "\n    outputs:", "\n    steps:"),
  "accepted_run_attempt",
  "${{ steps.accepted_run.outputs.accepted_run_attempt }}",
  "bound canonical run attempt output",
  6,
);
requireExactYamlKeySet(
  canonicalAcceptancePreflightStep,
  8,
  ["if", "env", "shell", "run"],
  "zero-prior-success canonical preflight step",
);
for (const marker of [
  "if: github.event_name == 'workflow_dispatch'",
  "GH_TOKEN: ${{ github.token }}",
  '"$GITHUB_RUN_ATTEMPT" != \'1\'',
  "canonical release acceptance forbids workflow reruns",
  '"$GITHUB_REF" != \'refs/heads/main\'',
  'git fetch --no-tags --depth=1 origin main',
  '"$(git rev-parse origin/main)" != "$GITHUB_SHA"',
  "node scripts/release/canonical-acceptance-run.mjs \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--source-commit "$GITHUB_SHA" \\',
  "--require zero",
]) {
  requireText(
    canonicalAcceptancePreflightStep,
    marker,
    `zero-prior-success canonical preflight ${marker}`,
  );
}
for (const stepName of [
  "Validate independent release regressions with bounded workers",
  "Archive the exact accepted source on Linux",
  "Validate every product version and changelog surface",
  "Require the upstream drift audit authority",
]) {
  const escaped = stepName.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  requireMatch(
    metadataJob,
    new RegExp(
      `^      - name: ${escaped}\\r?\\n        if: github\\.event_name == 'workflow_dispatch'$`,
      "mu",
    ),
    `${stepName} dispatch-only gate`,
  );
}
if (
  metadataJob.includes("tests/contracts/product_capability_registry.test.mjs") ||
  metadataJob.includes("apps/clearra-discord-bot/test/capability-registry.test.mjs")
) {
  throw new Error(
    "Linux metadata must remain dependency-free and must not duplicate runtime-backed capability-registry suites",
  );
}
if (
  /^        run: node scripts\/tools\/validate-release-cli-smokes\.mjs\s*$/mu.test(
    metadataJob,
  )
) {
  throw new Error(
    "Linux metadata must not duplicate the release smoke validator outside its mutation owner",
  );
}
requireExactYamlKeySet(
  acceptedRunStep,
  8,
  ["id", "env", "shell", "run"],
  "canonical acceptance binding step",
);
if (/^        if\s*:/mu.test(acceptedRunStep)) {
  throw new Error(
    "canonical acceptance binding must run for both dispatch and tag events",
  );
}
for (const forbiddenMetadataBuild of [
  "Install JavaScript workspace for product authority",
  "Build CTK3 workspace for product authority",
]) {
  if (metadataJob.includes(forbiddenMetadataBuild)) {
    throw new Error(
      `Linux metadata must not duplicate product build step ${forbiddenMetadataBuild}`,
    );
  }
}
for (const marker of [
  "id: accepted_run",
  'if [[ "$GITHUB_EVENT_NAME" == \'workflow_dispatch\' ]]; then',
  'echo "accepted_run_id=$GITHUB_RUN_ID" >> "$GITHUB_OUTPUT"',
  'echo "accepted_run_attempt=$GITHUB_RUN_ATTEMPT" >> "$GITHUB_OUTPUT"',
  'if [[ "$GITHUB_REF_TYPE" != \'tag\' ]]; then',
  'git fetch --no-tags --depth=1 origin main',
  '"$(git rev-parse origin/main)" != "$GITHUB_SHA"',
  "node scripts/release/canonical-acceptance-run.mjs \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--source-commit "$GITHUB_SHA" \\',
  "--require one \\",
  '--format github-output >> "$GITHUB_OUTPUT"',
]) {
  requireText(acceptedRunStep, marker, `canonical acceptance binding ${marker}`);
}
for (const marker of [
  "pattern: clearra-*-v${{ needs.metadata.outputs.version }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
  "merge-multiple: true",
  "run-id: ${{ needs.metadata.outputs.accepted_run_id }}",
  "github-token: ${{ github.token }}",
]) {
  requireText(crossRunDownloadStep, marker, `cross-run artifact reuse ${marker}`);
}
for (const [name, step] of [
  ["Discord", discordDownloadStep],
  ["Windows canonical acceptance", releaseAcceptanceDownloadStep],
]) {
  requireExactYamlKeySet(
    step,
    8,
    ["uses", "with"],
    `${name} accepted CTK3 download step`,
  );
  requireExactYamlScalar(
    step,
    "uses",
    "actions/download-artifact@v4",
    `${name} accepted CTK3 download action`,
    8,
  );
  requireExactYamlKeySet(
    step,
    10,
    ["name", "path"],
    `${name} accepted CTK3 download inputs`,
  );
  requireExactYamlScalar(
    step,
    "name",
    "ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
    `${name} accepted CTK3 artifact name`,
    10,
  );
  requireExactYamlScalar(
    step,
    "path",
    "packages/ctk3/dist",
    `${name} accepted CTK3 artifact path`,
    10,
  );
}
requireExactStepSkeleton(
  discordJob,
  [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Download accepted CTK3 distribution",
    "- name: Install JavaScript workspace",
    "- name: Verify accepted CTK3 distribution",
    "- name: Verify Clearrabot contracts",
    "- name: Require the product capability and alias parser authority",
  ],
  "Discord accepted CTK3 consumer",
);
requireText(
  discordJob,
  'run: node scripts/tools/accepted-ctk3-dist.mjs --verify packages/ctk3/dist --expected-source-commit "$CLEARRA_SOURCE_COMMIT" --expected-run-id "$GITHUB_RUN_ID" --expected-run-attempt "$GITHUB_RUN_ATTEMPT"',
  "Discord accepted CTK3 verification",
);
requireText(
  discordJob,
  "run: npm run test:built --workspace @clearra/discord-bot",
  "Discord built-only suite",
);
if (workflow.includes("npm test --workspace @clearra/discord-bot")) {
  throw new Error("release workflow must not rebuild CTK3 through the Discord suite");
}
requireExactYamlKeySet(
  releaseAcceptanceRunStep,
  8,
  ["env", "run"],
  "canonical Rust release acceptance shard step",
);
requireExactYamlKeySet(
  releaseAcceptanceRunStep,
  10,
  [
    "CLEARRA_ACCEPTED_CTK3_DIST",
    "CLEARRA_ACCEPTED_RUN_ID",
    "CLEARRA_ACCEPTED_RUN_ATTEMPT",
  ],
  "canonical Rust release acceptance shard environment",
);
requireExactYamlScalar(
  releaseAcceptanceRunStep,
  "CLEARRA_ACCEPTED_CTK3_DIST",
  "${{ github.workspace }}/packages/ctk3/dist",
  "canonical Rust release acceptance accepted CTK3 path",
  10,
);
requireExactYamlScalar(
  releaseAcceptanceRunStep,
  "CLEARRA_ACCEPTED_RUN_ID",
  "${{ github.run_id }}",
  "canonical Rust release acceptance accepted run ID",
  10,
);
requireExactYamlScalar(
  releaseAcceptanceRunStep,
  "CLEARRA_ACCEPTED_RUN_ATTEMPT",
  "${{ github.run_attempt }}",
  "canonical Rust release acceptance accepted run attempt",
  10,
);
requireExactYamlScalar(
  releaseAcceptancePagesRunStep,
  "CLEARRA_WEB_BASE_PATH",
  "/${{ github.event.repository.name }}",
  "canonical Pages release acceptance base path",
  10,
);
requireExactYamlScalar(
  releaseAcceptanceRunStep,
  "run",
  "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Rust -ExecutionSurface Trusted",
  "canonical Rust release acceptance shard command",
  8,
);
requireExactYamlKeySet(
  releaseAcceptancePagesRunStep,
  8,
  ["env", "run"],
  "canonical Pages release acceptance shard step",
);
requireExactYamlKeySet(
  releaseAcceptancePagesRunStep,
  10,
  ["CLEARRA_WEB_BASE_PATH"],
  "canonical Pages release acceptance shard environment",
);
requireExactYamlScalar(
  releaseAcceptancePagesRunStep,
  "run",
  "powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Pages -ExecutionSurface Trusted",
  "canonical Pages release acceptance shard command",
  8,
);
for (const [name, job, skeleton] of [
  ["foundation NoProductDebt", releaseAcceptanceFoundationNoProductDebtJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Archive the exact accepted source on Windows",
    "- id: release_toolchain_cache",
    "- name: Install JavaScript workspace",
    "- name: Verify canonical ReleaseAcceptance shard mapping",
    "- name: Run canonical release acceptance NoProductDebt leaf",
    "- name: Seal canonical release acceptance NoProductDebt leaf",
    "- name: Upload canonical release acceptance NoProductDebt leaf",
  ]],
  ["foundation AdversarialCorrectness", releaseAcceptanceFoundationAdversarialCorrectnessJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- id: release_toolchain_cache",
    "- name: Run canonical release acceptance AdversarialCorrectness leaf",
    "- name: Seal canonical release acceptance AdversarialCorrectness leaf",
    "- name: Upload canonical release acceptance AdversarialCorrectness leaf",
  ]],
  ["foundation DesktopHost", releaseAcceptanceFoundationDesktopHostJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- id: release_toolchain_cache",
    "- name: Install JavaScript workspace",
    "- name: Run canonical release acceptance DesktopHost leaf",
    "- name: Seal canonical release acceptance DesktopHost leaf",
    "- name: Upload canonical release acceptance DesktopHost leaf",
  ]],
  ["sanitizer", releaseAcceptanceSanitizerJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- id: release_toolchain_cache",
    "- name: Run canonical release acceptance sanitizer shard",
    "- name: Seal canonical release acceptance sanitizer shard",
    "- name: Upload canonical release acceptance sanitizer shard",
  ]],
  ["rust", releaseAcceptanceRustJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Download accepted CTK3 distribution",
    "- id: release_toolchain_cache",
    "- name: Install JavaScript workspace",
    "- name: Run canonical release acceptance rust shard",
    "- name: Seal canonical release acceptance rust shard",
    "- name: Upload canonical release acceptance rust shard",
  ]],
  ["pages", releaseAcceptancePagesJob, [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- id: release_toolchain_cache",
    "- name: Prepare acceptance toolchains",
    "- name: Install JavaScript workspace",
    "- name: Run canonical release acceptance Pages shard",
    "- name: Stamp and verify the accepted Pages build",
    "- name: Seal canonical release acceptance Pages shard",
    "- name: Upload accepted Pages build",
    "- name: Upload canonical release acceptance Pages shard",
  ]],
]) {
  requireExactStepSkeleton(job, skeleton, `canonical ${name} acceptance shard`);
}
requireExactStepSkeleton(
  releaseAcceptanceJob,
  [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Download all canonical release acceptance shard evidence",
    "- name: Produce canonical release gate evidence",
    "- name: Upload canonical release gate evidence",
  ],
  "canonical release acceptance fan-in",
);
requireText(
  releaseAcceptanceFoundationNoProductDebtJob,
  "run: pwsh -NoProfile -File scripts/test_release_acceptance_shards.ps1",
  "canonical ReleaseAcceptance shard mapping regression",
);
const releaseShardJobs = [
  releaseAcceptanceFoundationNoProductDebtJob,
  releaseAcceptanceFoundationAdversarialCorrectnessJob,
  releaseAcceptanceFoundationDesktopHostJob,
  releaseAcceptanceSanitizerJob,
  releaseAcceptanceRustJob,
  releaseAcceptancePagesJob,
];
for (const [index, job] of releaseShardJobs.entries()) {
  if ((job.match(/actions\/cache\/restore@v4/gu) ?? []).length !== 1) {
    throw new Error(`release shard ${index} must have exactly one restore-only cache reader`);
  }
  if (job.includes("actions/cache@v4") || job.includes("actions/cache/save@v4")) {
    throw new Error(`release shard ${index} must not own an automatic or explicit cache writer`);
  }
  for (const marker of [
    "~/.cargo/bin/wasm-bindgen.exe",
    "~/.cargo/registry",
    "~/.cargo/git",
    "~/AppData/Local/Clearra/build",
    "release-acceptance-${{ runner.os }}-bindgen-0.2.126-${{ hashFiles('Cargo.lock', 'apps/clearra-desktop/src-tauri/Cargo.lock', 'package-lock.json') }}-${{ github.sha }}",
    "release-acceptance-${{ runner.os }}-bindgen-0.2.126-${{ hashFiles('Cargo.lock', 'apps/clearra-desktop/src-tauri/Cargo.lock', 'package-lock.json') }}-",
  ]) {
    requireText(job, marker, `release shard ${index} cache ${marker}`);
  }
}
if ((workflow.match(/actions\/cache\/save@v4/gu) ?? []).length !== 0) {
  throw new Error("canonical acceptance must remain restore-only with no explicit cache writer");
}
if (
  releaseAcceptanceFoundationNoProductDebtJob.includes("actions/cache/save@v4") ||
  releaseAcceptanceFoundationAdversarialCorrectnessJob.includes("actions/cache/save@v4") ||
  releaseAcceptanceFoundationDesktopHostJob.includes("actions/cache/save@v4") ||
  releaseAcceptanceSanitizerJob.includes("actions/cache/save@v4") ||
  releaseAcceptanceRustJob.includes("actions/cache/save@v4") ||
  releaseAcceptancePagesJob.includes("actions/cache/save@v4")
) {
  throw new Error("no canonical acceptance shard may write a cache");
}
for (const [shard, job, caseName] of [
  ["foundation-no-product-debt", releaseAcceptanceFoundationNoProductDebtJob, "FoundationNoProductDebt"],
  ["foundation-adversarial-correctness", releaseAcceptanceFoundationAdversarialCorrectnessJob, "FoundationAdversarialCorrectness"],
  ["foundation-desktop-host", releaseAcceptanceFoundationDesktopHostJob, "FoundationDesktopHost"],
  ["sanitizer", releaseAcceptanceSanitizerJob, "Sanitizer"],
  ["rust", releaseAcceptanceRustJob, "Rust"],
  ["pages", releaseAcceptancePagesJob, "Pages"],
]) {
  for (const marker of [
    `-ReleaseAcceptanceShard ${caseName} -ExecutionSurface Trusted`,
    "node scripts/release/canonical-acceptance-evidence.mjs shard `",
    `--shard ${shard} \``,
    `clearra-release-acceptance-${shard}-shard.v1.json`,
    `release-acceptance-${shard}-shard-\${{ github.sha }}-run-\${{ needs.metadata.outputs.accepted_run_id }}-attempt-\${{ needs.metadata.outputs.accepted_run_attempt }}`,
  ]) {
    requireText(job, marker, `canonical ${shard} shard evidence ${marker}`);
  }
}
requireExactYamlKeySet(
  acceptedPagesStampStep,
  8,
  ["shell", "run"],
  "accepted Pages stamp step",
);
requireExactYamlScalar(
  acceptedPagesStampStep,
  "shell",
  "pwsh",
  "accepted Pages stamp shell",
  8,
);
for (const marker of [
  "node scripts/release/accepted-pages-build.mjs `",
  "--stamp apps/clearra-web/build `",
  "--verify apps/clearra-web/build `",
  "--source-commit $env:CLEARRA_SOURCE_COMMIT `",
  "--accepted-run-id $env:GITHUB_RUN_ID `",
  "--accepted-run-attempt $env:GITHUB_RUN_ATTEMPT `",
  '--base-path "/${{ github.event.repository.name }}" `',
  '--version "${{ needs.metadata.outputs.version }}"',
]) {
  requireText(acceptedPagesStampStep, marker, `accepted Pages stamp ${marker}`);
}
if (
  (acceptedPagesStampStep.match(/node scripts\/release\/accepted-pages-build\.mjs/gu) ?? [])
    .length !== 2
) {
  throw new Error("accepted Pages build must be stamped and immediately verified once");
}
requireExactYamlKeySet(
  acceptedPagesUploadStep,
  8,
  ["uses", "with"],
  "accepted Pages upload step",
);
requireExactYamlKeySet(
  acceptedPagesUploadStep,
  10,
  ["name", "path", "if-no-files-found", "include-hidden-files"],
  "accepted Pages upload inputs",
);
requireExactYamlScalar(
  acceptedPagesUploadStep,
  "uses",
  "actions/upload-artifact@v4",
  "accepted Pages upload action",
  8,
);
for (const [key, value] of [
  [
    "name",
    "accepted-pages-build-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
  ],
  ["path", "apps/clearra-web/build"],
  ["if-no-files-found", "error"],
  ["include-hidden-files", "true"],
]) {
  requireExactYamlScalar(
    acceptedPagesUploadStep,
    key,
    value,
    `accepted Pages upload ${key}`,
    10,
  );
}
if (
  (acceptedPagesUploadStep.match(/accepted-pages-build-\$\{\{ github\.sha \}\}-run-/gu) ?? [])
    .length !== 1
) {
  throw new Error("accepted Pages build must have exactly one canonical upload owner");
}
requireExactYamlKeySet(
  releaseGateEvidenceStep,
  8,
  ["shell", "run"],
  "canonical release gate evidence producer step",
);
for (const marker of [
  "shell: bash",
  "node scripts/release/canonical-acceptance-evidence.mjs gate \\",
  '--source-commit "$GITHUB_SHA" \\',
  '--run-id "$GITHUB_RUN_ID" \\',
  '--run-attempt "$GITHUB_RUN_ATTEMPT" \\',
  "--shards release-shard-evidence \\",
  "--output release-gate-evidence",
]) {
  requireText(
    releaseGateEvidenceStep,
    marker,
    `canonical release gate evidence producer ${marker}`,
  );
}
requireExactYamlKeySet(
  releaseGateUploadStep,
  8,
  ["uses", "with"],
  "canonical release gate evidence upload step",
);
for (const [key, value] of [
  ["uses", "actions/upload-artifact@v4"],
  [
    "name",
    "release-gate-evidence-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
  ],
  ["path", "release-gate-evidence"],
  ["if-no-files-found", "error"],
]) {
  requireExactYamlScalar(
    releaseGateUploadStep,
    key,
    value,
    `canonical release gate evidence upload ${key}`,
    key === "uses" ? 8 : 10,
  );
}

requireExactYamlKeySet(
  canonicalEvidenceJob,
  4,
  ["if", "needs", "runs-on", "steps"],
  "canonical acceptance evidence job",
);
requireExactYamlScalar(
  canonicalEvidenceJob,
  "if",
  "github.event_name == 'workflow_dispatch'",
  "canonical acceptance evidence dispatch-only condition",
  4,
);
requireExactYamlFlowSequence(
  canonicalEvidenceJob,
  "needs",
  [
    "metadata",
    "ctk3",
    "linux-cli",
    "discord-bot",
    "release-acceptance",
    "windows-cli",
    "windows-gui",
  ],
  "canonical acceptance evidence dependency on every accepted producer",
);
requireExactYamlScalar(
  canonicalEvidenceJob,
  "runs-on",
  "ubuntu-latest",
  "canonical acceptance evidence runner",
  4,
);
requireExactStepSkeleton(
  canonicalEvidenceJob,
  [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Download canonical release gate evidence",
    "- name: Download accepted CTK3 distribution",
    "- name: Download accepted Pages build",
    "- name: Download Linux product",
    "- name: Download Windows CLI product",
    "- name: Download Windows GUI product",
    "- name: Read exact workflow job evidence",
    "- name: Create canonical acceptance evidence",
    "- name: Upload canonical acceptance evidence",
  ],
  "canonical acceptance evidence assembly",
);
for (const marker of [
  '"repos/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}/jobs"',
  "-f filter=latest \\",
  "-f per_page=100",
  "node scripts/release/canonical-acceptance-evidence.mjs collect \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--version "${{ needs.metadata.outputs.version }}" \\',
  '--source-commit "$GITHUB_SHA" \\',
  '--run-id "$GITHUB_RUN_ID" \\',
  '--run-attempt "$GITHUB_RUN_ATTEMPT" \\',
  '--base-path "/${{ github.event.repository.name }}" \\',
  "--gate-evidence acceptance-inputs/gate \\",
  "--ctk3 acceptance-inputs/ctk3 \\",
  "--pages acceptance-inputs/pages \\",
  "--products acceptance-inputs/products \\",
  "--output canonical-acceptance-evidence/clearra-canonical-acceptance-evidence.v1.json",
  "name: canonical-acceptance-evidence-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
]) {
  requireText(
    canonicalEvidenceJob,
    marker,
    `canonical acceptance evidence assembly ${marker}`,
  );
}

requireExactYamlKeySet(
  publishEvidenceDownloadStep,
  8,
  ["uses", "with"],
  "tag publication acceptance evidence download step",
);
for (const marker of [
  "uses: actions/download-artifact@v4",
  "name: canonical-acceptance-evidence-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}",
  "path: canonical-acceptance-evidence",
  "run-id: ${{ needs.metadata.outputs.accepted_run_id }}",
  "github-token: ${{ github.token }}",
]) {
  requireText(
    publishEvidenceDownloadStep,
    marker,
    `tag publication acceptance evidence download ${marker}`,
  );
}

requireExactYamlKeySet(
  pagesAcceptedSourceJob,
  4,
  ["permissions", "outputs", "runs-on", "steps"],
  "Pages accepted-source job",
);
requireExactYamlKeySet(
  pagesAcceptedSourcePermissions,
  6,
  ["contents", "actions", "deployments"],
  "Pages accepted-source permissions",
);
for (const [key, value] of [
  ["contents", "read"],
  ["actions", "read"],
  ["deployments", "read"],
]) {
  requireExactYamlScalar(
    pagesAcceptedSourcePermissions,
    key,
    value,
    `Pages accepted-source ${key} permission`,
    6,
  );
}
const pagesAcceptedOutputs = section(
  pagesAcceptedSourceJob,
  "\n    outputs:",
  "\n    runs-on:",
);
requireExactYamlKeySet(
  pagesAcceptedOutputs,
  6,
  [
    "accepted_run_id",
    "accepted_run_attempt",
    "rollback_report_artifact_id",
    "rollback_report_artifact_name",
    "rollback_report_artifact_digest",
    "rollback_capture_artifact_name",
    "rollback_capture_tar_sha256",
  ],
  "Pages accepted-run outputs",
);
requireExactYamlScalar(
  pagesAcceptedOutputs,
  "accepted_run_id",
  "${{ steps.accepted_run.outputs.accepted_run_id }}",
  "Pages bound accepted run ID output",
  6,
);
for (const [key, value] of [
  ["rollback_report_artifact_id", "${{ steps.rollback-report.outputs.report_artifact_id }}"],
  ["rollback_report_artifact_name", "${{ steps.rollback-report.outputs.report_artifact_name }}"],
  ["rollback_report_artifact_digest", "${{ steps.rollback-report.outputs.report_artifact_digest }}"],
  ["rollback_capture_artifact_name", "${{ steps.rollback-authority.outputs.capture_artifact_name }}"],
  ["rollback_capture_tar_sha256", "${{ steps.rollback-authority.outputs.capture_tar_sha256 }}"],
]) {
  requireExactYamlScalar(
    pagesAcceptedOutputs,
    key,
    value,
    `Pages sealed rollback output ${key}`,
    6,
  );
}
requireExactYamlScalar(
  pagesAcceptedOutputs,
  "accepted_run_attempt",
  "${{ steps.accepted_run.outputs.accepted_run_attempt }}",
  "Pages bound accepted run attempt output",
  6,
);
requireExactYamlKeySet(
  pagesAcceptedRunStep,
  8,
  ["id", "env", "run"],
  "Pages accepted-run binding step",
);
for (const marker of [
  "id: accepted_run",
  "node scripts/release/canonical-acceptance-run.mjs \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--source-commit "$checked_sha" \\',
  "--require one \\",
  '--format github-output >> "$GITHUB_OUTPUT"',
]) {
  requireText(pagesAcceptedRunStep, marker, `Pages accepted-run binding ${marker}`);
}
requireExactYamlKeySet(
  pagesRollbackReportResolveStep,
  8,
  ["id", "env", "run"],
  "Pages sealed rollback report resolver step",
);
for (const marker of [
  "id: rollback-report",
  "PAGES_AUTHORITY_MODE: resolve-forward",
  "CAPTURE_RUN_ID: ${{ inputs.rollback_capture_run_id }}",
  "node scripts/release/pages-rollback-authority.mjs",
]) {
  requireText(
    pagesRollbackReportResolveStep,
    marker,
    `Pages sealed rollback report resolver ${marker}`,
  );
}
requireExactYamlKeySet(
  pagesRollbackAuthorityStep,
  8,
  ["id", "env", "run"],
  "Pages sealed rollback report consumer step",
);
for (const marker of [
  "id: rollback-authority",
  "CAPTURE_REPORT_PATH: rollback-report-initial/pages-rollback-capture-authority.json",
  "CAPTURE_REPORT_ARTIFACT_ID: ${{ steps.rollback-report.outputs.report_artifact_id }}",
  "CAPTURE_REPORT_ARTIFACT_NAME: ${{ steps.rollback-report.outputs.report_artifact_name }}",
  "CAPTURE_REPORT_ARTIFACT_DIGEST: ${{ steps.rollback-report.outputs.report_artifact_digest }}",
]) {
  requireText(
    pagesRollbackAuthorityStep,
    marker,
    `Pages sealed rollback report consumer ${marker}`,
  );
}
for (const marker of [
  "validateSealedCaptureConsumerAuthority({",
  "verifyCurrentPagesAgainstCapture({",
  "validatedCaptureReport: captureReportRecord.report",
  'if (captureKind === "legacy-v0.7.4")',
  'if (captureKind === "modern-v2")',
  "validateLegacyAuthority = validateLegacyForwardPublicAuthority",
  ').preartifact_public_readback',
  "`/deployments/${sealedDeploymentId}`",
  "`/deployments/${sealedDeploymentId}/statuses?per_page=100&page=1`",
  "`/deployments/${sealedDeploymentId}/statuses?per_page=100&page=2`",
  "validateCompleteDeploymentStatuses(",
  "second page must be exactly empty",
  "fetchPublicStatus(",
  "LEGACY_PAGES_PAYLOAD.manifest.bytes",
  "LEGACY_PAGES_PAYLOAD.bindings.bytes",
  "LEGACY_PAGES_PAYLOAD.wasm.bytes",
]) {
  requireText(
    pagesRollbackAuthoritySource,
    marker,
    `Pages legacy forward rollback authority ${marker}`,
  );
}
for (const marker of [
  "validateLegacyForwardPublicAuthorityProjection({",
  "validateLegacyPagesPublicSnapshot({",
  "legacy Pages forward public authority changed since the sealed capture",
  "canonicalJson(currentProjection) !==\n    canonicalJson(legacyPublicAuthorityProjectionFromEvidence(baseline))",
]) {
  requireText(
    pagesLegacyContractSource,
    marker,
    `Pages legacy forward immutable contract ${marker}`,
  );
}
for (const marker of [
  "        await validateLiveForwardPayloads({",
  "MAX_FORWARD_PUBLIC_FILE_COUNT",
  "MAX_FORWARD_PUBLIC_TOTAL_BYTES",
  "MAX_FORWARD_PUBLIC_PATH_LENGTH",
  'redirect: "error"',
  'cache: "no-store"',
  "exactBytes: expectedSize",
  "differs from the identity SHA-256",
]) {
  requireText(
    pagesDeploymentAuthoritySource,
    marker,
    `Pages deployed public byte authority ${marker}`,
  );
}
for (const forbidden of [
  "rollback_artifact_id:",
  "rollback_artifact_name:",
  "rollback_artifact_digest:",
  "rollback_tar_sha256:",
  "inputs.rollback_artifact_id",
  "inputs.rollback_artifact_name",
  "inputs.rollback_artifact_digest",
  "inputs.rollback_tar_sha256",
]) {
  if (pagesWorkflow.includes(forbidden)) {
    throw new Error(`Pages workflow must not accept manually transcribed rollback authority: ${forbidden}`);
  }
}
requireExactYamlKeySet(
  pagesBuildJob,
  4,
  ["needs", "outputs", "runs-on", "steps"],
  "Pages artifact reuse job",
);
const pagesBuildOutputs = section(
  pagesBuildJob,
  "\n    outputs:",
  "\n    runs-on:",
);
requireExactYamlKeySet(
  pagesBuildOutputs,
  6,
  ["pages_artifact_id"],
  "Pages uploaded artifact output",
);
requireExactYamlScalar(
  pagesBuildOutputs,
  "pages_artifact_id",
  "${{ steps.pages-artifact.outputs.artifact_id }}",
  "Pages uploaded artifact ID output",
  6,
);
requireExactYamlScalar(
  pagesBuildJob,
  "needs",
  "accepted-source",
  "Pages artifact reuse dependency",
);
requireExactYamlScalar(
  pagesBuildJob,
  "runs-on",
  "ubuntu-latest",
  "Pages artifact reuse runner",
);
requireExactStepSkeleton(
  pagesBuildJob,
  [
    "- uses: actions/checkout@v4",
    "- uses: actions/setup-node@v4",
    "- name: Download exact accepted Pages build",
    "- name: Configure Pages",
    "- name: Verify exact accepted Pages build",
    "- name: Upload Pages artifact",
  ],
  "Pages exact accepted artifact reuse",
);
for (const forbidden of [
  "actions/cache@",
  "rustup ",
  "cargo install",
  "npm ci",
  "npm run build",
  "vite build",
]) {
  if (pagesBuildJob.includes(forbidden)) {
    throw new Error(`Pages artifact reuse must not rebuild or install toolchains: ${forbidden}`);
  }
}
requireExactYamlKeySet(
  pagesDownloadStep,
  8,
  ["uses", "with"],
  "Pages accepted build download step",
);
requireExactYamlKeySet(
  pagesDownloadStep,
  10,
  ["name", "path", "github-token", "repository", "run-id"],
  "Pages accepted build download inputs",
);
for (const [key, value, indentation] of [
  ["uses", "actions/download-artifact@v4", 8],
  [
    "name",
    "accepted-pages-build-${{ inputs.accepted_sha }}-run-${{ needs.accepted-source.outputs.accepted_run_id }}-attempt-${{ needs.accepted-source.outputs.accepted_run_attempt }}",
    10,
  ],
  ["path", "apps/clearra-web/build", 10],
  ["github-token", "${{ github.token }}", 10],
  ["repository", "${{ github.repository }}", 10],
  ["run-id", "${{ needs.accepted-source.outputs.accepted_run_id }}", 10],
]) {
  requireExactYamlScalar(
    pagesDownloadStep,
    key,
    value,
    `Pages accepted build download ${key}`,
    indentation,
  );
}
requireExactYamlKeySet(
  pagesVerifyStep,
  8,
  ["env", "run"],
  "Pages accepted build verification step",
);
requireExactYamlKeySet(
  pagesUploadStep,
  8,
  ["id", "uses", "with"],
  "Pages artifact upload step",
);
requireExactYamlScalar(
  pagesUploadStep,
  "id",
  "pages-artifact",
  "Pages artifact upload step ID",
  8,
);
requireExactYamlScalar(
  pagesUploadStep,
  "uses",
  "actions/upload-pages-artifact@v3",
  "Pages artifact upload action",
  8,
);
requireExactYamlKeySet(
  pagesVerifyEnvironment,
  10,
  ["EXPECTED_SHA", "ACCEPTED_RUN_ID", "ACCEPTED_RUN_ATTEMPT", "EXPECTED_BASE_PATH"],
  "Pages accepted build verification environment",
);
for (const marker of [
  "node scripts/release/accepted-pages-build.mjs \\",
  "--verify apps/clearra-web/build \\",
  '--source-commit "$EXPECTED_SHA" \\',
  '--accepted-run-id "$ACCEPTED_RUN_ID" \\',
  '--accepted-run-attempt "$ACCEPTED_RUN_ATTEMPT" \\',
  '--base-path "$EXPECTED_BASE_PATH" \\',
  '--version "$(node scripts/release/validate-release-metadata.mjs)"',
]) {
  requireText(pagesVerifyStep, marker, `Pages accepted build verification ${marker}`);
}
requireExactYamlKeySet(
  pagesDeployHeader,
  4,
  ["permissions", "environment", "runs-on", "needs"],
  "Pages deploy header",
);
requireExactYamlKeySet(
  pagesDeployPermissions,
  6,
  ["contents", "actions", "deployments", "pages", "id-token"],
  "Pages deploy permissions",
);
for (const [key, value, label] of [
  ["contents", "read", "Pages deploy contents permission"],
  ["actions", "read", "Pages deploy actions permission"],
  ["deployments", "read", "Pages deploy deployments permission"],
  ["pages", "write", "Pages deploy mutation permission"],
  ["id-token", "write", "Pages deploy OIDC permission"],
]) {
  requireExactYamlScalar(pagesDeployPermissions, key, value, label, 6);
}
requireExactYamlFlowSequence(
  pagesDeployHeader,
  "needs",
  ["accepted-source", "build"],
  "Pages deploy dependency on authority and artifact reuse",
);
requireExactYamlKeySet(
  pagesLateAcceptedRunStep,
  8,
  ["env", "shell", "run"],
  "Pages late accepted-run validation step",
);
for (const marker of [
  "ACCEPTED_RUN_ID: ${{ needs.accepted-source.outputs.accepted_run_id }}",
  "ACCEPTED_RUN_ATTEMPT: ${{ needs.accepted-source.outputs.accepted_run_attempt }}",
  "node authority-source/scripts/release/canonical-acceptance-run.mjs \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--source-commit "$EXPECTED_SHA" \\',
  "--require one \\",
  '--expected-run-id "$ACCEPTED_RUN_ID" \\',
  '--expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
]) {
  requireText(pagesLateAcceptedRunStep, marker, `Pages late accepted-run validation ${marker}`);
}
requireExactYamlKeySet(
  pagesDeploymentAuthorityStep,
  8,
  ["id", "env", "run"],
  "sealed Pages deployment authority step",
);
for (const marker of [
  "id: deployment-authority",
  "GH_TOKEN: ${{ github.token }}",
  "PAGES_DEPLOYMENT_MODE: forward",
  "SOURCE_COMMIT: ${{ inputs.accepted_sha }}",
  "PAGES_ARTIFACT_ID: ${{ needs.build.outputs.pages_artifact_id }}",
  "PAGES_ARTIFACT_NAME: github-pages",
  "EXPECTED_ACCEPTED_RUN_ID: ${{ needs.accepted-source.outputs.accepted_run_id }}",
  "EXPECTED_ACCEPTED_RUN_ATTEMPT: ${{ needs.accepted-source.outputs.accepted_run_attempt }}",
  "EXPECTED_BASE_PATH: /${{ github.event.repository.name }}",
  "PAGE_URL: ${{ steps.deployment.outputs.page_url }}",
  "PAGES_AUTHORITY_REPORT_PATH: ${{ runner.temp }}/pages-deployment-authority.json",
  "node authority-source/scripts/release/pages-deployment-authority.mjs",
]) {
  requireText(
    pagesDeploymentAuthorityStep,
    marker,
    `sealed Pages deployment authority ${marker}`,
  );
}
requireExactYamlKeySet(
  pagesDeploymentAuthorityUploadStep,
  8,
  ["uses", "with"],
  "Pages deployment authority upload step",
);
for (const marker of [
  "actions/upload-artifact@v4",
  "clearra-pages-deployment-authority-${{ inputs.accepted_sha }}-run-${{ github.run_id }}-attempt-${{ github.run_attempt }}",
  "path: ${{ runner.temp }}/pages-deployment-authority.json",
  "if-no-files-found: error",
  "retention-days: 90",
]) {
  requireText(
    pagesDeploymentAuthorityUploadStep,
    marker,
    `Pages deployment authority evidence ${marker}`,
  );
}
if (
  (pagesWorkflow.match(/accepted-pages-build-\$\{\{ inputs\.accepted_sha \}\}-run-/gu) ?? [])
    .length !== 1
) {
  throw new Error("Pages must download exactly one source-bound accepted build artifact");
}
for (const marker of [
  "ACCEPTED_RUN_ID: ${{ needs.metadata.outputs.accepted_run_id }}",
  "ACCEPTED_RUN_ATTEMPT: ${{ needs.metadata.outputs.accepted_run_attempt }}",
  "node scripts/release/canonical-acceptance-run.mjs \\",
  '--repository "$GITHUB_REPOSITORY" \\',
  '--source-commit "$GITHUB_SHA" \\',
  "--require one \\",
  '--expected-run-id "$ACCEPTED_RUN_ID" \\',
  '--expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
  "node scripts/release/canonical-acceptance-evidence.mjs verify \\",
  "--report canonical-acceptance-evidence/clearra-canonical-acceptance-evidence.v1.json \\",
  '--version "${{ needs.metadata.outputs.version }}" \\',
  '--run-id "$ACCEPTED_RUN_ID" \\',
  '--run-attempt "$ACCEPTED_RUN_ATTEMPT" \\',
  '--base-path "/${{ github.event.repository.name }}" \\',
  "--products dist",
  "node scripts/release/finalize-discord-production-checkpoint.mjs verify-tag \\",
  "node scripts/release/finalize-discord-production-checkpoint.mjs verify-release \\",
  '--accepted-run-id "$ACCEPTED_RUN_ID" \\',
  '--accepted-run-attempt "$ACCEPTED_RUN_ATTEMPT" \\',
  "--acceptance-evidence canonical-acceptance-evidence/clearra-canonical-acceptance-evidence.v1.json",
  '--target "$GITHUB_SHA" \\',
]) {
  requireText(publishReleaseStep, marker, `late accepted-run validation ${marker}`);
}
requireText(
  publishReleaseStep,
  '--base-path "/${{ github.event.repository.name }}" \\\n            --products dist',
  "canonical acceptance verification product byte binding",
);
if (
  (publishReleaseStep.match(
    /node scripts\/release\/finalize-discord-production-checkpoint\.mjs verify-(?:tag|release)/gu,
  ) ?? []).length !== 2
) {
  throw new Error("tag publication must verify one checkpoint receipt before and after immutable publication");
}
const checkpointVerifyTagIndex = publishReleaseStep.indexOf(
  "finalize-discord-production-checkpoint.mjs verify-tag",
);
const releaseCreateIndex = publishReleaseStep.indexOf(
  'gh release create "$GITHUB_REF_NAME"',
);
const immutableReadbackIndex = publishReleaseStep.indexOf(
  "published release is not immutable",
);
const checkpointVerifyReleaseIndex = publishReleaseStep.indexOf(
  "finalize-discord-production-checkpoint.mjs verify-release",
);
if (
  checkpointVerifyTagIndex < 0 || releaseCreateIndex <= checkpointVerifyTagIndex ||
  immutableReadbackIndex <= releaseCreateIndex ||
  checkpointVerifyReleaseIndex <= immutableReadbackIndex
) {
  throw new Error("checkpoint tag and immutable Release verification order is invalid");
}
for (const marker of [
  "node scripts/release/release-publication-evidence.mjs recover \\",
  "--workflow-run-attempt \"$GITHUB_RUN_ATTEMPT\" \\",
  "--acceptance-evidence canonical-acceptance-evidence/clearra-canonical-acceptance-evidence.v1.json \\",
]) {
  requireText(publishReleaseStep, marker, `publication partial-draft recovery ${marker}`);
}
if (
  (workflow.match(/scripts\/tools\/run-release-regression-tests\.mjs/gu) ?? [])
    .length !== 1
) {
  throw new Error(
    "bounded release regression pool must have exactly one Linux metadata owner",
  );
}
requireMatch(
  linuxArchiveRegressionStep,
  /^        run: node scripts\/tools\/run-release-regression-tests\.mjs\s*$/mu,
  "executable bounded release regression pool command",
);
requireExactYamlScalar(
  metadataJob,
  "runs-on",
  "ubuntu-latest",
  "Linux metadata runner",
);
requireExactYamlLiteralScript(
  linuxAcceptedArchiveStep,
  [
    'archive_path="$RUNNER_TEMP/clearra-exact-source-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.tar.gz"',
    "archive_owned=false",
    'if [[ -e "$archive_path" ]]; then',
    "  echo 'exact source archive output already exists' >&2",
    "  exit 2",
    "fi",
    'trap \'if [[ "$archive_owned" == true ]]; then rm -f -- "$archive_path"; fi\' EXIT',
    "node scripts/release/create-exact-source-archive.mjs \\",
    '  --source-commit "$GITHUB_SHA" \\',
    '  --output "$archive_path"',
    "archive_owned=true",
    'test -s "$archive_path"',
  ],
  "Linux accepted source archive script",
);
if (
  (workflow.match(/scripts\/release\/create-exact-source-archive\.test\.mjs/gu) ?? [])
    .length !== 0
) {
  throw new Error(
    "exact source archive regression must remain inside the bounded metadata pool",
  );
}
if (
  (workflow.match(/scripts\/tools\/validate-release-cli-smokes\.test\.mjs/gu) ?? [])
    .length !== 0
) {
  throw new Error(
    "release workflow mutation tests must remain inside the bounded metadata pool",
  );
}
requireExactYamlScalar(
  releaseAcceptanceFoundationNoProductDebtJob,
  "runs-on",
  "windows-latest",
  "Windows foundation acceptance runner",
);
requireExactYamlLiteralScript(
  windowsAcceptedArchiveStep,
  [
    '$archivePath = Join-Path $env:RUNNER_TEMP ("clearra-exact-source-" + [Guid]::NewGuid().ToString("N") + ".tar.gz")',
    "$archiveOwned = $false",
    "try {",
    "  node scripts/release/create-exact-source-archive.mjs `",
    "    --source-commit $env:GITHUB_SHA `",
    "    --output $archivePath",
    '  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }',
    "  $archiveOwned = $true",
    "  $archive = Get-Item -LiteralPath $archivePath",
    '  if (-not $archive.Length) { throw "exact source archive is empty" }',
    "} finally {",
    "  if ($archiveOwned -and (Test-Path -LiteralPath $archivePath)) {",
    "    Remove-Item -LiteralPath $archivePath -Force",
    "  }",
    "}",
  ],
  "Windows accepted source archive script",
);
for (const [name, job] of [
  ["Linux metadata", metadataJob],
  ["Windows foundation NoProductDebt acceptance", releaseAcceptanceFoundationNoProductDebtJob],
  ["Windows foundation AdversarialCorrectness acceptance", releaseAcceptanceFoundationAdversarialCorrectnessJob],
  ["Windows foundation DesktopHost acceptance", releaseAcceptanceFoundationDesktopHostJob],
  ["Windows sanitizer acceptance", releaseAcceptanceSanitizerJob],
  ["Windows Rust acceptance", releaseAcceptanceRustJob],
  ["Windows Pages acceptance", releaseAcceptancePagesJob],
  ["Linux acceptance fan-in", releaseAcceptanceJob],
]) {
  requireExactYamlKeySet(
    job,
    4,
    name === "Linux metadata"
      ? ["outputs", "runs-on", "steps"]
      : name === "Linux acceptance fan-in"
        ? ["if", "needs", "runs-on", "steps"]
        : ["if", "needs", "runs-on", "steps", "timeout-minutes"],
    `${name} job`,
  );
  if (/^    continue-on-error\s*:/mu.test(job)) {
    throw new Error(`${name} job must fail closed`);
  }
}
requireExactYamlScalar(
  releaseAcceptanceFoundationNoProductDebtJob,
  "if",
  "github.event_name == 'workflow_dispatch'",
  "Windows foundation acceptance dispatch-only condition",
);
for (const [name, job] of [
  ["foundation NoProductDebt", releaseAcceptanceFoundationNoProductDebtJob],
  ["foundation AdversarialCorrectness", releaseAcceptanceFoundationAdversarialCorrectnessJob],
  ["foundation DesktopHost", releaseAcceptanceFoundationDesktopHostJob],
  ["sanitizer", releaseAcceptanceSanitizerJob],
  ["rust", releaseAcceptanceRustJob],
  ["Pages", releaseAcceptancePagesJob],
  ["fan-in", releaseAcceptanceJob],
]) {
  requireExactYamlScalar(
    job,
    "if",
    "github.event_name == 'workflow_dispatch'",
    `${name} acceptance dispatch-only condition`,
    4,
  );
}
requireExactYamlScalar(
  releaseAcceptanceFoundationNoProductDebtJob,
  "needs",
  "metadata",
  "foundation NoProductDebt acceptance metadata dependency",
  4,
);
requireExactYamlScalar(
  releaseAcceptanceFoundationAdversarialCorrectnessJob,
  "needs",
  "metadata",
  "foundation AdversarialCorrectness acceptance metadata dependency",
  4,
);
requireExactYamlScalar(
  releaseAcceptanceFoundationDesktopHostJob,
  "needs",
  "metadata",
  "foundation DesktopHost acceptance metadata dependency",
  4,
);
requireExactYamlScalar(
  releaseAcceptanceSanitizerJob,
  "needs",
  "metadata",
  "sanitizer acceptance metadata dependency",
  4,
);
requireExactYamlFlowSequence(
  releaseAcceptanceRustJob,
  "needs",
  ["metadata", "ctk3"],
  "Rust acceptance dependency on metadata and accepted CTK3",
);
requireExactYamlScalar(
  releaseAcceptancePagesJob,
  "needs",
  "metadata",
  "Pages acceptance metadata dependency",
  4,
);
requireExactYamlFlowSequence(
  releaseAcceptanceJob,
  "needs",
  [
    "metadata",
    "release-acceptance-foundation-no-product-debt",
    "release-acceptance-foundation-adversarial-correctness",
    "release-acceptance-foundation-desktop-host",
    "release-acceptance-sanitizer",
    "release-acceptance-rust",
    "release-acceptance-pages",
  ],
  "release acceptance exact six-shard fan-in dependencies",
);
requireExactYamlScalar(
  releaseAcceptanceJob,
  "runs-on",
  "ubuntu-latest",
  "release acceptance fan-in runner",
  4,
);
for (const [name, step, shell] of [
  ["Linux archive regression", linuxArchiveRegressionStep, "bash"],
  ["Linux accepted source archive", linuxAcceptedArchiveStep, "bash"],
  ["Windows accepted source archive", windowsAcceptedArchiveStep, "pwsh"],
]) {
  const metadataStep = name.startsWith("Linux");
  requireExactYamlKeySet(
    step,
    8,
    metadataStep ? ["if", "shell", "run"] : ["shell", "run"],
    `${name} step`,
  );
  if (metadataStep) {
    requireExactYamlScalar(
      step,
      "if",
      "github.event_name == 'workflow_dispatch'",
      `${name} dispatch-only condition`,
      8,
    );
  }
  requireExactYamlScalar(step, "shell", shell, `${name} shell`, 8);
  if (/^        continue-on-error\s*:/mu.test(step)) {
    throw new Error(`${name} step must fail closed`);
  }
}
for (const [name, prelude] of [
  ["Linux exact source archive", linuxProtectedPrelude],
  ["Windows exact source archive", windowsProtectedPrelude],
]) {
  requireExactStepSkeleton(
    prelude,
    name.startsWith("Linux")
      ? [
          "- uses: actions/checkout@v4",
          "- uses: actions/setup-node@v4",
          "- name: Require exact main and zero prior canonical success",
          "- name: Prepare approved legacy v0.7.4 annotated-tag fixture",
          "- name: Validate independent release regressions with bounded workers",
          "- name: Archive the exact accepted source on Linux",
        ]
      : [
          "- uses: actions/checkout@v4",
          "- uses: actions/setup-node@v4",
          "- name: Archive the exact accepted source on Windows",
        ],
    `${name} protected prelude`,
  );
}
requireExactNormalizedText(
  linuxCheckoutStep,
  "\n      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 1\n          fetch-tags: false",
  "Linux protected checkout step",
);
requireExactNormalizedText(
  linuxLegacyTagFixtureStep,
  "\n      - name: Prepare approved legacy v0.7.4 annotated-tag fixture\n        if: github.event_name == 'workflow_dispatch'\n        shell: bash\n        run: |\n          git fetch --no-tags --depth=1 origin refs/tags/v0.7.4:refs/tags/v0.7.4\n          tag_object=\"$(git rev-parse --verify 'refs/tags/v0.7.4^{tag}')\"\n          peeled_commit=\"$(git rev-parse --verify 'refs/tags/v0.7.4^{commit}')\"\n          if [[ \"$tag_object\" != 'a95973dbc1c3c1919478328d12e4d25ddaedea71' ]]; then\n            echo 'approved legacy tag ref does not resolve to the fixed annotated-tag object' >&2\n            exit 2\n          fi\n          if [[ \"$peeled_commit\" != '0438d85f90b47c4ce89835f6a6d665a0415aa25a' ]]; then\n            echo 'approved legacy annotated tag does not peel to the fixed source commit' >&2\n            exit 2\n          fi",
  "Linux approved legacy annotated-tag fixture step",
);
requireExactNormalizedText(
  linuxSetupNodeStep,
  "\n      - uses: actions/setup-node@v4\n        with:\n          node-version: 22",
  "Linux protected Node setup step",
);
requireExactNormalizedText(
  windowsCheckoutStep,
  "\n      - uses: actions/checkout@v4",
  "Windows protected checkout step",
);
requireExactNormalizedText(
  windowsSetupNodeStep,
  "\n      - uses: actions/setup-node@v4\n        with:\n          node-version: 22",
  "Windows protected Node setup step",
);
requireExactYamlFlowSequence(
  publishHeader,
  "needs",
  [
    "metadata",
    "release-acceptance",
    "linux-cli",
    "windows-cli",
    "windows-gui",
    "discord-bot",
  ],
  "publication dependency on every release acceptance job",
);
requireExactYamlScalar(
  publishHeader,
  "if",
  "always() && github.ref_type == 'tag' && needs.metadata.result == 'success'",
  "tag-only publication condition",
);
requireExactYamlKeySet(
  publishHeader,
  4,
  ["if", "needs", "runs-on"],
  "release publish header",
);
requireExactYamlScalar(
  publishHeader,
  "runs-on",
  "ubuntu-latest",
  "release publish runner",
);

console.log("Release CLI smoke contract passed.");

function section(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  if (start < 0 || end < 0) {
    throw new Error(
      `release workflow section is missing: ${startMarker.trim()}`,
    );
  }
  return source.slice(start, end);
}

function finalJobSection(source, startMarker) {
  const start = source.indexOf(startMarker);
  if (start < 0) {
    throw new Error(
      `release workflow final job is missing: ${startMarker.trim()}`,
    );
  }
  const job = source.slice(start);
  const remainder = job.slice(startMarker.length);
  for (const line of remainder.split(/\r?\n/u)) {
    if (/^ {2}\S/u.test(line) && !/^ {2}#/u.test(line)) {
      throw new Error(
        `${startMarker.trim()} must remain the final workflow job`,
      );
    }
  }
  return job;
}

function requireText(source, marker, description) {
  if (!source.includes(marker)) {
    throw new Error(`${description} is missing marker: ${marker}`);
  }
}

function requireMatch(source, pattern, description) {
  if (!pattern.test(source)) {
    throw new Error(`${description} is missing or malformed`);
  }
}

function requireExactYamlFlowSequence(source, key, expected, description) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const keyPattern = new RegExp(`^    ${escapedKey}\\s*:`, "gmu");
  if ([...source.matchAll(keyPattern)].length !== 1) {
    throw new Error(`${description} must have exactly one ${key} key`);
  }
  const match = new RegExp(
    `^    ${escapedKey}:\\s*(?:\\r?\\n\\s*)?\\[([^\\]]*)\\]\\s*$`,
    "mu",
  ).exec(source);
  if (!match) {
    throw new Error(`${description} is missing or malformed`);
  }
  const actual = match[1]
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  const actualSet = new Set(actual);
  if (
    actual.length !== expected.length ||
    actualSet.size !== expected.length ||
    expected.some((item) => !actualSet.has(item))
  ) {
    throw new Error(
      `${description} must be exactly [${expected.join(", ")}], got [${actual.join(", ")}]`,
    );
  }
}

function requireExactYamlScalar(
  source,
  key,
  expected,
  description,
  indentation = 4,
) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const escapedExpected = expected.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const prefix = " ".repeat(indentation);
  const matches = [
    ...source.matchAll(
      new RegExp(`^${prefix}${escapedKey}:\\s*${escapedExpected}\\s*$`, "gmu"),
    ),
  ];
  if (matches.length !== 1) {
    throw new Error(`${description} must be exactly ${key}: ${expected}`);
  }
}

function requireExactYamlKeySet(source, indentation, expected, description) {
  const prefix = " ".repeat(indentation);
  const actual = [];
  for (const line of source.split(/\r?\n/u)) {
    if (!line.startsWith(prefix) || line[indentation] === " ") continue;
    const content = line.slice(indentation);
    if (!content || content.startsWith("#")) continue;
    const match = /^([A-Za-z0-9_-]+):/u.exec(content);
    if (!match) {
      throw new Error(`${description} contains a noncanonical key: ${content}`);
    }
    actual.push(match[1]);
  }
  const expectedSet = new Set(expected);
  const actualSet = new Set(actual);
  if (
    expectedSet.size !== expected.length ||
    actual.length !== expected.length ||
    actualSet.size !== expected.length ||
    expected.some((key) => !actualSet.has(key))
  ) {
    throw new Error(
      `${description} keys must be exactly [${expected.join(", ")}], got [${actual.join(", ")}]`,
    );
  }
}

function requireExactYamlLiteralScript(source, expectedLines, description) {
  const lines = source.replaceAll("\r\n", "\n").split("\n");
  const runIndexes = [];
  for (let index = 0; index < lines.length; index += 1) {
    if (/^ {8}run: \|\s*$/u.test(lines[index])) runIndexes.push(index);
  }
  if (runIndexes.length !== 1) {
    throw new Error(
      `${description} must contain exactly one literal run block`,
    );
  }
  const scriptLines = lines.slice(runIndexes[0] + 1);
  while (scriptLines.at(-1) === "") scriptLines.pop();
  const actualLines = scriptLines.map((line) => {
    if (!line.startsWith(" ".repeat(10))) {
      throw new Error(
        `${description} contains a noncanonical script line: ${line}`,
      );
    }
    return line.slice(10);
  });
  if (
    actualLines.length !== expectedLines.length ||
    expectedLines.some((line, index) => actualLines[index] !== line)
  ) {
    throw new Error(
      `${description} must match the canonical fail-closed script exactly`,
    );
  }
}

function requireExactStepSkeleton(source, expected, description) {
  const actual = source
    .replaceAll("\r\n", "\n")
    .split("\n")
    .filter((line) => line.startsWith("      -"))
    .map((line) => line.slice(6));
  if (
    actual.length !== expected.length ||
    expected.some((entry, index) => actual[index] !== entry)
  ) {
    throw new Error(
      `${description} steps must be exactly [${expected.join(", ")}], got [${actual.join(", ")}]`,
    );
  }
}

function requireExactNormalizedText(source, expected, description) {
  const actual = source.replaceAll("\r\n", "\n");
  if (actual !== expected) {
    throw new Error(`${description} must match the canonical step exactly`);
  }
}

function requireTerminalSupplySmoke(source, platform) {
  for (const marker of [
    "--format",
    "json",
    "--include-solution-data",
    "pc-scenario",
    "--field",
    "0x1c0701c07",
    "--visible-height",
    "--queue",
    "STOILJZ",
    "--max-pieces",
    "--exact-pieces",
    "--count-policy",
    "count-unique",
    "--backend",
    "cpu",
    "--workers",
  ]) {
    requireText(
      source,
      marker,
      `${platform} terminal-supply fixture ${marker}`,
    );
  }
  if (source.includes("--no-hold")) {
    throw new Error(
      `${platform} terminal-supply fixture must preserve enabled empty hold`,
    );
  }
}

async function readStandardInput() {
  process.stdin.setEncoding("utf8");
  let raw = "";
  for await (const chunk of process.stdin) raw += chunk;
  return raw;
}

function validateTerminalSupplyJson(raw, expectedSourceCommit) {
  if (raw.trim().length === 0) {
    throw new Error("terminal-supply release asset returned empty JSON");
  }
  const response = JSON.parse(raw);
  requireEqual(response?.schema_version, 2, "schema_version");
  requireEqual(response?.kind, "pc-scenario", "kind");
  const expectedIdentity = {
    source_commit: expectedSourceCommit,
    engine_build_id: expectedSourceCommit,
    contract_schema_version: "clearra.search.contract.v2",
    supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
    artifact_schema_version: "clearra.solution-data.v1",
  };
  for (const [field, expected] of Object.entries(expectedIdentity)) {
    requireEqual(
      response?.runtime_identity?.[field],
      expected,
      `runtime_identity.${field}`,
    );
  }

  const expectedSummary = {
    actual_backend: "wasm-cpu",
    unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    normalized_unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_keys_complete: true,
    count_complete: true,
    supply_window_resolution: "projected-terminal-lookahead",
    projects_unplaced_lookahead: true,
    projects_standard_bag_lookahead: false,
    source_sequence_length: 7,
    total_possible_pattern_count: "1",
    normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH,
    actual_normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH,
  };
  for (const [field, expected] of Object.entries(expectedSummary)) {
    requireEqual(response?.summary?.[field], expected, `summary.${field}`);
  }

  requireEqual(
    response?.contract?.command?.kind,
    "pc-scenario",
    "contract.command.kind",
  );
  requireEqual(
    response?.contract?.command?.input_mode,
    "inline",
    "contract.command.input_mode",
  );
  requireEqual(
    response?.contract?.pc?.search?.backend_selected,
    "wasm-cpu",
    "contract.pc.search.backend_selected",
  );
  requireEqual(
    response?.contract?.pc?.execution_report?.replay?.trace_steps,
    7,
    "contract.pc.execution_report.replay.trace_steps",
  );
  requireEqual(
    response?.contract?.solution_data?.requested,
    true,
    "contract.solution_data.requested",
  );
  requireEqual(
    response?.contract?.solution_data?.status,
    "complete",
    "contract.solution_data.status",
  );
  requireEqual(
    response?.contract?.solution_data?.reason,
    null,
    "contract.solution_data.reason",
  );
  requireEqual(
    response?.contract?.artifacts?.schema_version,
    "clearra.solution-data.v1",
    "contract.artifacts.schema_version",
  );

  const solutionKeys = response?.contract?.artifacts?.solution_keys;
  if (!Array.isArray(solutionKeys)) {
    throw new Error("contract.artifacts.solution_keys must be an array");
  }
  requireEqual(
    solutionKeys.length,
    TERMINAL_SUPPLY_EXPECTED_COUNT,
    "solution_keys.length",
  );
  for (let index = 0; index < solutionKeys.length; index += 1) {
    const key = solutionKeys[index];
    if (typeof key !== "string") {
      throw new Error(`solution_keys[${index}] must be a string`);
    }
    if (index > 0 && solutionKeys[index - 1] >= key) {
      throw new Error("solution_keys must be strictly sorted and unique");
    }
    assertCanonicalTerminalSolutionKey(key);
  }

  const recomputedHash = normalizedSetHash(solutionKeys);
  requireEqual(
    recomputedHash,
    TERMINAL_SUPPLY_EXPECTED_HASH,
    "independently recomputed solution hash",
  );
  return Object.freeze({
    solutionCount: solutionKeys.length,
    normalizedSetHash: recomputedHash,
  });
}

function assertCanonicalTerminalSolutionKey(key) {
  const match = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/.exec(key);
  if (!match || BigInt(`0x${match[1]}`) !== TERMINAL_SUPPLY_INITIAL_MASK) {
    throw new Error(`non-canonical terminal-supply solution key: ${key}`);
  }

  const encodedPlacements = match[2].split(",");
  requireEqual(encodedPlacements.length, 7, "canonical placement count");
  const masksByPiece = new Map(
    CANONICAL_PIECE_ORDER.map((piece) => [piece, []]),
  );
  for (const encoded of encodedPlacements) {
    const placement = /^([IOTSZJL]):([0-9a-f]{16})$/.exec(encoded);
    if (!placement) throw new Error(`non-canonical placement: ${encoded}`);
    masksByPiece.get(placement[1]).push(BigInt(`0x${placement[2]}`));
  }

  let occupied = TERMINAL_SUPPLY_INITIAL_MASK;
  const canonicalPlacements = [];
  for (const piece of CANONICAL_PIECE_ORDER) {
    const masks = masksByPiece.get(piece).sort(compareBigInt);
    requireEqual(masks.length, 1, `canonical ${piece} placement count`);
    for (const mask of masks) {
      requireEqual(popcount(mask), 4, `canonical ${piece} cell count`);
      if ((occupied & mask) !== 0n) {
        throw new Error(`canonical ${piece} placement overlaps occupied cells`);
      }
      occupied |= mask;
      canonicalPlacements.push(`${piece}:${hex64(mask)}`);
    }
  }
  requireEqual(
    occupied,
    TERMINAL_SUPPLY_FULL_FIELD,
    "terminal-supply completed field mask",
  );
  requireEqual(
    key,
    `ctk1|initial=${hex64(TERMINAL_SUPPLY_INITIAL_MASK)}|placements=${canonicalPlacements.join(",")}`,
    "canonical solution key encoding",
  );
}

function normalizedSetHash(keys) {
  let hash = FNV_OFFSET;
  for (const key of [...keys].sort()) {
    for (const byte of Buffer.from(`${key}\0`, "utf8")) {
      hash ^= BigInt(byte);
      hash = (hash * FNV_PRIME) & U64_MASK;
    }
  }
  return `cts1:${hash.toString(16).padStart(16, "0")}`;
}

function popcount(value) {
  let count = 0;
  for (let remaining = value; remaining !== 0n; remaining &= remaining - 1n)
    count += 1;
  return count;
}

function compareBigInt(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function hex64(value) {
  return value.toString(16).padStart(16, "0");
}

function requireEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(
      `${label} mismatch: expected ${String(expected)}, received ${String(actual)}`,
    );
  }
}
