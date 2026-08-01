import assert from "node:assert/strict";
import test from "node:test";

import {
  ClearraProcessExecutor,
  parseClearraMessage,
  prepareClearraArguments,
  tilingOnlyRequested,
  tokenizeCommand,
} from "../src/clearra/command.mjs";
import {
  defaultSearchWorkersPerSession,
  loadDiscordBotConfig,
} from "../src/config.mjs";

test("Clearrabot defaults each search session to a three-minute timeout", () => {
  assert.equal(
    loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" }).searchTimeoutMs,
    180_000,
  );
});

test("Clearrabot derives each session's workers from the host-visible CPU limit", () => {
  const runtime = { availableParallelism: () => 8 };
  const config = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" }, runtime);
  assert.equal(config.processLogicalProcessors, 8);
  assert.equal(config.searchWorkersPerSession, 7);
  assert.equal(config.useAllLogicalProcessors, false);
  assert.equal(defaultSearchWorkersPerSession(8, 2), 3);
  assert.equal(
    loadDiscordBotConfig(
      {
        DISCORD_TOKEN: "test-token",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      },
      runtime,
    ).searchWorkersPerSession,
    7,
  );

  const shared = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "3",
    },
    runtime,
  );
  assert.equal(shared.searchWorkersPerSession, 3);
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "4",
        },
        runtime,
      ),
    /runtime limit of 3/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "8",
        },
        runtime,
      ),
    /runtime CPU capacity of 7/,
  );
});

test("command tokenizer preserves quoted queue syntax", () => {
  assert.deepEqual(
    tokenizeCommand('pc --lines 4 --patterns "[LOJ]!P7"'),
    ["pc", "--lines", "4", "--patterns", "[LOJ]!P7"],
  );
});

test("Discord commands always use the Clearra exact product path", () => {
  assert.deepEqual(
    prepareClearraArguments([
      "pc",
      "--lines",
      "4",
      "--tablebase",
      "--build-dependency-dag",
      "--format",
      "json",
    ]),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    parseClearraMessage("!setup --remaining SZ --priority pc"),
    [
      "setup",
      "--remaining",
      "SZ",
      "--priority",
      "pc",
      "--no-tablebase",
      "--format",
      "text",
    ],
  );
});

test("Discord owns the worker allocation instead of accepting a user override", () => {
  assert.deepEqual(
    prepareClearraArguments(
      [
        "pc",
        "--lines",
        "4",
        "--workers",
        "99",
        "--use-all-cpu-threads",
      ],
      { workers: 3 },
    ),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "setup",
        "--remaining",
        "SZ",
        "--workers",
        "99",
        "--use-all-cpu-threads",
      ],
      { workers: 3 },
    ),
    [
      "setup",
      "--remaining",
      "SZ",
      "--no-tablebase",
      "--workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(["percent", "--queue", "P7"], { workers: 3 }),
    ["percent", "--queue", "P7", "--format", "text"],
  );
});

test("tiling-only commands are recognized after Discord argument normalization", () => {
  assert.equal(
    tilingOnlyRequested(prepareClearraArguments(["pc", "--lines", "4", "--tiling-only"])),
    true,
  );
  assert.equal(
    tilingOnlyRequested(
      prepareClearraArguments(["pc", "--lines", "4", "--objective", "tiling"]),
    ),
    true,
  );
  assert.equal(
    tilingOnlyRequested(prepareClearraArguments(["pc", "--lines", "4"])),
    false,
  );
});

test("file-backed and unrelated commands are rejected", () => {
  assert.throws(
    () => prepareClearraArguments(["pc-scenario", "--fixture", "field.json"]),
    /pc, setup, path, percent, and cover/,
  );
  assert.throws(
    () => prepareClearraArguments(["pc", "--fixture", "field.json"]),
    /File and custom-code inputs/,
  );
});

test("Clearra executor passes arguments without shell interpretation", async () => {
  const executor = new ClearraProcessExecutor({
    executable: process.execPath,
    timeoutMs: 5_000,
  });
  const result = await executor.execute([
    "-e",
    "process.stdout.write(JSON.stringify(process.argv.slice(1)))",
    "queue with spaces",
    "literal;&|$()",
  ]);

  assert.equal(result.exitCode, 0);
  assert.deepEqual(JSON.parse(result.stdout), [
    "queue with spaces",
    "literal;&|$()",
  ]);
});

test("Clearra executor terminates a search when the Clearrabot timeout expires", async () => {
  const executor = new ClearraProcessExecutor({
    executable: process.execPath,
    timeoutMs: 50,
  });

  await assert.rejects(
    executor.execute(["-e", "setInterval(() => {}, 1000)"]),
    /Clearrabot search exceeded the 50-millisecond time limit/,
  );
});
