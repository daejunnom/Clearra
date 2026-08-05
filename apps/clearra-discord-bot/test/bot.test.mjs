import assert from "node:assert/strict";
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
import { decodeViewerDocument } from "../src/viewer/document.mjs";

const REPRESENTED_SFINDER_COMMANDS = [
  "path",
  "percent",
  "chance",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
  "cover",
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
  "spin-cover",
  "spin",
  "score-finder",
  "damage",
  "spin-structure",
  "pc-setup",
  "best-setup",
  "dpc-finder",
  "verify",
];

test("slash catalog registers only curated active commands", () => {
  assert.deepEqual(
    slashCommandCatalog.map((command) => command.name),
    ["help", "render-file", "channel-settings", "server-settings", ...REPRESENTED_SFINDER_COMMANDS],
  );
  assert.deepEqual(
    globalCommands.map((command) => command.name),
    [
      "help",
      "render-file",
      "channel-settings",
      "server-settings",
      ...REPRESENTED_SFINDER_COMMANDS,
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
  for (const command of slashCommandCatalog.filter(({ kind }) => kind === "search")) {
    const expectedPrefix = command.name === "score-finder"
      ? ["sfinder", "score-finder"]
      : command.name === "damage" || command.name === "spin-structure"
        ? [command.name]
        : ["sfinder", command.name];
    assert.deepEqual(command.argvPrefix, expectedPrefix);
    assert.equal(
      command.registration.options.some(({ name }) => name === "arguments"),
      false,
    );
  }
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
    ["next", "field", "kicktable"],
  );
  assert.deepEqual(
    findSlashCommand("spin-structure").registration.options.map(({ name }) => name),
    ["pieces", "field", "lines", "profile", "kicktable"],
  );
  assert.deepEqual(
    findSlashCommand("spin-structure").registration.options
      .find(({ name }) => name === "profile")
      .choices.map(({ value }) => value),
    [
      "t-spins",
      "t-spins-plus",
      "all-mini",
      "all-mini-plus",
      "all-spin",
      "all-spin-plus",
    ],
  );
  assert.equal(globalCommands.some(({ name }) => name === "cat-finder"), false);
  assert.equal(
    prepareClearraArguments(["damage", "--board-mask-v1", "0", "--queue", "I"])[0],
    "damage",
  );
  assert.deepEqual(
    findSlashCommand("pc-setup").registration.options.map(({ name }) => name),
    ["remaining", "kicktable"],
  );
  assert.equal(globalCommands.some(({ name }) => name === "clearra"), false);
  assert.equal(globalCommands.some(({ name }) => name === "view"), false);
});

test("registered command metadata and every help page stay inside Discord limits", () => {
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
    for (const option of command.options) {
      assert.match(option.name, /^[-_a-z0-9]{1,32}$/);
      assert.ok(option.description.length >= 1 && option.description.length <= 100);
      assert.ok((option.choices?.length ?? 0) <= 25);
      for (const choice of option.choices ?? []) {
        assert.ok(choice.name.length >= 1 && choice.name.length <= 100);
        assert.ok(String(choice.value).length >= 1 && String(choice.value).length <= 100);
      }
    }
  }

  assert.ok(formatSlashCommandHelp().length <= 2_000);
  for (const name of REPRESENTED_SFINDER_COMMANDS) {
    const help = formatSlashCommandHelp(name);
    assert.ok(help.length <= 2_000, `/${name} help exceeds Discord's message limit`);
    assert.match(help, new RegExp(`^\\*\\*/${name}\\*\\*`));
    assert.match(help, /guided Modal form/);
  }
  const englishPcHelp = formatSlashCommandHelp("path", "en");
  const koreanPcHelp = formatSlashCommandHelp("path", "ko");
  assert.match(englishPcHelp, /all 1–6-row targets/);
  assert.match(koreanPcHelp, /1–6줄 전체/);
  assert.doesNotMatch(englishPcHelp, /prefill|starts? (?:at|with) 4|default is 4/i);
  assert.doesNotMatch(koreanPcHelp, /기본(?:값)?(?:은)? 4줄|4줄로 시작/u);
  assert.doesNotMatch(englishPcHelp, /2L\/4L\/6L/);
  assert.doesNotMatch(koreanPcHelp, /2L\/4L\/6L/);

  const englishBuildHelp = formatSlashCommandHelp("setup", "en");
  const koreanBuildHelp = formatSlashCommandHelp("setup", "ko");
  assert.match(englishBuildHelp, /1–24 top-first rows/);
  assert.match(koreanBuildHelp, /1–24줄/u);
  assert.doesNotMatch(englishBuildHelp, /prefill|starts? (?:at|with) 8/i);
  assert.doesNotMatch(koreanBuildHelp, /기본(?:값)?(?:은)? 8줄|8줄로 시작/u);

  const englishCommandList = formatSlashCommandHelp("", "en");
  const koreanCommandList = formatSlashCommandHelp("", "ko");
  assert.match(englishCommandList, /every target height from 1 through 6 rows/u);
  assert.match(englishCommandList, /support 1 through 24 rows/u);
  assert.match(koreanCommandList, /1–6줄의 모든 목표 높이를 지원/u);
  assert.match(koreanCommandList, /1–24줄을 지원/u);
  assert.doesNotMatch(englishCommandList, /prefill/i);
  assert.doesNotMatch(koreanCommandList, /기본 4줄|기본 8줄/u);

  for (const name of ["render-file", ...REPRESENTED_SFINDER_COMMANDS]) {
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
  assert.match(koreanScoreFinderHelp, /initial_b2b=false/);
  assert.doesNotMatch(koreanScoreFinderHelp, /최대 대미지/u);
  const koreanDamageHelp = formatSlashCommandHelp("damage", "ko");
  assert.match(koreanDamageHelp, /최대 대미지/u);
  assert.match(koreanDamageHelp, /1–24줄/u);
  assert.doesNotMatch(koreanDamageHelp, /initial_b2b|기본값은 4줄/u);
  const englishSpinStructureHelp = formatSlashCommandHelp("spin-structure", "en");
  const koreanSpinStructureHelp = formatSlashCommandHelp("spin-structure", "ko");
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
      custom_id: "clearra:search:v3:setup",
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
  assert.equal(await handling, true);
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
        async execute(arguments_) {
          invocations.push(arguments_);
          return { exitCode: 0, stdout: "ok", stderr: "" };
        },
      },
    },
  );

  const field = fumenEncoder.encode([{ field: Field.create() }]);
  for (const name of REPRESENTED_SFINDER_COMMANDS) {
    const command = findSlashCommand(name);
    assert.equal(
      await bot.handleInteraction(
        slashInteraction(name, validSearchOptions(command, field)),
      ),
      true,
    );
  }

  assert.equal(edits.length, REPRESENTED_SFINDER_COMMANDS.length);
  for (let index = 0; index < REPRESENTED_SFINDER_COMMANDS.length; index += 1) {
    const command = findSlashCommand(REPRESENTED_SFINDER_COMMANDS[index]);
    assert.deepEqual(
      invocations[index].slice(0, command.argvPrefix.length),
      command.argvPrefix,
    );
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
    { name: "options", value: "clear=2 hold=avoid" },
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
              kind: "pc",
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
  assert.equal(ctk3File.name, "pc-result.ctk3");
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
  assert.match(messages[0].payload.content, /Clearra score-finder completed\./);
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
              kind: "pc",
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
    ["pc", "--lines", "2"],
  );

  assert.equal(messages.length, 1);
  assert.deepEqual(messages[0].files, []);
  assert.match(messages[0].payload.content, /Clearra pc completed\./);
  assert.match(messages[0].payload.content, /Solutions: 0/);
  assert.doesNotMatch(
    messages[0].payload.content,
    /(?:file|attachment).*(?:omit|skip|creat)|(?:omit|skip|creat).*(?:file|attachment)/i,
  );
});

test("Korean structured build kinds use the build-probability result name", async () => {
  const messages = [];
  const kinds = ["build-probability", "build_coverage"];
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
              kind: kinds.shift(),
              summary: { coverage_probability: 0.5 },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("setup", [], undefined, "build-kind-one"),
    ["sfinder", "setup"],
    null,
    "ko",
  );
  await bot.runInteractionCommand(
    slashInteraction("setup", [], undefined, "build-kind-two"),
    ["sfinder", "setup"],
    null,
    "ko",
  );

  assert.equal(messages.length, 2);
  for (const message of messages) {
    assert.match(message.payload.content, /Clearra 구축 확률 탐색/u);
    assert.match(message.payload.content, /커버 확률: 50%/u);
  }
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
              kind: "spin-structure",
              summary: {
                result_count: 2,
                regular_count: 1,
                mini_count: 1,
                minimum_placements: 2,
                workers_used: 8,
                cloud_run_revision: "private-revision",
                engine: "private-engine",
              },
              contract: {
                artifacts: {
                  schema_version: "clearra.solution-data.v1",
                  solution_keys: [
                    `ctk2|height=4|initial=${"0".repeat(64)}|placements=T:${"0".repeat(61)}807`,
                    `ctk2|height=4|initial=${"0".repeat(64)}|placements=T:${"0".repeat(60)}8070`,
                  ],
                },
              },
            }),
          };
        },
      },
    },
  );

  await bot.runInteractionCommand(
    slashInteraction("spin-structure", [], undefined, "spin-structure-en"),
    ["spin-structure"],
    null,
    "en",
  );
  await bot.runInteractionCommand(
    slashInteraction("spin-structure", [], undefined, "spin-structure-ko"),
    ["spin-structure"],
    null,
    "ko",
  );

  assert.match(messages[0].payload.content, /Clearra spin-structure search completed\./);
  assert.match(messages[0].payload.content, /Results: 2/);
  assert.match(messages[0].payload.content, /Regular structures: 1/);
  assert.match(messages[0].payload.content, /Mini structures: 1/);
  assert.match(messages[0].payload.content, /Minimum placements: 2/);
  assert.doesNotMatch(messages[0].payload.content, /Workers used|private|engine/i);
  assert.equal(messages[0].files[0].name, "spin-structure-result.ctk3");
  assert.match(messages[1].payload.content, /Clearra 스핀 구조 탐색/u);
  assert.match(messages[1].payload.content, /결과: 2/u);
  assert.match(messages[1].payload.content, /Regular 구조: 1/u);
  assert.match(messages[1].payload.content, /Mini 구조: 1/u);
  assert.match(messages[1].payload.content, /최소 배치 수: 2/u);
  assert.doesNotMatch(messages[1].payload.content, /사용 워커 수|private|engine/iu);
  assert.equal(messages[1].files[0].name, "spin-structure-result.ctk3");
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

test("Discord replaces an unrecognized structured kind with the requested public command", async () => {
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

  assert.match(messages[0].payload.content, /Clearra spin-structure search completed\./);
  assert.doesNotMatch(messages[0].payload.content, /private|worker|engine/i);
});

test("score-finder does not hide an unexpected engine result kind", async () => {
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

  assert.match(messages[0].payload.content, /Clearra damage completed\./);
  assert.doesNotMatch(messages[0].payload.content, /score-finder/);
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

function validSearchOptions(command, field) {
  switch (command.input) {
    case "pc":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
        { name: "lines", value: 4 },
      ];
    case "colored":
    case "spin":
      return [
        { name: "field", value: field },
        { name: "next", value: "I" },
      ];
    case "cover":
      return [
        { name: "base", value: field },
        {
          name: "target",
          value: fumenEncoder.encode([{ field: Field.create("IIII______") }]),
        },
        { name: "next", value: "I" },
      ];
    case "fixed-next":
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
    case "remaining":
      return [{ name: "remaining", value: "IOTS" }];
    case "spin-structure":
      return [
        { name: "pieces", value: "TTIO" },
        { name: "field", value: field },
        { name: "lines", value: "1+" },
        { name: "profile", value: "all-mini" },
      ];
    case "verify":
      return [];
    default:
      throw new Error(`unsupported test command input: ${command.input}`);
  }
}
