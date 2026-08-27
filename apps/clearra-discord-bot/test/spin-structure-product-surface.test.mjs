import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalClearraOperationalCommand,
  prepareClearraArguments,
} from "../src/clearra/command.mjs";
import { parseClearraTextRequest } from "../src/clearra/text-command.mjs";
import { findProductCapability } from "../src/discord/capability-registry.mjs";
import {
  findSlashCommand,
  formatSlashCommandHelp,
} from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";

const EMPTY = "grid:__________";

test("all three spin-structure products expose independent closed authorities", () => {
  const expected = [
    ["search", "unordered-spin-structure.v2", "spin-structure-inventory.v2", "spin-structure-family.v2"],
    ["cover", "unordered-spin-structure-coverage.v1", "spin-structure-cover.v1", "spin-structure-coverage.v1"],
    ["guaranteed", "unordered-guaranteed-spin-structure.v1", "spin-structure-guaranteed.v1", "spin-structure-guaranteed.v1"],
  ];
  const commands = findSlashCommand("spin-structure").subcommands;
  assert.deepEqual(Object.keys(commands), expected.map(([name]) => name));
  for (const [name, problem, input, result] of expected) {
    const capability = findProductCapability(`spin-structure.${name}`);
    const command = commands[name];
    assert.equal(capability.status, "active", name);
    assert.equal(capability.problemContractId, problem, name);
    assert.equal(capability.inputSchemaId, input, name);
    assert.equal(capability.resultContractId, result, name);
    assert.equal(command.input, `spin-structure-${name === "search" ? "v2" : `${name}-v1`}`);
    assert.deepEqual(command.argvPrefix, ["spin-structure", name]);
    assert.equal(command.timeoutClass, "structure_long");
  }
});

test("cover and guaranteed lower the canonical CPU-only Web grammar", () => {
  const commands = findSlashCommand("spin-structure").subcommands;
  const cover = buildSlashCommandArguments(commands.cover, [
    { name: "pieces", value: "tIo" },
    { name: "field", value: EMPTY },
    { name: "lines", value: "2+" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "kicktable", value: "srs" },
    { name: "fill-bottom", value: 1 },
    { name: "fill-top", value: 5 },
    { name: "max-placements", value: 2 },
    { name: "minimality", value: "minimum-piece-count" },
    { name: "max-patterns", value: 8 },
  ]);
  assert.deepEqual(cover.slice(0, 2), ["spin-structure", "cover"]);
  assertValue(cover, "--pieces", "TIO");
  assertValue(cover, "--objective", "min-cover");
  assertValue(cover, "--max-patterns", "8");
  assertValue(cover, "--rule", "srs");
  assert.equal(cover.some((token) => token.includes("backend")), false);
  assert.equal(cover.some((token) => token.includes("memory")), false);
  assert.equal(cover.some((token) => token.includes("tie")), false);

  const guaranteed = buildSlashCommandArguments(commands.guaranteed, [
    { name: "pieces", value: "TI" },
    { name: "field", value: EMPTY },
    { name: "final-piece", value: "T" },
    { name: "dependency-report", value: "on" },
  ]);
  assert.deepEqual(guaranteed.slice(0, 2), ["spin-structure", "guaranteed"]);
  assertValue(guaranteed, "--final-piece", "T");
  assert.equal(guaranteed.includes("--dependency-report"), true);
  assert.equal(guaranteed.includes("--no-dependency-report"), false);

  const prepared = prepareClearraArguments(cover, {
    workers: 2,
    logicalProcessors: 8,
    outputFormat: "json",
  });
  assertValue(prepared, "--auto-workers", "2");
  assertValue(prepared, "--format", "json");
  assert.equal(canonicalClearraOperationalCommand(cover), "spin-structure.cover");
  assert.equal(
    canonicalClearraOperationalCommand(guaranteed),
    "spin-structure.guaranteed",
  );
});

test("spin portfolio and guarantee options are capability-closed", () => {
  const commands = findSlashCommand("spin-structure").subcommands;
  const common = [
    { name: "pieces", value: "TI" },
    { name: "field", value: EMPTY },
  ];
  for (const name of ["cover", "guaranteed"]) {
    const optionNames = commands[name].registration.options.map(({ name: option }) => option);
    for (const forbidden of [
      "objective",
      "queue",
      "patterns",
      "hold",
      "backend",
      "gpu-device",
      "workers",
      "max-memory",
      "max-memory-mib",
      "ties",
      "tie-cursor",
      "tie-snapshot",
    ]) assert.equal(optionNames.includes(forbidden), false, `${name}:${forbidden}`);
    assert.throws(
      () => buildSlashCommandArguments(commands[name], [
        ...common,
        { name: "max-memory-mib", value: 128 },
      ]),
      /unsupported option 'max-memory-mib'/i,
    );
    assert.throws(
      () => buildSlashCommandArguments(commands[name], [
        ...common,
        { name: "kicktable", value: "no-kick" },
      ]),
      /kicktable must be srs-plus or srs/i,
    );
    assert.throws(
      () => buildSlashCommandArguments(commands[name], [
        ...common,
        { name: "lines", value: "0" },
      ]),
      /cannot be zero/i,
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(commands.guaranteed, [
      ...common,
      { name: "final-piece", value: "O" },
    ]),
    /must occur in pieces/i,
  );
  assert.throws(
    () => buildSlashCommandArguments(commands.guaranteed, [
      { name: "pieces", value: "TO" },
      { name: "field", value: EMPTY },
      { name: "final-piece", value: "O" },
    ]),
    /T-Spin profiles require final-piece T/i,
  );
});

test("grouped text routes share slash lowering and document canonical selection", () => {
  const execution = {
    workers: 2,
    logicalProcessors: 8,
    outputFormat: "json",
  };
  const cover = parseClearraTextRequest(
    "$spin-structure cover grid:__________ T --max-patterns 8",
    "$",
    execution,
  );
  assert.equal(cover.command.capabilityId, "spin-structure.cover");
  assert.deepEqual(cover.arguments_.slice(0, 2), ["spin-structure", "cover"]);
  assertValue(cover.arguments_, "--objective", "min-cover");

  const guaranteed = parseClearraTextRequest(
    ">spin-structure guaranteed grid:__________ TI --final-piece T --no-dependency-report",
    ">",
    execution,
  );
  assert.equal(guaranteed.command.capabilityId, "spin-structure.guaranteed");
  assert.deepEqual(guaranteed.arguments_.slice(0, 2), ["spin-structure", "guaranteed"]);
  assert.equal(guaranteed.arguments_.includes("--no-dependency-report"), true);
  assert.match(formatSlashCommandHelp("spin-structure cover", "en"), /first canonical portfolio/i);
  assert.doesNotMatch(formatSlashCommandHelp("spin-structure cover", "en"), /attack.*select/i);
});

function assertValue(arguments_, flag, expected) {
  const index = arguments_.indexOf(flag);
  assert.notEqual(index, -1, `${flag} in ${arguments_.join(" ")}`);
  assert.equal(arguments_[index + 1], expected, flag);
}
