import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { readFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { promisify } from "node:util";
import { discordP7p4Fixture, nativePathReceipt } from "../../../scripts/benchmark/discord-p7p4-runner.mjs";
import { assertDiscordCanonicalOnlyResult, prepareClearraArguments } from "../src/clearra/command.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArgumentPlan } from "../src/discord/slash-command-input.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";

const publicPc = findSlashCommand("pc");
const p7p4Options = [
  { name: "field", value: "grid:__________" },
  { name: "next", value: "P7P4" },
  { name: "hold", value: "empty" },
  { name: "kicktable", value: "srs-plus" },
];
const workerPolicy = { workers: 8, useAllLogicalProcessors: true,
  logicalProcessors: 8, outputFormat: "json", includeSolutionData: true };

function configuration(executable = "clearra") {
  return loadClearraJobServiceConfig({
    CLEARRA_EXECUTABLE: executable,
    CLEARRA_EXPECTED_VCPUS: "8", CLEARRA_SEARCH_WORKERS_PER_SESSION: "8",
    CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1", CLEARRA_MAX_CONCURRENT_JOBS: "1",
    CLEARRA_SEARCH_TIMEOUT_MS: "10000", CLEARRA_PC_SEARCH_TIMEOUT_MS: "10000",
    CLEARRA_MAX_OUTPUT_BYTES: "4194304", CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    CLEARRA_LISTEN_HOST: "127.0.0.1",
  });
}

test("P7P4 diagnostic defaults to actual public Path and preserves both automatic target requests", () => {
  const automatic = buildSlashCommandArgumentPlan(publicPc.subcommands.path, p7p4Options);
  assert.equal(automatic.automaticPcTargets, true);
  assert.equal(automatic.argumentSets.length, 2);
  for (const [index, mode, lines] of [[0, "path-2l", 2], [1, "path", 4]]) {
    const fixture = discordP7p4Fixture(mode);
    const explicit = buildSlashCommandArgumentPlan(publicPc.subcommands.path,
      [...p7p4Options, { name: "lines", value: lines }]);
    assert.deepEqual(fixture.arguments, automatic.argumentSets[index]);
    assert.deepEqual(fixture.arguments, explicit.argumentSets[0]);
    assert.equal(fixture.capabilityId, "pc.path");
    assert.equal(fixture.countPolicy, "CountAll");
    assert.equal(fixture.originalSequenceLength, 11);
    assert.equal(fixture.originalPatternCount, 4_233_600);
    assert.equal(fixture.arguments.includes("--count"), false);
    assert.deepEqual(prepareClearraArguments(fixture.arguments, workerPolicy), [
      ...fixture.arguments, "--auto-workers", "8", "--use-all-cpu-threads",
      "--format", "json", "--include-solution-data",
    ]);
  }
  assert.equal(discordP7p4Fixture().mode, "path");
  assert.equal(discordP7p4Fixture("legacy-unique").countPolicy, "CountUnique");
  assert.throws(() => discordP7p4Fixture("unique"), /diagnostic mode/u);
});

test("real runner prepares the complete original P7P4 request at eight declared workers without solving it", async () => {
  for (const mode of ["path", "path-2l"]) {
    const fixture = discordP7p4Fixture(mode);
    let captured;
    const runner = new ClearraCommandRunner(configuration(), {
      readOomKillCount: async () => null,
      spawn(file, argv, options) {
        captured = { file, argv, expectedVcpus: options.env.CLEARRA_EXPECTED_VCPUS };
        const child = new EventEmitter();
        child.stdout = new PassThrough(); child.stderr = new PassThrough();
        child.exitCode = null; child.signalCode = null; child.kill = () => true;
        queueMicrotask(() => { child.exitCode = 2; child.emit("close", 2, null); });
        return child;
      },
    });
    const result = await runner.execute({ arguments: fixture.arguments, timeoutClass: "pc_reverse",
      deadlineUnixMs: Date.now() + 10000, maxOutputBytes: 4 * 1024 * 1024 });
    assert.deepEqual(captured.argv, prepareClearraArguments(fixture.arguments, workerPolicy));
    assert.equal(captured.expectedVcpus, "8");
    assert.equal(result.failureStage, "execute");
  }
});

test("native diagnostic receipt retains only source identity and complete Path counts", () => {
  const receipt = nativePathReceipt(JSON.stringify({ kind: "pc-path-family.v2",
    runtime_identity: { source_commit: "a".repeat(40), engine_build_id: "a".repeat(40) },
    summary: { capability_id: "pc.path", problem_id: "bound-source",
      materialized_pattern_count: "4233600", witness_count: "123", complete: true,
      page_contract: "pc-replay-member-page.v2", page_source_available: true,
      page_source_identity_sha256: "b".repeat(64), geometry_count: "2",
      canonical_witness: { private: "not retained" }, witnesses: ["not retained"] },
  }));
  assert.equal(receipt.materializedPatternCount, "4233600");
  assert.equal(receipt.complete, true);
  assert.equal(JSON.stringify(receipt).includes("not retained"), false);
  assert.equal(nativePathReceipt("incomplete output"), null);
});

const executable = process.env.CLEARRA_PARITY_EXECUTABLE;
test("published CLI tiny fixtures match the exact Discord runner at eight declared workers", {
  skip: !executable, timeout: 30000,
}, async () => {
  assert.ok(isAbsolute(executable));
  const expectedDigest = process.env.CLEARRA_PARITY_EXECUTABLE_SHA256;
  assert.match(expectedDigest ?? "", /^[0-9a-f]{64}$/u);
  assert.equal(createHash("sha256").update(await readFile(executable)).digest("hex"), expectedDigest);
  const config = configuration(executable);
  for (const mode of ["path", "chance", "tiling"]) {
    const input = buildSlashCommandArgumentPlan(publicPc.subcommands[mode], [
      { name: "field", value: "grid:____XXXXXX" }, { name: "next", value: "I" },
      { name: "lines", value: 1 }, { name: "hold", value: "disabled" },
      ...(mode === "tiling" ? [] : [{ name: "kicktable", value: "srs-plus" }]),
    ]).argumentSets[0];
    let spawnedArguments;
    const runner = new ClearraCommandRunner(config, {
      spawn(file, argv, options) {
        spawnedArguments = [...argv];
        assert.equal(options.env.CLEARRA_EXPECTED_VCPUS, "8");
        return spawn(file, argv, options);
      },
    });
    const result = await runner.execute({ arguments: input, timeoutClass: "pc_reverse",
      deadlineUnixMs: Date.now() + 10000, maxOutputBytes: config.maxOutputBytes });
    assert.equal(result.exitCode, 0, result.stderr);
    assert.deepEqual(spawnedArguments, prepareClearraArguments(input, workerPolicy));
    const direct = await promisify(execFile)(executable, spawnedArguments, {
      encoding: "utf8", timeout: 10000, maxBuffer: 4 * 1024 * 1024,
      env: { ...process.env, CLEARRA_EXPECTED_VCPUS: "8" },
    });
    const expected = assertDiscordCanonicalOnlyResult({ exitCode: 0, signal: null,
      stdout: direct.stdout, stderr: direct.stderr });
    assert.deepEqual(JSON.parse(result.stdout), JSON.parse(expected.stdout), mode);
    if (mode === "path") {
      const native = JSON.parse(direct.stdout);
      assert.equal(native.summary.materialized_pattern_count, "1");
      assert.equal(native.summary.witness_count, "2");
      assert.equal(native.summary.complete, true);
      assert.equal(native.summary.canonical_witness.steps[0].rotation, "0");
      assert.equal(native.summary.canonical_witness.steps[0].board_after_line_clear_mask,
        "0x0000000000000000");
    }
  }
});
