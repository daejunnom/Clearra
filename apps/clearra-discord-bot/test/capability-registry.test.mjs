import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  CLI_COMMAND_LOWERING_AUTHORITY,
  assertProductCapabilityRegistry,
  discordRuntimeProjection,
  findProductCapability,
  lowerCapabilityRouteRequest,
  productCapabilityRegistry,
} from "../src/discord/capability-registry.mjs";
import {
  findSlashCommand,
  findTextCommand,
  formatSlashCommandHelp,
  globalCommands,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";
import { buildCommandModalResponse } from "../src/discord/field-modal.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";
import {
  DISCORD_HIDDEN_TEXT_SEARCH_CONTRACT,
  DISCORD_PUBLIC_SEARCH_CONTRACT,
} from "../src/discord/public-search-contract.mjs";
import { SUPPORTED_DISCORD_LOCALES } from "../src/discord/locale-rollout.mjs";
import {
  classifyClearraTextCommand,
  parseClearraTextMessage,
  parseClearraTextRequest,
} from "../src/clearra/text-command.mjs";

test("public registry i18n policy matches every released Discord locale", () => {
  const releasedPolicy = SUPPORTED_DISCORD_LOCALES.join("-");
  const publicCapabilities = productCapabilityRegistry.filter(
    ({ helpPolicy }) => helpPolicy === "public",
  );
  assert.ok(publicCapabilities.length > 0);
  for (const capability of publicCapabilities) {
    assert.equal(capability.i18nPolicy, releasedPolicy, capability.id);
  }
});

test("v0.8 capability registry separates problem, algorithm, timeout, form, and result authority", () => {
  assert.equal(assertProductCapabilityRegistry(), true);
  assert.ok(productCapabilityRegistry.length > 30);
  for (const capability of productCapabilityRegistry) {
    assert.notEqual(capability.algorithmFamily, capability.timeoutClass);
    assert.ok(Object.hasOwn(capability, "problemContractId"));
    assert.ok(Object.hasOwn(capability, "inputSchemaId"));
    assert.ok(Object.hasOwn(capability, "modalSchemaId"));
    assert.ok(Object.hasOwn(capability, "resultContractId"));
    if (["search", "utility"].includes(capability.kind)) {
      assert.equal(capability.loweringAuthority, CLI_COMMAND_LOWERING_AUTHORITY);
    }
  }
  assert.equal(
    findProductCapability("meta.help").loweringAuthority,
    "discord.help-registry-query.v1",
  );
  assert.equal(
    findProductCapability("settings.channel").loweringAuthority,
    "discord.channel-settings-handler.v1",
  );
  assert.equal(
    findProductCapability("settings.server").loweringAuthority,
    "discord.server-settings-handler.v1",
  );
  const searchEntries = slashCommandCatalog.flatMap((command) => command.subcommands
    ? Object.values(command.subcommands)
    : command.kind === "search" ? [command] : []);
  for (const command of searchEntries) {
    const capability = findProductCapability(command.capabilityId);
    assert.ok(capability, command.capabilityId);
    const route = capability.canonical
      ? [capability.canonical, ...capability.aliases].find((candidate) =>
          candidate.root === (command.rootName ?? command.name) &&
          candidate.subcommand === command.subcommand &&
          candidate.slash
        )
      : null;
    assert.deepEqual(
      {
        effectClasses: command.effectClasses,
        helpPolicy: command.helpPolicy,
        i18nPolicy: command.i18nPolicy,
        resultAllowlist: command.resultAllowlist,
        telemetryIdentity: command.telemetryIdentity,
        loweringAuthority: command.loweringAuthority,
      },
      {
        effectClasses: route?.effectClasses ?? capability.effectClasses,
        helpPolicy: capability.helpPolicy,
        i18nPolicy: capability.i18nPolicy,
        resultAllowlist: route?.engineKinds ?? capability.engineKinds,
        telemetryIdentity: capability.telemetryIdentity,
        loweringAuthority: capability.loweringAuthority,
      },
    );
  }

  assert.notEqual(findProductCapability("pc.saves"), findProductCapability("pc.best-save"));
  assert.notEqual(
    findProductCapability("build.setup").problemContractId,
    findProductCapability("build.congruent").problemContractId,
  );
  for (const id of [
    "utility.parity",
    "utility.fumen",
    "utility.render",
    "utility.to-gray",
    "utility.mirror",
  ]) {
    const capability = findProductCapability(id);
    assert.equal(capability.status, "active");
    assert.equal(capability.canonical.slash, true);
    assert.equal(capability.canonical.text, true);
    assert.equal(capability.engineKinds.length > 0, true);
  }
});

test("runtime projection covers every active and hidden row plus split legacy ingress", () => {
  const projection = discordRuntimeProjection();
  assert.deepEqual(
    projection.map(({ id }) => id),
    productCapabilityRegistry.map(({ id }) => id),
  );
  assert.ok(projection.some(({ status }) => status === "active"));
  assert.ok(projection.some(({ status }) => status === "hidden"));
  assert.equal(projection.some(({ status }) => status === "planned"), false);
  for (const capability of productCapabilityRegistry) {
    assert.equal(capability.telemetryIdentity, capability.id);
    assert.ok(capability.effectClasses.length > 0);
    for (const route of capability.aliases) {
      assert.notEqual(route.slash, route.text);
      assert.equal(route.slash ? route.deprecateAfter : route.lifetime, route.slash
        ? "v0.10.0"
        : "long-term");
      const projection = lowerCapabilityRouteRequest(capability, route);
      assert.equal(projection.route.input, route.input ?? capability.engine?.input ?? null);
      assert.equal(
        projection.route.inputSchemaId,
        route.inputSchemaId ?? capability.inputSchemaId,
      );
      assert.equal(
        projection.route.modalSchemaId,
        route.modalSchemaId ?? capability.modalSchemaId,
      );
      assert.deepEqual(
        projection.route.argvPrefix,
        route.argvPrefix ?? capability.engine?.argvPrefix ?? [],
      );
    }
  }
  const verify = findProductCapability("diagnostic.verify");
  assert.deepEqual(
    {
      status: verify.status,
      slash: verify.canonical.slash,
      text: verify.canonical.text,
      help: verify.helpPolicy,
      i18n: verify.i18nPolicy,
    },
    { status: "hidden", slash: false, text: true, help: "hidden", i18n: "hidden" },
  );
});

test("Clearra product aliases use exactly the canonical CLI and result authority", () => {
  const cases = [
    ["pc", "path", "path"],
    ["pc", "chance", "chance"],
    ["pc", "minimals", "minimals"],
    ["pc", "score", "score"],
    ["pc", "score-minimals", "score-minimals"],
    ["pc", "score-finder", "score-finder"],
    ["build", "cover", "cover"],
    ["build", "finesse-score", "finesse", "score"],
    ["forward", "spin", "spin"],
    ["forward", "damage", "damage"],
  ];
  for (const [root, verb, aliasName, aliasVerb] of cases) {
    const canonical = findSlashCommand(root).subcommands[verb];
    const alias = aliasVerb
      ? findSlashCommand(aliasName).subcommands[aliasVerb]
      : findSlashCommand(aliasName);
    for (const field of [
      "capabilityId", "input", "inputSchemaId", "modalSchemaId", "problemContractId",
      "argvPrefix", "resultContractId", "resultAuthorityId", "resultAllowlist",
      "timeoutClass", "loweringAuthority", "telemetryIdentity",
    ]) {
      assert.deepEqual(alias[field], canonical[field], `${aliasName}: ${field}`);
    }
    assert.deepEqual(alias.registration.options, canonical.registration.options, aliasName);
  }

  const initialPc = [
    { name: "field", value: "XXXXXX____" },
    { name: "next", value: "I" },
    { name: "lines", value: 1 },
  ];
  for (const verb of ["path", "chance", "minimals", "score", "score-minimals", "score-finder"]) {
    const exact = buildSlashCommandArguments(findSlashCommand("pc").subcommands[verb], initialPc);
    assert.deepEqual(buildSlashCommandArguments(findSlashCommand(verb), initialPc), exact);
    assert.deepEqual(exact.slice(0, 2), ["pc", verb]);
    for (const prefix of ["$", ">"]) {
      const canonicalText = parseClearraTextRequest(`${prefix}pc ${verb} XXXXXX____ I 1`, prefix);
      const aliasText = parseClearraTextRequest(`${prefix}${verb} XXXXXX____ I 1`, prefix);
      assert.deepEqual(aliasText.arguments_, canonicalText.arguments_, verb);
    }
  }
});

test("retired Sfinder-only routes do not reach registration, help, or text execution", () => {
  const names = [
    "sfinder", "percent", "spin-cover", "allspin-sol-finder", "allspin-pres-chance",
    "pc-setup", "best-setup", "dpc-finder",
  ];
  for (const name of names) {
    assert.equal(findSlashCommand(name), null, name);
    assert.equal(findTextCommand(name), null, name);
    assert.equal(classifyClearraTextCommand(`$${name} XXXXXX____ I 1`, "$"), null, name);
    assert.equal(parseClearraTextMessage(`$${name} XXXXXX____ I 1`, "$"), null, name);
  }
  assert.equal(findSlashCommand("finesse").subcommands.search, undefined);
  assert.equal(parseClearraTextMessage("$sfinder path XXXXXX____ I 1", "$"), null);
  assert.equal(
    DISCORD_PUBLIC_SEARCH_CONTRACT.some(({ loweringAuthority }) =>
      loweringAuthority !== CLI_COMMAND_LOWERING_AUTHORITY),
    false,
  );
});

test("PC save groups and best-save are distinct fixed-boundary Discord products", () => {
  const pc = findSlashCommand("pc");
  const saves = pc.subcommands.saves;
  const bestSave = pc.subcommands["best-save"];
  const saveCapability = findProductCapability("pc.saves");
  const bestCapability = findProductCapability("pc.best-save");

  assert.equal(saves.input, "pc-save-v2");
  assert.equal(bestSave.input, "pc-save-v2");
  assert.equal(saves.resultContractId, "pc-save-groups.v2");
  assert.equal(bestSave.resultContractId, "pc-best-save.v2");
  assert.deepEqual(saveCapability.engine.fixedSemantics, {
    bagBoundary: "fixed",
    groupIdentity: "terminal-hold-plus-active-bag-remainder",
    probabilityBasis: "whole-universe-unconditional",
  });
  assert.deepEqual(bestCapability.engine.fixedSemantics, {
    bagBoundary: "fixed",
    schema: "clearra-save-v1",
    probabilityBasis: "whole-universe-unconditional",
    discordTieSelection: "smallest-canonical-candidate-id",
  });

  const options = [
    { name: "field", value: "grid:####__####/####__####" },
    { name: "next", value: "IOTSZJL" },
    { name: "lines", value: 2 },
    { name: "hold", value: "disabled" },
    { name: "kicktable", value: "no-kick" },
  ];
  const expectedTail = [
    "--lines", "2",
    "--board-mask", "0xf3fcf",
    "--height", "2",
    "--pieces", "1",
    "--patterns", "IOTSZJL",
    "--no-hold",
    "--rule", "no-kick",
  ];
  assert.deepEqual(buildSlashCommandArguments(saves, options), [
    "pc", "saves", ...expectedTail,
  ]);
  assert.deepEqual(buildSlashCommandArguments(bestSave, options), [
    "pc", "best-save", ...expectedTail,
  ]);
  assert.doesNotMatch(formatSlashCommandHelp("pc best-save", "en"), /canonical candidate ID/i);
});

test("canonical slash topology uses family-specific subcommands and keeps compatibility routes explicit", () => {
  assert.deepEqual(
    ["pc", "build", "setup", "forward"].map((name) => [
      name,
      Object.keys(findSlashCommand(name).subcommands),
    ]),
    [
      ["pc", ["path", "chance", "minimals", "score", "saves", "best-save", "score-minimals", "tiling", "failed-queue", "score-finder", "allspin-sol", "allspin-pres-chance"]],
      ["build", [
        "cover",
        "probability",
        "finesse-score",
        "setup",
        "congruent",
        "congruent-cover",
        "setup-cover",
        "setup-cover-percent",
        "setup-cover-score",
        "evaluate-cover-percent",
        "evaluate-cover",
        "evaluate-minimals",
        "evaluate-score",
        "evaluate-b2b-cover",
      ]],
      ["setup", ["joint", "build", "pc", "score"]],
      ["forward", ["spin", "damage", "ren"]],
    ],
  );
  assert.deepEqual(
    Object.keys(findSlashCommand("spin-structure").subcommands),
    ["search", "cover", "guaranteed"],
  );
  assert.equal(findSlashCommand("spin-structure").subcommands.search.input, "spin-structure-v2");
  assert.equal(findSlashCommand("finesse").subcommands.search, undefined);
  assert.ok(findSlashCommand("finesse").subcommands.score);
  assert.equal(findSlashCommand("verify"), null);
  assert.equal(findTextCommand("verify"), null);
  for (const id of [
    "build.setup",
    "build.congruent",
    "build.congruent-cover",
    "build.setup-cover",
    "build.evaluate.cover-percent",
  ]) {
    assert.equal(findProductCapability(id).status, "active", id);
    assert.equal(findProductCapability(id).canonical.slash, true, id);
    assert.equal(findProductCapability(id).discordSurfaceStatus, "ready", id);
    assert.equal(findProductCapability(id).productActivationReady, true, id);
  }
  for (const id of ["pc.saves", "pc.best-save"]) {
    assert.equal(findProductCapability(id).status, "active", id);
    assert.equal(findProductCapability(id).canonical.slash, true, id);
    assert.equal(findProductCapability(id).canonical.text, true, id);
  }
  const ren = findProductCapability("forward.ren");
  assert.equal(ren.status, "active");
  assert.equal(ren.canonical.input, "forward-ren-v1");
  assert.deepEqual(ren.canonical.argvPrefix, ["ren"]);
  assert.equal(findProductCapability("build.special-cover"), null);
});

test("top-level help lists the complete v0.8 grouped surface and distinguishes legacy input grammar", () => {
  const english = formatSlashCommandHelp("", "en");
  const korean = formatSlashCommandHelp("", "ko");
  for (const root of ["pc", "build", "setup", "forward", "spin-structure", "utility"]) {
    const group = findSlashCommand(root);
    assert.ok(group?.subcommands, root);
    const exactPath = `/${root} ${Object.keys(group.subcommands).join("|")}`;
    assert.ok(english.includes(exactPath), exactPath);
    assert.ok(korean.includes(exactPath), exactPath);
  }
  assert.match(formatSlashCommandHelp("pc chance", "ko"), /lines:1\.\.24/u);
  assert.match(formatSlashCommandHelp("pc chance", "en"), /lines:1\.\.24/u);

  const canonical = formatSlashCommandHelp("pc path", "en");
  const compatibility = formatSlashCommandHelp("path", "en");
  assert.match(canonical, /hold:<disabled\|empty\|IOTSZJL>/u);
  assert.match(canonical, /spin-profile:<profile>/u);
  assert.match(compatibility, /hold:<disabled\|empty\|IOTSZJL>/u);
  assert.match(compatibility, /spin-profile:<profile>/u);
  assert.doesNotMatch(canonical, /finite path family|canonical candidate/iu);
  assert.doesNotMatch(formatSlashCommandHelp("pc path", "ko"), /path family|canonical candidate/iu);
});

test("forward REN registry authority is active, bounded, and isolated from score families", () => {
  const ren = findProductCapability("forward.ren");
  assert.deepEqual(
    {
      status: ren.status,
      problemContractId: ren.problemContractId,
      inputSchemaId: ren.inputSchemaId,
      modalSchemaId: ren.modalSchemaId,
      resultContractId: ren.resultContractId,
      algorithmFamily: ren.algorithmFamily,
      timeoutClass: ren.timeoutClass,
      resultAllowlist: ren.resultAllowlist,
      effectClasses: ren.effectClasses,
    },
    {
      status: "active",
      problemContractId: "ordered-forward-ren-search",
      inputSchemaId: "forward-ren-exact-queue",
      modalSchemaId: "forward-ren-exact-queue",
      resultContractId: "forward-ren",
      algorithmFamily: "forward_state_expansion",
      timeoutClass: "forward_long",
      resultAllowlist: ["ren"],
      effectClasses: ["search_space", "reachability_semantics", "result_materialization"],
    },
  );
  assert.equal(ren.aliases.length, 0);
});

test("forward REN canonical slash and text ingress share one typed lowering route", () => {
  const ren = findProductCapability("forward.ren");
  const catalog = findSlashCommand("forward").subcommands.ren;
  assert.deepEqual(
    {
      slash: ren.canonical.slash,
      text: ren.canonical.text,
      input: catalog.input,
      inputSchemaId: catalog.inputSchemaId,
      modalSchemaId: catalog.modalSchemaId,
      argvPrefix: catalog.argvPrefix,
      options: catalog.registration.options.map(({ name }) => name),
    },
    {
      slash: true,
      text: true,
      input: "forward-ren-v1",
      inputSchemaId: "forward-ren-exact-queue",
      modalSchemaId: "forward-ren-exact-queue",
      argvPrefix: ["ren"],
      options: ["next", "field", "height", "hold", "kicktable"],
    },
  );
  assert.equal(catalog.loweringAuthority, CLI_COMMAND_LOWERING_AUTHORITY);
  assert.equal(catalog.resultAuthorityId, "ren");

  const slashArguments = buildSlashCommandArguments(catalog, [
    { name: "field", value: "______XXXX" },
    { name: "next", value: "TI" },
    { name: "height", value: 4 },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "srs-plus" },
  ]);
  assert.deepEqual(slashArguments, [
    "ren",
    "--board-mask-v1",
    `${"0".repeat(57)}3c0`,
    "--height",
    "4",
    "--queue",
    "TI",
    "--no-hold",
    "--rule",
    "srs-plus",
  ]);
  assert.equal(slashArguments.includes("--spin-profile"), false);
  assert.equal(slashArguments.includes("--initial-combo"), false);
  assert.equal(slashArguments.includes("--minimum-damage"), false);
  assert.throws(
    () => buildSlashCommandArguments(catalog, [
      { name: "field", value: "__________" },
      { name: "next", value: "I".repeat(23) },
    ]),
    /at most 22 pieces/,
  );
});

test("All-Spin PC capabilities preserve exact-queue and pattern-probability contracts", () => {
  const pc = findSlashCommand("pc");
  const exact = pc.subcommands["allspin-sol"];
  const chance = pc.subcommands["allspin-pres-chance"];
  const exactAlias = findSlashCommand("allspin-sol-finder");
  const chanceAlias = findSlashCommand("allspin-pres-chance");
  const exactCapability = findProductCapability("pc.allspin-sol");
  const chanceCapability = findProductCapability("pc.allspin-pres-chance");

  assert.equal(exactCapability.status, "active");
  assert.equal(chanceCapability.status, "active");
  assert.equal(exact.inputSchemaId, "pc-allspin-exact-queue.v1");
  assert.equal(exact.modalSchemaId, "pc-allspin-exact-queue.v1");
  assert.equal(exact.resultContractId, "pc-b2b-preserving-witness.v1");
  assert.equal(chance.inputSchemaId, "pc-allspin-pattern.v1");
  assert.equal(chance.modalSchemaId, "pc-allspin-pattern.v1");
  assert.equal(chance.resultContractId, "pc-b2b-preservation-probability.v1");
  assert.deepEqual(exactCapability.engineKinds, ["pc", "pc-scenario"]);
  assert.deepEqual(chanceCapability.engineKinds, ["pc", "pc-scenario"]);

  const exactOptions = [
    { name: "field", value: "grid:__________/####______" },
    { name: "next", value: "IOTS" },
    { name: "lines", value: 2 },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "no-kick" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "max-nodes", value: 17 },
  ];
  const exactArguments = buildSlashCommandArguments(exact, exactOptions);
  assert.deepEqual(exactArguments, [
    "pc", "allspin-sol",
    "--lines", "2",
    "--board-mask", "0xf",
    "--height", "2",
    "--pieces", "4",
    "--queue", "IOTS",
    "--no-hold",
    "--spin-profile", "all-spin-plus",
    "--rule", "no-kick",
    "--max-nodes", "17",
  ]);
  assert.equal(exactAlias, null);
  assert.equal(exactArguments.includes("--preserve-b2b"), false);

  const chanceOptions = exactOptions.map((option) => ({ ...option }));
  chanceOptions.find(({ name }) => name === "next").value = "[IOTS]!";
  chanceOptions.find(({ name }) => name === "spin-profile").value = "all-mini-plus";
  const chanceArguments = buildSlashCommandArguments(chance, chanceOptions);
  assert.equal(chanceArguments[chanceArguments.indexOf("--patterns") + 1], "[IOTS]!");
  assert.equal(chanceArguments.includes("--queue"), false);
  assert.equal(chanceArguments.includes("--preserve-b2b"), false);
  assert.equal(chanceAlias, null);
  assert.deepEqual(exactCapability.aliases, []);
  assert.deepEqual(chanceCapability.aliases, []);

  const contracts = new Map(
    DISCORD_PUBLIC_SEARCH_CONTRACT.map((entry) => [entry.id, entry]),
  );
  assert.equal(
    contracts.get("allspin-sol").resultContractId,
    "pc-b2b-preserving-witness.v1",
  );
  assert.equal(contracts.has("allspin-sol-finder"), false);
  assert.equal(
    contracts.get("allspin-pres-chance").resultContractId,
    "pc-b2b-preservation-probability.v1",
  );
});

test("Discord capability IDs are exact members of the product authority", () => {
  const authority = JSON.parse(readFileSync(
    new URL("../../../tests/fixtures/contracts/product_capability_registry.v1.json", import.meta.url),
    "utf8",
  ));
  const candidate = JSON.parse(readFileSync(
    new URL("../../../tests/fixtures/contracts/boundary_recovery_candidate.v1.json", import.meta.url),
    "utf8",
  ));
  assert.equal(authority.schema_id, "clearra.product-capability-registry.v1");
  const expectedRuntimeIds = authority.runtime_projection.current_capabilities
    .map(({ id }) => id).concat(candidate.runtime_capability.id)
    .sort();
  const actualRuntimeIds = productCapabilityRegistry.map(({ id }) => id).sort();
  assert.equal(authority.runtime_projection.current_capabilities.length, 47);
  assert.equal(actualRuntimeIds.length, 48);
  assert.deepEqual(actualRuntimeIds, expectedRuntimeIds);
  const stableIds = new Set([...authority.capabilities.map(({ id }) => id), candidate.product.id]);
  for (const capability of productCapabilityRegistry) {
    assert.equal(stableIds.has(capability.id), true, capability.id);
  }
  assert.equal(candidate.target_release, "v0.9.0");

  assert.equal(findProductCapability("build.target-setup"), null);
  assert.equal(findProductCapability("build.evaluate-cover"), null);
  assert.equal(findProductCapability("build.cover-percent"), null);
  assert.equal(findProductCapability("build.setup").canonical.subcommand, "setup");
  assert.equal(
    findProductCapability("build.evaluate.cover-percent").canonical.subcommand,
    "evaluate-cover-percent",
  );
  const specialCover = authority.upstream_command_inventory.find(
    ({ id }) => id === "sfinder-man/special_cover",
  );
  assert.equal(specialCover.target, "pc.chance");
  assert.equal(findSlashCommand("special-cover"), null);
});

test("bare Build cover lowers through the typed Build cover producer", () => {
  const canonical = findSlashCommand("build").subcommands.cover;
  const alias = findSlashCommand("cover");
  const options = [
    { name: "base-mask", value: "0x0" },
    { name: "target-mask", value: "0xf" },
    { name: "queue", value: "I" },
    { name: "height", value: 1 },
    { name: "hold", value: "empty" },
  ];
  assert.deepEqual(buildSlashCommandArguments(alias, options),
    buildSlashCommandArguments(canonical, options));
  assert.deepEqual(alias.argvPrefix, ["build", "cover"]);
  assert.throws(() => buildSlashCommandArguments(alias, [
    { name: "base", value: "__________" },
    { name: "target", value: "####______" },
    { name: "next", value: "I" },
  ]), /unsupported option 'base'/i);
});

test("canonical PC routes use native typed observation, B2B, probability, tiling, and failed-queue contracts", () => {
  const pc = findSlashCommand("pc");
  const tilingCapability = findProductCapability("pc.tiling");
  const tiling = pc.subcommands.tiling;
  const failedQueueCapability = findProductCapability("pc.failed-queue");
  const failedQueue = pc.subcommands["failed-queue"];
  const base = [
    { name: "field", value: "__________\n__________" },
    { name: "next", value: "IOTSZ" },
    { name: "lines", value: 2 },
  ];
  assert.deepEqual(
    {
      problemContractId: failedQueue.problemContractId,
      inputSchemaId: failedQueue.inputSchemaId,
      modalSchemaId: failedQueue.modalSchemaId,
      resultContractId: failedQueue.resultContractId,
      resultAllowlist: failedQueue.resultAllowlist,
      argvPrefix: failedQueue.argvPrefix,
    },
    {
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-pattern.v2",
      modalSchemaId: "pc-pattern.v2",
      resultContractId: "pc-failed-queue.v2",
      resultAllowlist: ["pc-failed-queue.v2"],
      argvPrefix: ["pc", "failed-queue"],
    },
  );
  assert.deepEqual(failedQueueCapability.engine.argvPrefix, ["pc", "failed-queue"]);
  assert.deepEqual(failedQueueCapability.aliases, []);
  assert.equal(findSlashCommand("failed-queue"), null);
  assert.equal(findTextCommand("failed-queue"), null);
  const publicContract = DISCORD_PUBLIC_SEARCH_CONTRACT.find(
    ({ id }) => id === "failed-queue",
  );
  assert.deepEqual(
    {
      capabilityId: publicContract.capabilityId,
      problemContractId: publicContract.problemContractId,
      resultContractId: publicContract.resultContractId,
      engineKinds: publicContract.engineKinds,
    },
    {
      capabilityId: "pc.failed-queue",
      problemContractId: "pc-clear-to-empty.v2",
      resultContractId: "pc-failed-queue.v2",
      engineKinds: ["pc-failed-queue.v2"],
    },
  );
  assert.deepEqual(
    buildSlashCommandArguments(pc.subcommands.path, [
      ...base,
      { name: "hold", value: "T" },
      { name: "kicktable", value: "no-kick" },
      { name: "spin-profile", value: "all-spin-plus" },
      { name: "preserve-b2b", value: "on" },
    ]),
    [
      "pc", "path", "--lines", "2", "--board-mask", "0x0", "--height", "2",
      "--pieces", "5", "--queue", "IOTSZ", "--hold", "T",
      "--spin-profile", "all-spin-plus", "--preserve-b2b",
      "--rule", "no-kick",
    ],
  );
  for (const name of [
    "queue-knowledge",
    "solution-probabilities",
  ]) {
    assert.throws(
      () => buildSlashCommandArguments(pc.subcommands.path, [
        ...base,
        { name, value: "on" },
      ]),
      new RegExp(`unsupported option '${name}'`, "i"),
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(pc.subcommands.minimals, [
      ...base,
      { name: "queue-knowledge", value: "visible-7" },
    ]),
    /visible-7 is unavailable with minimum-cover/,
  );
  assert.deepEqual(
    {
      problemContractId: tiling.problemContractId,
      inputSchemaId: tiling.inputSchemaId,
      modalSchemaId: tiling.modalSchemaId,
      resultContractId: tiling.resultContractId,
      resultAuthorityId: tiling.resultAuthorityId,
      resultAllowlist: tiling.resultAllowlist,
      argvPrefix: tiling.argvPrefix,
      aliases: tilingCapability.aliases,
    },
    {
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-pattern.v2",
      modalSchemaId: "pc-pattern.v2",
      resultContractId: "pc-tiling-family.v1",
      resultAuthorityId: "tiling",
      resultAllowlist: ["pc-tiling-family.v1"],
      argvPrefix: ["pc", "tiling"],
      aliases: [],
    },
  );
  const tilingArguments = buildSlashCommandArguments(tiling, base);
  assert.deepEqual(
    tilingArguments,
    [
      "pc", "tiling", "--lines", "2", "--board-mask", "0x0", "--height", "2",
      "--pieces", "5", "--queue", "IOTSZ", "--hold", "empty",
    ],
  );
  assert.equal(tilingArguments.includes("--tiling-only"), false);
  assert.equal(tilingArguments.includes("--objective"), false);
  assert.equal(tilingArguments.includes("--rule"), false);
  assert.deepEqual(
    buildSlashCommandArguments(failedQueue, [
      ...base,
      { name: "failed-count", value: 17 },
    ]),
    [
      "pc", "failed-queue", "--lines", "2", "--board-mask", "0x0", "--height", "2",
      "--pieces", "5", "--queue", "IOTSZ", "--hold", "empty",
      "--failed-count", "17", "--rule", "srs-plus",
    ],
  );
  for (const prefix of ["$", ">"]) {
    const request = parseClearraTextRequest(
      `${prefix}pc failed-queue --field grid:__________/__________ --next IOTSZ --lines 2 --failed-count 17`,
      prefix,
    );
    assert.equal(request.command.capabilityId, "pc.failed-queue");
    assert.equal(request.command.publicResultKind, "failed-queue");
    assert.deepEqual(request.arguments_.slice(0, 2), ["pc", "failed-queue"]);
    assert.equal(
      classifyClearraTextCommand(
        `${prefix}pc failed-queue --field grid:__________/__________ --next IOTSZ --lines 2`,
        prefix,
      ),
      "pc.failed-queue",
    );
  }
  assert.equal(
    parseClearraTextRequest(
      "$failed-queue --field grid:__________/__________ --next IOTSZ --lines 2",
      "$",
    ),
    null,
  );
});

test("pc.minimals lowers only through the typed v2 minimum-cover authority", () => {
  const capability = findProductCapability("pc.minimals");
  const minimals = findSlashCommand("pc").subcommands.minimals;
  assert.deepEqual(
    {
      problemContractId: capability.problemContractId,
      inputSchemaId: capability.inputSchemaId,
      modalSchemaId: capability.modalSchemaId,
      resultContractId: capability.resultContractId,
      resultAllowlist: capability.resultAllowlist,
      effectClasses: capability.effectClasses,
      input: capability.engine.input,
      argvPrefix: capability.engine.argvPrefix,
    },
    {
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-pattern.v2",
      modalSchemaId: "pc-pattern.v2",
      resultContractId: "pc-minimum-cover.v2",
      resultAllowlist: ["pc-minimum-cover.v2"],
      effectClasses: [
        "search_space",
        "probability_semantics",
        "objective_selection",
        "result_materialization",
      ],
      input: "pc-v2",
      argvPrefix: ["pc", "minimals"],
    },
  );
  assert.deepEqual(
    buildSlashCommandArguments(minimals, [
      { name: "field", value: "__________\n__________" },
      { name: "next", value: "IOTSZ" },
      { name: "lines", value: 2 },
    ]),
    [
      "pc", "minimals", "--lines", "2", "--board-mask", "0x0",
      "--height", "2", "--pieces", "5", "--queue", "IOTSZ",
      "--hold", "empty", "--rule", "srs-plus",
    ],
  );
  const publicContract = DISCORD_PUBLIC_SEARCH_CONTRACT.find(
    ({ capabilityId }) => capabilityId === "pc.minimals",
  );
  assert.equal(publicContract.problemContractId, "pc-clear-to-empty.v2");
  assert.equal(publicContract.resultContractId, "pc-minimum-cover.v2");
  assert.deepEqual(publicContract.engineKinds, ["pc-minimum-cover.v2"]);
});

test("ordered forward spin never aliases the sfinder structural namespace", () => {
  const canonical = findSlashCommand("forward").subcommands.spin;
  assert.deepEqual(canonical.argvPrefix, ["spin-finder"]);
  assert.deepEqual(findSlashCommand("spin").argvPrefix, ["spin-finder"]);
  assert.equal(findSlashCommand("spin-cover"), null);

  assert.equal(parseClearraTextMessage("$sfinder spin PRIVATE", "$"), null);
  assert.equal(parseClearraTextMessage("$sfinder spincover PRIVATE", "$"), null);
  assert.equal(classifyClearraTextCommand("$sfinder spin PRIVATE", "$"), null);
  assert.equal(classifyClearraTextCommand("$sfinder spincover PRIVATE", "$"), null);

  for (const id of ["spin-structure.cover", "spin-structure.guaranteed"]) {
    assert.equal(findProductCapability(id).status, "active", id);
    assert.equal(findProductCapability(id).canonical.slash, true, id);
    assert.equal(findProductCapability(id).aliases.length, 0, id);
  }
});

test("verify is a hidden text-only diagnostic with no slash or help discovery", () => {
  const catalogSource = readFileSync(
    new URL("../src/discord/slash-command-catalog.mjs", import.meta.url),
    "utf8",
  );
  const modalSource = readFileSync(
    new URL("../src/discord/field-modal.mjs", import.meta.url),
    "utf8",
  );
  // README/docs discovery has one dependency-free owner in the metadata gate:
  // scripts/tools/public-command-docs.test.mjs. Keep live UI/parser checks here.
  assert.equal(globalCommands.some(({ name }) => name === "verify"), false);
  assert.equal(findTextCommand("verify"), null);
  assert.doesNotMatch(catalogSource, /verify|검증/iu);
  assert.doesNotMatch(modalSource, /verify|검증/iu);
  assert.doesNotMatch(formatSlashCommandHelp("", "en"), /verify|checks:/i);
  assert.doesNotMatch(formatSlashCommandHelp("", "ko"), /검증/u);
  assert.equal(
    DISCORD_PUBLIC_SEARCH_CONTRACT.some(({ id }) => id === "verify"),
    false,
  );
  assert.deepEqual(
    DISCORD_HIDDEN_TEXT_SEARCH_CONTRACT.map(({ id }) => id),
    ["verify"],
  );

  for (const prefix of ["$", ">"] ) {
    assert.equal(classifyClearraTextCommand(`${prefix}verify`, prefix), "verify");
    assert.deepEqual(
      parseClearraTextMessage(`${prefix}verify`, prefix),
      ["verify", "--format", "text"],
    );
    assert.equal(classifyClearraTextCommand(`${prefix}verify PRIVATE`, prefix), null);
    assert.equal(parseClearraTextMessage(`${prefix}verify kicks`, prefix), null);
  }
  assert.equal(parseClearraTextMessage("$sfinder verify", "$"), null);
});

test("objective is never a command and remains capability-closed on public inputs", () => {
  assert.equal(findSlashCommand("objective"), null);
  assert.equal(findTextCommand("objective"), null);
  assert.equal(findProductCapability("advanced.objective"), null);
  assert.match(
    formatSlashCommandHelp("objective", "en"),
    /`all`, `unique`, `min-cover`, `tiling`/,
  );
  assert.match(
    formatSlashCommandHelp("objective", "ko"),
    /`all`, `unique`, `min-cover`, `tiling`/,
  );
  assert.match(
    formatSlashCommandHelp("objective unique", "en"),
    /\$path.*--objective unique/,
  );
  assert.match(
    formatSlashCommandHelp("objective minimum-cover", "ko"),
    /objective min-cover/u,
  );
  for (const command of globalCommands.filter(({ name }) => !["build", "cover"].includes(name))) {
    assert.equal(JSON.stringify(command).includes('"name":"objective"'), false);
  }
  assert.equal(JSON.stringify(findSlashCommand("build")).includes('"name":"objective"'), true);
  assert.doesNotMatch(formatSlashCommandHelp("objective", "en"), /verify/i);
  assert.doesNotMatch(formatSlashCommandHelp("objective", "ko"), /검증/u);
});

test("finesse score uses one document parser through grouped and short names", () => {
  const score = findSlashCommand("build").subcommands["finesse-score"];
  const alias = findSlashCommand("finesse").subcommands.score;
  assert.equal(score.input, "finesse-score-v2");
  assert.equal(alias.input, score.input);
  assert.deepEqual(alias.registration.options, score.registration.options);
  assert.deepEqual(alias.argvPrefix, ["finesse", "score"]);
  assert.equal(findSlashCommand("finesse").subcommands.search, undefined);
});

test("family-specific Modals preserve their own required inputs and fail closed on overflow", () => {
  const pcModal = buildCommandModalResponse(interaction("pc", "path"));
  const buildModal = buildCommandModalResponse(interaction("build", "cover"));
  const coverAliasModal = buildCommandModalResponse(interaction("cover", null));
  const setupModal = buildCommandModalResponse(interaction("setup", "joint"));
  const forwardModal = buildCommandModalResponse(interaction("forward", "damage"));
  const structureModal = buildCommandModalResponse(interaction("spin-structure", "search"));
  assert.equal(pcModal.data.custom_id, "clearra:search:v4:pc~path");
  assert.equal(buildModal, null);
  assert.equal(coverAliasModal, null);
  assert.equal(setupModal.data.custom_id, "clearra:search:v4:setup~joint");
  assert.equal(forwardModal.data.custom_id, "clearra:search:v4:forward~damage");
  assert.equal(structureModal.data.custom_id, "clearra:search:v4:spin-structure~search");
  assert.notDeepEqual(componentIds(pcModal), componentIds(setupModal));
  assert.notDeepEqual(componentIds(forwardModal), componentIds(structureModal));

  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("cover"), [
      { name: "finesse", value: "inputs" },
    ]),
    /unsupported option 'finesse'/i,
  );
});

test("short PC replay and score-finder Modals have the same choices as the grouped commands", () => {
  for (const verb of ["path", "score-finder"]) {
    const alias = buildCommandModalResponse(interaction(verb, null));
    const canonical = buildCommandModalResponse(interaction("pc", verb));
    assert.deepEqual(alias.data.components, canonical.data.components, verb);
    const kicks = alias.data.components.find(({ component }) =>
      component.custom_id === "kicktable")?.component;
    assert.ok(kicks?.options.some(({ value }) => value === "no-kick"), verb);
  }
});

function interaction(root, subcommand, options = []) {
  return {
    type: 2,
    data: {
      type: 1,
      name: root,
      options: subcommand
        ? [{ type: 1, name: subcommand, options }]
        : options,
    },
  };
}

function componentIds(modal) {
  return modal.data.components.map(({ component }) => component.custom_id);
}

function argvContract(arguments_) {
  const result = [];
  for (let index = 0; index < arguments_.length; index += 1) {
    const token = arguments_[index];
    if (index === 0 || token === "--no-mirror" || token === "--include-mirror") {
      result.push([token, null]);
      continue;
    }
    if (!token.startsWith("--")) continue;
    result.push([token, arguments_[index + 1]]);
    index += 1;
  }
  return result.sort(([left], [right]) => left.localeCompare(right));
}
