import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../", import.meta.url));
const workflowPath = join(
  repositoryRoot,
  ".github",
  "workflows",
  "release-cli.yml",
);
const validatorPath = join(
  repositoryRoot,
  "scripts",
  "tools",
  "validate-release-cli-smokes.mjs",
);
const packageScriptPath = join(
  repositoryRoot,
  "scripts",
  "tools",
  "package-release-cli.sh",
);
const pagesWorkflowPath = join(
  repositoryRoot,
  ".github",
  "workflows",
  "pages.yml",
);

const [canonicalWorkflow, canonicalValidator, canonicalPackageScript, canonicalPagesWorkflow] =
  await Promise.all([
    readFile(workflowPath, "utf8"),
    readFile(validatorPath, "utf8"),
    readFile(packageScriptPath, "utf8"),
    readFile(pagesWorkflowPath, "utf8"),
  ]);
const normalizedWorkflow = canonicalWorkflow.replaceAll("\r\n", "\n");
const normalizedPagesWorkflow = canonicalPagesWorkflow.replaceAll("\r\n", "\n");

test("canonical release workflow passes the smoke validator", async () => {
  const result = await runValidator(normalizedWorkflow);
  assert.equal(result.status, 0, diagnostic(result));
  assert.match(result.stdout, /Release CLI smoke contract passed\./u);
});

test("rejects the generic Linux pc objective in place of canonical pc.tiling", async () => {
  const genericPackageScript = replaceExactlyOnce(
    canonicalPackageScript,
    "    --format json pc tiling --lines 2 --queue IIOOO --no-hold \\\n    --backend cpu --workers 1",
    "    --format json pc --lines 2 --queue IIOOO --no-hold \\\n    --objective tiling --backend cpu --workers 1",
  );
  const result = await runValidator(normalizedWorkflow, genericPackageScript);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a forged Linux pc.tiling result kind expectation", async () => {
  const forgedPackageScript = replaceExactlyOnce(
    canonicalPackageScript,
    '"kind":"pc-tiling-family.v1"',
    '"kind":"pc"',
  );
  const result = await runValidator(normalizedWorkflow, forgedPackageScript);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects the generic Windows pc objective in place of canonical pc.tiling", async () => {
  const genericWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "            -CommandArguments @('--format', 'json', 'pc', 'tiling', '--lines', '2', '--queue', 'IIOOO', '--no-hold', '--backend', 'cpu', '--workers', '1') `\n",
    "            -CommandArguments @('--format', 'json', 'pc', '--lines', '2', '--queue', 'IIOOO', '--no-hold', '--objective', 'tiling', '--backend', 'cpu', '--workers', '1') `\n",
  );
  const result = await runValidator(genericWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a Windows pc.tiling smoke without its typed result kind", async () => {
  const untypedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "            -ExpectedKind 'pc-tiling-family.v1' `\n",
    "",
  );
  const result = await runValidator(untypedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a skipped product capability registry authority", async () => {
  const skippedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "      - name: Require the product capability and alias parser authority\n        if: github.event_name == 'workflow_dispatch'\n        run: node --test tests/contracts/product_capability_registry.test.mjs\n",
    "      - name: Require the product capability and alias parser authority\n        if: false\n        run: node --test tests/contracts/product_capability_registry.test.mjs\n",
  );
  const result = await runValidator(skippedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a skipped upstream drift snapshot contract", async () => {
  const skippedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "      - name: Require the upstream drift audit authority\n        if: github.event_name == 'workflow_dispatch'\n        run: node --test scripts/tools/audit-upstream-drift.test.mjs\n",
    "      - name: Require the upstream drift audit authority\n        if: false\n        run: node --test scripts/tools/audit-upstream-drift.test.mjs\n",
  );
  const result = await runValidator(skippedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects removal of the final-source attempt journal regression", async () => {
  const canonicalCommand = "node --test scripts/release/canonical-acceptance-evidence.test.mjs scripts/release/final-source-attempt-journal.test.mjs scripts/release/final-source-event-contract.test.mjs scripts/release/final-source-stage-evidence.test.mjs scripts/release/observe-production-surfaces.test.mjs scripts/release/release-publication-evidence.test.mjs scripts/release/validate-final-source-revalidation.test.mjs";
  const weakenedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    canonicalCommand,
    canonicalCommand.replace("scripts/release/final-source-attempt-journal.test.mjs ", ""),
  );
  const result = await runValidator(weakenedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects disabled Linux typed pc.tiling result enforcement", async () => {
  const disabledPackageScript = replaceExactlyOnce(
    canonicalPackageScript,
    '            if (Object.hasOwn(expected, "kind") && parsed?.kind !== expected.kind) {',
    '            if (false && Object.hasOwn(expected, "kind") && parsed?.kind !== expected.kind) {',
  );
  const result = await runValidator(normalizedWorkflow, disabledPackageScript);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects disabled Windows typed pc.tiling result enforcement", async () => {
  const disabledWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "            if ($ExpectedKind -and $parsed.kind -ne $ExpectedKind) {",
    "            if ($false -and $ExpectedKind -and $parsed.kind -ne $ExpectedKind) {",
  );
  const result = await runValidator(disabledWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a Linux release package without Discord score-minimals validation", async () => {
  const weakenedPackageScript = replaceExactlyOnce(
    canonicalPackageScript,
    "            --validate-discord-score-minimals-json\n",
    "            --validate-terminal-supply-json\n",
  );
  const result = await runValidator(normalizedWorkflow, weakenedPackageScript);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a Linux release package without explicit tie rejection", async () => {
  const weakenedPackageScript = replaceExactlyOnce(
    canonicalPackageScript,
    "    if (!structured?.summary?.portfolio_alternative_page) {\n",
    "    if (false) {\n",
  );
  const result = await runValidator(normalizedWorkflow, weakenedPackageScript);
  assert.notEqual(result.status, 0, diagnostic(result));
});

for (const [name, mutate] of [
  [
    "rejects workflow defaults that can replace protected shells",
    (source) =>
      replaceExactlyOnce(
        source,
        "\npermissions:\n",
        "\ndefaults:\n  run:\n    shell: echo {0}\n\npermissions:\n",
      ),
  ],
  [
    "rejects removal of accepted-run lookup permission",
    (source) =>
      replaceExactlyOnce(
        source,
        "  actions: read\n",
        "  actions: none\n",
      ),
  ],
  [
    "rejects removal of exact-source release concurrency",
    (source) =>
      replaceExactlyOnce(
        source,
        "concurrency:\n  group: canonical-release-${{ github.sha }}\n  cancel-in-progress: false\n\n",
        "",
      ),
  ],
  [
    "rejects a canonical dispatch that permits a prior success",
    (source) =>
      replaceExactlyOnce(
        source,
        "            --require zero\n",
        "            --require one\n",
      ),
  ],
  [
    "rejects a canonical dispatch that permits workflow reruns",
    (source) =>
      replaceExactlyOnce(
        source,
        "          if [[ \"$GITHUB_RUN_ATTEMPT\" != '1' ]]; then\n            echo 'canonical release acceptance forbids workflow reruns; dispatch a fresh run after a failure' >&2\n            exit 2\n          fi\n",
        "",
      ),
  ],
  [
    "rejects metadata without the accepted run attempt",
    (source) =>
      replaceExactlyOnce(
        source,
        "      accepted_run_attempt: ${{ steps.accepted_run.outputs.accepted_run_attempt }}\n",
        "",
      ),
  ],
  [
    "rejects dispatch-only canonical acceptance binding",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Bind the exact canonical acceptance run\n        id: accepted_run\n",
        "      - name: Bind the exact canonical acceptance run\n        if: github.event_name == 'workflow_dispatch'\n        id: accepted_run\n",
      ),
  ],
  [
    "rejects a duplicate CTK product build in Linux metadata",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate every product version and changelog surface\n",
        "      - name: Build CTK3 workspace for product authority\n        if: github.event_name == 'workflow_dispatch'\n        run: npm run build --workspace @clearra/ctk3\n      - name: Validate every product version and changelog surface\n",
      ),
  ],
  [
    "rejects duplicate workflow mutation tests on Windows acceptance",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Archive the exact accepted source on Windows\n",
        "      - name: Duplicate release workflow mutation coverage on Windows\n        shell: pwsh\n        run: node --test scripts/tools/validate-release-cli-smokes.test.mjs\n      - name: Archive the exact accepted source on Windows\n",
      ),
  ],
  [
    "rejects duplicate Windows exact-source archive unit coverage",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Archive the exact accepted source on Windows\n",
        "      - name: Validate Windows exact source archive regression coverage\n        shell: pwsh\n        run: node --test scripts/release/create-exact-source-archive.test.mjs\n      - name: Archive the exact accepted source on Windows\n",
      ),
  ],
  [
    "rejects removal of the dedicated CTK3 build and test owner",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Build and test CTK3 once\n        run: npm test --workspace ctk3\n",
        "      - name: Build and test CTK3 once\n        run: echo skipped\n",
      ),
  ],
  [
    "rejects a publish-pattern name for the internal CTK3 artifact",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Upload accepted CTK3 distribution\n        uses: actions/upload-artifact@v4\n        with:\n          name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: packages/ctk3/dist\n          if-no-files-found: error\n",
        "      - name: Upload accepted CTK3 distribution\n        uses: actions/upload-artifact@v4\n        with:\n          name: clearra-ctk3-v${{ needs.metadata.outputs.version }}\n          path: packages/ctk3/dist\n          if-no-files-found: error\n",
      ),
  ],
  [
    "rejects a Discord suite that rebuilds CTK3",
    (source) =>
      replaceExactlyOnce(
        source,
        "        run: npm run test:built --workspace @clearra/discord-bot\n",
        "        run: npm test --workspace @clearra/discord-bot\n",
      ),
  ],
  [
    "rejects Discord consumption of a different CTK3 artifact",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Download accepted CTK3 distribution\n        uses: actions/download-artifact@v4\n        with:\n          name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: packages/ctk3/dist\n      - name: Install JavaScript workspace\n",
        "      - name: Download accepted CTK3 distribution\n        uses: actions/download-artifact@v4\n        with:\n          name: ctk3-unbound\n          path: packages/ctk3/dist\n      - name: Install JavaScript workspace\n",
      ),
  ],
  [
    "rejects canonical acceptance consumption of a different CTK3 artifact",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Download accepted CTK3 distribution\n        uses: actions/download-artifact@v4\n        with:\n          name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: packages/ctk3/dist\n      - uses: actions/cache@v4\n",
        "      - name: Download accepted CTK3 distribution\n        uses: actions/download-artifact@v4\n        with:\n          name: ctk3-unbound\n          path: packages/ctk3/dist\n      - uses: actions/cache@v4\n",
      ),
  ],
  [
    "rejects canonical acceptance without the accepted CTK3 release path",
    (source) =>
      replaceExactlyOnce(
        source,
        "          CLEARRA_ACCEPTED_CTK3_DIST: ${{ github.workspace }}/packages/ctk3/dist\n",
        "",
      ),
  ],
  [
    "rejects runtime-backed capability registry coverage in metadata",
    (source) =>
      replaceExactlyOnce(
        source,
        "        run: node --test scripts/tools/audit-upstream-drift.test.mjs\n",
        "        run: |\n          node --test scripts/tools/audit-upstream-drift.test.mjs\n          node --test tests/contracts/product_capability_registry.test.mjs\n",
      ),
  ],
  [
    "rejects a standalone release smoke validation outside its mutation owner",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Bind the exact canonical acceptance run\n",
        "      - name: Validate release CLI smoke wiring\n        if: github.event_name == 'workflow_dispatch'\n        run: node scripts/tools/validate-release-cli-smokes.mjs\n      - name: Bind the exact canonical acceptance run\n",
      ),
  ],
  [
    "rejects a preceding step that can poison protected executable resolution",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n",
        "      - name: Poison protected executable resolution\n        shell: bash\n        run: echo '/tmp/fake-tools' >> \"$GITHUB_PATH\"\n      - name: Validate exact source archive regression coverage\n",
      ),
  ],
  [
    "rejects a skipped dependency injected into the metadata root job",
    (source) =>
      replaceExactlyOnce(
        source,
        "jobs:\n  metadata:\n",
        "jobs:\n  skip-root:\n    if: false\n    runs-on: ubuntu-latest\n    steps: []\n\n  metadata:\n    needs: skip-root\n",
      ),
  ],
  [
    "rejects a skipped dependency substituted for canonical acceptance metadata",
    (source) => {
      const withSkippedJob = replaceExactlyOnce(
        source,
        "jobs:\n  metadata:\n",
        "jobs:\n  skip-acceptance:\n    if: false\n    runs-on: windows-latest\n    steps: []\n\n  metadata:\n",
      );
      return replaceExactlyOnce(
        withSkippedJob,
        "  release-acceptance:\n    if: github.event_name == 'workflow_dispatch'\n    needs: [metadata, ctk3]\n",
        "  release-acceptance:\n    if: github.event_name == 'workflow_dispatch'\n    needs: [skip-acceptance, ctk3]\n",
      );
    },
  ],
  [
    "rejects Discord without the accepted CTK3 dependency",
    (source) =>
      replaceExactlyOnce(
        source,
        "  discord-bot:\n    if: github.event_name == 'workflow_dispatch'\n    needs: [metadata, ctk3]\n",
        "  discord-bot:\n    if: github.event_name == 'workflow_dispatch'\n    needs: metadata\n",
      ),
  ],
  [
    "rejects canonical acceptance without the exact Pages base path",
    (source) =>
      replaceExactlyOnce(
        source,
        "          CLEARRA_WEB_BASE_PATH: /${{ github.event.repository.name }}\n",
        "",
      ),
  ],
  [
    "rejects an accepted Pages build without run-attempt binding",
    (source) =>
      source.replaceAll(
        "--accepted-run-attempt $env:GITHUB_RUN_ATTEMPT `",
        "--accepted-run-attempt 1 `",
      ),
  ],
  [
    "rejects a publication-pattern accepted Pages artifact name",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Upload accepted Pages build\n        uses: actions/upload-artifact@v4\n        with:\n          name: accepted-pages-build-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: apps/clearra-web/build\n",
        "      - name: Upload accepted Pages build\n        uses: actions/upload-artifact@v4\n        with:\n          name: clearra-pages-v${{ needs.metadata.outputs.version }}\n          path: apps/clearra-web/build\n",
      ),
  ],
  [
    "rejects a wrong runner whose comment contains the expected runner",
    (source) =>
      replaceExactlyOnce(
        source,
        "  metadata:\n    runs-on: ubuntu-latest\n",
        "  metadata:\n    runs-on: windows-latest # runs-on: ubuntu-latest\n",
      ),
  ],
  [
    "rejects noncanonical conditional keys with whitespace before the colon",
    (source) =>
      replaceExactlyOnce(
        source,
        "  metadata:\n    runs-on: ubuntu-latest\n",
        "  metadata:\n    runs-on: ubuntu-latest\n    if : false\n",
      ),
  ],
  [
    "rejects quoted continue-on-error on a protected step",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n        if: github.event_name == 'workflow_dispatch'\n        shell: bash\n",
        "      - name: Validate exact source archive regression coverage\n        if: github.event_name == 'workflow_dispatch'\n        shell: bash\n        'continue-on-error': true\n",
      ),
  ],
  [
    "rejects a custom shell that only echoes the protected script path",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n        if: github.event_name == 'workflow_dispatch'\n        shell: bash\n",
        "      - name: Validate exact source archive regression coverage\n        if: github.event_name == 'workflow_dispatch'\n        shell: echo {0}\n",
      ),
  ],
  [
    "rejects a comment-only regression command",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n        if: github.event_name == 'workflow_dispatch'\n        shell: bash\n        run: node --test scripts/release/create-exact-source-archive.test.mjs scripts/release/accepted-pages-build.test.mjs scripts/tools/validate-release-cli-smokes.test.mjs\n",
        '      - name: Validate exact source archive regression coverage\n        if: github.event_name == \'workflow_dispatch\'\n        shell: bash\n        run: "# node --test scripts/release/create-exact-source-archive.test.mjs scripts/release/accepted-pages-build.test.mjs scripts/tools/validate-release-cli-smokes.test.mjs"\n',
      ),
  ],
  [
    "rejects Linux product builds on tag publication runs",
    (source) =>
      replaceExactlyOnce(
        source,
        "  linux-cli:\n    if: github.event_name == 'workflow_dispatch'\n",
        "  linux-cli:\n    if: github.ref_type == 'tag'\n",
      ),
  ],
  [
    "rejects release acceptance on tag publication runs",
    (source) =>
      replaceExactlyOnce(
        source,
        "  release-acceptance:\n    if: github.event_name == 'workflow_dispatch'\n",
        "  release-acceptance:\n    if: github.ref_type == 'tag'\n",
      ),
  ],
  [
    "rejects release metadata tests on tag publication runs",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate release metadata regression coverage\n        if: github.event_name == 'workflow_dispatch'\n",
        "      - name: Validate release metadata regression coverage\n",
      ),
  ],
  [
    "rejects publication without always handling expected skipped jobs",
    (source) =>
      replaceExactlyOnce(
        source,
        "    if: always() && github.ref_type == 'tag' && needs.metadata.result == 'success'\n",
        "    if: github.ref_type == 'tag' && needs.metadata.result == 'success'\n",
      ),
  ],
  [
    "rejects metadata that binds publication to the tag run itself",
    (source) =>
      replaceExactlyOnce(
        source,
        "      accepted_run_id: ${{ steps.accepted_run.outputs.accepted_run_id }}\n",
        "      accepted_run_id: ${{ github.run_id }}\n",
      ),
  ],
  [
    "rejects tag publication that downloads artifacts from its own run",
    (source) =>
      replaceExactlyOnce(
        source,
        "          pattern: clearra-*-v${{ needs.metadata.outputs.version }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: dist\n          merge-multiple: true\n          run-id: ${{ needs.metadata.outputs.accepted_run_id }}\n",
        "          pattern: clearra-*-v${{ needs.metadata.outputs.version }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}\n          path: dist\n          merge-multiple: true\n",
      ),
  ],
  [
    "rejects accepted-run lookup without the exact head SHA binding",
    (source) =>
      replaceExactlyOnce(
        source,
        '            --source-commit "$GITHUB_SHA" \\\n            --require one \\\n            --format github-output >> "$GITHUB_OUTPUT"\n',
        '            --source-commit "$(git rev-parse HEAD^)" \\\n            --require one \\\n            --format github-output >> "$GITHUB_OUTPUT"\n',
      ),
  ],
  [
    "rejects late validation without the bound run attempt",
    (source) =>
      replaceExactlyOnce(
        source,
        '            --expected-run-id "$ACCEPTED_RUN_ID" \\\n            --expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"\n',
        '            --expected-run-id "$ACCEPTED_RUN_ID"\n',
      ),
  ],
  [
    "rejects a parent-commit archive hidden behind the expected SHA comment",
    (source) =>
      replaceExactlyOnce(
        source,
        '          node scripts/release/create-exact-source-archive.mjs \\\n            --source-commit "$GITHUB_SHA" \\\n            --output "$archive_path"\n',
        '          node scripts/release/create-exact-source-archive.mjs \\\n            # --source-commit "$GITHUB_SHA" \\\n            --source-commit "$(git rev-parse HEAD^)" \\\n            --output "$archive_path"\n',
      ),
  ],
  [
    "rejects publication dependencies spoofed in a later job",
    (source) => {
      const weakenedPublish = replaceExactlyOnce(
        source,
        "    needs:\n      [metadata, release-acceptance, linux-cli, windows-products, discord-bot]\n",
        "    needs: metadata\n",
      );
      return `${weakenedPublish}\n  audit-placeholder: # later job must not satisfy publish\n    needs:\n      [metadata, release-acceptance, linux-cli, windows-products, discord-bot]\n    runs-on: ubuntu-latest\n    steps: []\n`;
    },
  ],
  [
    "rejects canonical evidence without every accepted producer dependency",
    (source) =>
      replaceExactlyOnce(
        source,
        "    needs:\n      [metadata, ctk3, linux-cli, discord-bot, release-acceptance, windows-products]\n",
        "    needs: [metadata, ctk3, release-acceptance]\n",
      ),
  ],
  [
    "rejects a truncated canonical job-evidence lookup",
    (source) =>
      replaceExactlyOnce(
        source,
        "            -f per_page=100 > \"$RUNNER_TEMP/release-jobs.json\"\n",
        "            -f per_page=1 > \"$RUNNER_TEMP/release-jobs.json\"\n",
      ),
  ],
  [
    "rejects release gate evidence with a fixed run attempt",
    (source) =>
      replaceExactlyOnce(
        source,
        "            --run-attempt $env:GITHUB_RUN_ATTEMPT `\n            --output release-gate-evidence\n",
        "            --run-attempt 1 `\n            --output release-gate-evidence\n",
      ),
  ],
  [
    "rejects tag publication without canonical evidence verification",
    (source) =>
      replaceExactlyOnce(
        source,
        "          node scripts/release/canonical-acceptance-evidence.mjs verify \\\n",
        "          echo canonical-acceptance-evidence.mjs verify \\\n",
      ),
  ],
  [
    "rejects tag publication without downloaded product byte verification",
    (source) =>
      replaceExactlyOnce(
        source,
        '            --base-path "/${{ github.event.repository.name }}" \\\n            --products dist\n',
        '            --base-path "/${{ github.event.repository.name }}"\n',
      ),
  ],
  [
    "rejects ownership transfer moved before the archive helper succeeds",
    (source) => {
      const withoutSafeTransfer = replaceExactlyOnce(
        source,
        '          archive_owned=true\n          test -s "$archive_path"\n',
        '          # archive_owned=true\n          test -s "$archive_path"\n',
      );
      return replaceExactlyOnce(
        withoutSafeTransfer,
        "          node scripts/release/create-exact-source-archive.mjs \\\n",
        "          archive_owned=true\n          node scripts/release/create-exact-source-archive.mjs \\\n",
      );
    },
  ],
]) {
  test(name, async () => {
    const result = await runValidator(mutate(normalizedWorkflow));
    assert.notEqual(result.status, 0, diagnostic(result));
  });
}

for (const [name, mutate] of [
  [
    "rejects Pages acceptance lookup without GitHub-output binding",
    (source) =>
      replaceExactlyOnce(
        source,
        '            --format github-output >> "$GITHUB_OUTPUT"\n',
        "            --format summary\n",
      ),
  ],
  [
    "rejects Pages accepted-run resolver drift",
    (source) =>
      replaceExactlyOnce(
        source,
        "          node scripts/release/canonical-acceptance-run.mjs \\\n",
        "          node scripts/release/other-acceptance-run.mjs \\\n",
      ),
  ],
  [
    "rejects Pages download from an unbound workflow run",
    (source) =>
      replaceExactlyOnce(
        source,
        "          run-id: ${{ needs.accepted-source.outputs.accepted_run_id }}\n",
        "          run-id: ${{ github.run_id }}\n",
      ),
  ],
  [
    "rejects Pages rebuilding the already accepted GUI",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Configure Pages\n",
        "      - name: Rebuild accepted GUI\n        run: npm run build --workspace apps/clearra-web\n      - name: Configure Pages\n",
      ),
  ],
  [
    "rejects Pages without closed artifact verification",
    (source) =>
      replaceExactlyOnce(
        source,
        "          node scripts/release/accepted-pages-build.mjs \\\n",
        "          echo accepted-pages-build.mjs \\\n",
      ),
  ],
  [
    "rejects Pages deploy without the bound acceptance authority",
    (source) =>
      replaceExactlyOnce(
        source,
        "    needs: [accepted-source, build]\n",
        "    needs: build\n",
      ),
  ],
  [
    "rejects Pages upload without external artifact ID propagation",
    (source) =>
      replaceExactlyOnce(
        source,
        "      pages_artifact_id: ${{ steps.pages-artifact.outputs.artifact_id }}\n",
        "      pages_artifact_id: forged\n",
      ),
  ],
  [
    "rejects manually transcribed Pages rollback artifact authority",
    (source) =>
      replaceExactlyOnce(
        source,
        "      rollback_capture_run_id:\n        description: Successful Pages rollback capture workflow run ID\n        required: true\n        type: string\n",
        "      rollback_capture_run_id:\n        description: Successful Pages rollback capture workflow run ID\n        required: true\n        type: string\n      rollback_artifact_id:\n        description: forbidden manual authority\n        required: true\n        type: string\n",
      ),
  ],
  [
    "rejects Pages rollback admission without sealed report resolution",
    (source) =>
      replaceExactlyOnce(
        source,
        "          PAGES_AUTHORITY_MODE: resolve-forward\n",
        "          PAGES_AUTHORITY_MODE: forward\n",
      ),
  ],
  [
    "rejects Pages deployment without the tracked sealed authority producer",
    (source) =>
      replaceExactlyOnce(
        source,
        "        run: node authority-source/scripts/release/pages-deployment-authority.mjs\n",
        "        run: echo skipped\n",
      ),
  ],
  [
    "rejects short-lived Pages deployment authority evidence",
    (source) =>
      replaceExactlyOnce(
        source,
        "          retention-days: 90\n",
        "          retention-days: 1\n",
      ),
  ],
  [
    "rejects Pages download of a differently named accepted artifact",
    (source) =>
      replaceExactlyOnce(
        source,
        "          name: accepted-pages-build-${{ inputs.accepted_sha }}-run-${{ needs.accepted-source.outputs.accepted_run_id }}-attempt-${{ needs.accepted-source.outputs.accepted_run_attempt }}\n",
        "          name: accepted-pages-build-unbound\n",
      ),
  ],
  [
    "rejects deployed Pages authority without exact base-path binding",
    (source) =>
      replaceExactlyOnce(
        source,
        "          EXPECTED_ACCEPTED_RUN_ATTEMPT: ${{ needs.accepted-source.outputs.accepted_run_attempt }}\n          EXPECTED_BASE_PATH: /${{ github.event.repository.name }}\n",
        "          EXPECTED_ACCEPTED_RUN_ATTEMPT: ${{ needs.accepted-source.outputs.accepted_run_attempt }}\n          EXPECTED_BASE_PATH: /forged\n",
      ),
  ],
]) {
  test(name, async () => {
    const result = await runValidator(
      normalizedWorkflow,
      canonicalPackageScript,
      mutate(normalizedPagesWorkflow),
    );
    assert.notEqual(result.status, 0, diagnostic(result));
  });
}

async function runValidator(
  workflow,
  packageScript = canonicalPackageScript,
  pagesWorkflow = normalizedPagesWorkflow,
) {
  const fixtureRoot = await mkdtemp(join(tmpdir(), "clearra-release-smoke-"));
  try {
    const fixtureValidator = join(
      fixtureRoot,
      "scripts",
      "tools",
      "validate-release-cli-smokes.mjs",
    );
    const fixturePackageScript = join(
      fixtureRoot,
      "scripts",
      "tools",
      "package-release-cli.sh",
    );
    const fixtureWorkflow = join(
      fixtureRoot,
      ".github",
      "workflows",
      "release-cli.yml",
    );
    const fixturePagesWorkflow = join(
      fixtureRoot,
      ".github",
      "workflows",
      "pages.yml",
    );
    await Promise.all([
      mkdir(dirname(fixtureValidator), { recursive: true }),
      mkdir(dirname(fixtureWorkflow), { recursive: true }),
    ]);
    await Promise.all([
      writeFile(fixtureValidator, canonicalValidator, "utf8"),
      writeFile(fixturePackageScript, packageScript, "utf8"),
      writeFile(fixtureWorkflow, workflow, "utf8"),
      writeFile(fixturePagesWorkflow, pagesWorkflow, "utf8"),
    ]);
    const childEnvironment = { ...process.env };
    for (const name of ["NODE_OPTIONS", "BASH_ENV", "ENV"]) {
      delete childEnvironment[name];
    }
    return spawnSync(process.execPath, [fixtureValidator], {
      cwd: fixtureRoot,
      encoding: "utf8",
      env: childEnvironment,
      shell: false,
      windowsHide: true,
    });
  } finally {
    await rm(fixtureRoot, { force: true, recursive: true });
  }
}

function replaceExactlyOnce(source, search, replacement) {
  const first = source.indexOf(search);
  assert.notEqual(first, -1, `fixture marker is missing: ${search}`);
  assert.equal(
    source.indexOf(search, first + search.length),
    -1,
    `fixture marker is ambiguous: ${search}`,
  );
  return `${source.slice(0, first)}${replacement}${source.slice(first + search.length)}`;
}

function diagnostic(result) {
  return [
    `status=${result.status}`,
    `signal=${result.signal}`,
    `stdout=${result.stdout}`,
    `stderr=${result.stderr}`,
    `error=${result.error?.message ?? "none"}`,
  ].join("\n");
}
