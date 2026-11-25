import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import test from "node:test";

import { buildCtk3Result } from "../src/clearra/ctk3-result.mjs";
import { prepareClearraArguments, searchTimeoutClass } from "../src/clearra/command.mjs";
import { parseClearraTextRequest } from "../src/clearra/text-command.mjs";
import { findProductCapability } from "../src/discord/capability-registry.mjs";
import { buildCommandModalResponse } from "../src/discord/field-modal.mjs";
import { validDiscordPcSolutionsResult } from "../src/discord/pc-solutions-result.mjs";
import { findSlashCommand, formatSlashCommandHelp, globalCommands } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArgumentPlan, buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";

const solutions = findSlashCommand("pc").subcommands.solutions;
const execution = { workers: 8, logicalProcessors: 8, useAllLogicalProcessors: true,
  outputFormat: "json", includeSolutionData: true };
const option = (name, value) => ({ name, value });
const base = [option("field", "grid:__________"), option("next", "P7P4"), option("lines", 4)];
const value = (args, key) => args[args.indexOf(key) + 1];

test("Discord solutions owns an explicit native Unique route and a localized guided form", () => {
  const capability = findProductCapability("pc.solutions");
  assert.equal(capability.resultContractId, "pc-scenario");
  assert.deepEqual(capability.resultAllowlist, ["pc", "pc-scenario"]);
  assert.deepEqual(capability.engine.argvPrefix, ["pc", "--count", "unique"]);
  assert.equal(capability.engine.pcObjective, "unique");
  assert.equal(capability.engine.fixedSemantics.completeReplayPaths, false);
  assert.deepEqual(solutions.registration.options.map(({ name }) => name), [
    "next", "field", "lines", "hold", "kicktable", "queue-knowledge", "spin-profile",
    "preserve-b2b", "solution-probabilities",
  ]);
  assert.equal(searchTimeoutClass(buildSlashCommandArguments(solutions, base), "pc_reverse"), "pc_reverse");
  assert.doesNotMatch(formatSlashCommandHelp("pc solutions", "en"), /Above 6 PC lines/u);
  assert.doesNotMatch(formatSlashCommandHelp("pc solutions", "ko"), /6L 초과/u);
  for (const locale of ["en", "ko", "ja"]) {
    const help = formatSlashCommandHelp("pc solutions", locale);
    assert.match(help, /\/pc solutions/u);
    assert.match(help, /\/pc path/u);
    assert.ok(help.length <= 2000);
    const modal = buildCommandModalResponse({ type: 2,
      data: { type: 1, name: "pc", options: [{ type: 1, name: "solutions", options: [] }] } }, locale);
    assert.equal(modal.type, 9);
    assert.equal(modal.data.custom_id, "clearra:search:v4:pc~solutions");
  }
  const registered = globalCommands.find(({ name }) => name === "pc").options.find(({ name }) => name === "solutions");
  assert.equal(registered.name_localizations.ko, "해법");
  assert.equal(registered.name_localizations.ja, "解法");
});

test("solution-only lowering preserves all meaningful rule, Hold, queue and B2B combinations", () => {
  const filters = [[], [option("preserve-b2b", "on")], ...[
    "t-spins", "t-spins-plus", "all-mini", "all-mini-plus", "all-spin", "all-spin-plus",
  ].map((spin) => [option("preserve-b2b", "on"), option("spin-profile", spin)])];
  let checked = 0;
  for (const pattern of ["P7P4", "P7P7"]) for (const rule of ["srs-plus", "srs", "srs-x", "jstris-180", "no-kick"]) {
    for (const hold of ["disabled", "empty", ..."IOTSZJL"]) for (const knowledge of ["oracle", "visible-7"]) {
      for (const probability of ["off", "on"]) for (const filter of filters) {
        const raw = [option("field", "grid:__________"), option("next", pattern), option("lines", 4),
          option("hold", hold), option("kicktable", rule), option("queue-knowledge", knowledge),
          option("solution-probabilities", probability), ...filter];
        const argv = buildSlashCommandArguments(solutions, raw);
        assert.equal(argv[0], "pc");
        assert.equal(value(argv, "--count"), "unique");
        assert.equal(value(argv, "--objective"), "unique");
        assert.equal(value(argv, "--patterns"), pattern);
        assert.equal(value(argv, "--rule"), rule);
        assert.equal(argv.includes("--no-hold"), hold === "disabled");
        if (hold !== "disabled") assert.equal(value(argv, "--hold"), hold);
        assert.equal(argv.includes("--queue-knowledge"), knowledge === "visible-7");
        assert.equal(argv.includes("--solution-probabilities"), probability === "on");
        assert.equal(argv.includes("--preserve-b2b"), filter.length > 0);
        assert.equal(argv.includes("--spin-profile"), filter.length > 1);
        for (const replayOrTiling of ["path", "--tiling-only", "--pc-path"]) assert.ok(!argv.includes(replayOrTiling));
        const prepared = prepareClearraArguments(argv, execution);
        assert.equal(value(prepared, "--auto-workers"), "8");
        assert.ok(prepared.includes("--use-all-cpu-threads"));
        checked += 1;
      }
    }
  }
  assert.equal(checked, 2880);
  const fullHeight = buildSlashCommandArguments(solutions, [option("field", "grid:__________"),
    option("next", "I".repeat(60)), option("lines", 24), option("queue-knowledge", "visible-7")]);
  assert.equal(value(fullHeight, "--height"), "24");
  assert.equal(value(fullHeight, "--queue"), "I".repeat(60));
  assert.equal(value(fullHeight, "--queue-knowledge"), "visible-7");
  assert.throws(() => buildSlashCommandArguments(solutions, [...base,
    option("spin-profile", "all-spin-plus")]), /requires preserve-b2b=on/u);
  for (const name of ["objective", "count", "target", "score-profile"]) {
    assert.throws(() => buildSlashCommandArguments(solutions, [...base, option(name, "all")]), /unsupported option/u);
  }
});

test("automatic solution targets preserve the full original queue and explicit path meaning", () => {
  const raw = base.filter(({ name }) => name !== "lines");
  const plan = buildSlashCommandArgumentPlan(solutions, raw);
  assert.equal(plan.automaticPcTargets, true);
  assert.deepEqual(plan.argumentSets.map((argv) => value(argv, "--lines")), ["2", "4"]);
  for (const argv of plan.argumentSets) {
    assert.equal(value(argv, "--patterns"), "P7P4");
    assert.equal(value(argv, "--count"), "unique");
  }
  for (const prefix of ["$", ">"]) {
    const named = parseClearraTextRequest(`${prefix}pc solutions grid:__________ P7P4`, prefix, execution);
    const advanced = parseClearraTextRequest(`${prefix}path grid:__________ P7P4 --objective unique`, prefix, execution);
    assert.equal(named.command.capabilityId, "pc.solutions");
    assert.deepEqual(advanced.argumentSets, named.argumentSets);
    assert.equal(named.automaticPcTargets, true);
    const explicit = parseClearraTextRequest(`${prefix}pc path grid:__________ P7P4`, prefix, execution);
    assert.equal(explicit.command.capabilityId, "pc.path");
    assert.deepEqual(explicit.argumentSets[0].slice(0, 2), ["pc", "path"]);
    assert.throws(() => parseClearraTextRequest(`${prefix}pc grid:__________ P7P4`, prefix), /requires one of/u);
  }
});

function result() {
  return { kind: "pc-scenario", summary: { objective: "unique", search_output_policy: "trace",
    postprocess_pc_path_requested: "false", packing_candidate_is_solution: "false", buildability_verified: true,
    count_complete: true,
    solution_count_calculated: true, solution_set_materialized: true, solution_keys_complete: true,
    unique_solution_count: 1, normalized_unique_solution_count: 1, actual_normalized_unique_solution_count: 1 },
  contract: { command: { kind: "pc-scenario" }, solution_data: { requested: true, status: "complete", reason: null },
    artifacts: { schema_version: "clearra.solution-data.v1",
      solution_keys: ["ctk1|initial=000000000000003f|placements=I:00000000000003c0"] } } };
}

test("solution presentation keeps colored native keys and rejects probability, tiling and replay substitution", () => {
  const native = result();
  assert.equal(validDiscordPcSolutionsResult(native), true);
  const ctk3 = buildCtk3Result(native);
  assert.equal(ctk3.pageCount, 1);
  assert.equal(ctk3.complete, true);
  assert.match(ctk3.source, /^ctk3/u);
  const partial = structuredClone(native);
  partial.summary.count_complete = false;
  assert.equal(validDiscordPcSolutionsResult(partial), true);
  assert.equal(buildCtk3Result(partial).complete, false);
  for (const changes of [
    { objective: "all" }, { objective: "tiling" }, { search_output_policy: "coverage-summary" },
    { postprocess_pc_path_requested: "true" }, { packing_candidate_is_solution: "true" },
    { buildability_verified: false }, { buildability_verified: undefined },
  ]) assert.equal(validDiscordPcSolutionsResult({ ...native, summary: { ...native.summary, ...changes } }), false);
  for (const kind of ["pc-path-family.v2", "pc-probability.v2", "pc-tiling-family.v1"]) {
    assert.equal(validDiscordPcSolutionsResult({ ...native, kind }), false);
  }
});

test("tiny real native Unique search uses identical public argv and produces colored solution keys", {
  skip: !process.env.CLEARRA_PARITY_EXECUTABLE,
}, async () => {
  const executable = process.env.CLEARRA_PARITY_EXECUTABLE;
  const metadata = await stat(executable);
  assert.ok(metadata.isFile() && metadata.size > 0 && metadata.size < 256 * 1024 ** 2);
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(executable)) digest.update(bytes);
  assert.equal(digest.digest("hex"), process.env.CLEARRA_PARITY_EXECUTABLE_SHA256);
  const raw = [option("field", "grid:######____"), option("next", "I"), option("lines", 1),
    option("hold", "disabled"), option("kicktable", "srs-plus")];
  const argv = prepareClearraArguments(buildSlashCommandArguments(solutions, raw), execution);
  const output = execFileSync(executable, argv, { encoding: "utf8", timeout: 5000,
    maxBuffer: 1024 * 1024, env: { ...process.env, CLEARRA_EXPECTED_VCPUS: "8" },
    stdio: ["ignore", "pipe", "pipe"] });
  const native = JSON.parse(output);
  if (process.env.CLEARRA_PARITY_SOURCE_COMMIT !== undefined) {
    assert.match(process.env.CLEARRA_PARITY_SOURCE_COMMIT, /^[0-9a-f]{40}$/u);
    assert.equal(native.runtime_identity?.source_commit, process.env.CLEARRA_PARITY_SOURCE_COMMIT);
    assert.equal(native.runtime_identity?.engine_build_id, process.env.CLEARRA_PARITY_SOURCE_COMMIT);
  }
  assert.equal(validDiscordPcSolutionsResult(native), true);
  assert.equal(native.summary.normalized_unique_solution_count, 1);
  assert.equal(native.summary.buildability_verified, true);
  assert.equal(value(argv, "--auto-workers"), "8");
  assert.equal(native.summary.workers_requested, "auto");
  assert.ok(native.summary.workers_used >= 1 && native.summary.workers_used <= 8);
  assert.equal(buildCtk3Result(native).pageCount, 1);
});
