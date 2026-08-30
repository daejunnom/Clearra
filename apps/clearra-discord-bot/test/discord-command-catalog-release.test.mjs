import assert from "node:assert/strict";
import test from "node:test";

import { canonicalSha256 } from "../../../scripts/release/canonical-release-evidence.mjs";

import {
  createCanonicalDiscordCatalog,
  normalizeDiscordCatalog,
  restoreDiscordCatalogRelease,
  synchronizeDiscordCatalogRelease,
  validateCanonicalDiscordCatalog,
  validateDiscordCatalogRestoreReport,
  validateDiscordCatalogSnapshot,
  validateDiscordCatalogSyncReport,
} from "../scripts/discord-command-catalog-release.mjs";

const COMMIT = "1".repeat(40);
const APPLICATION_ID = "223456789012345678";

test("creates a deterministic canonical catalog without Discord response identity", () => {
  const catalog = createCanonicalDiscordCatalog({
    sourceCommit: COMMIT,
    commands: [
      { name: "zeta", type: 1, description: "Z" },
      { name: "alpha", description: "A" },
    ],
  });
  assert.equal(validateCanonicalDiscordCatalog(catalog, COMMIT), catalog);
  assert.deepEqual(catalog.commands.map((command) => command.name), ["alpha", "zeta"]);
  assert.match(catalog.catalog_sha256, /^[0-9a-f]{64}$/u);
  assert.throws(
    () => createCanonicalDiscordCatalog({
      sourceCommit: COMMIT,
      commands: [{ name: "help", id: "123456789012345678" }],
    }),
    /response-only metadata/u,
  );
  assert.throws(
    () => createCanonicalDiscordCatalog({
      sourceCommit: COMMIT,
      commands: [{ name: "help", api_token: "forbidden" }],
    }),
    /forbidden secret material/u,
  );
});

test("persists the exact prior snapshot before one sync write and seals readback", async () => {
  const expected = createCanonicalDiscordCatalog({
    sourceCommit: COMMIT,
    commands: [{ name: "help", type: 1, description: "New help" }],
  });
  const prior = [{ name: "help", type: 1, description: "Old help" }];
  let state = serverCatalog(prior, 1);
  let persisted = null;
  let writes = 0;
  const rest = {
    async getGlobalCommands() { return structuredClone(state); },
    async registerGlobalCommands(applicationId, commands) {
      assert.equal(applicationId, APPLICATION_ID);
      assert.ok(persisted, "prior snapshot must be durable before mutation");
      writes += 1;
      state = serverCatalog(commands, 2);
      return structuredClone(state);
    },
  };
  const times = [
    "2026-08-30T00:00:00.000Z",
    "2026-08-30T00:00:01.000Z",
  ];
  const { priorSnapshot, report } = await synchronizeDiscordCatalogRelease({
    rest,
    applicationId: APPLICATION_ID,
    sourceCommit: COMMIT,
    catalog: expected,
    async persistPriorSnapshot(snapshot) { persisted = structuredClone(snapshot); },
    now: () => times.shift(),
    synchronizationOptions: { retryDelayMs: 0, async wait() {} },
  });

  assert.equal(writes, 1);
  assert.deepEqual(persisted, priorSnapshot);
  assert.equal(
    validateDiscordCatalogSnapshot(priorSnapshot, {
      expectedSourceCommit: COMMIT,
      expectedApplicationId: APPLICATION_ID,
    }),
    priorSnapshot,
  );
  assert.equal(
    validateDiscordCatalogSyncReport(report, {
      expectedSourceCommit: COMMIT,
      expectedApplicationId: APPLICATION_ID,
      expectedCatalog: expected,
    }),
    report,
  );
  assert.equal(report.current_before_sha256, report.prior_catalog_sha256);
  assert.notEqual(report.current_after_sha256, report.prior_catalog_sha256);
  assert.equal(report.changed, true);
  assert.doesNotMatch(JSON.stringify({ priorSnapshot, report }), /token|secret/iu);
});

test("restore is conditional on the exact current digest and verifies the prior readback", async () => {
  const priorCommands = [{ name: "help", type: 1, description: "Old help" }];
  const candidateCommands = [{ name: "help", type: 1, description: "New help" }];
  const priorNormalized = normalizeDiscordCatalog(serverCatalog(priorCommands, 1), {
    allowResponseMetadata: true,
  });
  const unsignedPriorSnapshot = {
    schema_id: "clearra.discord.command-catalog-snapshot.v1",
    source_commit: COMMIT,
    application_id: APPLICATION_ID,
    captured_at: "2026-08-30T00:00:00.000Z",
    command_count: priorNormalized.length,
    catalog_sha256: catalogDigest(priorNormalized),
    commands: priorNormalized,
  };
  const priorSnapshot = {
    ...unsignedPriorSnapshot,
    snapshot_sha256: canonicalSha256(unsignedPriorSnapshot),
  };
  const candidateNormalized = normalizeDiscordCatalog(
    serverCatalog(candidateCommands, 2),
    { allowResponseMetadata: true },
  );
  const candidateDigest = catalogDigest(candidateNormalized);
  let state = serverCatalog(candidateCommands, 2);
  let writes = 0;
  const rest = {
    async getGlobalCommands() { return structuredClone(state); },
    async registerGlobalCommands(_applicationId, commands) {
      writes += 1;
      state = serverCatalog(commands, 3);
      return structuredClone(state);
    },
  };

  await assert.rejects(
    restoreDiscordCatalogRelease({
      rest,
      applicationId: APPLICATION_ID,
      sourceCommit: COMMIT,
      priorSnapshot,
      expectedCurrentDigest: "f".repeat(64),
    }),
    /current digest changed/u,
  );
  assert.equal(writes, 0);

  const times = [
    "2026-08-30T00:20:00.000Z",
    "2026-08-30T00:20:01.000Z",
  ];
  const report = await restoreDiscordCatalogRelease({
    rest,
    applicationId: APPLICATION_ID,
    sourceCommit: COMMIT,
    priorSnapshot,
    expectedCurrentDigest: candidateDigest,
    now: () => times.shift(),
    synchronizationOptions: { retryDelayMs: 0, async wait() {} },
  });
  assert.equal(writes, 1);
  assert.equal(
    validateDiscordCatalogRestoreReport(report, {
      expectedSourceCommit: COMMIT,
      expectedApplicationId: APPLICATION_ID,
    }),
    report,
  );
  assert.equal(report.current_before_sha256, candidateDigest);
  assert.equal(report.current_after_sha256, priorSnapshot.catalog_sha256);
});

test("sync and restore reports reject canonical-content tampering", async () => {
  const expected = createCanonicalDiscordCatalog({
    sourceCommit: COMMIT,
    commands: [{ name: "help", type: 1, description: "Help" }],
  });
  const state = serverCatalog(expected.commands, 1);
  const { report } = await synchronizeDiscordCatalogRelease({
    rest: {
      async getGlobalCommands() { return structuredClone(state); },
      async registerGlobalCommands() { throw new Error("unexpected write"); },
    },
    applicationId: APPLICATION_ID,
    sourceCommit: COMMIT,
    catalog: expected,
    async persistPriorSnapshot() {},
    now: () => "2026-08-30T00:00:00.000Z",
  });
  const tampered = { ...report, command_count: report.command_count + 1 };
  assert.throws(
    () => validateDiscordCatalogSyncReport(tampered),
    /SHA-256 differs/u,
  );
});

function serverCatalog(commands, versionSeed) {
  return commands.map((command, index) => ({
    id: `${versionSeed}${String(index + 1).padStart(17, "0")}`,
    application_id: APPLICATION_ID,
    version: `${versionSeed + 3}${String(index + 1).padStart(17, "0")}`,
    ...structuredClone(command),
  }));
}

function catalogDigest(commands) {
  const catalog = createCanonicalDiscordCatalog({
    sourceCommit: COMMIT,
    commands,
  });
  return catalog.catalog_sha256;
}
