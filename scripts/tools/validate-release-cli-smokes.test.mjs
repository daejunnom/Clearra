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

const [canonicalWorkflow, canonicalValidator, canonicalPackageScript] =
  await Promise.all([
    readFile(workflowPath, "utf8"),
    readFile(validatorPath, "utf8"),
    readFile(packageScriptPath, "utf8"),
  ]);
const normalizedWorkflow = canonicalWorkflow.replaceAll("\r\n", "\n");

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
    "      - name: Require the product capability and alias parser authority\n        run: |\n",
    "      - name: Require the product capability and alias parser authority\n        if: false\n        run: |\n",
  );
  const result = await runValidator(skippedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects a skipped upstream drift snapshot contract", async () => {
  const skippedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "          node --test scripts/tools/audit-upstream-drift.test.mjs\n",
    "          # node --test scripts/tools/audit-upstream-drift.test.mjs\n",
  );
  const result = await runValidator(skippedWorkflow);
  assert.notEqual(result.status, 0, diagnostic(result));
});

test("rejects removal of the final-source attempt journal regression", async () => {
  const weakenedWorkflow = replaceExactlyOnce(
    normalizedWorkflow,
    "node --test scripts/release/final-source-attempt-journal.test.mjs scripts/release/validate-final-source-revalidation.test.mjs",
    "node --test scripts/release/validate-final-source-revalidation.test.mjs",
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
        "  release-acceptance:\n    needs: metadata\n",
        "  release-acceptance:\n    needs: skip-acceptance\n",
      );
    },
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
        "      - name: Validate exact source archive regression coverage\n        shell: bash\n",
        "      - name: Validate exact source archive regression coverage\n        shell: bash\n        'continue-on-error': true\n",
      ),
  ],
  [
    "rejects a custom shell that only echoes the protected script path",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n        shell: bash\n",
        "      - name: Validate exact source archive regression coverage\n        shell: echo {0}\n",
      ),
  ],
  [
    "rejects a comment-only regression command",
    (source) =>
      replaceExactlyOnce(
        source,
        "      - name: Validate exact source archive regression coverage\n        shell: bash\n        run: node --test scripts/release/create-exact-source-archive.test.mjs scripts/tools/validate-release-cli-smokes.test.mjs\n",
        '      - name: Validate exact source archive regression coverage\n        shell: bash\n        run: "# node --test scripts/release/create-exact-source-archive.test.mjs scripts/tools/validate-release-cli-smokes.test.mjs"\n',
      ),
  ],
  [
    "rejects a parent-commit archive hidden behind the expected SHA comment",
    (source) =>
      replaceExactlyOnce(
        source,
        '            --source-commit "$GITHUB_SHA" \\\n',
        '            # --source-commit "$GITHUB_SHA" \\\n            --source-commit "$(git rev-parse HEAD^)" \\\n',
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

async function runValidator(workflow, packageScript = canonicalPackageScript) {
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
    await Promise.all([
      mkdir(dirname(fixtureValidator), { recursive: true }),
      mkdir(dirname(fixtureWorkflow), { recursive: true }),
    ]);
    await Promise.all([
      writeFile(fixtureValidator, canonicalValidator, "utf8"),
      writeFile(fixturePackageScript, packageScript, "utf8"),
      writeFile(fixtureWorkflow, workflow, "utf8"),
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
