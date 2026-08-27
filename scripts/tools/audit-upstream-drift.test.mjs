import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  auditUpstreamDrift,
  parseAuditCliArguments,
  parseActiveBotCommands,
  validateAuditSnapshot,
  writeAuditSnapshotNew,
} from "./audit-upstream-drift.mjs";

const MAN_COMMIT = "1111111111111111111111111111111111111111";
const BOT_COMMIT = "2222222222222222222222222222222222222222";
const SOLUTION_COMMIT = "3333333333333333333333333333333333333333";
const MOVED_COMMIT = "4444444444444444444444444444444444444444";
const OBSERVED_AT = "2026-08-26T16:28:57.675Z";

const manSource = `
@bot.command()
async def alpha(ctx):
    pass

@bot.command(
    aliases=["common"],
)
@commands.cooldown(
    1,
    15,
    commands.BucketType.guild,
)
async def shared(ctx):
    pass
`;

const botSource = `
@bot.command()
async def alpha(ctx):
    pass

@bot.command()
async def shared(ctx):
    pass

@bot.command(name="bot_only", aliases=["only"])
async def implementation_name(ctx):
    pass
`;

const registry = Object.freeze({
  schema_id: "clearra.product-capability-registry.v1",
  target_release: "v0.8.0",
  snapshot_date: "2026-08-23",
  upstream_sources: [
    {
      id: "sfinder-man",
      repository: "https://github.com/example/sfinder-man",
      commit: MAN_COMMIT,
      path: "main.py",
      expected_active_command_count: 2,
    },
    {
      id: "sfinderbot",
      repository: "https://github.com/example/sfinderbot",
      commit: BOT_COMMIT,
      path: "main.py",
      expected_sfinderbot_only_command_count: 1,
    },
    {
      id: "solution-finder",
      repository: "https://github.com/example/solution-finder",
      commit: SOLUTION_COMMIT,
      path: "docs/source/contents",
    },
  ],
  upstream_command_inventory: [
    { id: "sfinder-man/alpha", source_id: "sfinder-man", name: "alpha" },
    { id: "sfinder-man/shared", source_id: "sfinder-man", name: "shared" },
    {
      id: "sfinderbot/bot_only",
      source_id: "sfinderbot",
      name: "bot_only",
    },
  ],
});

test("parses active commands through multiline and stacked decorators", () => {
  const source = `
# @bot.command()
# async def commented(ctx):
#     pass

@bot.command(
    name="public_name",
    aliases=["alias"],
)
@commands.cooldown(1, 15,
    commands.BucketType.guild)
async def internal_name(ctx):
    pass

@bot.command()
def plain(ctx):
    pass
`;
  assert.deepEqual(parseActiveBotCommands(source), ["plain", "public_name"]);
});

test("rejects a command decorator without an owned function", () => {
  assert.throws(
    () => parseActiveBotCommands("@bot.command()\nvalue = 1\n"),
    /unexpected source/u,
  );
});

test("mocked no-drift audit preserves pins and exact command inventories", async () => {
  const audit = await mockedAudit();
  assert.equal(audit.status, "no-drift");
  assert.deepEqual(audit.drift_reasons, []);
  assert.equal(audit.sources.length, 3);
  assert.equal(audit.command_inventories[0].observed_count, 2);
  assert.equal(audit.command_inventories[1].observed_count, 1);
  assert.equal(
    validateAuditSnapshot(audit, registry, {
      expectedPhase: "implementation-start",
    }),
    true,
  );
});

test("a moved remote HEAD fails closed even when the selected file is unchanged", async () => {
  const audit = await mockedAudit({
    heads: new Map([
      ["https://github.com/example/sfinder-man", MOVED_COMMIT],
    ]),
  });
  assert.equal(audit.status, "drift-detected");
  assert.deepEqual(audit.drift_reasons, ["sfinder-man:head-moved"]);
  assert.equal(audit.sources[0].snapshot_matches_pin, true);
});

test("a pinned command inventory mismatch fails closed", async () => {
  const audit = await mockedAudit({
    man: manSource.replace("async def shared", "async def renamed"),
  });
  assert.equal(audit.status, "drift-detected");
  assert.ok(
    audit.drift_reasons.includes("sfinder-man:command-inventory-mismatch"),
  );
});

test("snapshot validation rejects forged no-drift booleans", async () => {
  const audit = structuredClone(await mockedAudit());
  audit.sources[0].observed_head = MOVED_COMMIT;
  assert.throws(
    () => validateAuditSnapshot(audit, registry),
    /head_matches_pin is inconsistent/u,
  );
});

test("an implementation-start snapshot cannot satisfy the release-freeze phase", async () => {
  const audit = await mockedAudit();
  assert.throws(
    () =>
      validateAuditSnapshot(audit, registry, {
        expectedPhase: "release-freeze",
      }),
    /audit phase mismatch/u,
  );
});

test("release-freeze validation rejects a stale registry snapshot identity", async () => {
  const audit = structuredClone(await mockedAudit({ phase: "release-freeze" }));
  audit.registry.snapshot_date = "2026-08-22";
  assert.throws(
    () => validateAuditSnapshot(audit, registry, { expectedPhase: "release-freeze" }),
    /audit registry identity does not match/u,
  );
});

test("implementation-start validation rejects impossible historical registry dates", async () => {
  const invalid = structuredClone(await mockedAudit());
  invalid.registry.snapshot_date = "not-a-date";
  assert.throws(
    () => validateAuditSnapshot(invalid, registry),
    /canonical ISO-8601 date/u,
  );
  const future = structuredClone(await mockedAudit());
  future.registry.snapshot_date = "2026-08-27";
  assert.throws(
    () => validateAuditSnapshot(future, registry),
    /audit registry identity does not match/u,
  );
});

test("the recorded implementation-start audit is structurally current", async () => {
  const [canonicalRegistry, snapshot] = await Promise.all([
    readJson(
      new URL(
        "../../tests/fixtures/contracts/product_capability_registry.v1.json",
        import.meta.url,
      ),
    ),
    readJson(
      new URL(
        "../../tests/fixtures/contracts/upstream_drift_implementation_start.v1.json",
        import.meta.url,
      ),
    ),
  ]);
  assert.equal(
    validateAuditSnapshot(snapshot, canonicalRegistry, {
      expectedPhase: "implementation-start",
    }),
    true,
  );
  assert.equal(snapshot.status, "no-drift");
});

test("release-freeze snapshot output is validated, new-file-only, and phase-bound", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-upstream-audit-"));
  try {
    const audit = await mockedAudit({ phase: "release-freeze" });
    const output = join(root, "release-freeze.json");
    assert.equal(
      await writeAuditSnapshotNew({
        audit,
        registry,
        outputPath: output,
        expectedPhase: "release-freeze",
      }),
      output,
    );
    assert.deepEqual(JSON.parse(await readFile(output, "utf8")), audit);
    await assert.rejects(
      writeAuditSnapshotNew({
        audit,
        registry,
        outputPath: output,
        expectedPhase: "release-freeze",
      }),
      (error) => error?.code === "EEXIST",
    );
    await assert.rejects(
      writeAuditSnapshotNew({
        audit,
        registry,
        outputPath: join(root, "wrong-phase.json"),
        expectedPhase: "implementation-start",
      }),
      /audit phase mismatch/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("audit CLI parser is strict while allowing an explicit output path", () => {
  assert.deepEqual(
    parseAuditCliArguments([
      "--output",
      "freeze.json",
      "--phase",
      "release-freeze",
    ]),
    { phase: "release-freeze", outputPath: "freeze.json" },
  );
  assert.throws(
    () => parseAuditCliArguments(["--phase", "release-freeze", "--future", "x"]),
    /unsupported upstream drift audit argument/u,
  );
  assert.throws(
    () => parseAuditCliArguments(["--phase", "release-freeze", "--phase", "implementation-start"]),
    /duplicate upstream drift audit argument/u,
  );
  assert.throws(
    () => parseAuditCliArguments(["--output", "freeze.json"]),
    /--phase must be/u,
  );
});

async function mockedAudit({
  heads = new Map(),
  man = manSource,
  phase = "implementation-start",
} = {}) {
  const files = new Map([
    ["https://github.com/example/sfinder-man", man],
    ["https://github.com/example/sfinderbot", botSource],
  ]);
  const commits = new Map(
    registry.upstream_sources.map((source) => [source.repository, source.commit]),
  );
  return auditUpstreamDrift({
    registry,
    phase,
    observedAt: OBSERVED_AT,
    resolveHead: async (repository) => heads.get(repository) ?? commits.get(repository),
    fetchFile: async (repository) => files.get(repository),
    fetchTree: async () => [
      {
        mode: "100644",
        path: "docs/source/contents/command.rst",
        sha: "5555555555555555555555555555555555555555",
        size: 12,
        type: "blob",
      },
      {
        mode: "100644",
        path: "outside.txt",
        sha: "6666666666666666666666666666666666666666",
        size: 7,
        type: "blob",
      },
    ],
  });
}

async function readJson(url) {
  return JSON.parse(await readFile(url, "utf8"));
}
