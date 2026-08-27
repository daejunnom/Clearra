import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";

import {
  assertDiscordCanonicalOnlyResult,
  canonicalClearraOperationalCommand,
  prepareClearraArguments,
  searchTimeoutClass,
} from "../src/clearra/command.mjs";
import {
  classifyClearraTextCommand,
  parseClearraTextRequest,
} from "../src/clearra/text-command.mjs";
import {
  discordBuildV2ResultAuthority,
  projectDiscordBuildV2Result,
  validDiscordBuildV2Result,
} from "../src/discord/build-v2-result.mjs";
import {
  findProductCapability,
  productCapabilityRegistry,
} from "../src/discord/capability-registry.mjs";
import {
  findSlashCommand,
  globalCommands,
} from "../src/discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArguments,
  normalizeBuildV2ColoredDocument,
} from "../src/discord/slash-command-input.mjs";

const BUILD_V2_CASES = Object.freeze([
  ["build.cover", "cover", ["build", "cover"], "build-v2-cover", "min-cover", "build-coverage-portfolio.v2", "portfolio"],
  ["build.setup", "setup", ["build", "setup"], "build-v2-target", "unique", "build-target-family.v2", "candidate-family"],
  ["build.congruent", "congruent", ["build", "congruent"], "build-v2-target", "unique", "build-congruence-family.v1", "candidate-family"],
  ["build.congruent-cover", "congruent-cover", ["build", "congruent-cover"], "build-v2-target", "min-cover", "build-congruence-coverage.v1", "portfolio"],
  ["build.setup-cover", "setup-cover", ["build", "setup-cover"], "build-v2-target", "min-cover", "build-setup-cover.v1", "portfolio"],
  ["build.setup-cover-percent", "setup-cover-percent", ["build", "setup-cover-percent"], "build-v2-target", "unique", "build-setup-cover-probability.v1", "probability"],
  ["build.setup-cover-score", "setup-cover-score", ["build", "setup-cover-score"], "build-v2-target", "max-score-cover", "build-setup-cover-score.v1", "score-portfolio"],
  ["build.evaluate.cover", "evaluate-cover", ["build", "evaluate", "cover"], "build-v2-supplied", "all", "build-supplied-coverage.v1", "candidate-family"],
  ["build.evaluate.minimals", "evaluate-minimals", ["build", "evaluate", "minimals"], "build-v2-supplied", "min-cover", "build-supplied-minimum-cover.v1", "portfolio"],
  ["build.evaluate.score", "evaluate-score", ["build", "evaluate", "score"], "build-v2-supplied", "max-score-cover", "build-supplied-score.v1", "score-portfolio"],
  ["build.evaluate.b2b-cover", "evaluate-b2b-cover", ["build", "evaluate", "b2b-cover"], "build-v2-supplied", "all", "build-supplied-b2b-coverage.v1", "candidate-family"],
  ["build.evaluate.cover-percent", "evaluate-cover-percent", ["build", "evaluate", "cover-percent"], "build-v2-supplied", "unique", "build-supplied-probability.v1", "probability"],
]);

const COLORED_DOCUMENT = encodeCtk3({
  width: 10,
  pages: [{
    height: 1,
    cells: ["I", "I", "I", "I", ...Array(6).fill(null)],
  }],
});
const GRAY_DOCUMENT = encodeCtk3({
  width: 10,
  pages: [{
    height: 1,
    cells: ["G", "G", "G", "G", ...Array(6).fill(null)],
  }],
});

test("all twelve Build v2 capabilities have an active product-authorized Discord surface", () => {
  const build = findSlashCommand("build");
  assert.ok(build?.subcommands);
  assert.deepEqual(
    BUILD_V2_CASES.map(([, subcommand]) => subcommand).sort(),
    Object.keys(build.subcommands)
      .filter((subcommand) => subcommand !== "finesse-score")
      .sort(),
  );
  assert.equal(
    productCapabilityRegistry.filter(({ id }) =>
      BUILD_V2_CASES.some(([capabilityId]) => capabilityId === id)
    ).length,
    12,
  );

  for (const [capabilityId, subcommand, argvPrefix, input] of BUILD_V2_CASES) {
    const capability = findProductCapability(capabilityId);
    const command = build.subcommands[subcommand];
    assert.equal(capability.discordSurfaceStatus, "ready", capabilityId);
    assert.equal(capability.productActivationReady, true, capabilityId);
    assert.equal(capability.status, "active", capabilityId);
    assert.equal(command.capabilityId, capabilityId);
    assert.equal(command.input, input);
    assert.deepEqual(command.argvPrefix, argvPrefix);
    assert.equal(command.timeoutClass, "build_long");
    const optionNames = command.registration.options.map(({ name }) => name);
    for (const forbidden of [
      "backend", "gpu-device", "workers", "max-memory", "max-memory-mib",
      "ties", "tie-cursor", "tie-snapshot",
    ]) {
      assert.equal(optionNames.includes(forbidden), false, `${capabilityId}:${forbidden}`);
    }
    assert.equal(optionNames.includes("queue"), true, capabilityId);
    assert.equal(optionNames.includes("patterns"), true, capabilityId);
    assert.equal(optionNames.includes("queue-knowledge"), true, capabilityId);
    assert.equal(optionNames.includes("objective"), true, capabilityId);
    assert.equal(
      optionNames.includes("score-profile"),
      ["build.setup-cover-score", "build.evaluate.score"].includes(capabilityId),
      capabilityId,
    );
    assert.equal(
      optionNames.includes("source-pieces"),
      capabilityId === "build.cover",
      capabilityId,
    );
  }

  const registeredBuild = globalCommands.find(({ name }) => name === "build");
  assert.equal(registeredBuild.options.length, 13);
  assert.ok(registeredBuild.options.every(({ options }) => options.length <= 25));
});

test("all twelve slash forms lower to the frozen Web grammar and capability-closed defaults", () => {
  const build = findSlashCommand("build");
  for (const [capabilityId, subcommand, argvPrefix, input, objective] of BUILD_V2_CASES) {
    const command = build.subcommands[subcommand];
    const arguments_ = buildSlashCommandArguments(command, [
      ...sourceOptions(input),
      { name: "queue", value: "I" },
    ]);
    assert.deepEqual(arguments_.slice(0, argvPrefix.length), argvPrefix, capabilityId);
    assertOption(arguments_, "--queue", "I", capabilityId);
    assertOption(arguments_, "--hold", "empty", capabilityId);
    assertOption(arguments_, "--queue-knowledge", "oracle", capabilityId);
    assertOption(arguments_, "--objective", objective, capabilityId);
    assertOption(arguments_, "--rule", "srs-plus", capabilityId);
    assertOption(arguments_, "--backend", "cpu", capabilityId);
    assert.equal(arguments_.includes("--no-backend-fallback"), true, capabilityId);
    assert.equal(arguments_.some((token) => token.includes("max-memory")), false, capabilityId);
    assert.equal(arguments_.some((token) => token.includes("tie")), false, capabilityId);

    if (input === "build-v2-target") {
      assertOption(arguments_, "--target-format", "ctk3", capabilityId);
      assertOption(arguments_, "--target-document", COLORED_DOCUMENT, capabilityId);
      assert.equal(arguments_.includes("--solution-document"), false, capabilityId);
    } else if (input === "build-v2-supplied") {
      assertOption(arguments_, "--solution-format", "ctk3", capabilityId);
      assertOption(arguments_, "--solution-document", COLORED_DOCUMENT, capabilityId);
      assert.equal(arguments_.includes("--target-document"), false, capabilityId);
    } else {
      assertOption(arguments_, "--base-mask", "0x0", capabilityId);
      assertOption(arguments_, "--target-mask", "0xf", capabilityId);
    }

    const scoreCapable = ["build.setup-cover-score", "build.evaluate.score"]
      .includes(capabilityId);
    assert.equal(arguments_.includes("--score-profile"), scoreCapable, capabilityId);
    assert.equal(arguments_.includes("--initial-b2b"), scoreCapable, capabilityId);
  }
});

test("Build v2 source roles, colors, supply, objective, and score options fail closed", () => {
  const build = findSlashCommand("build").subcommands;
  assert.throws(
    () => normalizeBuildV2ColoredDocument("grid:IIII______", {
      name: "target-document",
      format: "ctk3",
    }),
    /canonical CTK3 or v115 Fumen/u,
  );
  assert.throws(
    () => normalizeBuildV2ColoredDocument(GRAY_DOCUMENT, {
      name: "target-document",
      format: "ctk3",
    }),
    /lost its piece colors|gray or occupancy-only/u,
  );
  assert.throws(
    () => normalizeBuildV2ColoredDocument(COLORED_DOCUMENT, {
      name: "target-document",
      format: "fumen",
    }),
    /does not match/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(build.setup, [
      ...sourceOptions("build-v2-target"),
      { name: "solution-document", value: COLORED_DOCUMENT },
      { name: "queue", value: "I" },
    ]),
    /unsupported option 'solution-document'/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(build.setup, sourceOptions("build-v2-target")),
    /exactly one of queue or patterns/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(build.setup, [
      ...sourceOptions("build-v2-target"),
      { name: "queue", value: "I" },
      { name: "patterns", value: "*!" },
    ]),
    /exactly one of queue or patterns/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(build["evaluate-cover"], [
      ...sourceOptions("build-v2-supplied"),
      { name: "queue", value: "I" },
      { name: "objective", value: "min-cover" },
    ]),
    /accepts only objective all/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(build.setup, [
      ...sourceOptions("build-v2-target"),
      { name: "queue", value: "I" },
      { name: "score-profile", value: "tetrio" },
    ]),
    /unsupported option 'score-profile'/u,
  );
});

test("all twelve text forms use canonical named inputs and nested evaluate paths", () => {
  for (const [capabilityId, subcommand, argvPrefix, input] of BUILD_V2_CASES) {
    const path = capabilityId.startsWith("build.evaluate.")
      ? `build evaluate ${capabilityId.slice("build.evaluate.".length)}`
      : `build ${subcommand}`;
    const source = `$${path} ${textSourceOptions(input)} --patterns "*!"`;
    const request = parseClearraTextRequest(source, "$", {
      workers: 2,
      outputFormat: "json",
    });
    assert.equal(request.command.capabilityId, capabilityId);
    assert.equal(classifyClearraTextCommand(source, "$"), capabilityId);
    assert.deepEqual(request.arguments_.slice(0, argvPrefix.length), argvPrefix);
    assertOption(request.arguments_, "--patterns", "*!", capabilityId);
    assertOption(request.arguments_, "--backend", "cpu", capabilityId);
    assertOption(request.arguments_, "--auto-workers", "2", capabilityId);
    assertOption(request.arguments_, "--format", "json", capabilityId);
  }

  assert.throws(
    () => parseClearraTextRequest(
      "$build cover --base-mask 0 --target-mask 15 --height 1 --queue I --max-memory-mib 10",
      "$",
    ),
    /does not expose option '--max-memory-mib'/u,
  );
  assert.throws(
    () => parseClearraTextRequest(
      `$build setup ${textSourceOptions("build-v2-target")} --queue I --objective max-score-cover`,
      "$",
    ),
    /accepts only objective unique or all/u,
  );
});

test("command client accepts only registered CPU Build v2 execution paths", () => {
  assert.equal(searchTimeoutClass(["build", "cover"]), "build_long");
  assert.equal(
    canonicalClearraOperationalCommand(["build", "evaluate", "score", "--queue", "I"]),
    "build.evaluate.score",
  );
  for (const [capabilityId, subcommand, , input] of BUILD_V2_CASES) {
    const command = findSlashCommand("build").subcommands[subcommand];
    const arguments_ = buildSlashCommandArguments(command, [
      ...sourceOptions(input),
      { name: "queue", value: "I" },
    ]);
    assert.deepEqual(
      prepareClearraArguments(arguments_).slice(0, command.argvPrefix.length),
      command.argvPrefix,
      capabilityId,
    );
  }
  assert.throws(
    () => prepareClearraArguments(["build", "unknown"]),
    /registered Build v2 capability path/u,
  );
  assert.throws(
    () => prepareClearraArguments(["build", "cover", "--backend", "gpu"]),
    /CPU-only/u,
  );
  assert.throws(
    () => prepareClearraArguments(["build", "cover", "--allow-backend-fallback"]),
    /forbids backend fallback/u,
  );
  assert.throws(
    () => prepareClearraArguments(["build", "cover", "--max-memory", "1"]),
    /no max-memory request authority/u,
  );
  const validCover = buildSlashCommandArguments(
    findSlashCommand("build").subcommands.cover,
    [...sourceOptions("build-v2-cover"), { name: "queue", value: "I" }],
  );
  for (const [option, value] of [
    ["--target-document", COLORED_DOCUMENT],
    ["--solution-document", COLORED_DOCUMENT],
    ["--score-profile", "guideline"],
    ["--workers", "2"],
  ]) {
    assert.throws(
      () => prepareClearraArguments([...validCover, option, value]),
      /does not expose the Build v2 option/u,
      option,
    );
  }
  const objectiveIndex = validCover.indexOf("--objective");
  const forgedObjective = [...validCover];
  forgedObjective[objectiveIndex + 1] = "all";
  assert.throws(
    () => prepareClearraArguments(forgedObjective),
    /does not expose objective 'all'/u,
  );
});

test("all twelve result contracts preserve ordinary families and narrow exact/score output", () => {
  assert.deepEqual(
    discordBuildV2ResultAuthority().map(({ capabilityId }) => capabilityId),
    BUILD_V2_CASES.map(([capabilityId]) => capabilityId),
  );
  for (const [capabilityId, , , , , resultContract, payloadKind] of BUILD_V2_CASES) {
    const structured = buildResult(capabilityId, resultContract, payloadKind);
    assert.equal(validDiscordBuildV2Result(structured), true, capabilityId);
    const projected = projectDiscordBuildV2Result(structured);
    assert.equal(projected.summary.capability_id, capabilityId);
    assert.equal(projected.summary.payload_kind, payloadKind);
    assert.equal(Object.hasOwn(projected.summary, "page_source_available"), false);
    assert.equal(JSON.stringify(projected).toLowerCase().includes("attack"), false);
    assert.equal(JSON.stringify(projected).toLowerCase().includes("tie_"), false);
    if (payloadKind === "candidate-family") {
      assert.equal(projected.summary.candidates.length, 2, capabilityId);
    }
    if (payloadKind === "score-portfolio") {
      assert.equal(projected.summary.score_equality_basis, "score-only", capabilityId);
      assert.equal(projected.summary.winners.length, 2, capabilityId);
    }
  }

  const ordinary = buildResult(
    "build.evaluate.cover",
    "build-supplied-coverage.v1",
    "candidate-family",
  );
  ordinary.summary.candidates = Array.from({ length: 30 }, (_, index) => ({
    candidate_key: `candidate-${String(index).padStart(2, "0")}`,
    covered_pattern_count: "1",
  }));
  const bounded = projectDiscordBuildV2Result(ordinary);
  assert.equal(bounded.summary.candidates.length, 24);
  assert.equal(bounded.summary.discord_family_display_truncated, true);

  const integrated = assertDiscordCanonicalOnlyResult({
    exitCode: 0,
    stdout: JSON.stringify(buildResult(
      "build.evaluate.score",
      "build-supplied-score.v1",
      "score-portfolio",
    )),
    stderr: "",
  });
  assert.equal(JSON.parse(integrated.stdout).summary.informational_attack_basis, undefined);

  const nestedAttack = buildResult(
    "build.evaluate.score",
    "build-supplied-score.v1",
    "score-portfolio",
  );
  nestedAttack.debug = { informational_attack: "must-not-cross-discord" };
  assert.equal(
    JSON.stringify(projectDiscordBuildV2Result(nestedAttack)).toLowerCase().includes("attack"),
    false,
  );
});

test("Build v2 result projection rejects every alternative/tie page and attack-based score policy", () => {
  const base = buildResult(
    "build.evaluate.minimals",
    "build-supplied-minimum-cover.v1",
    "portfolio",
  );
  for (const key of [
    "tie_count",
    "tie_cursor",
    "tie_page",
    "tie_metadata",
    "alternative_count",
    "alternative_cursor",
    "portfolio_alternative_page",
    "metadata",
    "canonical_candidate_ids",
  ]) {
    const widened = structuredClone(base);
    widened.summary[key] = key.includes("candidate") ? ["a", "b"] : 2;
    assert.throws(
      () => projectDiscordBuildV2Result(widened),
      /does not expose alternative or tie paging metadata/u,
      key,
    );
  }

  const score = buildResult(
    "build.evaluate.score",
    "build-supplied-score.v1",
    "score-portfolio",
  );
  score.summary.score_equality_basis = "score-then-attack";
  assert.throws(
    () => projectDiscordBuildV2Result(score),
    /score-only equality/u,
  );
  const attackSelection = buildResult(
    "build.evaluate.score",
    "build-supplied-score.v1",
    "score-portfolio",
  );
  attackSelection.summary.attack_based_selection = true;
  assert.throws(
    () => projectDiscordBuildV2Result(attackSelection),
    /does not expose alternative or tie paging metadata/u,
  );
});

function sourceOptions(input) {
  if (input === "build-v2-cover") {
    return [
      { name: "base-mask", value: "0" },
      { name: "target-mask", value: "15" },
      { name: "height", value: 1 },
    ];
  }
  if (input === "build-v2-target") {
    return [
      { name: "target-format", value: "ctk3" },
      { name: "target-document", value: COLORED_DOCUMENT },
    ];
  }
  return [
    { name: "solution-format", value: "ctk3" },
    { name: "solution-document", value: COLORED_DOCUMENT },
  ];
}

function textSourceOptions(input) {
  if (input === "build-v2-cover") {
    return "--base-mask 0 --target-mask 15 --height 1";
  }
  if (input === "build-v2-target") {
    return `--target-format ctk3 --target-document ${COLORED_DOCUMENT}`;
  }
  return `--solution-format ctk3 --solution-document ${COLORED_DOCUMENT}`;
}

function assertOption(arguments_, option, value, message) {
  const index = arguments_.indexOf(option);
  assert.notEqual(index, -1, `${message}:${option}`);
  assert.equal(arguments_[index + 1], value, `${message}:${option}`);
}

function buildResult(capabilityId, resultContract, payloadKind) {
  const summary = {
    capability_id: capabilityId,
    result_contract: resultContract,
    payload_kind: payloadKind,
    input_identity_sha256: "a".repeat(64),
    objective: "all",
    source_candidate_count: "2",
    reachable_candidate_count: "2",
    pattern_count: "2",
    candidates: [
      { candidate_key: "candidate-a", covered_pattern_count: "1" },
      { candidate_key: "candidate-b", covered_pattern_count: "1" },
    ],
    canonical_candidate_keys: ["candidate-a", "candidate-b"],
    winners: [],
    completeness: {
      enumeration_complete: true,
      reachability_complete: true,
      probability_complete: true,
      portfolio_complete: true,
      exact: true,
    },
    page_source_available: true,
    page_source_identity_sha256: "b".repeat(64),
  };
  if (payloadKind === "score-portfolio") {
    summary.score_equality_basis = "score-only";
    summary.informational_attack_basis = "canonical-equal-score-trace";
    summary.winners = [
      { pattern_id: "0", candidate_key: "candidate-a", score: "1200", informational_attack: "4" },
      { pattern_id: "1", candidate_key: "candidate-b", score: "1200", informational_attack: "9" },
    ];
  }
  return {
    kind: resultContract,
    contract: { command: { kind: resultContract } },
    summary,
  };
}
