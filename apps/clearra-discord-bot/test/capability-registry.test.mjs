import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

import {
  CLI_COMMAND_LOWERING_AUTHORITY,
  CLI_COMPATIBILITY_LOWERING_AUTHORITY,
  assertDiscordGenericCompatibilityRoutes,
  assertProductCapabilityRegistry,
  discordGenericCompatibilityRouteProjection,
  discordLegacyRouteProjection,
  discordRuntimeProjection,
  findDiscordGenericCompatibilityRoute,
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
import {
  classifyClearraTextCommand,
  parseClearraTextMessage,
  parseClearraTextRequest,
} from "../src/clearra/text-command.mjs";

function readMarkdownTree(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const target = new URL(
        `${entry.name}${entry.isDirectory() ? "/" : ""}`,
        directory,
      );
      if (entry.isDirectory()) {
        return readMarkdownTree(target);
      }
      return entry.isFile() && entry.name.endsWith(".md")
        ? [readFileSync(target, "utf8")]
        : [];
    });
}

const legacyAliasFixture = JSON.parse(readFileSync(new URL(
  "../../../tests/fixtures/contracts/legacy_alias_equivalence.v1.json",
  import.meta.url,
), "utf8"));

test("v0.8 capability registry separates problem, algorithm, timeout, form, and result authority", () => {
  assert.equal(assertProductCapabilityRegistry(), true);
  assert.equal(assertDiscordGenericCompatibilityRoutes(), true);
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
    const capability = findProductCapability(command.capabilityId) ??
      findDiscordGenericCompatibilityRoute(command.capabilityId);
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

test("score-minimals compatibility routes preserve the canonical typed portfolio authority", () => {
  const scoreMinimals = findProductCapability("pc.score-minimals");
  const alias = scoreMinimals.aliases.find(({ slash }) => slash);
  const projected = lowerCapabilityRouteRequest(scoreMinimals, alias);
  assert.equal(projected.route.input, "pc-score-v2");
  assert.deepEqual(projected.route.argvPrefix, ["pc", "score-minimals"]);
  assert.deepEqual(projected.parameters, {});
  assert.deepEqual(
    lowerCapabilityRouteRequest(scoreMinimals, alias, { scoreProfile: "tetrio" }).parameters,
    { scoreProfile: "tetrio" },
  );

  const forged = {
    ...alias,
    input: "forward-damage-v2",
    inputSchemaId: "forged-input.v1",
    modalSchemaId: "forged-modal.v1",
    argvPrefix: ["damage"],
  };
  assert.notDeepEqual(
    lowerCapabilityRouteRequest(scoreMinimals, forged),
    projected,
    "route input/schema/argv drift must remain visible to the registry projection",
  );

  const authority = legacyAliasFixture.routes.find(
    ({ id }) => id === "pc.score-minimals/slash/score-minimals",
  );
  const live = discordLegacyRouteProjection().find(({ id }) => id === authority.id);
  const normalizedLive = {
    ...live,
    case_id: "pc.score-minimals/score-minimals/",
    path: live.path ?? live.name.split(" "),
  };
  delete normalizedLive.name;
  assert.deepEqual(normalizedLive, authority);
  for (const [field, forgedValue] of Object.entries({
    input: "forward-damage-v2",
    input_schema_id: "forged-input.v1",
    modal_schema_id: "forged-modal.v1",
    argv_prefix: ["damage"],
    public_result_kind: "damage",
  })) {
    assert.throws(
      () => assert.deepEqual({ ...normalizedLive, [field]: forgedValue }, authority),
      /Expected values to be strictly deep-equal/u,
      `forged ingress ${field} must fail the independent JSON authority comparison`,
    );
  }
});

test("every remaining typed slash and text alias executes the frozen parser-equivalence fixture", () => {
  assert.equal(legacyAliasFixture.schema_id, "clearra.legacy-alias-equivalence.v1");
  const promotedCapabilities = new Set([
    "pc.path",
    "pc.score-finder",
    "setup.joint",
    "setup.build",
    "setup.pc",
    "spin-structure.search",
  ]);
  const typedFixtureRoutes = legacyAliasFixture.routes.filter(
    ({ capability_id: capabilityId }) =>
      !["pc.chance", "pc.score"].includes(capabilityId) &&
      !promotedCapabilities.has(capabilityId),
  );
  const typedFixtureCases = legacyAliasFixture.cases.filter(
    ({ capability_id: capabilityId }) =>
      !["pc.chance", "pc.score", "build.cover"].includes(capabilityId) &&
      !promotedCapabilities.has(capabilityId),
  );
  const actualRoutes = discordLegacyRouteProjection()
    .filter(({ capability_id: capabilityId }) =>
      capabilityId !== "pc.score" && !promotedCapabilities.has(capabilityId)
    )
    .map((route) => {
    const path = route.path ?? route.name.split(" ");
    return {
      id: route.id,
      case_id: [
        route.capability_id,
        ...path,
        ...(path.length === 1 ? [""] : []),
      ].join("/"),
      capability_id: route.capability_id,
      surface: route.surface,
      path,
      classification: route.classification,
      input: route.input,
      input_schema_id: route.input_schema_id,
      modal_schema_id: route.modal_schema_id,
      argv_prefix: route.argv_prefix,
      public_result_kind: route.public_result_kind,
      ...(route.preset ? { preset: route.preset } : {}),
      ...(route.remove_in ? { remove_in: route.remove_in } : {}),
      ...(route.lifetime ? { lifetime: route.lifetime } : {}),
    };
  });
  assert.deepEqual(actualRoutes, typedFixtureRoutes);
  assert.equal(actualRoutes.length, 20);
  assert.equal(typedFixtureCases.length, 8);

  for (const fixtureCase of typedFixtureCases) {
    const capability = findProductCapability(fixtureCase.capability_id);
    assert.ok(capability, fixtureCase.id);
    assert.equal(fixtureCase.problem_contract_id, capability.problemContractId, fixtureCase.id);
    assert.equal(fixtureCase.result_contract_id, capability.resultContractId, fixtureCase.id);

    const canonical = commandAtPath(fixtureCase.canonical_path);
    const alias = commandAtPath(fixtureCase.alias_path);
    assert.equal(canonical.capabilityId, capability.id, fixtureCase.id);
    assert.equal(alias.capabilityId, capability.id, fixtureCase.id);
    assert.equal(canonical.input, fixtureCase.canonical_input, fixtureCase.id);
    assert.equal(alias.input, fixtureCase.alias_input, fixtureCase.id);

    const canonicalArguments = buildSlashCommandArguments(
      canonical,
      fixtureCase.canonical_options,
    );
    const aliasArguments = buildSlashCommandArguments(alias, fixtureCase.alias_options);
    assert.deepEqual(canonicalArguments, fixtureCase.canonical_argv, fixtureCase.id);
    assert.deepEqual(aliasArguments, fixtureCase.alias_argv, fixtureCase.id);
    assert.equal(
      fixtureCase.canonical_web_command,
      ["clearra", ...canonicalArguments].join(" "),
      fixtureCase.id,
    );
    assert.equal(
      fixtureCase.alias_web_command,
      ["clearra", ...aliasArguments].join(" "),
      fixtureCase.id,
    );

    const canonicalText = parseClearraTextRequest(fixtureCase.canonical_text, "$");
    const aliasText = parseClearraTextRequest(fixtureCase.alias_text, "$");
    const canonicalTextArgv = withoutRetiredPcExecutionFlags(
      fixtureCase.canonical_text_argv,
    );
    const aliasTextArgv = withoutRetiredPcExecutionFlags(fixtureCase.alias_text_argv);
    assert.equal(canonicalText.command.capabilityId, capability.id, fixtureCase.id);
    assert.equal(aliasText.command.capabilityId, capability.id, fixtureCase.id);
    assert.deepEqual(canonicalText.arguments_, canonicalTextArgv, fixtureCase.id);
    assert.deepEqual(aliasText.arguments_, aliasTextArgv, fixtureCase.id);
    assert.equal(
      ["clearra", ...canonicalTextArgv].join(" "),
      ["clearra", ...canonicalText.arguments_].join(" "),
      fixtureCase.id,
    );
    assert.equal(
      ["clearra", ...aliasTextArgv].join(" "),
      ["clearra", ...aliasText.arguments_].join(" "),
      fixtureCase.id,
    );

    const authorityBySurface = new Map(
      typedFixtureRoutes
        .filter(({ case_id }) => case_id === fixtureCase.id)
        .map((route) => [route.surface, route]),
    );
    for (const [surface, liveCommand] of [
      ["discord-slash", alias],
      ["discord-text", aliasText.command],
    ]) {
      const routeAuthority = authorityBySurface.get(surface);
      assert.ok(routeAuthority, `${fixtureCase.id}:${surface}`);
      assert.deepEqual(
        {
          input: liveCommand.input,
          input_schema_id: liveCommand.inputSchemaId,
          modal_schema_id: liveCommand.modalSchemaId,
          argv_prefix: liveCommand.argvPrefix,
          public_result_kind: liveCommand.publicResultKind,
        },
        {
          input: routeAuthority.input,
          input_schema_id: routeAuthority.input_schema_id,
          modal_schema_id: routeAuthority.modal_schema_id,
          argv_prefix: routeAuthority.argv_prefix,
          public_result_kind: routeAuthority.public_result_kind,
        },
        `${fixtureCase.id}:${surface}: live parser route escaped JSON authority`,
      );
    }

    if (fixtureCase.classification !== "equivalence") {
      assert.equal(fixtureCase.classification, "fixed-preset", fixtureCase.id);
      const slashRoute = capability.aliases.find((route) =>
        route.slash &&
        route.root === fixtureCase.alias_path[0] &&
        route.subcommand === (fixtureCase.alias_path[1] ?? null)
      );
      assert.deepEqual(alias.compatibilityPreset, slashRoute.preset, fixtureCase.id);
      for (const [field, value] of Object.entries(slashRoute.preset)) {
        const option = presetOptionName(field);
        assert.deepEqual(
          buildSlashCommandArguments(alias, [
            ...fixtureCase.alias_options,
            { name: option, value },
          ]),
          aliasArguments,
          `${fixtureCase.id}:${field}`,
        );
        assert.deepEqual(
          parseClearraTextRequest(`${fixtureCase.alias_text} --${option} ${value}`, "$")
            .arguments_,
          fixtureCase.alias_text_argv,
          `${fixtureCase.id}:${field}:text`,
        );
      }
      assert.throws(
        () => buildSlashCommandArguments(alias, [
          ...fixtureCase.alias_options,
          { name: fixtureCase.conflict.option, value: fixtureCase.conflict.value },
        ]),
        /Compatibility command .* fixes/u,
        fixtureCase.id,
      );
      assert.throws(
        () => parseClearraTextRequest(
          `${fixtureCase.alias_text} --${fixtureCase.conflict.option} ${fixtureCase.conflict.value}`,
          "$",
        ),
        /Compatibility command .* fixes/u,
        `${fixtureCase.id}:text`,
      );
    }
  }
});

test("typed PC results and generic chance/percent/score retain independent live route authority", () => {
  const capability = findProductCapability("pc.chance");
  const canonical = findSlashCommand("pc").subcommands.chance;
  const generic = discordGenericCompatibilityRouteProjection();
  const contracts = new Map(
    DISCORD_PUBLIC_SEARCH_CONTRACT.map((entry) => [entry.id, entry]),
  );

  assert.deepEqual(capability.aliases, []);
  assert.deepEqual(
    {
      input: canonical.input,
      problemContractId: canonical.problemContractId,
      inputSchemaId: canonical.inputSchemaId,
      modalSchemaId: canonical.modalSchemaId,
      resultContractId: canonical.resultContractId,
      resultAuthorityId: canonical.resultAuthorityId,
      resultAllowlist: canonical.resultAllowlist,
      argvPrefix: canonical.argvPrefix,
      effectClasses: canonical.effectClasses,
    },
    {
      input: "pc-chance-v2",
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-pattern.v2",
      modalSchemaId: "pc-pattern.v2",
      resultContractId: "pc-probability.v2",
      resultAuthorityId: "pc-chance",
      resultAllowlist: ["pc-probability.v2"],
      argvPrefix: ["pc", "chance"],
      effectClasses: ["search_space", "supply_semantics", "probability_semantics"],
    },
  );
  assert.equal(Object.hasOwn(capability.engine, "pcObjective"), false);
  assert.deepEqual(capability.engine.fixedSemantics, {
    solutionIdentity: "unique",
    queueKnowledge: "full-oracle",
  });
  assert.deepEqual(
    generic.map((route) => ({
      id: route.id,
      root: route.root,
      input: route.input,
      inputSchemaId: route.inputSchemaId,
      modalSchemaId: route.modalSchemaId,
      argvPrefix: route.argvPrefix,
      resultContractId: route.resultContractId,
      resultAuthorityId: route.resultAuthorityId,
      resultAllowlist: route.resultAllowlist,
    })),
    [
      {
        id: "discord.compat.chance",
        root: "chance",
        input: "pc",
        inputSchemaId: "pc-pattern",
        modalSchemaId: "pc-pattern",
        argvPrefix: ["sfinder", "chance"],
        resultContractId: "pc-scenario",
        resultAuthorityId: "chance",
        resultAllowlist: ["pc-scenario"],
      },
      {
        id: "discord.compat.percent",
        root: "percent",
        input: "pc",
        inputSchemaId: "pc-pattern",
        modalSchemaId: "pc-pattern",
        argvPrefix: ["sfinder", "percent"],
        resultContractId: "pc-scenario",
        resultAuthorityId: "percent",
        resultAllowlist: ["pc-scenario"],
      },
      {
        id: "discord.compat.score",
        root: "score",
        input: "pc",
        inputSchemaId: "pc-pattern",
        modalSchemaId: "pc-pattern",
        argvPrefix: ["sfinder", "score"],
        resultContractId: "pc-scenario",
        resultAuthorityId: "score",
        resultAllowlist: ["pc-scenario"],
      },
    ],
  );
  for (const [name, id] of [
    ["chance", "discord.compat.chance"],
    ["percent", "discord.compat.percent"],
    ["score", "discord.compat.score"],
  ]) {
    assert.equal(findSlashCommand(name).capabilityId, id);
    assert.equal(findTextCommand(name).capabilityId, id);
    assert.equal(findSlashCommand(name).subcommands, undefined);
  }
  assert.equal(
    Object.keys(findSlashCommand("pc").subcommands).filter((name) => name === "chance")
      .length,
    1,
  );
  assert.deepEqual(
    {
      typed: contracts.get("pc-chance").engineKinds,
      chance: contracts.get("chance").engineKinds,
      percent: contracts.get("percent").engineKinds,
    },
    {
      typed: ["pc-probability.v2"],
      chance: ["pc-scenario"],
      percent: ["pc-scenario"],
    },
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
  assert.match(formatSlashCommandHelp("pc saves", "en"), /unconditional probability/i);
  assert.match(formatSlashCommandHelp("pc saves", "en"), /conditional probability given/i);
  assert.match(formatSlashCommandHelp("pc best-save", "en"), /first result in deterministic order/i);
  assert.doesNotMatch(formatSlashCommandHelp("pc best-save", "en"), /canonical candidate ID/i);
  assert.doesNotMatch(formatSlashCommandHelp("pc saves", "ko"), /같은 기능/u);
  assert.doesNotMatch(formatSlashCommandHelp("pc best-save", "ko"), /같은 기능/u);
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
  assert.ok(findSlashCommand("finesse").subcommands.search);
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
  assert.deepEqual(
    buildSlashCommandArguments(exactAlias, exactOptions),
    exactArguments,
  );
  assert.equal(exactArguments.includes("--preserve-b2b"), false);

  const chanceOptions = exactOptions.map((option) => ({ ...option }));
  chanceOptions.find(({ name }) => name === "next").value = "[IOTS]!";
  chanceOptions.find(({ name }) => name === "spin-profile").value = "all-mini-plus";
  const chanceArguments = buildSlashCommandArguments(chance, chanceOptions);
  assert.equal(chanceArguments[chanceArguments.indexOf("--patterns") + 1], "[IOTS]!");
  assert.equal(chanceArguments.includes("--queue"), false);
  assert.equal(chanceArguments.includes("--preserve-b2b"), false);
  assert.deepEqual(
    buildSlashCommandArguments(chanceAlias, chanceOptions),
    chanceArguments,
  );

  const exactRoutes = exactCapability.aliases.filter(({ root }) => root === "allspin-sol-finder");
  const chanceRoutes = chanceCapability.aliases.filter(({ root }) => root === "allspin-pres-chance");
  assert.deepEqual(exactRoutes.map(({ slash, text, deprecateAfter }) => [slash, text, deprecateAfter]), [
    [true, false, "v0.10.0"],
    [false, true, null],
  ]);
  assert.deepEqual(chanceRoutes.map(({ slash, text, deprecateAfter }) => [slash, text, deprecateAfter]), [
    [true, false, "v0.10.0"],
    [false, true, null],
  ]);
  assert.match(formatSlashCommandHelp("pc allspin-sol", "en"), /command-intent compatibility only/i);
  assert.match(formatSlashCommandHelp("pc allspin-sol", "ko"), /명령 의도만 보장/u);
  assert.match(formatSlashCommandHelp("allspin-pres-chance", "en"), /removal in v0\.10/i);
  assert.match(formatSlashCommandHelp("allspin-pres-chance", "ko"), /v0\.10에 제거/u);

  const contracts = new Map(
    DISCORD_PUBLIC_SEARCH_CONTRACT.map((entry) => [entry.id, entry]),
  );
  assert.equal(
    contracts.get("allspin-sol").resultContractId,
    "pc-b2b-preserving-witness.v1",
  );
  assert.equal(
    contracts.get("allspin-sol-finder").resultContractId,
    "pc-b2b-preserving-witness.v1",
  );
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
  assert.equal(authority.schema_id, "clearra.product-capability-registry.v1");
  const expectedRuntimeIds = authority.runtime_projection.current_capabilities
    .map(({ id }) => id)
    .sort();
  const actualRuntimeIds = productCapabilityRegistry.map(({ id }) => id).sort();
  assert.equal(expectedRuntimeIds.length, 47);
  assert.equal(actualRuntimeIds.length, 47);
  assert.deepEqual(actualRuntimeIds, expectedRuntimeIds);
  const stableIds = new Set(authority.capabilities.map(({ id }) => id));
  for (const capability of productCapabilityRegistry) {
    assert.equal(stableIds.has(capability.id), true, capability.id);
  }

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

test("typed Build cover and legacy fieldwise cover retain separate authorities", () => {
  const canonical = findSlashCommand("build").subcommands.cover;
  const compatibility = findSlashCommand("cover");
  assert.deepEqual(canonical.argvPrefix, ["build", "cover"]);
  assert.equal(canonical.input, "build-v2-cover");
  assert.deepEqual(compatibility.argvPrefix, ["build-probability"]);
  assert.equal(findTextCommand("cover").argvPrefix[0], "build-probability");

  const base = "__________";
  const target = "####______";
  const canonicalArguments = buildSlashCommandArguments(canonical, [
    { name: "base-mask", value: "0x0" },
    { name: "target-mask", value: "0xf" },
    { name: "queue", value: "I" },
    { name: "height", value: 1 },
    { name: "hold", value: "empty" },
  ]);
  const compatibilityArguments = buildSlashCommandArguments(compatibility, [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
    { name: "options", value: "hold=use" },
  ]);
  assert.deepEqual(canonicalArguments.slice(0, 2), ["build", "cover"]);
  assert.equal(compatibilityArguments[0], "build-probability");
  assert.notDeepEqual(canonicalArguments, compatibilityArguments);
  assert.throws(
    () => buildSlashCommandArguments(canonical, [
      { name: "base", value: base },
      { name: "target", value: target },
      { name: "next", value: "I" },
    ]),
    /unsupported option 'base'/i,
  );
  assert.equal(parseClearraTextMessage("$sfinder cover PRIVATE", "$"), null);
});

test("canonical PC scoring owns typed summary authority while legacy score stays generic", () => {
  const capability = findProductCapability("pc.score");
  const scoreMinimalsCapability = findProductCapability("pc.score-minimals");
  const genericRoute = findDiscordGenericCompatibilityRoute("discord.compat.score");
  const score = findSlashCommand("pc").subcommands.score;
  const scoreMinimals = findSlashCommand("pc").subcommands["score-minimals"];
  const legacy = findSlashCommand("score");
  assert.deepEqual(
    {
      input: score.input,
      problemContractId: score.problemContractId,
      inputSchemaId: score.inputSchemaId,
      modalSchemaId: score.modalSchemaId,
      resultContractId: score.resultContractId,
      resultAuthorityId: score.resultAuthorityId,
      resultAllowlist: score.resultAllowlist,
      argvPrefix: score.argvPrefix,
      effectClasses: score.effectClasses,
    },
    {
      input: "pc-score-v2",
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-score.v2",
      modalSchemaId: "pc-score.v2",
      resultContractId: "pc-score-summary.v2",
      resultAuthorityId: "pc-score",
      resultAllowlist: ["pc-score-summary.v2"],
      argvPrefix: ["pc", "score"],
      effectClasses: [
        "search_space",
        "reachability_semantics",
        "score_semantics",
        "objective_selection",
      ],
    },
  );
  assert.equal(Object.hasOwn(capability.engine, "pcObjective"), false);
  assert.deepEqual(capability.aliases, []);
  assert.deepEqual(
    {
      input: scoreMinimals.input,
      problemContractId: scoreMinimals.problemContractId,
      inputSchemaId: scoreMinimals.inputSchemaId,
      modalSchemaId: scoreMinimals.modalSchemaId,
      resultContractId: scoreMinimals.resultContractId,
      resultAuthorityId: scoreMinimals.resultAuthorityId,
      resultAllowlist: scoreMinimals.resultAllowlist,
      argvPrefix: scoreMinimals.argvPrefix,
      fixedSemantics: scoreMinimalsCapability.engine.fixedSemantics,
    },
    {
      input: "pc-score-v2",
      problemContractId: "pc-clear-to-empty.v2",
      inputSchemaId: "pc-score.v2",
      modalSchemaId: "pc-score.v2",
      resultContractId: "pc-score-portfolio.v2",
      resultAuthorityId: "score-minimals",
      resultAllowlist: ["pc-score-portfolio.v2"],
      argvPrefix: ["pc", "score-minimals"],
      fixedSemantics: {
        scoreEquality: "score-only",
        attackRole: "informational-only",
        discordTieSelection: "smallest-canonical-candidate-id",
      },
    },
  );
  assert.equal(legacy.capabilityId, "discord.compat.score");
  assert.equal(genericRoute.telemetryIdentity, "discord.compat.score");
  assert.equal(
    genericRoute.loweringAuthority,
    CLI_COMPATIBILITY_LOWERING_AUTHORITY,
  );
  assert.notEqual(genericRoute.telemetryIdentity, capability.telemetryIdentity);
  assert.notEqual(genericRoute.loweringAuthority, capability.loweringAuthority);
  assert.equal(Object.hasOwn(genericRoute, "preset"), false);
  assert.equal(legacy.compatibilityPreset, null);
  assert.deepEqual(
    {
      input: legacy.input,
      problemContractId: legacy.problemContractId,
      inputSchemaId: legacy.inputSchemaId,
      modalSchemaId: legacy.modalSchemaId,
      resultContractId: legacy.resultContractId,
      resultAuthorityId: legacy.resultAuthorityId,
      resultAllowlist: legacy.resultAllowlist,
      argvPrefix: legacy.argvPrefix,
      effectClasses: legacy.effectClasses,
    },
    {
      input: "pc",
      problemContractId: "pc-clear-to-empty",
      inputSchemaId: "pc-pattern",
      modalSchemaId: "pc-pattern",
      resultContractId: "pc-scenario",
      resultAuthorityId: "score",
      resultAllowlist: ["pc-scenario"],
      argvPrefix: ["sfinder", "score"],
      effectClasses: ["search_space", "supply_semantics", "result_materialization"],
    },
  );
  assert.notDeepEqual(score.resultAllowlist, legacy.resultAllowlist);
  assert.deepEqual(
    score.registration.options.map(({ name }) => name),
    [
      "next", "field", "lines", "hold", "kicktable", "score-profile",
      "spin-profile", "initial-b2b",
    ],
  );

  const base = [
    { name: "field", value: "__________\n__________" },
    { name: "next", value: "IOTSZ" },
    { name: "lines", value: 2 },
  ];
  const canonical = buildSlashCommandArguments(score, base);
  assert.deepEqual(canonical, [
    "pc", "score",
    "--lines", "2",
    "--board-mask", "0x0",
    "--height", "2",
    "--pieces", "5",
    "--queue", "IOTSZ",
    "--hold", "empty",
    "--score-profile", "tetrio",
    "--rule", "srs-plus",
  ]);
  assert.equal(canonical.includes("--objective"), false);
  assert.equal(canonical.includes("--score"), false);
  const scoreMinimalsArguments = buildSlashCommandArguments(scoreMinimals, base);
  assert.deepEqual(scoreMinimalsArguments, [
    "pc", "score-minimals",
    "--lines", "2",
    "--board-mask", "0x0",
    "--height", "2",
    "--pieces", "5",
    "--queue", "IOTSZ",
    "--hold", "empty",
    "--score-profile", "tetrio",
    "--rule", "srs-plus",
  ]);
  for (const forbidden of ["--objective", "--score", "--solution-probabilities", "--ties", "--tie-snapshot", "--tie-cursor"]) {
    assert.equal(scoreMinimalsArguments.includes(forbidden), false, forbidden);
  }
  assert.equal(buildSlashCommandArguments(legacy, base).includes("--score-profile"), false);
  assert.doesNotMatch(formatSlashCommandHelp("pc score", "en"), /profile-specific exact scoring/i);
  assert.match(formatSlashCommandHelp("pc score", "en"), /basic-approximation/);
  assert.match(formatSlashCommandHelp("pc score", "en"), /profile_specific_exact=false/);
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
  assert.equal(canonical.description, "Find ordered forward selected-profile spin completions");
  assert.match(formatSlashCommandHelp("forward spin", "ko"), /선택한 스핀 프로필/u);
  assert.doesNotMatch(formatSlashCommandHelp("forward spin", "ko"), /TSM은 지원하지/u);
  assert.deepEqual(canonical.argvPrefix, ["spin-finder"]);
  assert.deepEqual(findSlashCommand("spin").argvPrefix, ["spin-finder"]);
  assert.deepEqual(findSlashCommand("spin-cover").argvPrefix, ["spin-finder"]);

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
  const discordReadme = readFileSync(
    new URL("../README.md", import.meta.url),
    "utf8",
  );
  const rootReadme = readFileSync(
    new URL("../../../README.md", import.meta.url),
    "utf8",
  );
  const publicDiagnosticDocs = readMarkdownTree(
    new URL("../../../docs/", import.meta.url),
  ).join("\n");
  const hiddenDiagnosticDescription =
    /--diagnostics|(?:^|[\s`])\/verify(?=$|[\s`])|\$verify|>verify|diagnostic\.verify|VerifyKicks|`Verify`|\b(?:clearra|sfinder)\s+verify\b|\bverify\s+kicks\b|\bhidden\s+verify\b|\bverification\s+(?:scope|commands)\b|\b(?:reserved|hidden|internal|non-search)\s+diagnostic(?:s|\s+probes?)?\b|\bdiagnostic\s+(?:root|route|modal|boundary|probes?|feature)\b|\bdiagnostics?\s+intentionally\b/iu;
  assert.equal(globalCommands.some(({ name }) => name === "verify"), false);
  assert.equal(findTextCommand("verify"), null);
  assert.doesNotMatch(catalogSource, /verify|검증/iu);
  assert.doesNotMatch(modalSource, /verify|검증/iu);
  assert.doesNotMatch(discordReadme, hiddenDiagnosticDescription);
  assert.doesNotMatch(rootReadme, hiddenDiagnosticDescription);
  assert.doesNotMatch(publicDiagnosticDocs, hiddenDiagnosticDescription);
  assert.doesNotMatch(formatSlashCommandHelp("", "en"), /verify|checks:/i);
  assert.doesNotMatch(formatSlashCommandHelp("", "ko"), /검증/u);
  assert.match(formatSlashCommandHelp("verify", "en"), /Unknown Clearra command/);
  assert.match(formatSlashCommandHelp("verify", "ko"), /알 수 없는 Clearra 명령어/u);
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
      ["sfinder", "verify", "--format", "text"],
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
  assert.match(
    formatSlashCommandHelp("objective tiling-only", "en"),
    /Unknown objective/,
  );
  for (const command of globalCommands.filter(({ name }) => name !== "build")) {
    assert.equal(JSON.stringify(command).includes('"name":"objective"'), false);
  }
  assert.equal(JSON.stringify(findSlashCommand("build")).includes('"name":"objective"'), true);
  assert.doesNotMatch(formatSlashCommandHelp("objective", "en"), /verify/i);
  assert.doesNotMatch(formatSlashCommandHelp("objective", "ko"), /검증/u);
});

test("finesse search keeps its legacy fieldwise preset while score keeps its document form", () => {
  const compatibility = findSlashCommand("finesse").subcommands.search;
  const base = "__________";
  const target = "####______";
  const alias = buildSlashCommandArguments(compatibility, [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
    { name: "options", value: "hold=use knowledge=both" },
  ]);
  assert.equal(alias[0], "build-probability");
  assert.ok(alias.includes("--finesse"));
  assert.ok(alias.includes("--no-mirror"));

  const score = findSlashCommand("build").subcommands["finesse-score"];
  assert.equal(score.input, "finesse-score-v2");
  assert.deepEqual(
    score.registration.options.map(({ name }) => name),
    ["document", "next", "kicktable", "hold", "knowledge", "source-pieces"],
  );
  assert.equal(findSlashCommand("finesse").subcommands.score.input, "finesse-score");
});

test("family-specific Modals preserve their own required inputs and fail closed on overflow", () => {
  const pcModal = buildCommandModalResponse(interaction("pc", "path"));
  const buildModal = buildCommandModalResponse(interaction("build", "cover"));
  const legacyBuildModal = buildCommandModalResponse(interaction("cover", null));
  const setupModal = buildCommandModalResponse(interaction("setup", "joint"));
  const forwardModal = buildCommandModalResponse(interaction("forward", "damage"));
  const structureModal = buildCommandModalResponse(interaction("spin-structure", "search"));
  assert.equal(pcModal.data.custom_id, "clearra:search:v4:pc~path");
  assert.equal(buildModal, null);
  assert.equal(legacyBuildModal.data.custom_id, "clearra:search:v4:cover");
  assert.equal(setupModal.data.custom_id, "clearra:search:v4:setup~joint");
  assert.equal(forwardModal.data.custom_id, "clearra:search:v4:forward~damage");
  assert.equal(structureModal.data.custom_id, "clearra:search:v4:spin-structure~search");
  assert.notDeepEqual(componentIds(pcModal), componentIds(legacyBuildModal));
  assert.notDeepEqual(componentIds(legacyBuildModal), componentIds(setupModal));
  assert.notDeepEqual(componentIds(forwardModal), componentIds(structureModal));

  assert.throws(
    () => buildCommandModalResponse(interaction("cover", null, [
      { name: "finesse", value: "inputs" },
    ])),
    /unsupported option 'finesse'/i,
  );
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

function commandAtPath(path) {
  const root = findSlashCommand(path[0]);
  const command = path.length === 1 ? root : root?.subcommands?.[path[1]];
  assert.ok(command, `missing slash command: ${path.join(" ")}`);
  return command;
}

function presetOptionName(field) {
  const name = ({
    finesse: "finesse",
    mirror: "mirror",
    scoreProfile: "score-profile",
    setupPriority: "priority",
  })[field];
  assert.ok(name, `unknown compatibility preset field: ${field}`);
  return name;
}

function withoutRetiredPcExecutionFlags(arguments_) {
  return arguments_.filter((token) =>
    token !== "--no-tablebase" && token !== "--no-build-dependency-dag"
  );
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
