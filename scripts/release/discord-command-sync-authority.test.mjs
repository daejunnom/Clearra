import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  canonicalJson,
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";
import {
  createDiscordCommandSyncAuthority,
  DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID,
  parseDiscordCommandSyncAuthorityCliArguments,
  readDiscordCommandSyncAuthority,
  validateDiscordCommandSyncAuthority,
  writeDiscordCommandSyncAuthority,
} from "./discord-command-sync-authority.mjs";
import { sealAcceptedCtk3Dist } from "../tools/accepted-ctk3-dist.mjs";
import {
  createCanonicalDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

const SOURCE_COMMIT = "7".repeat(40);
const RUN_ID = "12345";
const RUN_ATTEMPT = "1";
const REPOSITORY = "daejunnom/Clearra";
const VERSION = "0.8.0";
const BASE_PATH = "/Clearra";
const HASH = "a".repeat(64);
const AUTHORITY_CLI = fileURLToPath(
  new URL("./discord-command-sync-authority.mjs", import.meta.url),
);

test("materializes one canonical authority from accepted CTK3, acceptance, and catalog", async () => {
  const fixture = await createFixture();
  const authority = await createDiscordCommandSyncAuthority(fixture.options);

  assert.equal(authority.schema_id, DISCORD_COMMAND_SYNC_AUTHORITY_SCHEMA_ID);
  assert.equal(authority.source_commit, SOURCE_COMMIT);
  assert.equal(authority.accepted_run_id, RUN_ID);
  assert.equal(authority.accepted_run_attempt, RUN_ATTEMPT);
  assert.equal(
    authority.accepted_ctk3_manifest_sha256,
    fixture.acceptedCtk3ManifestSha256,
  );
  assert.equal(
    authority.canonical_acceptance_evidence_sha256,
    fixture.acceptance.report_sha256,
  );
  assert.equal(authority.command_catalog_sha256, fixture.catalog.catalog_sha256);
  assert.equal(validateDiscordCommandSyncAuthority(authority, {
    sourceCommit: SOURCE_COMMIT,
    acceptedRunId: RUN_ID,
    acceptedRunAttempt: RUN_ATTEMPT,
    catalog: fixture.catalog,
    catalogFileSha256: fixture.catalogFileSha256,
  }), authority);

  const output = join(fixture.root, "discord-command-sync-authority.json");
  await writeDiscordCommandSyncAuthority(output, authority);
  const outputFileSha256 = sha256(await readFile(output));
  const readback = await readDiscordCommandSyncAuthority(
    output,
    outputFileSha256,
    {
      sourceCommit: SOURCE_COMMIT,
      catalog: fixture.catalog,
      catalogFileSha256: fixture.catalogFileSha256,
    },
  );
  assert.deepEqual(readback.authority, authority);
  assert.equal(readback.fileSha256, outputFileSha256);
});

test("rejects any divergence among CTK3, canonical acceptance, and catalog authorities", async () => {
  const mismatchedEvidence = await createFixture({
    acceptanceCtk3ManifestSha256: "f".repeat(64),
  });
  await assert.rejects(
    createDiscordCommandSyncAuthority(mismatchedEvidence.options),
    /accepted CTK3 manifest differs/u,
  );

  const modifiedDist = await createFixture();
  await writeFile(join(modifiedDist.ctk3, "index.js"), "tampered");
  await assert.rejects(
    createDiscordCommandSyncAuthority(modifiedDist.options),
    /does not match its sealed file set/u,
  );

  const fixture = await createFixture();
  const authority = await createDiscordCommandSyncAuthority(fixture.options);
  const output = join(fixture.root, "authority.json");
  await writeDiscordCommandSyncAuthority(output, authority);
  await assert.rejects(
    readDiscordCommandSyncAuthority(output, "0".repeat(64)),
    /file SHA-256 differs/u,
  );
  const differentCatalog = createCanonicalDiscordCatalog({
    sourceCommit: SOURCE_COMMIT,
    commands: [{ name: "different", type: 1, description: "Different" }],
  });
  await assert.rejects(
    readDiscordCommandSyncAuthority(
      output,
      sha256(await readFile(output)),
      { catalog: differentCatalog },
    ),
    /differs from the canonical catalog/u,
  );
});

test("authority validation is closed, hash-bound, and source/run-bound", async () => {
  const fixture = await createFixture();
  const authority = await createDiscordCommandSyncAuthority(fixture.options);

  assert.throws(
    () => validateDiscordCommandSyncAuthority({ ...authority, extra: true }),
    /fields differ/u,
  );
  assert.throws(
    () => validateDiscordCommandSyncAuthority({
      ...authority,
      accepted_run_attempt: "2",
    }),
    /SHA-256 differs/u,
  );
  assert.throws(
    () => validateDiscordCommandSyncAuthority(authority, {
      sourceCommit: "8".repeat(40),
    }),
    /source differs/u,
  );
  assert.throws(
    () => validateDiscordCommandSyncAuthority(authority, {
      acceptedRunId: "54321",
    }),
    /run ID differs/u,
  );
});

test("authority CLI requires every exact named argument once", () => {
  const args = [
    "--source-commit", SOURCE_COMMIT,
    "--repository", REPOSITORY,
    "--version", VERSION,
    "--base-path", BASE_PATH,
    "--accepted-run-id", RUN_ID,
    "--accepted-run-attempt", RUN_ATTEMPT,
    "--accepted-ctk3-dist", "ctk3",
    "--canonical-acceptance-evidence", "acceptance.json",
    "--catalog", "catalog.json",
    "--output", "authority.json",
  ];
  assert.equal(
    parseDiscordCommandSyncAuthorityCliArguments(args)["--accepted-run-id"],
    RUN_ID,
  );
  assert.throws(
    () => parseDiscordCommandSyncAuthorityCliArguments(args.slice(0, -2)),
    /--output is required/u,
  );
  assert.throws(
    () => parseDiscordCommandSyncAuthorityCliArguments([
      ...args,
      "--output", "duplicate.json",
    ]),
    /duplicate/u,
  );
  assert.throws(
    () => parseDiscordCommandSyncAuthorityCliArguments([
      ...args,
      "--future", "x",
    ]),
    /unsupported/u,
  );
});

test("authority CLI writes a new canonical hash-bound report", async () => {
  const fixture = await createFixture();
  const output = join(fixture.root, "authority-cli.json");
  const result = spawnSync(process.execPath, [
    AUTHORITY_CLI,
    "--source-commit", SOURCE_COMMIT,
    "--repository", REPOSITORY,
    "--version", VERSION,
    "--base-path", BASE_PATH,
    "--accepted-run-id", RUN_ID,
    "--accepted-run-attempt", RUN_ATTEMPT,
    "--accepted-ctk3-dist", fixture.options.acceptedCtk3DistPath,
    "--canonical-acceptance-evidence",
    fixture.options.canonicalAcceptanceEvidencePath,
    "--catalog", fixture.options.catalogPath,
    "--output", output,
  ], {
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /^clearra\.discord\.command-sync-authority\.v1 [0-9a-f]{64}\n$/u,
  );
  const authority = JSON.parse(await readFile(output, "utf8"));
  assert.equal(validateDiscordCommandSyncAuthority(authority), authority);

  const repeated = spawnSync(process.execPath, [
    AUTHORITY_CLI,
    "--source-commit", SOURCE_COMMIT,
    "--repository", REPOSITORY,
    "--version", VERSION,
    "--base-path", BASE_PATH,
    "--accepted-run-id", RUN_ID,
    "--accepted-run-attempt", RUN_ATTEMPT,
    "--accepted-ctk3-dist", fixture.options.acceptedCtk3DistPath,
    "--canonical-acceptance-evidence",
    fixture.options.canonicalAcceptanceEvidencePath,
    "--catalog", fixture.options.catalogPath,
    "--output", output,
  ], {
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(repeated.status, 2);
  assert.match(repeated.stderr, /failed/u);
});

async function createFixture({ acceptanceCtk3ManifestSha256 } = {}) {
  const root = await mkdtemp(join(tmpdir(), "clearra-discord-sync-authority-"));
  const ctk3 = join(root, "ctk3");
  await mkdir(ctk3);
  for (const [name, contents] of [
    ["decodeWorker.js", "decode"],
    ["index.cjs", "cjs"],
    ["index.d.ts", "types"],
    ["index.js", "esm"],
  ]) {
    await writeFile(join(ctk3, name), contents);
  }
  const acceptedCtk3Manifest = await sealAcceptedCtk3Dist(
    ctk3,
    SOURCE_COMMIT,
    RUN_ID,
    RUN_ATTEMPT,
  );
  const acceptedCtk3ManifestSha256 = canonicalSha256(acceptedCtk3Manifest);

  const catalog = createCanonicalDiscordCatalog({
    sourceCommit: SOURCE_COMMIT,
    commands: [{ name: "help", type: 1, description: "Help" }],
  });
  const catalogPath = join(root, "catalog.json");
  await writeCanonicalJson(catalogPath, catalog);
  const catalogFileSha256 = sha256(await readFile(catalogPath));

  const acceptance = createAcceptanceEvidence(
    acceptanceCtk3ManifestSha256 ?? acceptedCtk3ManifestSha256,
  );
  const acceptancePath = join(root, "canonical-acceptance.json");
  await writeCanonicalJson(acceptancePath, acceptance);

  return {
    root,
    ctk3,
    catalog,
    catalogFileSha256,
    acceptance,
    acceptedCtk3ManifestSha256,
    options: {
      sourceCommit: SOURCE_COMMIT,
      repository: REPOSITORY,
      version: VERSION,
      basePath: BASE_PATH,
      acceptedRunId: RUN_ID,
      acceptedRunAttempt: RUN_ATTEMPT,
      acceptedCtk3DistPath: ctk3,
      canonicalAcceptanceEvidencePath: acceptancePath,
      catalogPath,
    },
  };
}

function createAcceptanceEvidence(ctk3ManifestSha256) {
  const surfaces = ["desktop", "discord", "native", "wasm"];
  return sealCanonicalReport({
    schema_id: "clearra.canonical-acceptance-evidence.v1",
    repository: REPOSITORY,
    release_version: VERSION,
    pages_base_path: BASE_PATH,
    source_commit: SOURCE_COMMIT,
    run_id: RUN_ID,
    run_attempt: RUN_ATTEMPT,
    workflow_path: ".github/workflows/release-cli.yml",
    status: "passed",
    jobs: [
      "metadata",
      "ctk3",
      "linux-cli",
      "discord-bot",
      "release-acceptance",
      "windows-cli",
      "windows-gui",
    ].map((name, index) => ({
      name,
      job_id: String(9000 + index),
      status: "passed",
    })),
    accepted_inputs: {
      ctk3_manifest_sha256: ctk3ManifestSha256,
      pages_identity_sha256: "b".repeat(64),
      gate_index_sha256: "c".repeat(64),
    },
    final_source_fragments: {
      toolchains: {
        source_commit: SOURCE_COMMIT,
        manifest_sha256: HASH,
        rust: "rustc 1.91.0",
        node: "v22.18.0",
        wasm_bindgen: "wasm-bindgen 0.2.126",
      },
      canonical_gate: {
        id: `release-acceptance-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`,
        sha256: HASH,
        source_commit: SOURCE_COMMIT,
        status: "passed",
        readiness_open_count: 0,
      },
      surface_reports: surfaces.map((surface) => ({
        id: `${surface}-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`,
        sha256: HASH,
        source_commit: SOURCE_COMMIT,
        surface,
        status: "passed",
      })),
      release_artifacts: ["linux-cli", "windows-cli", "windows-gui"].map(
        (role, index) => ({
          role,
          name: `artifact-${index}`,
          sha256: HASH,
          size_bytes: index + 1,
          source_commit: SOURCE_COMMIT,
        }),
      ),
    },
  });
}

async function writeCanonicalJson(path, value) {
  await writeFile(path, `${canonicalJson(value)}\n`, "utf8");
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}
