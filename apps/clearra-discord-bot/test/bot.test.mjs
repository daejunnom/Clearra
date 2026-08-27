import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  CTK3_FILE_MIME_TYPE,
  decodeCtk3,
  encodeCtk3,
  encodeCtk3File,
} from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { Clearrabot } from "../src/bot.mjs";
import { prepareClearraArguments } from "../src/clearra/command.mjs";
import { OracleMessageIngress } from "../src/ingress/oracle-message-ingress.mjs";
import {
  findSlashCommand,
  formatSlashCommandHelp,
  globalCommands,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";
import { DiscordLocalePreferences } from "../src/discord/locale-preferences.mjs";
import { DISCORD_PUBLIC_SEARCH_CONTRACT } from "../src/discord/public-search-contract.mjs";
import { decodeViewerDocument } from "../src/viewer/document.mjs";

const LEGACY_PUBLIC_SEARCH_COMMANDS = [
  "path",
  "chance",
  "percent",
  "score",
  "minimals",
  "score-minimals",
  "score-finder",
  "allspin-sol-finder",
  "allspin-pres-chance",
  "cover",
  "pc-setup",
  "best-setup",
  "dpc-finder",
  "spin",
  "spin-cover",
  "damage",
];

const CANONICAL_SEARCH_ROOTS = ["pc", "build", "setup", "forward", "spin-structure", "utility"];
const COMPATIBILITY_SEARCH_ROOTS = ["finesse"];

test("slash catalog registers only curated active commands", () => {
  assert.equal(slashCommandCatalog.length, 27);
  assert.equal(globalCommands.length, 28);
  assert.equal(globalCommands.filter(({ type }) => type === 3).length, 1);
  assert.deepEqual(
    slashCommandCatalog.map((command) => command.name),
    [
      "help",
      "render-file",
      "channel-settings",
      "server-settings",
      ...CANONICAL_SEARCH_ROOTS,
      ...COMPATIBILITY_SEARCH_ROOTS,
      ...LEGACY_PUBLIC_SEARCH_COMMANDS,
    ],
  );
  assert.deepEqual(
    globalCommands.map((command) => command.name),
    [
      "help",
      "render-file",
      "channel-settings",
      "server-settings",
      ...CANONICAL_SEARCH_ROOTS,
      ...COMPATIBILITY_SEARCH_ROOTS,
      ...LEGACY_PUBLIC_SEARCH_COMMANDS,
      "Get original GIF",
    ],
  );
  assert.equal(globalCommands[0].options[0].type, 3);
  assert.equal(globalCommands[0].options[0].required, false);
  assert.equal(globalCommands[0].options[0].choices, undefined);
  assert.equal(globalCommands[0].options[0].autocomplete, undefined);
  assert.equal(
    globalCommands.find(({ name }) => name === "channel-settings")
      .default_member_permissions,
    "16",
  );
  assert.equal(
    globalCommands.find(({ name }) => name === "server-settings")
      .default_member_permissions,
    "32",
  );
  assert.deepEqual(
    globalCommands.find(({ name }) => name === "channel-settings").contexts,
    [0],
  );
  assert.deepEqual(
    globalCommands.find(({ name }) => name === "channel-settings")
      .integration_types,
    [0],
  );
  assert.deepEqual(
    globalCommands.find(({ name }) => name === "server-settings")
      .integration_types,
    [0],
  );
  assert.deepEqual(
    globalCommands.find(({ name }) => name === "Get original GIF"),
    {
      type: 3,
      name: "Get original GIF",
      name_localizations: { ko: "원본 GIF 받기" },
      integration_types: [0],
      contexts: [0],
    },
  );
  for (const { command } of registeredSearchRoutes()) {
    assert.ok(command.argvPrefix.length > 0);
    assert.equal(
      command.registration.options.some(({ name }) => name === "arguments"),
      false,
    );
    assert.equal(
      command.registration.options.some(({ name }) => name === "objective"),
      command.input?.startsWith("build-v2-") === true,
    );
  }
  assert.deepEqual(findVariant("pc", "path").argvPrefix, ["pc", "path"]);
  assert.deepEqual(findVariant("pc", "chance").argvPrefix, ["pc", "chance"]);
  assert.deepEqual(findVariant("pc", "chance").resultAllowlist, ["pc-probability.v2"]);
  assert.equal(findVariant("pc", "chance").resultAuthorityId, "pc-chance");
  assert.deepEqual(findVariant("pc", "score").argvPrefix, ["pc", "score"]);
  assert.deepEqual(findVariant("pc", "score").resultAllowlist, ["pc-score-summary.v2"]);
  assert.equal(findVariant("pc", "score").resultAuthorityId, "pc-score");
  assert.deepEqual(findSlashCommand("score").argvPrefix, ["sfinder", "score"]);
  assert.deepEqual(findSlashCommand("score").resultAllowlist, ["pc-scenario"]);
  assert.equal(findSlashCommand("score").resultAuthorityId, "score");
  for (const name of ["chance", "percent"]) {
    const generic = findSlashCommand(name);
    assert.equal(generic.input, "pc");
    assert.deepEqual(generic.argvPrefix, ["sfinder", name]);
    assert.deepEqual(generic.resultAllowlist, ["pc-scenario"]);
    assert.equal(generic.resultAuthorityId, name);
    assert.equal(generic.compatibilityClassification, "generic-compatibility");
  }
  assert.deepEqual(findVariant("pc", "failed-queue").argvPrefix, ["pc", "failed-queue"]);
  assert.deepEqual(findVariant("build", "cover").argvPrefix, ["build", "cover"]);
  assert.deepEqual(findSlashCommand("cover").argvPrefix, ["build-probability"]);
  assert.deepEqual(findVariant("setup", "joint").argvPrefix, ["setup", "joint"]);
  assert.deepEqual(findVariant("forward", "spin").argvPrefix, ["spin-finder"]);
  assert.deepEqual(
    findVariant("spin-structure", "search").argvPrefix,
    ["spin-structure", "search"],
  );
  assert.deepEqual(
    findVariant("spin-structure", "cover").argvPrefix,
    ["spin-structure", "cover"],
  );
  assert.deepEqual(
    findVariant("spin-structure", "guaranteed").argvPrefix,
    ["spin-structure", "guaranteed"],
  );
  assert.deepEqual(findVariant("finesse", "search").argvPrefix, ["build-probability"]);
  assert.deepEqual(findVariant("finesse", "score").argvPrefix, ["finesse", "score"]);
  assert.deepEqual(
    findSlashCommand("path").registration.options.map(({ name }) => name),
    ["next", "field", "lines", "kicktable", "options"],
  );
  assert.deepEqual(
    findSlashCommand("path").registration.options.find(({ name }) => name === "lines"),
    {
      type: 4,
      name: "lines",
      description: "PC target height 1–6; omit to evaluate every height through 6",
      required: false,
      min_value: 1,
      max_value: 6,
      choices: [1, 2, 3, 4, 5, 6].map((value) => ({
        name: `${value} line`,
        value,
      })),
    },
  );
  assert.deepEqual(
    findSlashCommand("cover").registration.options.map(({ name }) => name),
    ["next", "base", "target", "kicktable", "options"],
  );
  assert.deepEqual(
    findSlashCommand("score-finder").registration.options.map(({ name }) => name),
    ["next", "field", "lines", "kicktable", "options"],
  );
  assert.equal(findSlashCommand("cat-finder"), null);
  assert.deepEqual(
    findSlashCommand("damage").registration.options.map(({ name }) => name),
    ["next", "field", "kicktable", "options"],
  );
  assert.deepEqual(
    findVariant("spin-structure", "search").registration.options.map(({ name }) => name),
    [
      "pieces",
      "field",
      "height",
      "lines",
      "spin-profile",
      "kicktable",
      "fill-bottom",
      "fill-top",
      "max-placements",
      "minimality",
    ],
  );
  assert.deepEqual(
    findVariant("spin-structure", "search").registration.options
      .find(({ name }) => name === "spin-profile")
      .choices.map(({ value }) => value),
    [
      "t-spins",
      "t-spins-plus",
      "all-spin",
      "all-spin-plus",
      "all-mini",
      "all-mini-plus",
    ],
  );
  assert.equal(globalCommands.some(({ name }) => name === "cat-finder"), false);
  assert.equal(findSlashCommand("verify"), null);
  assert.equal(findSlashCommand("objective"), null);
  assert.equal(
    findVariant("build", "cover").registration.options
      .some(({ name }) => name === "objective"),
    true,
  );
  assert.equal(
    findSlashCommand("cover").registration.options
      .some(({ name }) => name === "objective"),
    false,
  );
  assert.equal(
    prepareClearraArguments(["damage", "--board-mask-v1", "0", "--queue", "I"])[0],
    "damage",
  );
  assert.deepEqual(
    findSlashCommand("pc-setup").registration.options.map(({ name }) => name),
    [
      "remaining",
      "priority",
      "max-setup-pieces",
      "queue-knowledge",
      "next-cycle-remaining",
      "setup-length",
      "kicktable",
      "options",
    ],
  );
  assert.match(
    findSlashCommand("score-finder").registration.options
      .find(({ name }) => name === "lines").description,
    /defaults to 4/,
  );
  assert.equal(globalCommands.some(({ name }) => name === "clearra"), false);
  assert.equal(globalCommands.some(({ name }) => name === "view"), false);
});

test("registered command metadata and every help page stay inside Discord limits", () => {
  const validateOption = (option) => {
    assert.match(option.name, /^[-_a-z0-9]{1,32}$/);
    assert.ok(option.description.length >= 1 && option.description.length <= 100);
    assert.ok((option.options?.length ?? 0) <= 25);
    assert.ok((option.choices?.length ?? 0) <= 25);
    for (const choice of option.choices ?? []) {
      assert.ok(choice.name.length >= 1 && choice.name.length <= 100);
      assert.ok(String(choice.value).length >= 1 && String(choice.value).length <= 100);
    }
    for (const child of option.options ?? []) validateOption(child);
  };
  for (const command of globalCommands) {
    if (command.type === 3) {
      assert.ok(command.name.length >= 1 && command.name.length <= 32);
      assert.equal(Object.hasOwn(command, "description"), false);
      assert.equal(Object.hasOwn(command, "options"), false);
      continue;
    }
    assert.match(command.name, /^[-_a-z0-9]{1,32}$/);
    assert.ok(command.description.length >= 1 && command.description.length <= 100);
    assert.ok(command.options.length <= 25);
    for (const option of command.options) validateOption(option);
  }

  assert.ok(formatSlashCommandHelp().length <= 2_000);
  for (const { path, command } of registeredSearchRoutes()) {
    const help = formatSlashCommandHelp(path);
    assert.ok(help.length <= 2_000, `/${path} help exceeds Discord's message limit`);
    assert.equal(help.startsWith(`**/${path}**`), true);
    if (command.modalSchemaId === null) assert.doesNotMatch(help, /guided Modal form/);
    else assert.match(help, /guided Modal form/);
  }
  const englishPcHelp = formatSlashCommandHelp("path", "en");
  const koreanPcHelp = formatSlashCommandHelp("path", "ko");
  assert.match(englishPcHelp, /all 1–6-row targets/);
  assert.match(koreanPcHelp, /1–6줄 전체/);
  assert.doesNotMatch(englishPcHelp, /prefill|starts? (?:at|with) 4|default is 4/i);
  assert.doesNotMatch(koreanPcHelp, /기본(?:값)?(?:은)? 4줄|4줄로 시작/u);
  assert.doesNotMatch(englishPcHelp, /2L\/4L\/6L/);
  assert.doesNotMatch(koreanPcHelp, /2L\/4L\/6L/);

  const englishBuildHelp = formatSlashCommandHelp("build cover", "en");
  const koreanBuildHelp = formatSlashCommandHelp("build cover", "ko");
  assert.match(englishBuildHelp, /base-mask.*target-mask.*height.*1\.\.6/su);
  assert.match(koreanBuildHelp, /base-mask.*target-mask.*height.*1\.\.6/su);
  assert.match(englishBuildHelp, /Plain grids.*rejected/u);
  assert.match(koreanBuildHelp, /일반 격자.*거부/u);

  const englishCommandList = formatSlashCommandHelp("", "en");
  const koreanCommandList = formatSlashCommandHelp("", "ko");
  assert.match(englishCommandList, /every target height from 1 through 6 rows/u);
  assert.match(englishCommandList, /support 1 through 24 rows/u);
  assert.match(koreanCommandList, /1–6줄의 모든 목표 높이를 지원/u);
  assert.match(koreanCommandList, /1–24줄을 지원/u);
  assert.doesNotMatch(englishCommandList, /prefill/i);
  assert.doesNotMatch(koreanCommandList, /기본 4줄|기본 8줄/u);

  const englishObjectiveHelp = formatSlashCommandHelp("objective", "en");
  const koreanObjectiveHelp = formatSlashCommandHelp("objective", "ko");
  assert.match(englishObjectiveHelp, /`all`, `unique`, `min-cover`, `tiling`/);
  assert.match(koreanObjectiveHelp, /`all`, `unique`, `min-cover`, `tiling`/);
  assert.match(englishObjectiveHelp, /PC objectives.*absent from slash options, Modals, and autocomplete/i);
  assert.match(englishObjectiveHelp, /Build v2.*capability-closed slash objective choices/i);
  assert.match(englishObjectiveHelp, /\$path.*--objective <ID>/);
  assert.match(koreanObjectiveHelp, /minimum-cover.*min-cover/u);
  assert.match(formatSlashCommandHelp("objective minimum-cover", "en"), /objective min-cover/);
  assert.match(formatSlashCommandHelp("objective tiling-only", "en"), /Unknown objective/);
  assert.match(formatSlashCommandHelp("objective minimum_cover", "en"), /Unknown objective/);
  for (const help of [
    englishCommandList,
    koreanCommandList,
    englishObjectiveHelp,
    koreanObjectiveHelp,
    ...registeredSearchRoutes().flatMap(({ path }) => [
      formatSlashCommandHelp(path, "en"),
      formatSlashCommandHelp(path, "ko"),
    ]),
  ]) {
    assert.doesNotMatch(help, /verify|검증/iu);
  }

  for (const name of ["render-file", ...registeredSearchRoutes().map(({ path }) => path)]) {
    const koreanHelp = formatSlashCommandHelp(name, "ko");
    assert.ok(koreanHelp.length <= 2_000, `/${name} Korean help exceeds Discord's message limit`);
    assert.doesNotMatch(
      koreanHelp,
      /same-channel|exact IOTSZJL queue|unordered IOTSZJL inventory|<built-in>|<grid\|document|<pattern>|<target>|<delta>/i,
      `/${name} Korean help retains an English syntax placeholder`,
    );
  }
  const koreanScoreFinderHelp = formatSlashCommandHelp("score-finder", "ko");
  assert.match(koreanScoreFinderHelp, /Jstris 점수가 가장 높은 퍼펙트 클리어/u);
  assert.match(koreanScoreFinderHelp, /정확한 IOTSZJL 큐/u);
  assert.match(koreanScoreFinderHelp, /패턴이 아닌 정확한/u);
  assert.match(koreanScoreFinderHelp, /1–6줄 중 원하는 퍼펙트 클리어 목표 높이/u);
  assert.doesNotMatch(koreanScoreFinderHelp, /기본값은 4줄/u);
  assert.match(koreanScoreFinderHelp, /initial-b2b=true\|false/);
  assert.doesNotMatch(koreanScoreFinderHelp, /최대 대미지/u);
  const koreanDamageHelp = formatSlashCommandHelp("damage", "ko");
  assert.match(koreanDamageHelp, /최대 대미지/u);
  assert.match(koreanDamageHelp, /1–24줄/u);
  assert.doesNotMatch(koreanDamageHelp, /initial_b2b|기본값은 4줄/u);
  const englishSpinStructureHelp = formatSlashCommandHelp("spin-structure search", "en");
  const koreanSpinStructureHelp = formatSlashCommandHelp("spin-structure search", "ko");
  assert.match(englishSpinStructureHelp, /unordered IOTSZJL inventory/);
  assert.match(englishSpinStructureHelp, /Regular and Mini results are always reported separately/);
  assert.match(englishSpinStructureHelp, /All-Mini.*All-Spin/);
  assert.match(koreanSpinStructureHelp, /순서 없는 IOTSZJL 미노/u);
  assert.match(koreanSpinStructureHelp, /Regular와 Mini는 항상 따로 출력/u);
  assert.match(koreanSpinStructureHelp, /All-Mini.*All-Spin/u);
  assert.match(formatSlashCommandHelp("cat-finder", "en"), /Unknown Clearra command/);
});

test("bot locale resolution preserves Discord interaction locale behind stored overrides", async () => {
  const preferences = new DiscordLocalePreferences();
  const bot = Object.create(Clearrabot.prototype);
  bot.localePreferences = preferences;
  const interaction = {
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    locale: "ko",
  };

  assert.deepEqual(bot.resolveLocale(interaction), {
    locale: "ko",
    source: "interaction",
  });
  await preferences.setGuild(interaction.guild_id, "en");
  assert.deepEqual(bot.resolveLocale(interaction), {
    locale: "en",
    source: "guild",
  });
});

test("terminal Modal failures preserve an explicit language selection", async () => {
  const messages = [];
  const bot = Object.create(Clearrabot.prototype);
  bot.localePreferences = new DiscordLocalePreferences();
  bot.operationFailureText = (_error, locale) => `failure:${locale}`;
  bot.editInteraction = async (_interaction, message) => messages.push(message);
  const interaction = {
    type: 5,
    locale: "en-US",
    data: {
      custom_id: "clearra:search:v4:pc~tiling",
      components: [
        {
          type: 18,
          component: { type: 4, custom_id: "field", value: "__________" },
        },
        {
          type: 18,
          component: { type: 3, custom_id: "locale", values: ["ko"] },
        },
      ],
    },
  };

  assert.deepEqual(bot.resolveResponseLocale(interaction), {
    locale: "ko",
    source: "explicit",
  });
  await bot.handleInteractionFailure(interaction, new Error("failed"));
  assert.equal(messages[0].payload.content, "failure:ko");
});

test("remote execution inherits the configured Discord output ceiling", () => {
  const bot = new Clearrabot(
    {},
    {
      jobEndpoint: "https://jobs.example.test/jobs",
      jobToken: "job-token",
      searchTimeoutMs: 1_000,
      maxOutputBytes: 9 * 1024 * 1024,
      maxConcurrentSearches: 1,
    },
  );

  assert.equal(bot.executor.maxOutputBytes, 9 * 1024 * 1024);
});

test("help explains one command without starting a search", async () => {
  const messages = [];
  let executions = 0;
  let administratorChecks = 0;
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      botAdministratorAuthority: {
        async allows() {
          administratorChecks += 1;
          return false;
        },
      },
      executor: {
        async execute() {
          executions += 1;
          return { exitCode: 0, stdout: "unexpected", stderr: "" };
        },
      },
    },
  );

  assert.equal(
    await bot.handleInteraction(slashInteraction("help", [
      { name: "arguments", value: "path" },
    ])),
    true,
  );
  assert.equal(executions, 0);
  assert.equal(administratorChecks, 0);
  assert.equal(messages.length, 1);
  assert.match(messages[0].payload.content, /Direct syntax: `\/path next:/);
  assert.match(messages[0].payload.content, /\[options:hold=use\]/);
  assert.doesNotMatch(messages[0].payload.content, /options:\"/);
  assert.match(messages[0].payload.content, /all 1–6-row targets/);
  assert.doesNotMatch(messages[0].payload.content, /2L\/4L\/6L/);
});

test("interaction replies use the signed application ID without metadata lookup", async () => {
  const configuredApplicationId = "1533373054309371924";
  const interactionApplicationIds = [
    configuredApplicationId,
    "2533373054309371924",
  ];

  for (const interactionApplicationId of interactionApplicationIds) {
    const replyApplicationIds = [];
    let applicationRequests = 0;
    const bot = new Clearrabot(
      {
        async application() {
          applicationRequests += 1;
          throw new Error("help must not resolve application metadata");
        },
        async deferInteraction() {},
        async editOriginalInteraction(applicationId) {
          replyApplicationIds.push(applicationId);
        },
      },
      { maxConcurrentSearches: 1 },
      {
        applicationId: configuredApplicationId,
        executor: { async execute() { throw new Error("unexpected search"); } },
      },
    );
    await bot.handleInteraction({
      ...slashInteraction("help", []),
      application_id: interactionApplicationId,
    });

    assert.deepEqual(replyApplicationIds, [interactionApplicationId]);
    assert.equal(applicationRequests, 0);
  }
});

test("management mutations lazily resolve and cache the Discord application owner", async () => {
  const defers = [];
  const messages = [];
  const events = [];
  let applicationRequests = 0;
  const administratorId = "323456789012345678";
  const bot = new Clearrabot(
    {
      async application() {
        events.push("application");
        applicationRequests += 1;
        return {
          id: "1533373054309371924",
          owner: { id: administratorId },
          team: null,
        };
      },
      async deferInteraction(_interaction, options) {
        events.push("defer");
        defers.push(options);
      },
      async editOriginalInteraction(_applicationId, _token, message) {
        events.push("edit");
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: { async execute() { throw new Error("unexpected search"); } },
    },
  );
  const languageOptions = [{
    name: "language-set",
    options: [{ name: "language", value: "ko" }],
  }];

  const denied = {
    ...slashInteraction("server-settings", languageOptions),
    application_id: "1533373054309371924",
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    member: {
      permissions: "0",
      user: { id: "423456789012345678" },
    },
  };
  await bot.handleInteraction(denied);
  assert.match(messages.at(-1).payload.content, /Manage Server permission/);
  assert.equal(applicationRequests, 1);
  assert.deepEqual(events, ["defer", "application", "edit"]);

  const owner = structuredClone(denied);
  owner.id = "owner-interaction";
  owner.token = "owner-token";
  owner.member.user.id = administratorId;
  await bot.handleInteraction(owner);

  assert.deepEqual(defers, [{ ephemeral: true }, { ephemeral: true }]);
  assert.match(messages.at(-1).payload.content, /서버 기본 언어/u);
  assert.equal(applicationRequests, 1);
  assert.deepEqual(
    events,
    ["defer", "application", "edit", "defer", "edit"],
  );
});

test("ordinary work and native manager paths never consult bot authority", async () => {
  const defers = [];
  const messages = [];
  let administratorChecks = 0;
  const bot = new Clearrabot(
    {
      async deferInteraction(_interaction, options) { defers.push(options); },
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      botAdministratorAuthority: {
        async allows() {
          administratorChecks += 1;
          return false;
        },
      },
      executor: { async execute() { throw new Error("unexpected search"); } },
    },
  );

  await bot.handleInteraction({
    ...slashInteraction("channel-settings", [{
      name: "language-show",
      options: [],
    }]),
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    member: {
      permissions: "16",
      user: { id: "423456789012345678" },
    },
  });
  await bot.handleInteraction({
    ...slashInteraction("server-settings", [{
      name: "language-set",
      options: [{ name: "language", value: "ko" }],
    }], undefined, "native-manager-interaction"),
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    member: {
      permissions: "32",
      user: { id: "523456789012345678" },
    },
  });

  assert.equal(administratorChecks, 0);
  assert.deepEqual(defers, [{ ephemeral: true }, { ephemeral: true }]);
  assert.equal(messages.length, 2);
});

test("configured bot administrators mutate settings without an application lookup", async () => {
  const administratorId = "623456789012345678";
  let applicationRequests = 0;
  const messages = [];
  const bot = new Clearrabot(
    {
      async application() {
        applicationRequests += 1;
        throw new Error("configured administrators must not resolve ownership");
      },
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    {
      maxConcurrentSearches: 1,
      discordAdminUserIds: [administratorId],
    },
    { executor: { async execute() { throw new Error("unexpected search"); } } },
  );

  await bot.handleInteraction({
    ...slashInteraction("channel-settings", [{
      name: "language-set",
      options: [{ name: "language", value: "ko" }],
    }]),
    application_id: "1533373054309371924",
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    member: {
      permissions: "0",
      user: { id: administratorId },
    },
  });

  assert.equal(applicationRequests, 0);
  assert.match(messages.at(-1).payload.content, /채널 기본 언어/u);
});

test("field Modal submit reuses the slash parser, interaction locale, and serial automatic PC targets", async () => {
  const calls = [];
  const edits = [];
  const followups = [];
  let defers = 0;
  const bot = new Clearrabot(
    {
      async deferInteraction() { defers += 1; },
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message);
      },
      async createInteractionFollowup(_applicationId, _token, message) {
        followups.push(message);
      },
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 5_000,
      searchTimeoutMs: 1_000,
    },
    {
      executor: {
        async execute(arguments_, options) {
          calls.push({ arguments_, options });
          return { exitCode: 0, stdout: "search result", stderr: "" };
        },
      },
    },
  );

  const interactionCreatedAt = Date.now();
  const interactionId = discordSnowflakeAt(interactionCreatedAt);
  const interaction = {
    id: interactionId,
    token: "modal-token",
    application_id: "application-id",
    locale: "ko",
    type: 5,
    data: {
      custom_id: "clearra:board:v1:path",
      components: [
        {
          type: 18,
          component: {
            type: 4,
            custom_id: "next",
            value: "iotszjliotszjli",
          },
        },
        {
          type: 18,
          component: { type: 4, custom_id: "field", value: ".........." },
        },
      ],
    },
  };
  assert.equal(await bot.handleInteraction(interaction), true);
  assert.equal(defers, 1);
  assert.deepEqual(
    calls.map(({ arguments_ }) => arguments_[arguments_.indexOf("--lines") + 1]),
    ["2", "4", "6"],
  );
  assert.deepEqual(
    calls.map(({ options }) => options.jobId),
    [
      `discord-interaction:${interactionId}:0`,
      `discord-interaction:${interactionId}:1`,
      `discord-interaction:${interactionId}:2`,
    ],
  );
  assert.deepEqual(
    calls.map(({ options }) => options.deadlineUnixMs),
    Array(3).fill(interactionCreatedAt + 1_000),
  );
  assert.equal(edits.length, 1);
  assert.equal(followups.length, 2);
  assert.match(edits[0].payload.content, /자동 PC 목표: 2L/);
  assert.match(followups[1].payload.content, /자동 PC 목표: 6L/);
});

test("one feasible automatic slash PC target stays labeled while explicit 1L does not", async () => {
  const calls = [];
  const edits = [];
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, token, message) {
        edits.push({ token, message });
        return { attachments: [] };
      },
      async createInteractionFollowup() {
        throw new Error("one feasible automatic target must not create a followup");
      },
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 1_000,
      searchTimeoutMs: 1_000,
    },
    {
      executor: {
        async execute(arguments_, options) {
          calls.push({ arguments_, options });
          return { exitCode: 0, stdout: "search result", stderr: "" };
        },
      },
      gifRenderer: resolvedGifRenderer(),
    },
  );
  const input = [
    { name: "field", value: "######____" },
    { name: "next", value: "I" },
  ];

  await bot.handleInteraction(
    slashInteraction("path", input, undefined, "automatic-one-line"),
  );
  const automatic = edits.at(-1).message.payload.content;

  await bot.handleInteraction(
    slashInteraction("path", [
      ...input,
      { name: "lines", value: 1 },
    ], undefined, "explicit-one-line"),
  );
  const explicit = edits.at(-1).message.payload.content;

  assert.deepEqual(
    calls.map(({ arguments_ }) => arguments_[arguments_.indexOf("--lines") + 1]),
    ["1", "1"],
  );
  assert.deepEqual(
    calls.map(({ options }) => Object.hasOwn(options, "jobId")),
    [false, false],
  );
  assert.match(automatic, /Automatic PC target: 1L/);
  assert.doesNotMatch(explicit, /Automatic PC target:/);
});

test("dormant viewer replies carry an internally rendered GIF and Clearra link", async () => {
  const messages = [];
  const rest = {
    deferInteraction: async () => {},
    editOriginalInteraction: async () => {},
    createInteractionFollowup: async (_applicationId, _token, message) =>
      messages.push(message),
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      jobEndpoint: "https://jobs.example.test/jobs",
      viewerBaseUrl: "https://example.test/Clearra/",
      searchTimeoutMs: 1000,
      maxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        execute: async () => ({ exitCode: 0, stdout: "ok", stderr: "" }),
      },
    },
  );
  const source = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["I", "I", "I", "I", ...Array(6).fill(null)],
      },
    ],
  });

  await bot.sendViewerReplies(async (message) => messages.push(message), [
    { format: "ctk3", source, document: decodeViewerDocument(source) },
  ]);

  assert.equal(messages.length, 1);
  assert.equal(messages[0].files.length, 1);
  assert.equal(messages[0].files[0].contentType, "image/gif");
  assert.match(messages[0].payload.content, /tool=ctk/);
  assert.match(messages[0].payload.content, /ctk=/);
});

test("oversized viewer links fall back to an attached canonical CTK3 document", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {},
    {
      viewerBaseUrl: "https://example.test/Clearra/",
      maxGifBytes: 1,
      maxConcurrentSearches: 1,
    },
    { executor: { execute: async () => ({ exitCode: 0, stdout: "", stderr: "" }) } },
  );
  const source = fumenEncoder.encode([
    {
      field: Field.create("TTT_______"),
      comment: Array.from({ length: 700 }, (_, index) => `note-${index}`).join("|"),
    },
  ]);
  const document = decodeViewerDocument(source);

  await bot.sendViewerReplies(async (message) => messages.push(message), [
    { format: "fumen", source, document },
  ]);

  assert.equal(messages.length, 1);
  assert.match(messages[0].payload.content, /2,000-character limit/);
  assert.match(messages[0].payload.content, /tool=ctk/);
  assert.equal(messages[0].payload.content.length <= 2000, true);
  const ctkFile = messages[0].files.find((file) => file.name.endsWith(".ctk3"));
  assert.ok(ctkFile);
  assert.equal(ctkFile.contentType, CTK3_FILE_MIME_TYPE);
  assert.deepEqual(
    decodeCtk3(new TextDecoder().decode(ctkFile.bytes)),
    document,
  );
  assert.match(messages[0].payload.content, /GIF preview could not be rendered/);
  assert.equal(
    messages[0].files.some((file) => file.contentType === "image/gif"),
    false,
  );
});

test("dormant CTK3 attachment reader remains available to a future renderer ingress", async () => {
  const messages = [];
  const document = {
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["T", "T", "T", null, null, null, null, null, null, null],
      },
    ],
  };
  const bytes = encodeCtk3File(document);
  const rest = {
    async deferInteraction() {},
    async editOriginalInteraction() {},
    async createInteractionFollowup(_applicationId, _token, message) {
      messages.push(message);
    },
    async downloadAttachment(url, limit) {
      assert.equal(url, "https://cdn.discordapp.com/attachments/a/b/field.ctk3");
      assert.equal(limit, 1024 * 1024);
      return bytes;
    },
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      viewerBaseUrl: "https://example.test/Clearra/",
      maxGifBytes: 1024 * 1024,
      maxCtk3FileBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    { executor: { execute: async () => ({ exitCode: 0, stdout: "", stderr: "" }) } },
  );

  const documents = await bot.readAttachmentDocuments([
    {
      filename: "field.ctk3",
      content_type: CTK3_FILE_MIME_TYPE,
      size: bytes.byteLength,
      url: "https://cdn.discordapp.com/attachments/a/b/field.ctk3",
    },
  ]);
  await bot.sendViewerReplies(async (message) => messages.push(message), documents);

  assert.equal(messages.length, 1);
  assert.equal(messages[0].files[0].contentType, "image/gif");
  assert.match(messages[0].payload.content, /ctk=/);
});

test("operation-document utilities resolve exactly one bounded CTK3 attachment into the document field", async () => {
  const document = {
    width: 10,
    pages: [{
      height: 0,
      cells: [],
      operation: { piece: "O", rotation: "spawn", x: 1, y: 0 },
      flags: { lock: true },
    }],
  };
  const bytes = encodeCtk3File(document);
  const attachment = {
    id: "operation-file",
    filename: "operations.ctk3",
    content_type: CTK3_FILE_MIME_TYPE,
    size: bytes.byteLength,
    url: "https://cdn.discordapp.com/attachments/a/b/operations.ctk3",
  };
  const bot = new Clearrabot(
    {
      async downloadAttachment(url, limit) {
        assert.equal(url, attachment.url);
        assert.equal(limit, 1024 * 1024);
        return bytes;
      },
    },
    { maxCtk3FileBytes: 1024 * 1024, maxConcurrentSearches: 1 },
    { executor: { execute: async () => ({ exitCode: 0, stdout: "", stderr: "" }) } },
  );
  const command = findVariant("utility", "sequence-dependencies");
  const options = await bot.resolveOperationDocumentAttachmentOptions(
    { data: { resolved: { attachments: { [attachment.id]: attachment } } } },
    command,
    [{ name: "attachment", type: 11, value: attachment.id }],
  );

  assert.deepEqual(options.map(({ name }) => name), ["document"]);
  const decoded = decodeViewerDocument(options[0].value);
  assert.equal(decoded.width, document.width);
  assert.deepEqual(decoded.pages[0].operation, document.pages[0].operation);
  assert.equal(decoded.pages[0].flags.lock, true);
  const sequenceOptions = await bot.resolveOperationDocumentAttachmentOptions(
    { data: { resolved: { attachments: { [attachment.id]: attachment } } } },
    findVariant("utility", "sequence"),
    [{ name: "attachment", type: 11, value: attachment.id }],
  );
  assert.deepEqual(sequenceOptions, options);
  await assert.rejects(
    bot.resolveOperationDocumentAttachmentOptions(
      { data: { resolved: { attachments: { [attachment.id]: attachment } } } },
      command,
      [
        { name: "document", type: 3, value: options[0].value },
        { name: "attachment", type: 11, value: attachment.id },
      ],
    ),
    /exactly one document string or one CTK3 attachment/,
  );
});

test("Oracle starts remote work immediately, then edits a render-first preview with the result", async () => {
  const events = [];
  let releaseExecution;
  const execution = new Promise((resolve) => {
    releaseExecution = resolve;
  });
  const rest = {
    async createChannelMessage(channelId, outgoing) {
      events.push({ kind: "create", channelId, outgoing });
      if (events.filter(({ kind }) => kind === "create").length === 1) {
        return {
          id: "preview-message",
          attachments: [
            {
              id: "preview-attachment",
              filename: "clearra-input-preview.gif",
              description: "Oracle Fumen and CTK3 preview",
            },
          ],
        };
      }
      return { id: "result-message", attachments: [] };
    },
    async editChannelMessage(channelId, messageId, outgoing) {
      events.push({ kind: "edit", channelId, messageId, outgoing });
      return { id: messageId };
    },
  };
  const bot = oracleBot(
    rest,
    {
      async execute(arguments_, options) {
        events.push({ kind: "execute", arguments_, options });
        await execution;
        return { exitCode: 0, stdout: "remote result", stderr: "" };
      },
    },
    { gifRenderer: resolvedGifRenderer() },
  );
  const incoming = oracleMessage(
    "oracle-command",
    "$path --field XXXXXX____ --patterns I --lines 1",
  );

  assert.equal(
    bot.acceptsOracleMessage(incoming, { botUserId: "clearra-bot" }),
    true,
  );
  const handling = bot.handleOracleMessage(incoming);
  await new Promise((resolve) => setImmediate(resolve));

  assert.deepEqual(events.map(({ kind }) => kind), ["execute", "create"]);
  const preview = events[1].outgoing;
  assert.match(preview.payload.content, /search is running/);
  assert.equal(preview.files.length, 1);
  assert.equal(preview.files[0].contentType, "image/gif");
  assert.equal(preview.payload.message_reference.message_id, "oracle-command");
  assert.equal(Object.hasOwn(events[0].options, "jobId"), false);
  assert.deepEqual(events[0].arguments_.slice(0, 4), [
    "sfinder",
    "path",
    "--field-mask-v1",
    "000000000000003f",
  ]);
  releaseExecution();
  assert.equal(await handling, true);
  assert.deepEqual(
    events.map(({ kind }) => kind),
    ["execute", "create", "edit"],
  );
  const settled = events[2];
  assert.equal(settled.channelId, "oracle-channel");
  assert.equal(settled.messageId, "preview-message");
  assert.match(settled.outgoing.payload.content, /remote result/);
  assert.deepEqual(settled.outgoing.payload.attachments, [
    {
      id: "preview-attachment",
      filename: "clearra-input-preview.gif",
    },
  ]);
  assert.deepEqual(settled.outgoing.files, []);
});

test("Oracle text help returns the same help page without starting a search", async () => {
  const events = [];
  const bot = oracleBot(
    {
      async createChannelMessage(channelId, outgoing) {
        events.push({ channelId, outgoing });
        return { id: "help-message", attachments: [] };
      },
    },
    {
      async execute() {
        throw new Error("help must not execute Clearra");
      },
    },
  );

  assert.equal(
    await bot.handleOracleMessage(oracleMessage("text-help", ">help path")),
    true,
  );
  assert.equal(events.length, 1);
  assert.equal(events[0].channelId, "oracle-channel");
  assert.equal(
    events[0].outgoing.payload.message_reference.message_id,
    "text-help",
  );
  assert.match(events[0].outgoing.payload.content, /\*\*\/path\*\*/);
  assert.match(events[0].outgoing.payload.content, /Direct syntax/);
});

test("Oracle text PC auto targets execute serially like slash commands", async () => {
  const lines = [];
  const messages = [];
  const bot = oracleBot(
    {
      async createChannelMessage(channelId, outgoing) {
        messages.push({ channelId, outgoing });
        return { id: `message-${messages.length}`, attachments: [] };
      },
      async editChannelMessage() {
        throw new Error("an immediate test renderer should use one combined send");
      },
    },
    {
      async execute(arguments_) {
        const index = arguments_.indexOf("--lines");
        lines.push(arguments_[index + 1]);
        return {
          exitCode: 0,
          stdout: `result for ${arguments_[index + 1]} lines`,
          stderr: "",
        };
      },
    },
    {
      gifRenderer: {
        async render() {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return new Uint8Array([0x47, 0x49, 0x46]);
        },
        stop() {},
      },
    },
  );

  assert.equal(
    await bot.handleOracleMessage(
      oracleMessage(
        "text-auto-lines",
        "$path --field __________ --patterns IOTSZJLIOTSZJLI",
      ),
    ),
    true,
  );
  assert.deepEqual(lines, ["2", "4", "6"]);
  assert.equal(messages.length, 3);
  assert.ok(messages.every(({ outgoing }) =>
    outgoing.payload.message_reference.message_id === "text-auto-lines"
  ));
  assert.match(messages[0].outgoing.payload.content, /Automatic PC target: 2L/);
  assert.match(messages[1].outgoing.payload.content, /Automatic PC target: 4L/);
  assert.match(messages[2].outgoing.payload.content, /Automatic PC target: 6L/);
});

test("one feasible automatic text PC target stays labeled while explicit 1L does not", async () => {
  const calls = [];
  const messages = [];
  const bot = oracleBot(
    {
      async createChannelMessage(channelId, outgoing) {
        messages.push({ channelId, outgoing });
        return { id: `message-${messages.length}`, attachments: [] };
      },
      async editChannelMessage() {
        throw new Error("an immediate result must use one combined send");
      },
    },
    {
      async execute(arguments_, options) {
        calls.push({ arguments_, options });
        return { exitCode: 0, stdout: "search result", stderr: "" };
      },
    },
    {
      gifRenderer: {
        async render() {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return new Uint8Array([0x47, 0x49, 0x46]);
        },
        stop() {},
      },
    },
  );

  await bot.handleOracleMessage(oracleMessage(
    "automatic-text-one-line",
    "$path --field ######____ --patterns I",
  ));
  await bot.handleOracleMessage(oracleMessage(
    "explicit-text-one-line",
    ">path --field ######____ --patterns I --lines 1",
  ));

  assert.equal(messages.length, 2);
  assert.deepEqual(
    calls.map(({ arguments_ }) => arguments_[arguments_.indexOf("--lines") + 1]),
    ["1", "1"],
  );
  assert.deepEqual(
    calls.map(({ options }) => Object.hasOwn(options, "jobId")),
    [false, false],
  );
  assert.match(messages[0].outgoing.payload.content, /Automatic PC target: 1L/);
  assert.doesNotMatch(messages[1].outgoing.payload.content, /Automatic PC target:/);
});

test("Oracle renders one deferred Cloud Run CTK3 update without another search", async () => {
  const channelId = "123456789012345678";
  const messageId = "234567890123456789";
  const document = {
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["L", "L", "L", null, null, null, null, null, null, null],
      },
    ],
  };
  const bytes = encodeCtk3File(document);
  const messages = [];
  let executions = 0;
  let hydrations = 0;
  const rest = {
    async downloadAttachment(url, maxBytes) {
      assert.equal(url, "https://cdn.discordapp.com/attachments/a/b/result.ctk3");
      assert.equal(maxBytes, 1024 * 1024);
      return bytes;
    },
    async createChannelMessage(channelId, outgoing) {
      messages.push({ channelId, outgoing });
      return { id: "rendered-result", attachments: [] };
    },
  };
  const bot = oracleBot(rest, {
    async execute() {
      executions += 1;
      return { exitCode: 0, stdout: "unexpected", stderr: "" };
    },
  });
  const incoming = {
    ...oracleMessage(messageId, ""),
    channel_id: channelId,
    author: { id: "clearra-bot", bot: true },
    webhook_id: "interaction-webhook",
    attachments: [
      {
        filename: "result.ctk3",
        content_type: CTK3_FILE_MIME_TYPE,
        size: bytes.byteLength,
        url: "https://cdn.discordapp.com/attachments/a/b/result.ctk3",
      },
    ],
  };
  const { author: _author, ...partialUpdate } = incoming;
  const ingress = new OracleMessageIngress(bot, {
    oracleRenderEnabled: true,
    oracleTextEnabled: false,
    oracleMaxConcurrentMessages: 1,
    oracleMaxPendingMessages: 1,
    oracleMaxPendingSelfMessages: 1,
  }, {
    async fetchMessage(candidateChannelId, candidateMessageId) {
      hydrations += 1;
      assert.equal(candidateChannelId, channelId);
      assert.equal(candidateMessageId, messageId);
      return incoming;
    },
  }).setBotUserId("clearra-bot");

  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", {
      ...incoming,
      attachments: [],
    }),
    { accepted: false, reason: "unsupported-message" },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_UPDATE", partialUpdate),
    { accepted: true },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_UPDATE", partialUpdate),
    { accepted: false, reason: "duplicate-message" },
  );
  assert.equal(hydrations, 1);
  assert.equal(executions, 0);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].channelId, channelId);
  assert.match(messages[0].outgoing.payload.content, /rendered the Clearra result/);
  assert.equal(messages[0].outgoing.files[0].contentType, "image/gif");
  assert.equal(
    messages[0].outgoing.payload.message_reference.message_id,
    messageId,
  );
});

test("Oracle does not render an integrated GIF and CTK3 result a second time", () => {
  const bot = oracleBot({}, { async execute() {} });
  assert.equal(
    bot.acceptsOracleMessage(
      {
        ...oracleMessage("integrated-result", "done"),
        author: { id: "clearra-bot", bot: true },
        attachments: [
          {
            filename: "clearra-input-preview.gif",
            content_type: "image/gif",
          },
          {
            filename: "pc-result.ctk3",
            content_type: CTK3_FILE_MIME_TYPE,
          },
        ],
      },
      { botUserId: "clearra-bot" },
    ),
    false,
  );
  assert.equal(
    bot.acceptsOracleMessage(
      {
        ...oracleMessage(
          "integrated-series-result",
          "Automatic PC target: 4L\nClearra pc-scenario completed.",
        ),
        author: { id: "clearra-bot", bot: true },
        webhook_id: "interaction-webhook",
        attachments: [
          {
            filename: "pc-scenario-result.ctk3",
            content_type: CTK3_FILE_MIME_TYPE,
          },
        ],
      },
      { botUserId: "clearra-bot" },
    ),
    false,
  );
  assert.equal(
    bot.acceptsOracleMessage(
      {
        ...oracleMessage("spin-structure-result", "done"),
        author: { id: "clearra-bot", bot: true },
        webhook_id: "interaction-webhook",
        attachments: [{
          filename: "spin-structure-result.ctk3",
          content_type: CTK3_FILE_MIME_TYPE,
        }],
      },
      { botUserId: "clearra-bot" },
    ),
    false,
  );
  assert.equal(
    bot.acceptsOracleMessage(
      {
        ...oracleMessage("user-result-upload", ""),
        attachments: [{
          filename: "spin-structure-result.ctk3",
          content_type: CTK3_FILE_MIME_TYPE,
        }],
      },
      { botUserId: "clearra-bot" },
    ),
    true,
  );
});

test("Oracle sends a search-first failure and later GIF together in one reply", async () => {
  const events = [];
  let releaseRender;
  const render = new Promise((resolve) => {
    releaseRender = () => resolve(new Uint8Array([0x47, 0x49, 0x46]));
  });
  const rest = {
    async createChannelMessage(_channelId, outgoing) {
      events.push({ kind: "create", outgoing });
      return { id: "failure-result" };
    },
    async editChannelMessage(_channelId, messageId, outgoing) {
      events.push({ kind: "edit", messageId, outgoing });
      return { id: messageId };
    },
  };
  const bot = oracleBot(
    rest,
    {
      async execute() {
        events.push({ kind: "execute" });
        throw new Error("remote search failed");
      },
    },
    { gifRenderer: { render: () => render, stop() {} } },
  );
  const fumen = fumenEncoder.encode([{ field: Field.create("IIII______") }]);

  const handling = bot.handleOracleMessage(
      oracleMessage(
        "failed-command",
        `$path --field ${fumen} --patterns I --lines 4`,
      ),
    );
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(events.map(({ kind }) => kind), ["execute"]);

  releaseRender();
  await assert.rejects(handling, /remote search failed/);
  assert.deepEqual(events.map(({ kind }) => kind), ["execute", "create"]);
  assert.match(
    events[1].outgoing.payload.content,
    /^The request could not be completed\. Please try again\.\nOpen in Clearra:/,
  );
  assert.doesNotMatch(events[1].outgoing.payload.content, /remote search failed/i);
  assert.equal(events[1].outgoing.files.length, 1);
  assert.equal(events[1].outgoing.files[0].name, "clearra-input-preview.gif");
  assert.equal(
    events[1].outgoing.payload.message_reference.message_id,
    "failed-command",
  );
});

test("Oracle ignores unknown prefix text before it consumes ingress capacity", () => {
  const bot = oracleBot({}, { async execute() {} });
  assert.equal(
    bot.acceptsOracleMessage(
      oracleMessage("unknown-prefix", "$hello world"),
      { botUserId: "clearra-bot" },
    ),
    false,
  );
});

test("Oracle returns a terminal reply for an invalid CTK3 attachment", async () => {
  const messages = [];
  const bot = oracleBot(
    {
      async downloadAttachment() {
        return new TextEncoder().encode("not-a-ctk3-document");
      },
      async createChannelMessage(channelId, outgoing) {
        messages.push({ channelId, outgoing });
        return { id: "attachment-error" };
      },
    },
    { async execute() { throw new Error("must not execute"); } },
  );
  const message = {
    ...oracleMessage("invalid-attachment", ""),
    attachments: [
      {
        filename: "invalid.ctk3",
        content_type: CTK3_FILE_MIME_TYPE,
        size: 21,
        url: "https://cdn.discordapp.com/attachments/test/invalid.ctk3",
      },
    ],
  };

  assert.equal(await bot.handleOracleMessage(message), true);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].channelId, "oracle-channel");
  assert.match(messages[0].outgoing.payload.content, /invalid or exceeds/);
  assert.equal(
    messages[0].outgoing.payload.message_reference.message_id,
    "invalid-attachment",
  );
});

test("Oracle auto-renders only a standalone 10-column #/_ field without searching", async () => {
  const messages = [];
  let executions = 0;
  let rendered = null;
  const bot = new Clearrabot(
    {
      async createChannelMessage(channelId, outgoing) {
        messages.push({ channelId, outgoing });
        return { id: "1533373999999999999", attachments: [] };
      },
    },
    {
      oracleRenderEnabled: true,
      oracleTextEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      oracleMaxInputChars: 2_000,
      oracleMaxPages: 128,
      oracleMaxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        async execute() {
          executions += 1;
          throw new Error("standalone fields must not run a search");
        },
      },
      gifRenderer: {
        async render(document) {
          rendered = document;
          return Uint8Array.from([0x47, 0x49, 0x46]);
        },
        stop() {},
      },
    },
  );
  const message = {
    id: "1533373000000000001",
    channel_id: "1533373000000000002",
    guild_id: "1533373000000000003",
    content: "__________\n####______",
    author: { id: "1533373000000000004", bot: false },
    attachments: [],
  };

  assert.equal(bot.acceptsOracleMessage(message), true);
  assert.equal(await bot.handleOracleMessage(message), true);
  assert.equal(executions, 0);
  assert.deepEqual(rendered.pages[0].cells, [
    "G", "G", "G", "G", null, null, null, null, null, null,
    ...Array(10).fill(null),
  ]);
  assert.equal(messages[0].outgoing.files[0].name, "clearra-input-preview.gif");
  assert.equal(
    messages[0].outgoing.payload.message_reference.message_id,
    message.id,
  );
  assert.equal(bot.acceptsOracleMessage({ ...message, id: "bad", content: "text ####______" }), false);
});

test("legacy clearra and view slash aliases are inactive", async () => {
  let calls = 0;
  const bot = new Clearrabot(
    {
      async deferInteraction() { calls += 1; },
      async editOriginalInteraction() { calls += 1; },
    },
    {
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        async execute() { calls += 1; },
      },
    },
  );

  assert.equal(await bot.handleInteraction(slashInteraction("clearra", [])), false);
  assert.equal(await bot.handleInteraction(slashInteraction("view", [])), false);
  assert.equal(calls, 0);
});

test("every registered slash command reaches its fixed engine argv prefix", async () => {
  const invocations = [];
  const edits = [];
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute(arguments_, options) {
          invocations.push({ arguments_, options });
          return { exitCode: 0, stdout: "ok", stderr: "" };
        },
      },
    },
  );

  const field = fumenEncoder.encode([{ field: Field.create() }]);
  const coloredDocument = fumenEncoder.encode([{
    field: Field.create("IIII______"),
  }]);
  const finesseDocument = fumenEncoder.encode([{
    field: Field.create(),
    operation: { type: "I", rotation: "spawn", x: 4, y: 0 },
  }]);
  const routes = registeredSearchRoutes();
  for (const route of routes) {
    assert.equal(
      await bot.handleInteraction(
        searchRouteInteraction(
          route,
          validSearchOptions(route.command, field, finesseDocument, coloredDocument),
        ),
      ),
      true,
    );
  }

  assert.equal(edits.length, routes.length);
  for (let index = 0; index < routes.length; index += 1) {
    const command = routes[index].command;
    assert.deepEqual(
      invocations[index].arguments_.slice(0, command.argvPrefix.length),
      command.argvPrefix,
    );
    if (command.capabilityId?.startsWith("pc.allspin-")) {
      assert.equal(invocations[index].options.timeoutClass, "pc_reverse");
    }
  }
});

test("bot execution keeps typed pc chance separate from generic chance and percent", async () => {
  const invocations = [];
  const edits = [];
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute(arguments_) {
          invocations.push(arguments_);
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: arguments_[0] === "pc"
                ? "pc-probability.v2"
                : "pc-scenario",
              summary: {},
            }),
          };
        },
      },
    },
  );
  const common = [
    { name: "field", value: "grid:######____" },
    { name: "next", value: "I" },
    { name: "lines", value: 1 },
    { name: "kicktable", value: "srs" },
  ];

  await bot.handleInteraction(slashInteraction("pc", [{
    type: 1,
    name: "chance",
    options: [...common, { name: "hold", value: "empty" }],
  }]));
  for (const name of ["chance", "percent"]) {
    await bot.handleInteraction(slashInteraction(name, common));
  }

  assert.deepEqual(invocations, [
    [
      "pc", "chance", "--lines", "1", "--board-mask", "0x3f",
      "--height", "1", "--pieces", "1", "--queue", "I",
      "--hold", "empty", "--rule", "srs", "--format", "json",
      "--include-solution-data",
    ],
    [
      "sfinder", "chance", "--field-mask-v1", "000000000000003f",
      "--queue", "I", "--lines", "1", "--rule", "srs",
      "--format", "json", "--include-solution-data",
    ],
    [
      "sfinder", "percent", "--field-mask-v1", "000000000000003f",
      "--queue", "I", "--lines", "1", "--rule", "srs",
      "--format", "json", "--include-solution-data",
    ],
  ]);
  assert.equal(invocations[0].includes("--objective"), false);
  assert.equal(edits.length, 3);
  assert.equal(edits.every((message) => !/inconsistent result/i.test(message)), true);
});

test("Discord sequence dependencies executes the exact argv and returns only the small canonical report", async () => {
  const invocations = [];
  const edits = [];
  const source = fumenEncoder.encode([{
    field: Field.create(),
    operation: { type: "O", rotation: "spawn", x: 1, y: 0 },
  }]);
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute(arguments_) {
          invocations.push(arguments_);
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "sequence-dependencies",
              summary: {
                candidate_id: "candidate-000000",
                operation_count: 1,
                exact_order_count: "1",
                universal_dependency_count: 0,
                transitive_reduction_count: 0,
                independent_pair_count: 0,
                representative_order: "0",
                reachability_evidence: "must-not-enter-small-discord-report",
              },
            }),
          };
        },
      },
    },
  );

  await bot.handleInteraction(slashInteraction("utility", [{
    type: 1,
    name: "sequence-dependencies",
    options: [
      { name: "document", value: source },
      { name: "rule-profile", value: "srs-x" },
      { name: "kick-profile", value: "no-kick" },
      { name: "timeout-seconds", value: 17 },
    ],
  }]));

  assert.deepEqual(invocations, [[
    "utility", "sequence-dependencies",
    "--document", source,
    "--rule-profile", "srs-x",
    "--kick-profile", "no-kick",
    "--timeout-seconds", "17",
    "--format", "json",
  ]]);
  assert.equal(edits.length, 1);
  for (const expected of [
    "candidate-000000", "Exact accepted orders: 1", "Canonical representative order: 0",
  ]) {
    assert.match(edits[0], new RegExp(expected));
  }
  assert.doesNotMatch(edits[0], /must-not-enter-small-discord-report|reachability_evidence/);
  assert.doesNotMatch(edits[0], /CTK3 pages/);
});

test("Discord sequence executes the exact trace argv and returns only the bounded canonical report", async () => {
  const invocations = [];
  const edits = [];
  const source = fumenEncoder.encode([{
    field: Field.create(),
    operation: { type: "O", rotation: "spawn", x: 1, y: 0 },
  }]);
  const tracePrefix = "0:O:0:1:0;".repeat(30);
  const traceTail = "must-not-enter-bounded-trace-preview";
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute(arguments_) {
          invocations.push(arguments_);
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "sequence",
              summary: {
                operation_count: 1,
                cleared_line_count: 0,
                trace_key: "4a910fab1f82e125",
                rule_profile: "srs-x",
                kick_profile: "no-kick",
                normalized_trace: `${tracePrefix}${traceTail}`,
                replay_evidence: "must-not-enter-small-discord-report",
              },
            }),
          };
        },
      },
    },
  );

  await bot.handleInteraction(slashInteraction("utility", [{
    type: 1,
    name: "sequence",
    options: [
      { name: "document", value: source },
      { name: "rule-profile", value: "srs-x" },
      { name: "kick-profile", value: "no-kick" },
      { name: "timeout-seconds", value: 17 },
    ],
  }]));

  assert.deepEqual(invocations, [[
    "utility", "sequence",
    "--document", source,
    "--rule-profile", "srs-x",
    "--kick-profile", "no-kick",
    "--timeout-seconds", "17",
    "--format", "json",
  ]]);
  assert.equal(edits.length, 1);
  for (const expected of [
    "Operations: 1",
    "Cleared lines: 0",
    "Canonical trace key: 4a910fab1f82e125",
    "Rule profile: srs-x",
    "Kick profile: no-kick",
    "Canonical trace preview:",
  ]) {
    assert.match(edits[0], new RegExp(expected));
  }
  assert.match(edits[0], /…/u);
  assert.doesNotMatch(edits[0], new RegExp(traceTail));
  assert.doesNotMatch(edits[0], /must-not-enter-small-discord-report|replay_evidence/);
  assert.doesNotMatch(edits[0], /CTK3 pages/);
  assert.ok(edits[0].length < 600, `bounded sequence report grew to ${edits[0].length} chars`);
});

test("bot execution keeps typed pc score separate from the generic score preset", async () => {
  const invocations = [];
  const edits = [];
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute(arguments_) {
          invocations.push(arguments_);
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify(
              arguments_[0] === "pc"
                ? validPcScoreStructured()
                : { kind: "pc-scenario", summary: {} },
            ),
          };
        },
      },
    },
  );
  const common = [
    { name: "field", value: "grid:######____" },
    { name: "next", value: "I" },
    { name: "lines", value: 1 },
    { name: "kicktable", value: "srs" },
  ];

  await bot.handleInteraction(slashInteraction("pc", [{
    type: 1,
    name: "score",
    options: [
      ...common,
      { name: "hold", value: "empty" },
      { name: "score-profile", value: "guideline" },
      { name: "spin-profile", value: "all-mini-plus" },
      { name: "initial-b2b", value: 2 },
    ],
  }]));
  await bot.handleInteraction(slashInteraction("score", common));

  assert.deepEqual(invocations[0], [
    "pc", "score", "--lines", "1", "--board-mask", "0x3f",
    "--height", "1", "--pieces", "1", "--queue", "I",
    "--hold", "empty", "--score-profile", "guideline",
    "--spin-profile", "all-mini-plus", "--initial-b2b", "2",
    "--rule", "srs", "--format", "json", "--include-solution-data",
  ]);
  assert.deepEqual(invocations[1].slice(0, 2), ["sfinder", "score"]);
  assert.equal(invocations[0].includes("--objective"), false);
  assert.equal(invocations[0].includes("--score"), false);
  assert.match(edits[0], /Score accuracy: basic-approximation/);
  assert.match(edits[0], /Profile-specific exact: No/);
  assert.equal(edits.every((message) => !/inconsistent result/i.test(message)), true);
});

test("pc score rejects cross-contract and semantically incomplete typed results", async () => {
  const messages = [];
  let structured = validPcScoreStructured();
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify(structured),
          };
        },
      },
    },
  );
  const typed = findSearchRouteByPublicKind("pc-score");
  const legacy = findSearchRouteByPublicKind("score");
  const run = async (route, publicKind) => {
    await bot.runInteractionCommand(
      searchRouteInteraction(route, []),
      route.command.argvPrefix,
      null,
      "en",
      publicKind,
    );
    return messages.at(-1);
  };

  assert.doesNotMatch(await run(typed, "pc-score"), /inconsistent result/i);
  structured = { kind: "pc-scenario", summary: {} };
  assert.match(await run(typed, "pc-score"), /inconsistent result/i);
  structured = validPcScoreStructured();
  assert.match(await run(legacy, "score"), /inconsistent result/i);

  const malformed = [
    (value) => { delete value.contract.command.kind; },
    (value) => { delete value.summary.score_accuracy_level; },
    (value) => { value.summary.score_accuracy_level = "profile-exact"; },
    (value) => { value.summary.score_profile_specific_exact = true; },
    (value) => { value.summary.score_summary_complete = false; },
    (value) => { delete value.summary.resource_probability_complete; },
    (value) => { delete value.resource_report; },
    (value) => { value.resource_report.truncated = true; },
    (value) => { value.contract.pc.scoring.score_accuracy_level = "profile-exact"; },
    (value) => { delete value.contract.pc.execution_report.scoring; },
    (value) => { value.summary.resource_truncated = true; },
    (value) => { value.summary.solution_probabilities_requested = true; },
    (value) => { value.summary.problem_owner = "private"; },
    (value) => { value.summary.transient_worker_owner = "private"; },
  ];
  for (const mutate of malformed) {
    structured = validPcScoreStructured();
    mutate(structured);
    assert.match(await run(typed, "pc-score"), /inconsistent result/i);
    assert.doesNotMatch(messages.at(-1), /private|transient|profile-exact/i);
  }
});

test("compute slash output combines one input GIF without a separate followup", async () => {
  const edits = [];
  const followups = [];
  const jobOptions = [];
  const rest = {
    deferInteraction: async () => {},
    editOriginalInteraction: async (_applicationId, _token, message) =>
      edits.push(message),
    createInteractionFollowup: async (_applicationId, _token, message) =>
      followups.push(message),
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      jobEndpoint: "https://jobs.example.test/jobs",
      viewerBaseUrl: "https://example.test/Clearra/",
      searchTimeoutMs: 1000,
      maxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        execute: async (_arguments, options) => {
          jobOptions.push(options);
          return { exitCode: 0, stdout: "search result", stderr: "" };
        },
      },
    },
  );

  const source = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["O", "O", null, null, null, null, null, null, null, null],
      },
    ],
  });
  await bot.handleInteraction(slashInteraction("path", [
    { name: "field", value: source },
    { name: "next", value: "*p2" },
    { name: "lines", value: 2 },
    { name: "options", value: "hold=avoid" },
  ]));
  assert.equal(edits.length, 1);
  assert.equal(followups.length, 0);
  assert.equal(edits[0].files.length, 1);
  assert.equal(edits[0].files[0].name, "clearra-input-preview.gif");
  assert.equal(edits[0].files[0].contentType, "image/gif");
  assert.match(edits[0].payload.content, /search result/);
  assert.equal(jobOptions.length, 1);
  assert.equal(Object.hasOwn(jobOptions[0], "jobId"), false);
});

test("render-first slash edits the deferred original, then retains its GIF in the final edit", async () => {
  const edits = [];
  let releaseExecution;
  const execution = new Promise((resolve) => {
    releaseExecution = resolve;
  });
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message);
        return edits.length === 1
          ? {
              attachments: [{
                id: "preview-id",
                filename: "clearra-input-preview.gif",
              }],
            }
          : { attachments: [] };
      },
    },
    {
      viewerBaseUrl: "https://example.test/Clearra/",
      interactionDeadlineMs: 1_000,
      maxConcurrentSearches: 1,
      maxCtk3FileBytes: 1024 * 1024,
    },
    {
      gifRenderer: resolvedGifRenderer(),
      executor: {
        async execute() {
          await execution;
          return { exitCode: 0, stdout: "final result", stderr: "" };
        },
      },
    },
  );

  const handling = bot.handleInteraction(slashInteraction("path", [
    { name: "field", value: "....XXXXXX" },
    { name: "next", value: "I" },
    { name: "lines", value: 1 },
  ]));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(edits.length, 1);
  assert.equal(edits[0].files[0].name, "clearra-input-preview.gif");

  releaseExecution();
  assert.equal(await handling, true);
  assert.equal(edits.length, 2);
  assert.match(edits[1].payload.content, /final result/);
  assert.deepEqual(edits[1].payload.attachments, [{
    id: "preview-id",
    filename: "clearra-input-preview.gif",
  }]);
  assert.deepEqual(edits[1].files, []);
});

test("search solutions are returned as one color-preserving CTK3 attachment", async () => {
  const edits = [];
  let executedArguments;
  const rest = {
    deferInteraction: async () => {},
    editOriginalInteraction: async (_applicationId, _token, message) =>
      edits.push(message),
  };
  const key =
    "ctk1|initial=0000000000000003|placements=T:000000000000003c,I:0000000000003c00";
  const bot = new Clearrabot(
    rest,
    {
      searchTimeoutMs: 1000,
      maxConcurrentSearches: 1,
      maxCtk3FileBytes: 1024 * 1024,
    },
    {
      executor: {
        execute: async (arguments_) => {
          executedArguments = arguments_;
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              schema_version: 2,
              kind: "pc-scenario",
              summary: {
                count_complete: true,
                coverage_probability: 0.625,
                normalized_unique_solution_count: 1,
              },
              contract: {
                artifacts: {
                  schema_version: "clearra.solution-data.v1",
                  solution_keys: [key],
                },
              },
            }),
          };
        },
      },
    },
  );
  const source = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: Array(10).fill(null) }],
  });

  await bot.handleInteraction(slashInteraction("path", [
    { name: "field", value: source },
    { name: "next", value: "II" },
    { name: "lines", value: 2 },
  ]));

  assert.deepEqual(
    executedArguments.slice(-3),
    ["--format", "json", "--include-solution-data"],
  );
  assert.equal(edits.length, 1);
  assert.equal(edits[0].files.length, 2);
  const ctk3File = edits[0].files.find(
    (file) => file.contentType === CTK3_FILE_MIME_TYPE,
  );
  assert.ok(ctk3File);
  assert.equal(ctk3File.name, "path-result.ctk3");
  assert.match(edits[0].payload.content, /Coverage probability: 62\.5%/);
  const document = decodeCtk3(new TextDecoder().decode(ctk3File.bytes));
  assert.deepEqual(document.pages[0].cells.slice(0, 6), [
    "G", "G", "T", "T", "T", "T",
  ]);
  assert.deepEqual(document.pages[0].cells.slice(10, 14), ["I", "I", "I", "I"]);
});

test("score-finder projects the internal PC scenario kind onto its Discord artifact", async () => {
  const messages = [];
  const key =
    "ctk1|initial=0000000000000003|placements=T:000000000000003c,I:0000000000003c00";
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1, maxCtk3FileBytes: 1024 * 1024 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              schema_version: 2,
              kind: "pc-scenario",
              summary: { normalized_unique_solution_count: 1 },
              contract: {
                artifacts: {
                  schema_version: "clearra.solution-data.v1",
                  solution_keys: [key],
                },
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("score-finder", []),
    ["sfinder", "score-finder"],
  );

  assert.equal(messages.length, 1);
  assert.match(messages[0].payload.content, /Clearra Jstris-score perfect-clear search completed\./);
  assert.doesNotMatch(messages[0].payload.content, /pc-scenario/);
  assert.equal(messages[0].files[0].name, "score-finder-result.ctk3");
});

test("zero-solution search output omits a stale initial-only CTK3 attachment", async () => {
  const messages = [];
  const initialOnlyKey =
    "ctk1|initial=000000000000003f|placements=";
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    {
      searchTimeoutMs: 1_000,
      maxConcurrentSearches: 1,
      maxCtk3FileBytes: 1024 * 1024,
    },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              schema_version: 2,
              kind: "pc-scenario",
              summary: {
                count_complete: true,
                total_solution_count: 0,
                unique_solution_count: 0,
                normalized_unique_solution_count: 0,
              },
              contract: {
                artifacts: {
                  schema_version: "clearra.solution-data.v1",
                  solution_keys: [initialOnlyKey],
                },
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("path", []),
    ["sfinder", "path", "--lines", "2"],
  );

  assert.equal(messages.length, 1);
  assert.deepEqual(messages[0].files, []);
  assert.match(messages[0].payload.content, /Clearra path search completed\./);
  assert.match(messages[0].payload.content, /Solutions: 0/);
  assert.doesNotMatch(
    messages[0].payload.content,
    /(?:file|attachment).*(?:omit|skip|creat)|(?:omit|skip|creat).*(?:file|attachment)/i,
  );
});

test("Korean canonical and compatibility Build routes use the proven cover result name", async () => {
  const messages = [];
  let executionCount = 0;
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          executionCount += 1;
          if (executionCount === 1) {
            const resultContract = "build-coverage-portfolio.v2";
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify({
                kind: resultContract,
                contract: { command: { kind: resultContract } },
                summary: {
                  capability_id: "build.cover",
                  result_contract: resultContract,
                  payload_kind: "portfolio",
                  input_identity_sha256: "a".repeat(64),
                  objective: "min-cover",
                  coverage_probability: 0.5,
                  source_candidate_count: "1",
                  reachable_candidate_count: "1",
                  pattern_count: "1",
                  candidates: [{
                    candidate_key: "candidate-a",
                    covered_pattern_count: "1",
                  }],
                  canonical_candidate_keys: ["candidate-a"],
                  winners: [],
                  completeness: {
                    enumeration_complete: true,
                    reachability_complete: true,
                    probability_complete: true,
                    portfolio_complete: true,
                    exact: true,
                  },
                },
              }),
            };
          }
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "build-probability",
              summary: { coverage_probability: 0.5 },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("build", [{ type: 1, name: "cover", options: [] }], undefined, "build-kind-one"),
    ["build-probability"],
    null,
    "ko",
    "build-cover-v2",
  );
  await bot.runInteractionCommand(
    slashInteraction("cover", [], undefined, "build-kind-two"),
    ["build-probability"],
    null,
    "ko",
    "cover",
  );

  assert.equal(messages.length, 2);
  for (const message of messages) {
    assert.match(message.payload.content, /Clearra 구축 커버리지 탐색/u);
    assert.match(message.payload.content, /커버 확률: 50%/u);
  }
});

test("Discord finesse summaries expose only typed user-level costs", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "build-probability",
              summary: {
                coverage_probability: 0.75,
                workers_used: 8,
                backend: "private-backend",
                server: "private-server",
              },
              finesse_report: {
                mode: "search",
                metric: "inputs",
                pattern_knowledge: "both",
                complete: true,
                exact_total_inputs: "7",
                representative_witness: {
                  policy: "oracle",
                  solution_key: "PRIVATE-SOLUTION",
                  pattern_ids: [999],
                  queue: ["T", "I"],
                  total_inputs: 7,
                  input_sequence: [
                    "hold",
                    "tap-left",
                    "rotate-clockwise",
                    "soft-drop",
                    "das-right",
                    "rotate-180",
                    "hard-drop",
                  ],
                  placements: [{ piece: "T", rotation: 2, x: 4, y: 0 }],
                  backend: "private-witness-backend",
                },
                policy_results: [
                  {
                    policy: "oracle",
                    overall_average_inputs: "8.25",
                    complete: true,
                    solution_averages: [{ private_queue: "PRIVATE", inputs: 8 }],
                  },
                  {
                    policy: "visible-7",
                    overall_average_inputs: "9.5",
                    oracle_on_covered_average_inputs: "8.5",
                    information_penalty_inputs: "1",
                    success_probability_gap: "0.125",
                    complete: true,
                  },
                ],
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("finesse", []),
    ["finesse", "search"],
  );
  const content = messages[0].payload.content;
  assert.match(content, /Clearra finesse search completed/);
  assert.match(content, /Minimum total: 7 inputs/);
  assert.match(content, /Full-queue average: 8\.25 inputs/);
  assert.match(content, /Visible-7 average: 9\.5 inputs/);
  assert.match(content, /Information cost: 1 input/);
  assert.match(content, /Success probability gap: 12\.5%/);
  assert.doesNotMatch(content, /Representative queue|Representative inputs/i);
  assert.doesNotMatch(content, /PRIVATE|999|worker|backend|server/i);

  await bot.runInteractionCommand(
    slashInteraction("finesse", [], undefined, "finesse-ko"),
    ["finesse", "search"],
    null,
    "ko",
  );
  const korean = messages[1].payload.content;
  assert.doesNotMatch(korean, /대표 큐|대표 입력/u);
  assert.doesNotMatch(korean, /PRIVATE|999|worker|backend|server/i);
});

test("Discord pattern finesse reports averages without exposing a representative route", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "build-probability",
              summary: { coverage_probability: 1 },
              finesse_report: {
                mode: "search",
                metric: "inputs",
                pattern_knowledge: "oracle",
                complete: false,
                exact_total_inputs: null,
                representative_witness: {
                  policy: "oracle",
                  solution_key: "private-solution-key",
                  pattern_ids: [4],
                  queue: ["O"],
                  total_inputs: 1,
                  input_sequence: ["hard-drop"],
                  placements: [{ piece: "O", rotation: 0, x: 4, y: 0 }],
                },
                policy_results: [{
                  policy: "oracle",
                  overall_average_inputs: "1.5",
                  complete: false,
                  successful_probability_mass: "1",
                  successful_unique_queue_count: 2,
                  total_unique_queue_count: 2,
                  solution_averages: [],
                }],
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("finesse", [], undefined, "finesse-pattern"),
    ["finesse", "search"],
  );
  const content = messages[0].payload.content;
  assert.match(content, /Full-queue average: 1\.5 inputs/);
  assert.doesNotMatch(content, /Minimum total|Representative queue|Representative inputs|private-solution|pattern_ids/i);
});

test("spin-structure structured results keep their neutral localized result name", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "spin-structure-family.v2",
              contract: { command: { kind: "spin-structure-family.v2" } },
              summary: {
                capability_id: "spin-structure.search",
                result_contract: "spin-structure-family.v2",
                payload_kind: "spin-structure-family",
                ordering: "candidate-id-ascending",
                regular_count: "1",
                mini_count: "1",
                candidate_count: "2",
                complete: true,
                minimum_placements: "2",
                candidates: [
                  { candidate_id: "1", partition: "regular", placement_count: "2" },
                  { candidate_id: "2", partition: "mini", placement_count: "2" },
                ],
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("spin-structure", [{
      type: 1,
      name: "search",
      options: [],
    }], undefined, "spin-structure-en"),
    ["spin-structure", "search"],
    null,
    "en",
    "spin-structure-search-v2",
  );
  await bot.runInteractionCommand(
    slashInteraction("spin-structure", [{
      type: 1,
      name: "search",
      options: [],
    }], undefined, "spin-structure-ko"),
    ["spin-structure", "search"],
    null,
    "ko",
    "spin-structure-search-v2",
  );

  assert.match(messages[0].payload.content, /Clearra spin-structure search completed\./);
  assert.match(messages[0].payload.content, /Regular structures: 1/);
  assert.match(messages[0].payload.content, /Mini structures: 1/);
  assert.match(messages[0].payload.content, /Minimum placements: 2/);
  assert.doesNotMatch(messages[0].payload.content, /Workers used|private|engine/i);
  assert.deepEqual(messages[0].files, []);
  assert.match(messages[1].payload.content, /Clearra 스핀 구조 탐색/u);
  assert.match(messages[1].payload.content, /Regular 구조: 1/u);
  assert.match(messages[1].payload.content, /Mini 구조: 1/u);
  assert.match(messages[1].payload.content, /최소 배치 수: 2/u);
  assert.doesNotMatch(messages[1].payload.content, /사용 워커 수|private|engine/iu);
  assert.deepEqual(messages[1].files, []);
});

test("score-finder keeps its public result kind when the engine reports pc-scenario", async () => {
  const messages = [];
  const kinds = ["pc-scenario", "damage"];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({ kind: kinds.shift(), summary: {} }),
          };
        },
      },
    },
  );

  for (const [index, name] of ["score-finder", "damage"].entries()) {
    await bot.runInteractionCommand(
      slashInteraction(name, [], undefined, `kind-${index}`),
      findSlashCommand(name).argvPrefix,
      null,
      "ko",
    );
  }

  assert.match(messages[0].payload.content, /Jstris 점수 퍼펙트 클리어 탐색/u);
  assert.doesNotMatch(messages[0].payload.content, /대미지/u);
  assert.match(messages[1].payload.content, /대미지 탐색/u);
});

test("Discord fails closed on an unrecognized structured result kind", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "private-worker-engine",
              summary: { result_count: 0 },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("spin-structure", []),
    ["spin-structure"],
  );

  assert.match(messages[0].payload.content, /inconsistent result/i);
  assert.doesNotMatch(messages[0].payload.content, /private|worker|engine/i);
});

test("score-finder fails closed on an unexpected engine result kind", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({ kind: "damage", summary: {} }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("score-finder", []),
    ["sfinder", "score-finder"],
  );

  assert.match(messages[0].payload.content, /inconsistent result/i);
  assert.doesNotMatch(messages[0].payload.content, /score-finder|damage/);
});

test("every public search result kind has an exact allowed engine kind and EN/KO label", async () => {
  const labels = {
    path: ["path search", "경로 탐색"],
    percent: ["perfect-clear probability", "퍼펙트 클리어 확률 계산"],
    chance: ["perfect-clear probability", "퍼펙트 클리어 확률 계산"],
    minimals: ["minimum-cover perfect-clear search", "최소 커버 퍼펙트 클리어 탐색"],
    score: ["scored perfect-clear search", "점수 퍼펙트 클리어 탐색"],
    saves: ["perfect-clear save groups", "퍼펙트 클리어 세이브 그룹"],
    best_save: ["best perfect-clear save", "최적 퍼펙트 클리어 세이브"],
    score_minimals: ["minimum-cover scored search", "최소 커버 점수 탐색"],
    tiling: ["perfect-clear tiling search", "퍼펙트 클리어 타일링 탐색"],
    failed_queue: ["failed-queue search", "실패 큐 탐색"],
    cover: ["build-coverage search", "구축 커버리지 탐색"],
    spin_cover: ["forward spin search", "정방향 스핀 탐색"],
    spin: ["forward spin search", "정방향 스핀 탐색"],
    score_finder: ["Jstris-score perfect-clear search", "Jstris 점수 퍼펙트 클리어 탐색"],
    damage: ["damage search", "대미지 탐색"],
    ren: ["maximum REN search", "최대 REN 탐색"],
    spin_structure: ["spin-structure search", "스핀 구조 탐색"],
    spin_structure_cover: ["spin-structure coverage", "스핀 구조 커버리지"],
    spin_structure_guaranteed: ["guaranteed spin-structure search", "보장 스핀 구조 탐색"],
    pc_setup: ["joint setup search", "종합 셋업 탐색"],
    best_setup: ["build-first setup search", "구축 우선 셋업 탐색"],
    dpc_finder: ["PC-first setup search", "PC 우선 셋업 탐색"],
    setup_score: ["setup score ranking", "셋업 점수 순위 계산"],
    finesse_search: ["finesse search", "피네스 탐색"],
    finesse_score: ["finesse score", "피네스 계산"],
    allspin_sol: ["B2B-preserving PC witness search", "B2B 보존 PC 증거 탐색"],
    allspin_sol_finder: ["B2B-preserving PC witness search", "B2B 보존 PC 증거 탐색"],
    allspin_pres_chance: ["B2B-preserving PC probability", "B2B 보존 PC 확률 계산"],
    sequence: ["operation trace validation", "operation trace 유효성 확인"],
    sequence_dependencies: ["operation-order dependency analysis", "operation 순서 의존성 분석"],
    parity: ["field-document parity observation", "field-document 패리티 관찰"],
    fumen: ["Fumen document transform", "Fumen 문서 변환"],
    render: ["exact field-document render", "정확한 field-document 렌더"],
    to_gray: ["occupied-color normalization", "점유 색상 회색화"],
    mirror: ["mirror transform", "좌우 반전"],
    setup: ["colored-target build family", "색상 목표 구축 패밀리"],
    congruent: ["colored-target congruence family", "색상 목표 합동 패밀리"],
    congruent_cover: ["congruence coverage portfolio", "합동 커버리지 포트폴리오"],
    setup_cover: ["setup coverage portfolio", "셋업 커버리지 포트폴리오"],
    setup_cover_percent: ["setup coverage probability", "셋업 커버리지 확률"],
    setup_cover_score: ["score-only setup coverage portfolio", "점수 전용 셋업 커버리지 포트폴리오"],
    evaluate_cover: ["supplied-solution coverage family", "제공 해법 커버리지 패밀리"],
    evaluate_minimals: ["supplied-solution minimum portfolio", "제공 해법 최소 포트폴리오"],
    evaluate_score: ["supplied-solution score portfolio", "제공 해법 점수 포트폴리오"],
    evaluate_b2b_cover: ["supplied-solution B2B coverage family", "제공 해법 B2B 커버리지 패밀리"],
    evaluate_cover_percent: ["supplied-solution coverage probability", "제공 해법 커버리지 확률"],
  };
  assert.equal(DISCORD_PUBLIC_SEARCH_CONTRACT.length, 54);
  assert.equal(Object.isFrozen(DISCORD_PUBLIC_SEARCH_CONTRACT), true);
  assert.equal(
    DISCORD_PUBLIC_SEARCH_CONTRACT.every((entry) =>
      Object.isFrozen(entry) && Object.isFrozen(entry.engineKinds)
    ),
    true,
  );
  assert.equal(DISCORD_PUBLIC_SEARCH_CONTRACT.some(({ id }) => id === "verify"), false);
  assert.deepEqual(
    new Set(DISCORD_PUBLIC_SEARCH_CONTRACT.map(({ resultKey }) => resultKey)),
    new Set(Object.keys(labels)),
  );
  assert.deepEqual(
    new Set(DISCORD_PUBLIC_SEARCH_CONTRACT.map(({ id }) => id)),
    new Set(registeredSearchRoutes().map(({ command }) =>
      command.resultAuthorityId ?? command.publicResultKind
    )),
  );
  const messages = [];
  let engineKind = "";
  let finesse = false;
  let resultContractId = null;
  let requestedPublicKind = null;
  let requestedCapabilityId = null;
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          if (engineKind === "pc-score-summary.v2") {
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify(validPcScoreStructured()),
            };
          }
          if (engineKind === "pc-score-portfolio.v2") {
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify(validPcScoreMinimalsStructured()),
            };
          }
          if (engineKind === "pc-save-groups.v2") {
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify(validPcSaveGroupsStructured()),
            };
          }
          if (engineKind === "pc-best-save.v2") {
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify(validPcBestSaveStructured()),
            };
          }
          if (requestedCapabilityId?.startsWith("build.")) {
            return {
              exitCode: 0,
              stderr: "",
              stdout: JSON.stringify(validBuildV2Structured(
                requestedCapabilityId,
                engineKind,
              )),
            };
          }
          const typedDocumentResult = validTypedDocumentUtilityResult(
            engineKind,
            requestedPublicKind,
          );
          if (typedDocumentResult) return typedDocumentResult;
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: engineKind,
              summary: resultContractId
                ? validAllspinSummary(resultContractId)
                : {},
              ...(finesse ? { finesse_report: {} } : {}),
            }),
          };
        },
      },
    },
  );

  for (const {
    id: publicKind,
    engineKinds,
    resultKey,
    resultContractId: contractId,
    capabilityId,
  } of DISCORD_PUBLIC_SEARCH_CONTRACT) {
    resultContractId = contractId?.startsWith("pc-b2b-") ? contractId : null;
    requestedPublicKind = publicKind;
    requestedCapabilityId = capabilityId;
    const [english, korean] = labels[resultKey];
    const route = findSearchRouteByPublicKind(publicKind);
    finesse = publicKind.startsWith("finesse-");
    const argv = executableSearchArguments(route);
    engineKind = resultContractId ? "pc" : engineKinds.at(-1);
    for (const [locale, label] of [["en", english], ["ko", korean]]) {
      const before = messages.length;
      await bot.runInteractionCommand(
        searchRouteInteraction(route, []),
        argv,
        null,
        locale,
        publicKind,
      );
      assert.equal(messages.length, before + 1, `${publicKind}/${locale}`);
      const lines = messages.at(-1).split("\n");
      if (publicKind === "tiling") {
        assert.match(
          lines[0],
          locale === "ko" ? /주의|경고/u : /^WARNING:/u,
          `${publicKind}/${locale}: warning`,
        );
      }
      assert.equal(
        publicKind === "tiling"
          ? lines.find((line) => line.startsWith("Clearra "))
          : lines[0],
        locale === "ko"
          ? `Clearra ${label}을(를) 완료했습니다.`
          : `Clearra ${label} completed.`,
        `${publicKind}/${locale}`,
      );
      if (publicKind === "score-minimals") {
        const rendered = messages.at(-1);
        assert.equal(
          rendered.split("\n").filter((line) =>
            /(?:Canonical candidate ID|정규 후보 ID)/u.test(line)
          ).length,
          1,
        );
        assert.doesNotMatch(rendered, /tie|alternative|cursor|pages?/iu);
      }
    }
  }
});

test("All-Spin results require the requested contract and expose incomplete status safely", async () => {
  const messages = [];
  let structured = null;
  let rawStdout;
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
      async createChannelMessage(_channelId, message) {
        messages.push(message.payload.content);
        return { id: "allspin-result-message" };
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: rawStdout === undefined
              ? JSON.stringify(structured)
              : rawStdout,
          };
        },
      },
    },
  );

  const exact = findSearchRouteByPublicKind("allspin-sol");
  const exactArguments = executableSearchArguments(exact);
  const exactScenarioArguments = executableSearchArguments(exact, { scenario: true });
  structured = {
    kind: "pc-scenario",
    summary: validAllspinSummary("pc-b2b-preserving-witness.v1", {
      pc_allspin_problem_preset: "scenario-pc",
      pc_allspin_initial_field_supplied: true,
      pc_allspin_preserving_queue_count: 1,
      pc_allspin_preservation_probability: 1,
      pc_allspin_witness_required: true,
      pc_allspin_complete: false,
      pc_allspin_incomplete_reason: "preserving-count-incomplete",
      pc_allspin_count_complete: false,
      pc_allspin_preserves_b2b: "not-calculated",
    }),
  };
  await bot.runInteractionCommand(
    searchRouteInteraction(exact, []),
    exactScenarioArguments,
    null,
    "en",
    exact.command.publicResultKind,
  );
  assert.match(messages.at(-1), /B2B-preserving PC witness search completed with a partial result/);
  assert.match(messages.at(-1), /B2B-preserving queues: 1/);
  assert.match(messages.at(-1), /B2B-preservation probability: 100%/);
  assert.match(messages.at(-1), /Preserves B2B: not-calculated/);
  assert.match(messages.at(-1), /Witness available: No/);
  assert.match(messages.at(-1), /Some results may be incomplete/);
  assert.doesNotMatch(messages.at(-1), /preserving-count-incomplete/);

  const chance = findSearchRouteByPublicKind("allspin-pres-chance");
  const chanceArguments = executableSearchArguments(chance);
  const chanceAllMiniArguments = executableSearchArguments(chance, {
    profile: "all-mini-plus",
  });
  structured = {
    kind: "pc",
    summary: validAllspinSummary("pc-b2b-preservation-probability.v1", {
      pc_allspin_spin_profile: "all-mini-plus",
      pc_allspin_preserving_queue_count: "not-calculated",
      pc_allspin_original_queue_count: 24,
      pc_allspin_preservation_probability: "not-calculated",
      pc_allspin_count_complete: false,
      pc_allspin_probability_complete: false,
      pc_allspin_complete: false,
      pc_allspin_incomplete_reason: "preserving-count-incomplete",
    }),
  };
  await bot.runInteractionCommand(
    searchRouteInteraction(chance, []),
    chanceAllMiniArguments,
    null,
    "ko",
    chance.command.publicResultKind,
  );
  assert.match(messages.at(-1), /B2B 보존 PC 확률 계산.*일부 결과/u);
  assert.match(messages.at(-1), /큐 개수 완전성: 아니요/u);
  assert.match(messages.at(-1), /확률 완전성: 아니요/u);
  assert.match(messages.at(-1), /일부 결과가 불완전/u);

  for (const { arguments_, kind, summary } of [
    {
      arguments_: executableSearchArguments(exact, { profile: "t-spins" }),
      kind: "pc",
      summary: validAllspinSummary("pc-b2b-preserving-witness.v1", {
        pc_allspin_spin_profile: "all-mini-plus",
      }),
    },
    {
      arguments_: exactArguments,
      kind: "pc-scenario",
      summary: validAllspinSummary("pc-b2b-preserving-witness.v1"),
    },
    {
      arguments_: exactScenarioArguments,
      kind: "pc",
      summary: validAllspinSummary("pc-b2b-preserving-witness.v1", {
        pc_allspin_problem_preset: "scenario-pc",
        pc_allspin_initial_field_supplied: true,
      }),
    },
    {
      arguments_: exactArguments,
      kind: "pc",
      summary: validAllspinSummary("pc-b2b-preserving-witness.v1", {
        pc_allspin_problem_preset: "scenario-pc",
        pc_allspin_initial_field_supplied: true,
      }),
    },
    {
      arguments_: exactScenarioArguments,
      kind: "pc-scenario",
      summary: validAllspinSummary("pc-b2b-preserving-witness.v1"),
    },
  ]) {
    structured = { kind, summary };
    await bot.runInteractionCommand(
      searchRouteInteraction(exact, []),
      arguments_,
      null,
      "en",
      exact.command.publicResultKind,
    );
    assert.equal(
      messages.at(-1),
      "Clearra returned an inconsistent result. Please retry the command.",
    );
  }

  for (const [publicKind, contractId, engineKind] of [
    ["allspin-sol", "pc-b2b-preservation-probability.v1", "pc"],
    ["allspin-pres-chance", "pc-b2b-preserving-witness.v1", "pc"],
    ["allspin-sol", "pc-b2b-preserving-witness.v1", "pc-scenario"],
  ]) {
    const route = findSearchRouteByPublicKind(publicKind);
    structured = {
      kind: engineKind,
      summary: { pc_allspin_result_contract: contractId },
    };
    await bot.runInteractionCommand(
      searchRouteInteraction(route, []),
      executableSearchArguments(route),
      null,
      "en",
      route.command.publicResultKind,
    );
    assert.equal(
      messages.at(-1),
      "Clearra returned an inconsistent result. Please retry the command.",
    );
  }

  const adversarial = [
    {},
    { pc_allspin_result_contract: "pc-b2b-preserving-witness.v1" },
    validAllspinSummary("pc-b2b-preserving-witness.v1", {
      pc_allspin_preserving_queue_count: 4,
      pc_allspin_original_queue_count: 3,
      pc_allspin_preservation_probability: 2,
      pc_allspin_preserves_b2b: false,
      pc_allspin_witness_available: true,
    }),
    validAllspinSummary("pc-b2b-preserving-witness.v1", {
      pc_allspin_count_complete: "true",
    }),
    validAllspinSummary("pc-b2b-preserving-witness.v1", {
      pc_allspin_unregistered_field: true,
    }),
  ];
  for (const summary of adversarial) {
    structured = { kind: "pc", summary };
    await bot.runInteractionCommand(
      searchRouteInteraction(exact, []),
      exactArguments,
      null,
      "en",
      exact.command.publicResultKind,
    );
    assert.equal(
      messages.at(-1),
      "Clearra returned an inconsistent result. Please retry the command.",
    );
  }

  for (const [preserving, original, probability] of [
    [0, 4, 0.5],
    [4, 4, 0.5],
    [2, 4, 0],
    [2, 4, 1],
  ]) {
    structured = {
      kind: "pc",
      summary: validAllspinSummary("pc-b2b-preservation-probability.v1", {
        pc_allspin_preserving_queue_count: preserving,
        pc_allspin_original_queue_count: original,
        pc_allspin_preservation_probability: probability,
      }),
    };
    await bot.runInteractionCommand(
      searchRouteInteraction(chance, []),
      chanceArguments,
      null,
      "en",
      chance.command.publicResultKind,
    );
    assert.equal(
      messages.at(-1),
      "Clearra returned an inconsistent result. Please retry the command.",
      `${preserving}/${original}/${probability}`,
    );
  }

  for (const [preserving, original, probability] of [
    [0, 4, 0],
    [2, 4, 0.25],
    [4, 4, 1],
  ]) {
    structured = {
      kind: "pc",
      summary: validAllspinSummary("pc-b2b-preservation-probability.v1", {
        pc_allspin_preserving_queue_count: preserving,
        pc_allspin_original_queue_count: original,
        pc_allspin_preservation_probability: probability,
      }),
    };
    await bot.runInteractionCommand(
      searchRouteInteraction(chance, []),
      chanceArguments,
      null,
      "en",
      chance.command.publicResultKind,
    );
    assert.match(messages.at(-1), /B2B-preserving PC probability completed/);
  }

  const exactAlias = findSearchRouteByPublicKind("allspin-sol-finder");
  for (const invalidStdout of ["", "ok", "search result"]) {
    rawStdout = invalidStdout;
    for (const route of [exact, exactAlias, chance]) {
      await bot.runInteractionCommand(
        searchRouteInteraction(route, []),
        executableSearchArguments(route),
        null,
        "en",
        route.command.publicResultKind,
      );
      assert.equal(
        messages.at(-1),
        "Clearra returned an inconsistent result. Please retry the command.",
      );
    }
  }
  rawStdout = "ok";
  await bot.runOracleMessageCommand(
    oracleMessage("allspin-text-malformed", "$allspin_sol_finder IIOOO"),
    exactArguments,
    null,
    "en",
    exact.command.publicResultKind,
  );
  assert.equal(
    messages.at(-1),
    "Clearra returned an inconsistent result. Please retry the command.",
  );
  await bot.runOracleMessageCommand(
    oracleMessage("allspin-invalid-argv", "$allspin_sol_finder IIOOO"),
    ["pc", "allspin-sol", "--queue", "IIOOO"],
    null,
    "en",
    exact.command.publicResultKind,
  );
  assert.equal(
    messages.at(-1),
    "Clearra returned an inconsistent result. Please retry the command.",
  );
  rawStdout = undefined;
});

test("public aliases reject every formerly broad alternate engine kind in EN and KO", async () => {
  const cases = [
    ["failed-queue", "percent"],
    ["failed-queue", "pc-scenario"],
    ["pc-chance", "pc-scenario"],
    ["pc-chance", "percent"],
    ["pc-score", "pc-scenario"],
    ["pc-score", "score"],
    ["score-minimals", "pc-scenario"],
    ["score-minimals", "pc-score-summary.v2"],
    ["tiling", "pc-tiling-family"],
    ["tiling", "pc-scenario"],
    ["tiling", "pc"],
    ["score", "pc-score-summary.v2"],
    ["chance", "pc-probability.v2"],
    ["percent", "pc-probability.v2"],
    ...DISCORD_PUBLIC_SEARCH_CONTRACT
      .filter(({ engineKinds }) => engineKinds.length === 1 && engineKinds[0] === "pc-scenario")
      .flatMap(({ id }) => [[id, "pc"], [id, "percent"]]),
    ...DISCORD_PUBLIC_SEARCH_CONTRACT
      .filter(({ engineKinds }) => engineKinds.length === 1 && engineKinds[0] === "build-probability")
      .filter(({ id }) => !id.startsWith("finesse-"))
      .map(({ id }) => [id, "build-coverage"]),
  ];
  const messages = [];
  let engineKind = "";
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({ kind: engineKind, summary: {} }),
          };
        },
      },
    },
  );

  for (const [publicKind, forbiddenEngine] of cases) {
    engineKind = forbiddenEngine;
    const route = findSearchRouteByPublicKind(publicKind);
    for (const [locale, expected] of [
      ["en", "Clearra returned an inconsistent result. Please retry the command."],
      ["ko", "Clearra 결과가 요청한 명령과 일치하지 않습니다. 명령어를 다시 실행해 주세요."],
    ]) {
      await bot.runInteractionCommand(
        searchRouteInteraction(route, []),
        route.command.argvPrefix,
        null,
        locale,
        publicKind,
      );
      assert.equal(messages.at(-1), expected, `${publicKind}/${forbiddenEngine}/${locale}`);
    }
  }
  assert.equal(messages.length, cases.length * 2);
});

test("structured result kind mismatches return the stable localized consistency error", async () => {
  const messages = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({ kind: "private-engine-kind", summary: {} }),
          };
        },
      },
    },
  );
  for (const [locale, expected] of [
    ["en", "Clearra returned an inconsistent result. Please retry the command."],
    ["ko", "Clearra 결과가 요청한 명령과 일치하지 않습니다. 명령어를 다시 실행해 주세요."],
  ]) {
    const route = findSearchRouteByPublicKind("path");
    await bot.runInteractionCommand(
      searchRouteInteraction(route, []),
      route.command.argvPrefix,
      null,
      locale,
      route.command.publicResultKind,
    );
    assert.equal(messages.at(-1), expected);
  }
});

test("CoverageSummary distinguishes not-calculated solution counts from calculated zero", async () => {
  const messages = [];
  let calculated;
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message.payload.content);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              kind: "pc-scenario",
              summary: {
                solution_count_calculated: calculated,
                total_solution_count: 0,
                unique_solution_count: calculated === true ? 0 : "not-calculated",
                normalized_unique_solution_count: 0,
              },
            }),
          };
        },
      },
    },
  );

  for (const [locale, notCalculated] of [
    ["en", "Solution count: Not calculated"],
    ["ko", "해법 수: 계산하지 않음"],
  ]) {
    for (const legacyOrExplicitFalse of [false, undefined]) {
      calculated = legacyOrExplicitFalse;
      await bot.runInteractionCommand(
        slashInteraction("path", []),
        findSlashCommand("path").argvPrefix,
        null,
        locale,
      );
      assert.match(messages.at(-1), new RegExp(notCalculated));
      assert.doesNotMatch(messages.at(-1), /(?:Solutions|Unique solutions|Normalized solutions|해법): 0/u);
    }

    calculated = true;
    await bot.runInteractionCommand(
      slashInteraction("path", []),
      findSlashCommand("path").argvPrefix,
      null,
      locale,
    );
    assert.doesNotMatch(messages.at(-1), /Not calculated|계산하지 않음/u);
    if (locale === "en") {
      assert.match(messages.at(-1), /Solutions: 0/);
      assert.match(messages.at(-1), /Unique solutions: 0/);
      assert.match(messages.at(-1), /Normalized solutions: 0/);
    } else {
      assert.match(messages.at(-1), /해법: 0/);
      assert.match(messages.at(-1), /고유 해법: 0/);
      assert.match(messages.at(-1), /정규화 해법: 0/);
    }
  }
});

test("CoverageSummary availability fails closed atomically without Discord artifacts in EN and KO", async () => {
  const messages = [];
  let summary;
  const staleKey =
    "ctk1|initial=0000000000000003|placements=T:000000000000003c,I:0000000000003c00";
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
      executor: {
        async execute() {
          return {
            exitCode: 0,
            stderr: "",
            stdout: JSON.stringify({
              schema_version: 2,
              kind: "pc-scenario",
              summary,
              contract: {
                artifacts: {
                  schema_version: "clearra.solution-data.v1",
                  solution_keys: [staleKey],
                },
              },
            }),
          };
        },
      },
    },
  );

  const canonical = () => ({
    search_output_policy: "coverage-summary",
    unique_solution_count: "not-calculated",
    normalized_unique_solution_count: "not-calculated",
    solution_count_calculated: false,
    solution_set_materialized: false,
    solution_keys_materialized_count: 0,
    solution_keys_complete: false,
    solution_page_available: false,
    normalized_solution_set_hash: "not-calculated",
    actual_normalized_solution_set_hash: "not-calculated",
    mirror_normalized_solution_set_hash: "not-calculated",
  });
  const malformedCases = [
    ["canonical", canonical()],
    ["malformed policy", { ...canonical(), search_output_policy: "coverage_summary" }],
    ["inconsistent calculated", { ...canonical(), solution_count_calculated: true }],
    ["inconsistent materialized", { ...canonical(), solution_set_materialized: true }],
    ["inconsistent key count", { ...canonical(), solution_keys_materialized_count: 1 }],
    ["stale normalized count", { ...canonical(), normalized_unique_solution_count: 0 }],
    ["stale normalized hash", { ...canonical(), normalized_solution_set_hash: "cts1:stale" }],
    ["stale mirror hash", { ...canonical(), mirror_normalized_solution_set_hash: "cts1:stale" }],
  ];
  const missingPolicy = canonical();
  delete missingPolicy.search_output_policy;
  malformedCases.push(["missing policy", missingPolicy]);
  const missingPageFlag = canonical();
  delete missingPageFlag.solution_page_available;
  malformedCases.push(["missing page flag", missingPageFlag]);

  for (const [locale, notCalculated] of [
    ["en", "Solution count: Not calculated"],
    ["ko", "해법 수: 계산하지 않음"],
  ]) {
    for (const [label, candidate] of malformedCases) {
      summary = candidate;
      await bot.runInteractionCommand(
        slashInteraction("path", []),
        findSlashCommand("path").argvPrefix,
        null,
        locale,
      );
      const message = messages.at(-1);
      assert.deepEqual(message.files, [], `${locale}/${label}: stale artifact`);
      assert.match(message.payload.content, new RegExp(notCalculated), `${locale}/${label}`);
      assert.doesNotMatch(
        message.payload.content,
        /(?:Solutions|Unique solutions|Normalized solutions|해법): 0/u,
        `${locale}/${label}: fake zero`,
      );
    }
  }
});

test("tiling-only result delivery warns before Discord output", async () => {
  const messages = [];
  const rest = {
    deferInteraction: async () => {},
    editOriginalInteraction: async (_applicationId, _token, message) =>
      messages.push(message),
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      jobEndpoint: "https://jobs.example.test/jobs",
      viewerBaseUrl: "https://example.test/Clearra/",
      searchTimeoutMs: 1000,
      maxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        execute: async () => ({ exitCode: 0, stdout: "tiling result", stderr: "" }),
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("path", []),
    ["pc", "--lines", "2", "--tiling-only"],
  );

  assert.equal(messages.length, 1);
  const content = messages[0].payload.content;
  assert.ok(content.indexOf("WARNING:") < content.indexOf("tiling result"));
  assert.match(content, /cannot be built/);
});

test("command failures and malformed structured output do not expose engine diagnostics", async () => {
  const messages = [];
  const rest = {
    async editOriginalInteraction(_applicationId, _token, message) {
      messages.push(message);
    },
  };
  const results = [
    {
      exitCode: 2,
      stdout: "",
      stderr: "E_CLI_INVALID_VALUE E_WASM_COMMAND_INVALID_VALUE: backend WebGPU worker failed",
    },
    {
      exitCode: 0,
      stdout: '{"schema_version":2,"token":"private"',
      stderr: "",
    },
  ];
  const bot = new Clearrabot(
    rest,
    { maxConcurrentSearches: 1 },
    { executor: { execute: async () => results.shift() } },
  );

  await bot.runInteractionCommand(
    slashInteraction("path", [], undefined, "diagnostic-one"),
    ["sfinder", "path"],
  );
  await bot.runInteractionCommand(
    slashInteraction("path", [], undefined, "diagnostic-two"),
    ["sfinder", "path"],
  );

  assert.equal(messages.length, 2);
  assert.match(messages[0].payload.content, /check the command input/i);
  assert.doesNotMatch(messages[0].payload.content, /E_CLI|E_WASM|backend|WebGPU|worker/i);
  assert.match(messages[1].payload.content, /could not be completed/i);
  assert.doesNotMatch(messages[1].payload.content, /schema_version|token|private/i);
});

test("ordinary Discord messages are disabled at the bot dispatch boundary", async () => {
  let calls = 0;
  const rest = new Proxy({}, {
    get() {
      return async () => {
        calls += 1;
      };
    },
  });
  const bot = new Clearrabot(
    rest,
    { maxConcurrentSearches: 1 },
    { executor: { execute: async () => ({ exitCode: 0, stdout: "", stderr: "" }) } },
  );

  const accepted = await bot.handleDispatch("MESSAGE_CREATE", {
    channel_id: "channel",
    content: "!pc --lines 4",
  });

  assert.equal(accepted, false);
  assert.equal(calls, 0);
});

test("slash search queue is bounded and drains in FIFO order", async () => {
  const executions = [];
  const edits = [];
  let releaseFirst;
  const firstExecution = new Promise((resolve) => {
    releaseFirst = resolve;
  });
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, token, message) {
        edits.push({ token, content: message.payload.content });
      },
    },
    {
      maxConcurrentSearches: 1,
      maxPendingSearches: 1,
      interactionDeadlineMs: 1_000,
    },
    {
      executor: {
        async execute(_arguments, options) {
          executions.push(options);
          if (executions.length === 1) await firstExecution;
          return { exitCode: 0, stdout: "search result", stderr: "" };
        },
      },
    },
  );

  const field = fumenEncoder.encode([{ field: Field.create() }]);
  const options = validSearchOptions(findSlashCommand("path"), field);
  const first = bot.handleInteraction(slashInteraction("path", options, undefined, "one"));
  await new Promise((resolve) => setImmediate(resolve));
  const second = bot.handleInteraction(slashInteraction("path", options, undefined, "two"));
  await new Promise((resolve) => setImmediate(resolve));
  await bot.handleInteraction(slashInteraction("path", options, undefined, "three"));

  assert.equal(executions.length, 1);
  assert.equal(Object.hasOwn(executions[0], "jobId"), false);
  assert.match(
    edits.find(({ token }) => token === "token-three").content,
    /busy/iu,
  );

  releaseFirst();
  await Promise.all([first, second]);
  assert.equal(executions.length, 2);
  assert.equal(executions.every((options) => !Object.hasOwn(options, "jobId")), true);
});

test("queued slash work expires before the Discord interaction deadline", async () => {
  let executionCount = 0;
  let releaseFirst;
  const firstExecution = new Promise((resolve) => {
    releaseFirst = resolve;
  });
  const edits = [];
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, token, message) {
        edits.push({ token, content: message.payload.content });
      },
    },
    {
      maxConcurrentSearches: 1,
      maxPendingSearches: 2,
      interactionDeadlineMs: 20,
    },
    {
      executor: {
        async execute() {
          executionCount += 1;
          if (executionCount === 1) await firstExecution;
          return { exitCode: 0, stdout: "done", stderr: "" };
        },
      },
    },
  );

  const field = fumenEncoder.encode([{ field: Field.create() }]);
  const options = validSearchOptions(findSlashCommand("path"), field);
  const first = bot.handleInteraction(slashInteraction("path", options, undefined, "first"));
  await new Promise((resolve) => setImmediate(resolve));
  await bot.handleInteraction(slashInteraction("path", options, undefined, "queued"));

  assert.equal(executionCount, 1);
  assert.match(
    edits.find(({ token }) => token === "token-queued").content,
    /time limit/iu,
  );
  releaseFirst();
  await first;
});

test("long-running setup interaction publishes one localized progress update before the final result", async () => {
  const edits = [];
  let finishSearch;
  const search = new Promise((resolve) => {
    finishSearch = resolve;
  });
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 1_000,
      searchTimeoutMs: 1_000,
      reverseSearchTimeoutMs: 1_000,
      forwardSearchTimeoutMs: 1_000,
      setupProgressNoticeMs: 5,
    },
    {
      executor: {
        async execute() {
          await search;
          return { exitCode: 0, stdout: "setup complete", stderr: "" };
        },
      },
    },
  );

  const execution = bot.runInteractionCommand(
    slashInteraction("pc-setup", [], undefined, "setup-progress"),
    ["setup-finder", "--remaining", "IOTSZJL"],
    null,
    "ko",
  );
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(edits.length, 1);
  assert.match(edits[0], /셋업 탐색이 아직 진행 중/u);

  finishSearch();
  await execution;
  assert.equal(edits.length, 2);
  assert.doesNotMatch(edits[1], /아직 진행 중/u);
  assert.match(edits[1], /setup complete/u);
});

test("setup progress timer stays silent for fast searches", async () => {
  const edits = [];
  const bot = new Clearrabot(
    {
      async editOriginalInteraction(_applicationId, _token, message) {
        edits.push(message.payload.content);
      },
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 1_000,
      searchTimeoutMs: 1_000,
      reverseSearchTimeoutMs: 1_000,
      forwardSearchTimeoutMs: 1_000,
      setupProgressNoticeMs: 50,
    },
    {
      executor: {
        async execute() {
          return { exitCode: 0, stdout: "fast setup", stderr: "" };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("best-setup", [], undefined, "setup-fast"),
    ["setup-finder", "--remaining", "IOTSZJL"],
  );
  assert.deepEqual(edits, ["```text\nfast setup\n```"]);
});

test("long-running setup text search replaces its English progress reply", async () => {
  const created = [];
  const edited = [];
  let finishSearch;
  const search = new Promise((resolve) => {
    finishSearch = resolve;
  });
  const bot = new Clearrabot(
    {
      async createChannelMessage(channelId, outgoing) {
        created.push({ channelId, outgoing });
        return { id: "setup-progress-message" };
      },
      async editChannelMessage(channelId, messageId, outgoing) {
        edited.push({ channelId, messageId, outgoing });
      },
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 1_000,
      searchTimeoutMs: 1_000,
      reverseSearchTimeoutMs: 1_000,
      forwardSearchTimeoutMs: 1_000,
      setupProgressNoticeMs: 5,
    },
    {
      executor: {
        async execute() {
          await search;
          return { exitCode: 0, stdout: "text setup complete", stderr: "" };
        },
      },
    },
  );

  const execution = bot.runOracleMessageCommand(
    oracleMessage("setup-source", "$pc-setup IOTSZJL"),
    ["setup-finder", "--remaining", "IOTSZJL"],
    null,
    "en",
  );
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(created.length, 1);
  assert.match(created[0].outgoing.payload.content, /still running/iu);

  finishSearch();
  await execution;
  assert.equal(edited.length, 1);
  assert.equal(edited[0].messageId, "setup-progress-message");
  assert.match(edited[0].outgoing.payload.content, /text setup complete/u);
});

test("gateway applies a five-minute reverse deadline and a fourteen-minute Discord delivery cap", async () => {
  const calls = [];
  const createdAt = Date.now();
  const bot = new Clearrabot(
    {
      async editOriginalInteraction() {},
    },
    {
      maxConcurrentSearches: 1,
      interactionDeadlineMs: 14 * 60_000,
      searchTimeoutMs: 3 * 60_000,
      reverseSearchTimeoutMs: 5 * 60_000,
      forwardSearchTimeoutMs: 15 * 60_000,
      setupProgressNoticeMs: 5 * 60_000,
    },
    {
      executor: {
        async execute(arguments_, options) {
          calls.push({ arguments_, options });
          return { exitCode: 0, stdout: "done", stderr: "" };
        },
      },
    },
  );

  for (const [ordinal, arguments_] of [
    [0, ["pc", "--lines", "4"]],
    [1, ["damage", "--queue", "I"]],
    [2, ["setup-finder", "--remaining", "IOTSZJL"]],
    [3, ["finesse", "search"]],
    [4, ["finesse", "score"]],
  ]) {
    await bot.runInteractionCommand(
      slashInteraction(
        ordinal === 0
          ? "path"
          : ordinal === 1
            ? "damage"
            : ordinal === 2
              ? "pc-setup"
              : "finesse",
        [],
        undefined,
        discordSnowflakeAt(createdAt + ordinal),
      ),
      arguments_,
    );
  }

  assert.deepEqual(
    calls.map(({ options }) => options.deadlineUnixMs),
    [
      createdAt + 5 * 60_000,
      createdAt + 1 + 14 * 60_000,
      createdAt + 2 + 14 * 60_000,
      createdAt + 3 + 840_000,
      createdAt + 4 + 840_000,
    ],
  );
});

function slashInteraction(name, options, resolved = undefined, id = "interaction-id") {
  return {
    id,
    token: id === "interaction-id" ? "interaction-token" : `token-${id}`,
    application_id: "application-id",
    type: 2,
    data: {
      type: 1,
      name,
      options,
      ...(resolved ? { resolved } : {}),
    },
  };
}

function findVariant(root, subcommand) {
  const variant = findSlashCommand(root)?.subcommands?.[subcommand] ?? null;
  assert.ok(variant, `missing /${root} ${subcommand}`);
  return variant;
}

function registeredSearchRoutes() {
  return slashCommandCatalog.flatMap((root) => {
    if (root.kind !== "search") return [];
    if (!root.subcommands) {
      return [{ root: root.name, subcommand: null, path: root.name, command: root }];
    }
    return Object.values(root.subcommands).map((command) => ({
      root: root.name,
      subcommand: command.subcommand,
      path: `${root.name} ${command.subcommand}`,
      command,
    }));
  });
}

function findSearchRouteByPublicKind(publicResultKind) {
  const route = registeredSearchRoutes().find(
    ({ command }) =>
      (command.resultAuthorityId ?? command.publicResultKind) === publicResultKind,
  );
  assert.ok(route, `missing public search route '${publicResultKind}'`);
  return route;
}

function executableSearchArguments(route, options = {}) {
  const prefix = route.command.argvPrefix;
  if (prefix[0] !== "pc" || ![
    "allspin-sol",
    "allspin-pres-chance",
  ].includes(prefix[1])) return prefix;
  const scenario = options.scenario === true;
  const profile = options.profile ?? "all-spin-plus";
  const source = prefix[1] === "allspin-sol"
    ? ["--queue", "IIOOO"]
    : ["--patterns", "[IO]!OOO"];
  return [
    ...prefix,
    "--lines", "2",
    ...(scenario
      ? ["--board-mask", "0xf", "--height", "2", "--pieces", "4"]
      : []),
    ...source,
    "--spin-profile", profile,
    "--rule", "srs-plus",
  ];
}

function validPcScoreStructured(overrides = {}) {
  const summary = {
    search_output_policy: "trace",
    coverage_pattern_count: 1,
    materialized_pattern_count: 1,
    total_possible_pattern_count: 1,
    materialized_probability_mass: 1,
    probability_complete: true,
    resource_probability_complete: true,
    count_complete: true,
    count_truncated_reason: "none",
    resource_truncated: false,
    resource_truncation_reason: "none",
    objective_search_complete: true,
    objective_complete: true,
    objective_incomplete_reason: "none",
    postprocess_scoring_requested: true,
    score_objective_mode: "summary",
    score_profile_requested: "tetrio",
    spin_profile_requested: "t-spins",
    score_initial_b2b: 0,
    score_profile: "tetrio-basic",
    score_accuracy_level: "basic-approximation",
    score_accuracy_reason:
      "profile-specific basic score/attack tables with configurable spin detection",
    score_profile_specific_exact: false,
    score_evaluation_complete: true,
    score_matrix_materialized: true,
    score_matrix_complete: true,
    score_matrix_cell_count: 1,
    score_matrix_pattern_count: 1,
    score_matrix_profile_id: "tetrio-basic",
    score_matrix_accuracy_level: "basic-approximation",
    score_matrix_incomplete_reason: "none",
    score_best_complete: true,
    score_summary_complete: true,
    score_summary_incomplete_reason: "none",
    score_all_universe_patterns_covered: true,
    score_pattern_optimal_count: 1,
    score_failed_pc_pattern_count: 0,
    score_failed_pc_pattern_score: 0,
    score_covered_probability: 1,
    score_unconditional_expected_score: 800,
    score_unconditional_expected_attack: 2,
    score_best_score: 800,
    score_best_attack: 2,
    score_covered_pattern_conditional_average_score: 800,
    solution_probabilities_requested: false,
    ...overrides,
  };
  const scoring = {
    score_profile: summary.score_profile,
    score_matrix_profile_id: summary.score_matrix_profile_id,
    score_accuracy_level: summary.score_accuracy_level,
    score_matrix_accuracy_level: summary.score_matrix_accuracy_level,
    score_profile_accuracy_mode: "basic-approximation",
    score_accuracy_reason: summary.score_accuracy_reason,
    score_profile_specific_exact: summary.score_profile_specific_exact,
    score_evaluation_complete: summary.score_evaluation_complete,
    score_matrix_materialized: summary.score_matrix_materialized,
    score_matrix_complete: summary.score_matrix_complete,
    score_best_complete: summary.score_best_complete,
    score_summary_complete: summary.score_summary_complete,
    score_all_universe_patterns_covered: summary.score_all_universe_patterns_covered,
    score_matrix_incomplete_reason: summary.score_matrix_incomplete_reason,
    score_summary_incomplete_reason: summary.score_summary_incomplete_reason,
  };
  return {
    kind: "pc-score-summary.v2",
    contract: {
      command: { kind: "pc-score-summary.v2" },
      pc: {
        scoring: { ...scoring },
        execution_report: { scoring: { ...scoring } },
      },
    },
    resource_report: {
      probability_complete: true,
      count_complete: true,
      truncated: false,
      truncation_reason: null,
      count_truncated_reason: null,
      materialized_probability_mass: 1,
      renormalized: false,
    },
    summary,
  };
}

function validPcScoreMinimalsStructured() {
  return {
    kind: "pc-score-portfolio.v2",
    contract: {
      command: { kind: "pc-score-portfolio.v2" },
    },
    resource_report: {
      probability_complete: true,
      count_complete: true,
      truncated: false,
      truncation_reason: null,
      count_truncated_reason: null,
      renormalized: false,
    },
    summary: {
      score_minimals_contract: "pc-score-portfolio.v2",
      score_minimals_score_equality: "score-only",
      score_minimals_attack_role: "informational-only",
      score_minimals_canonical_selection: "smallest-canonical-candidate-id",
      score_minimals_canonical_candidate_id: "2",
      score_minimals_canonical_solution_key: "pc:solution:02",
      score_best_score: 40_000,
      score_best_attack: 11,
    },
  };
}

function validPcSaveGroup(candidateId = "1") {
  return {
    identity: "hold:-|bag:I",
    successful_pattern_count: 1,
    unconditional_probability: 1 / 7,
    conditional_probability_given_pc: 1,
    canonical_candidate_id: candidateId,
    witnesses: [{ candidate_id: candidateId }],
  };
}

function validPcSaveGroupsStructured() {
  return {
    kind: "pc-save-groups.v2",
    summary: {
      save_contract: "pc-save-groups.v2",
      save_pc_probability: 1 / 7,
      save_groups: [validPcSaveGroup()],
    },
  };
}

function validPcBestSaveStructured() {
  const group = validPcSaveGroup();
  return {
    kind: "pc-best-save.v2",
    summary: {
      best_save_contract: "pc-best-save.v2",
      best_save_schema: "clearra-save-v1",
      best_save_probability_basis: "whole-universe-unconditional",
      best_save_pc_probability: 1 / 7,
      best_save_winners: [{
        weighted_total: 6,
        balanced_jl_count: 0,
        exact_group_probability: group.unconditional_probability,
        group,
      }],
    },
  };
}

function validTypedDocumentUtilityResult(engineKind, publicKind = "fumen") {
  if (engineKind === "parity-report.v1") {
    return {
      exitCode: 0,
      stderr: "",
      stdout: JSON.stringify({
        kind: "parity-report.v1",
        contract_id: "parity-report.v1",
        result_kind: "parity",
        payload_kind: "parity-report-page",
        pages: [{
          document_format: "fumen",
          page_number: 1,
          total_pages: 1,
          coordinate_basis: "bottom-left",
          width: 10,
          height: 0,
          occupied_cell_count: 0,
          checker_black_count: 0,
          checker_white_count: 0,
          checker_delta: 0,
          four_color_counts: [0, 0, 0, 0],
          even_column_count: 0,
          odd_column_count: 0,
          column_parity_delta: 0,
          occupied_area_mod_four: 0,
          pending_garbage_occupied_cell_count: 0,
          feasibility_claim: false,
          pruning_authority: "none",
          page_handle_available: true,
        }],
      }),
    };
  }
  if (["field-document.v1", "field-document-set.v1"].includes(engineKind)) {
    const document = "v115@vhAAgH";
    const transform = ["to-gray", "mirror"].includes(publicKind)
      ? publicKind
      : "fumen";
    const payload = {
      format: "fumen",
      document,
      page_count: 1,
      canonical_sha256: createHash("sha256").update(document).digest("hex"),
      filename: transform === "fumen"
        ? "clearra-fumen-page-0001.txt"
        : `clearra-${transform}-v115.txt`,
    };
    return {
      exitCode: 0,
      stderr: "",
      stdout: JSON.stringify(engineKind === "field-document.v1"
        ? {
            kind: "field-document.v1",
            contract_id: "field-document.v1",
            result_kind: transform,
            payload_kind: "field-document",
            payload,
          }
        : {
            kind: "field-document-set.v1",
            contract_id: "field-document-set.v1",
            result_kind: "fumen",
            payload_kind: "field-document-set",
            payload: {
              document_contract: "field-document.v1",
              documents: [payload],
            },
          }),
    };
  }
  if (engineKind === "render-artifact.v1") {
    const bytes = Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      Buffer.from("bot-render-test"),
    ]);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const filename = "clearra-render-page-0001.png";
    return {
      exitCode: 0,
      stderr: "",
      stdout: JSON.stringify({
        kind: "render-artifact.v1",
        contract_id: "render-artifact.v1",
        result_kind: "render",
        payload_kind: "render-artifact",
        payload: {
          document_format: "fumen",
          artifact_format: "png",
          selected_page_number: 1,
          document_page_count: 1,
          media_type: "image/png",
          filename,
          byte_length: bytes.length,
          sha256,
          render_exact: true,
          skin_id: "clearra-exact-v1",
          product_max_bytes: 4096,
          transport_max_bytes: 4096,
        },
      }),
      artifact: {
        contract: "clearra.discord-render-artifact.v1",
        artifactFormat: "png",
        mediaType: "image/png",
        filename,
        byteLength: bytes.length,
        sha256,
        bytesBase64: bytes.toString("base64"),
        renderExact: true,
      },
    };
  }
  return null;
}

function validAllspinSummary(contractId, overrides = {}) {
  const witness = contractId === "pc-b2b-preserving-witness.v1";
  const summary = {
    pc_allspin_result_contract: contractId,
    pc_allspin_mode: witness
      ? "exact-queue-witness"
      : "pattern-preservation-chance",
    pc_allspin_spin_profile: "all-spin-plus",
    pc_allspin_problem_preset: "opening-pc",
    pc_allspin_initial_field_supplied: false,
    pc_allspin_target_field_supplied: false,
    pc_allspin_clear_contract: "inverse-lock-clear-to-empty",
    pc_allspin_semantics: "clearra-explicit-spin-profile",
    pc_allspin_compatibility: "sfinderbot-command-intent-only",
    pc_allspin_complete: true,
    pc_allspin_incomplete_reason: "none",
    pc_allspin_denominator_semantics: "original-materialized-queue",
    pc_allspin_evaluation_basis: "candidate-pattern-existence",
    pc_allspin_path_multiplicity_counted: false,
    pc_allspin_preserving_queue_count: 0,
    pc_allspin_original_queue_count: 1,
    pc_allspin_preservation_probability: 0,
    pc_allspin_count_complete: true,
    pc_allspin_probability_complete: true,
    ...(witness
      ? {
          pc_allspin_preserves_b2b: false,
          pc_allspin_witness_required: false,
          pc_allspin_witness_available: false,
          pc_allspin_witness_deterministic: false,
          pc_allspin_witness_kind: "none",
          pc_allspin_witness_candidate_key: "not-materialized",
          pc_allspin_witness_pattern_index: "not-materialized",
        }
      : {}),
    ...overrides,
  };
  return summary;
}

function searchRouteInteraction(route, options, resolved = undefined) {
  return slashInteraction(
    route.root,
    route.subcommand
      ? [{ type: 1, name: route.subcommand, options }]
      : options,
    resolved,
  );
}

function discordSnowflakeAt(timestampMs) {
  return ((BigInt(timestampMs) - 1_420_070_400_000n) << 22n).toString();
}

function oracleBot(rest, executor, options = {}) {
  return new Clearrabot(
    rest,
    {
      oracleRenderEnabled: true,
      oracleTextEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      oracleMaxInputChars: 2_000,
      oracleMaxPages: 128,
      oracleMaxCtk3FileBytes: 1024 * 1024,
      oracleMaxGifBytes: 1024 * 1024,
      viewerBaseUrl: "https://example.test/Clearra/",
      maxCtk3FileBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
      maxPendingSearches: 1,
      interactionDeadlineMs: 1_000,
    },
    { executor, ...options },
  );
}

function resolvedGifRenderer() {
  return {
    async render() {
      return new Uint8Array([0x47, 0x49, 0x46]);
    },
    stop() {},
  };
}

function oracleMessage(id, content) {
  return {
    id,
    channel_id: "oracle-channel",
    content,
    author: { id: "oracle-user", bot: false },
    attachments: [],
  };
}

function validBuildV2Structured(capabilityId, resultContract) {
  const payloadKind = new Set([
    "build.setup",
    "build.congruent",
    "build.evaluate.cover",
    "build.evaluate.b2b-cover",
  ]).has(capabilityId)
    ? "candidate-family"
    : new Set([
        "build.setup-cover-percent",
        "build.evaluate.cover-percent",
      ]).has(capabilityId)
      ? "probability"
      : new Set([
          "build.setup-cover-score",
          "build.evaluate.score",
        ]).has(capabilityId)
        ? "score-portfolio"
        : "portfolio";
  return {
    kind: resultContract,
    contract: { command: { kind: resultContract } },
    summary: {
      capability_id: capabilityId,
      result_contract: resultContract,
      payload_kind: payloadKind,
      candidates: [],
      canonical_candidate_keys: payloadKind.includes("portfolio")
        ? ["candidate-a"]
        : [],
      winners: [],
      completeness: { exact: true },
      ...(payloadKind === "score-portfolio"
        ? { score_equality_basis: "score-only" }
        : {}),
    },
  };
}

function validSearchOptions(command, field, finesseDocument, coloredDocument) {
  switch (command.input) {
    case "pc":
    case "pc-v2":
    case "pc-path-v2":
    case "pc-chance-v2":
    case "pc-save-v2":
    case "pc-score-v2":
    case "pc-tiling-v2":
    case "pc-failed-v2":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
        { name: "lines", value: 4 },
      ];
    case "pc-allspin-exact-v1":
      return [
        { name: "field", value: field },
        { name: "next", value: "IOTSZJLIOT" },
        { name: "lines", value: 4 },
        { name: "spin-profile", value: "all-spin-plus" },
      ];
    case "pc-allspin-pattern-v1":
      return [
        { name: "field", value: field },
        { name: "next", value: "*!P3" },
        { name: "lines", value: 4 },
        { name: "spin-profile", value: "all-spin-plus" },
      ];
    case "build-v2-cover":
      return [
        { name: "base-mask", value: "0" },
        { name: "target-mask", value: "15" },
        { name: "height", value: 1 },
        { name: "queue", value: "I" },
      ];
    case "build-v2-target":
      return [
        { name: "target-format", value: "fumen" },
        { name: "target-document", value: coloredDocument },
        { name: "queue", value: "I" },
      ];
    case "build-v2-supplied":
      return [
        { name: "solution-format", value: "fumen" },
        { name: "solution-document", value: coloredDocument },
        { name: "queue", value: "I" },
      ];
    case "colored":
    case "spin":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
      ];
    case "cover":
    case "build-cover":
      return [
        { name: "base", value: field },
        {
          name: "target",
          value: fumenEncoder.encode([{ field: Field.create("IIII______") }]),
        },
        { name: "next", value: "I" },
      ];
    case "finesse-search":
      return [
        { name: "base", value: field },
        {
          name: "target",
          value: fumenEncoder.encode([{ field: Field.create("IIII______") }]),
        },
        { name: "next", value: "I" },
      ];
    case "finesse-score":
    case "finesse-score-v2":
      return [
        { name: "document", value: finesseDocument },
        { name: "next", value: "I" },
      ];
    case "operation-document-v1":
      return [{ name: "document", value: finesseDocument }];
    case "field-document-v1":
      return [{ name: "document", value: field }];
    case "fumen-transform-v1":
      return [
        { name: "transform", value: "roundtrip" },
        { name: "document", value: field },
      ];
    case "render-document-v1":
      return [
        { name: "document", value: field },
        { name: "artifact-format", value: "png" },
        { name: "page", value: 1 },
      ];
    case "fixed-next":
    case "forward-spin-v2":
    case "forward-damage-v2":
    case "forward-ren-v1":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
      ];
    case "score-fixed-next":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
        { name: "lines", value: 4 },
        { name: "options", value: "initial_b2b=false" },
      ];
    case "score-fixed-next-v2":
    case "pc-score-finder-v2":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
        { name: "lines", value: 4 },
        { name: "initial-b2b", value: 0 },
      ];
    case "remaining":
    case "setup-v2":
      return [{ name: "remaining", value: "IOTS" }];
    case "setup-score-v1":
      return [
        { name: "document-format", value: "fumen" },
        { name: "document", value: coloredDocument },
        { name: "setup-queue", value: "I" },
        { name: "solution-queue", value: "I" },
      ];
    case "spin-structure":
    case "spin-structure-v2":
    case "spin-structure-cover-v1":
    case "spin-structure-guaranteed-v1":
      return [
        { name: "pieces", value: "TTIO" },
        { name: "field", value: field },
        { name: "lines", value: "1+" },
        {
          name: command.input === "spin-structure" ? "profile" : "spin-profile",
          value: "all-mini",
        },
      ];
    default:
      throw new Error(`unsupported test command input: ${command.input}`);
  }
}
