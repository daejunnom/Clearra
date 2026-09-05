import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  generateBootstrap,
  parseCanonicalManifest,
  validateManifest,
} from "./oracle/create-inactive-stage-v080.mjs";

const generator = fileURLToPath(
  new URL("./oracle/create-inactive-stage-v080.mjs", import.meta.url),
);
const wrapperTest = fileURLToPath(
  new URL("./oracle/invoke-inactive-stage-v080.test.ps1", import.meta.url),
);

function digest(character) {
  return character.repeat(64);
}

function sampleManifest() {
  const sourceCommit = "0123456789abcdef0123456789abcdef01234567";
  return {
    schemaVersion: "clearra.oracle.inactive-stage.v080.v1",
    sourceCommit,
    releaseId: "v0.8.0-0123456",
    active: {
      releasePath: "/opt/clearra/releases/v0.7.5-042ec21",
      treeSha256: digest("a"),
      settingsSha256: digest("b"),
      settingsSize: 367,
      configSha256: digest("c"),
      configSize: 3432,
    },
    candidate: {
      treeSha256: digest("d"),
      counts: {
        directories: 500,
        files0644: 3200,
        files0755: 8,
        symlinks: 2,
      },
    },
    layers: {
      source: {
        sha256: digest("e"),
        size: 4_100_000,
        counts: { files: 3100, directories: 480, symlinks: 0 },
      },
      overlay: {
        sha256: digest("f"),
        size: 470_000,
        counts: { files: 33, directories: 1, symlinks: 0 },
      },
      ctk3Dist: {
        sha256: digest("1"),
        size: 940_000,
        counts: { files: 26, directories: 1, symlinks: 0 },
      },
      dependencies: {
        sha256: digest("2"),
        size: 110_000,
        counts: { files: 24, directories: 5, symlinks: 2 },
      },
    },
    tools: {
      launcher: {
        sha256: digest("3"),
        size: 20_000,
        prior: { sha256: digest("4"), size: 19_000 },
      },
      digester: {
        sha256: digest("5"),
        size: 3_100,
        prior: null,
      },
    },
  };
}

function canonicalBytes(value) {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
}

test("v0.8 Oracle inactive-stage generator binds every frozen authority", () => {
  const manifest = validateManifest(sampleManifest());
  const bootstrap = generateBootstrap(manifest);
  const text = bootstrap.toString("utf8");
  assert.match(text, /^source_commit=0123456789abcdef0123456789abcdef01234567$/mu);
  assert.match(text, /^release_id=v0\.8\.0-0123456$/mu);
  assert.match(text, /^expected_tree_sha256=d{64}$/mu);
  assert.match(text, /^expected_source_sha256=e{64}$/mu);
  assert.match(text, /^expected_prior_digester_sha256=absent$/mu);
  assert.match(text, /\.clearra-v080-upload-0123456-\$stage_nonce/u);
  assert.match(text, /install_canonical_tools/u);
  assert.match(text, /oracle_inactive_stage=recovery-failed/u);
  assert.match(text, /remove_success_root "\$upload_root"/u);
  assert.match(text, /! -name clearra-oracle-inactive-stage-v080/u);
  assert.match(
    text,
    /upload_self=\$upload_root\/clearra-oracle-inactive-stage-v080/u,
  );
  assert.match(text, /validate_digester=\$2/u);
  assert.match(
    text,
    /require_exact_hash "\$validate_digester" "\$expected_digester_sha256"/u,
  );
  assert.match(
    text,
    /validate_digest=\$\(\/usr\/bin\/python3 "\$validate_digester" "\$validate_root"\)/u,
  );
  const cleanupStart = text.indexOf('if [ "$cleanup_only" -eq 1 ]; then');
  const cleanupEnd = text.indexOf("\nfi\n\ncapture_baseline", cleanupStart);
  assert.notEqual(cleanupStart, -1, "CleanupOnly branch is missing");
  assert.notEqual(cleanupEnd, -1, "CleanupOnly branch has no closed boundary");
  const cleanupBranch = text.slice(cleanupStart, cleanupEnd);
  assert.match(
    cleanupBranch,
    /cleanup_digester=\$upload_root\/clearra-release-tree-digest\.py[\s\S]*validate_candidate "\$candidate_path" "\$cleanup_digester"/u,
  );
  assert.doesNotMatch(
    cleanupBranch,
    /\$input_root\/clearra-release-tree-digest\.py/u,
  );
  assert.match(
    text,
    /validate_candidate "\$candidate_path" "\$input_root\/clearra-release-tree-digest\.py"/u,
  );
  assert.match(text, /oracle_source_commit=\$source_commit/u);
  assert.match(
    text,
    /"apps\/clearra-discord-bot\/src\/admin"\),/u,
  );
  assert.match(text, /"packages\/ctk3\/dist"\),/u);
  assert.match(text, /"node_modules"\),/u);
  assert.match(text, /apps\/clearra-discord-bot\/src\/admin\/config\.mjs/u);
  assert.doesNotMatch(text, /@[A-Z0-9_]+@/u);
  assert.doesNotMatch(text, /\.clearra-v075-upload/u);

  const syntax =
    process.platform === "win32"
      ? spawnSync("wsl.exe", ["-e", "dash", "-n", "-"], {
          input: bootstrap,
          encoding: "utf8",
        })
      : spawnSync("dash", ["-n", "-"], { input: bootstrap, encoding: "utf8" });
  assert.equal(syntax.status, 0, syntax.stderr);
});

test("v0.8 Oracle manifest parser requires canonical closed JSON", () => {
  const value = sampleManifest();
  assert.equal(parseCanonicalManifest(canonicalBytes(value)).releaseId, value.releaseId);
  assert.throws(
    () => parseCanonicalManifest(Buffer.from(JSON.stringify(value), "utf8")),
    /canonical UTF-8 JSON|canonical JSON/u,
  );
  assert.throws(
    () => validateManifest({ ...value, unexpected: true }),
    /closed schema/u,
  );
  assert.throws(
    () => validateManifest({ ...value, releaseId: "v0.8.0-fffffff" }),
    /exact-source identity/u,
  );
  assert.throws(
    () =>
      validateManifest({
        ...value,
        active: {
          ...value.active,
          releasePath: `/opt/clearra/releases/${value.releaseId}`,
        },
      }),
    /distinct from the inactive candidate/u,
  );
  assert.throws(
    () =>
      validateManifest({
        ...value,
        layers: {
          ...value.layers,
          source: {
            ...value.layers.source,
            counts: { ...value.layers.source.counts, symlinks: 1 },
          },
        },
      }),
    /two-link dependency contract/u,
  );
});

test("v0.8 Oracle generator CLI audits, creates once, and checks exact bytes", () => {
  const directory = mkdtempSync(join(tmpdir(), "clearra-oracle-stage-test-"));
  try {
    const manifestPath = join(directory, "manifest.json");
    const outputPath = join(directory, "bootstrap");
    writeFileSync(manifestPath, canonicalBytes(sampleManifest()), { flag: "wx" });

    const audit = spawnSync(
      process.execPath,
      [generator, "--manifest", manifestPath, "--audit"],
      { encoding: "utf8" },
    );
    assert.equal(audit.status, 0, audit.stderr);
    assert.match(audit.stdout, /^oracle_stage_manifest=ok$/mu);
    assert.match(audit.stdout, /^oracle_stage_mode=audit$/mu);

    const output = spawnSync(
      process.execPath,
      [generator, "--manifest", manifestPath, "--output", outputPath],
      { encoding: "utf8" },
    );
    assert.equal(output.status, 0, output.stderr);
    assert.equal(
      createHash("sha256").update(readFileSync(outputPath)).digest("hex"),
      /^oracle_bootstrap_sha256=([0-9a-f]{64})$/mu.exec(output.stdout)?.[1],
    );

    const duplicate = spawnSync(
      process.execPath,
      [generator, "--manifest", manifestPath, "--output", outputPath],
      { encoding: "utf8" },
    );
    assert.notEqual(duplicate.status, 0);

    const check = spawnSync(
      process.execPath,
      [generator, "--manifest", manifestPath, "--check", outputPath],
      { encoding: "utf8" },
    );
    assert.equal(check.status, 0, check.stderr);
    assert.match(check.stdout, /^oracle_stage_mode=check$/mu);

    writeFileSync(outputPath, "tampered", { flag: "w" });
    const tampered = spawnSync(
      process.execPath,
      [generator, "--manifest", manifestPath, "--check", outputPath],
      { encoding: "utf8" },
    );
    assert.notEqual(tampered.status, 0);
    assert.match(tampered.stderr, /bytes do not match/u);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("v0.8 Oracle inactive-stage wrapper passes its cross-host audit", () => {
  const wrapperAudit = spawnSync(
    "pwsh",
    ["-NoProfile", "-File", wrapperTest],
    { encoding: "utf8", windowsHide: true },
  );
  assert.equal(
    wrapperAudit.status,
    0,
    wrapperAudit.error?.message ?? wrapperAudit.stderr ?? wrapperAudit.stdout,
  );
  assert.match(
    wrapperAudit.stdout,
    /^oracle_inactive_stage_wrapper_test=pass$/mu,
  );
});
