import assert from "node:assert/strict";
import test from "node:test";

import {
  findSlashCommand,
  formatSlashCommandHelp,
} from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";
import {
  canonicalClearraOperationalCommand,
  prepareClearraArguments,
  searchTimeoutClass,
} from "../src/clearra/command.mjs";

const command = findSlashCommand("recovery")?.subcommands?.boundary;

function lower(scenario) {
  return buildSlashCommandArguments(command, [
    { name: "scenario", value: JSON.stringify(scenario) },
  ]);
}

test("Discord accepts the entire selected recovery scenario in one interaction", () => {
  assert.ok(command);
  assert.deepEqual(command.argvPrefix, ["recovery", "boundary"]);
  const args = lower({
    initial_board_mask: "0x3f0",
    target_board_mask: "0xc030",
    height: 4,
    queue: "IO",
    stage_one_count: 1,
    placements: 2,
    borrow_role_position: 2,
    borrow_placement_mask: "0x300c000",
    hold: false,
    initial_b2b: false,
    preserve_b2b_bags: [2],
  });
  assert.deepEqual(args.slice(0, 3), ["recovery", "boundary", "--initial-board-mask"]);
  assert.deepEqual(args.slice(args.indexOf("--borrow-role-position"), args.indexOf("--borrow-role-position") + 2),
    ["--borrow-role-position", "2"]);
  assert.deepEqual(args.slice(args.indexOf("--preserve-b2b-bag"), args.indexOf("--preserve-b2b-bag") + 2),
    ["--preserve-b2b-bag", "2"]);
  assert.ok(args.includes("--no-hold"));
  assert.equal(args.includes("--queue-pattern"), false);
  assert.equal(searchTimeoutClass(args, "forward_long"), "forward_long");
  assert.equal(canonicalClearraOperationalCommand(args), "recovery.boundary");
  assert.deepEqual(prepareClearraArguments(args).slice(0, 2), ["recovery", "boundary"]);
  assert.throws(() => prepareClearraArguments(["recovery", "other"]), /boundary subcommand/u);
});

test("weighted recovery holds reference roles fixed while supplying a bounded pattern", () => {
  const args = lower({
    initial_board_mask: "0x0",
    target_board_mask: "0x0",
    height: 8,
    queue: "IJLOSTZIJLOSTZ",
    stage_one_count: 7,
    placements: 14,
    role_masks: Array(14).fill("0xf"),
    max_early_placements: 0,
    preserve_b2b_bags: [2],
    queue_pattern: "IJLOSTZP7",
    max_pattern_evaluations: 1,
    max_total_states: 100,
  });
  assert.equal(args.filter((arg) => arg === "--role-mask").length, 14);
  assert.ok(args.includes("1:0xf"));
  assert.ok(args.includes("14:0xf"));
  assert.equal(args.includes("--borrow-role-position"), false);
  assert.deepEqual(args.slice(args.indexOf("--queue-pattern"), args.indexOf("--queue-pattern") + 2),
    ["--queue-pattern", "IJLOSTZP7"]);
  assert.ok(args.includes("--max-total-states"));
});

test("Discord rejects ambiguous role, bag and pattern requests before starting a job", () => {
  const base = {
    initial_board_mask: "0x3f0", target_board_mask: "0xc030", height: 4,
    queue: "IO", stage_one_count: 1, placements: 2,
    borrow_role_position: 2, borrow_placement_mask: "0x300c000",
  };
  assert.throws(() => lower({ ...base, borrow_placement_mask: undefined }), /borrow_placement_mask/u);
  assert.throws(() => lower({ ...base, preserve_b2b_bags: [1, 1] }), /preserve_b2b_bags/u);
  assert.throws(() => lower({ ...base, role_masks: ["0xf"] }), /role_masks/u);
  assert.throws(() => lower({ ...base, queue_pattern: "IO" }), /queue_pattern/u);
  assert.throws(() => lower({ ...base, height: 1, initial_board_mask: "0x10000" }), /initial_board_mask/u);
  assert.throws(() => lower({ ...base, unexpected: true }), /unknown field/u);
});

test("recovery command help is available in every released Discord locale", () => {
  for (const locale of ["en", "ko", "ja"]) {
    assert.match(formatSlashCommandHelp("recovery boundary", locale), /recovery boundary/u);
  }
});

test('multi-role maximum and a global B2B policy reach the CLI without selected-role flags', () => {
  const args = lower({ initial_board_mask: '0x0', stage_one_board_mask: '0xc030000000000',
    target_board_mask: '0xc0300c0300c03', height: 6, queue: 'OOO', placements: 3,
    stage_one_count: 1, max_early_placements: 2, preserve_b2b: true });
  assert.equal(args[args.indexOf('--max-early-placements') + 1], '2');
  assert.ok(args.includes('--preserve-b2b'));
  assert.ok(args.includes('--stage-one-board-mask'));
  assert.ok(!args.includes('--borrow-role-position'));
  assert.ok(!args.includes('--borrow-placement-mask'));
  assert.throws(() => lower({ initial_board_mask: '0x0', target_board_mask: '0x0',
    height: 6, queue: 'OOO', placements: 3, stage_one_count: 1, max_early_placements: 3 }), /max_early_placements/u);
});


test("automatic early scope and unlimited defaults survive Discord lowering", () => {
  const scenario = { initial_board_mask: "0x0", target_board_mask: "0x0", height: 8,
    queue: "IJLOSTZIJLOSTZ", placements: 14, stage_one_count: 7,
    role_masks: Array(14).fill("0xf"), queue_pattern: "P7P7" };
  for (const settings of [{}, {max_early_placements: "auto", max_states: "unlimited",
      max_pattern_evaluations: "unlimited", max_total_states: "unlimited"}]) {
    const args = lower({...scenario, ...settings});
    assert.equal(args[args.indexOf("--max-early-placements") + 1], "auto");
    for (const flag of ["--max-states", "--max-pattern-evaluations", "--max-total-states"])
      assert.equal(args.includes(flag), false);
  }
  assert.throws(() => lower({...scenario, max_early_placements: "auto", borrow_role_position: 8}), /selected early role/u);
  for (const field of ["max_states", "max_pattern_evaluations", "max_total_states"])
    for (const value of [0, -1, null, 1.5, "100"])
      assert.throws(() => lower({...scenario, [field]: value}), new RegExp(field, "u"));
});
