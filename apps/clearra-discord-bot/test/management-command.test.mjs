import assert from "node:assert/strict";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";
import { DiscordAccessPreferences } from "../src/discord/access-preferences.mjs";
import {
  canManageDiscordSettings,
  formatTextManagementHelp,
  isTextManagementCandidate,
  readDiscordManagementRequest,
  readTextManagementRequest,
} from "../src/discord/management-command.mjs";
import { SlashCommandIngress } from "../src/ingress/slash-command-ingress.mjs";
import { OracleMessageIngress } from "../src/ingress/oracle-message-ingress.mjs";

const APPLICATION_ID = "1533373054309371924";
const GUILD_ID = "123456789012345678";
const CHANNEL_ID = "223456789012345678";
const ADMIN_ID = "323456789012345678";

test("management command parsers keep slash and hidden text recovery syntax strict", () => {
  assert.deepEqual(
    readDiscordManagementRequest("channel-settings", [{
      name: "language-set",
      options: [{ name: "language", value: "ko" }],
    }]),
    { scope: "channel", action: "language-set", locale: "ko" },
  );
  assert.deepEqual(
    readDiscordManagementRequest("server-settings", [{
      name: "resume",
      options: [],
    }]),
    { scope: "guild", action: "resume", locale: null },
  );
  assert.deepEqual(
    readTextManagementRequest("$bot-control server resume", "$"),
    { scope: "guild", action: "resume", locale: null },
  );
  assert.deepEqual(
    readTextManagementRequest(">BOT-CONTROL channel language set KO", ">"),
    { scope: "channel", action: "language-set", locale: "ko" },
  );
  assert.deepEqual(
    readTextManagementRequest("$bot-control help", "$"),
    { scope: null, action: "help", locale: null },
  );
  assert.equal(isTextManagementCandidate("$BOT-CONTROL unknown", "$"), true);
  assert.equal(isTextManagementCandidate("$help", "$"), false);
  assert.equal(readTextManagementRequest("$path", "$"), null);
  assert.throws(
    () => readTextManagementRequest("$bot-control help channel", "$"),
    /does not accept arguments/,
  );
  assert.throws(
    () => readTextManagementRequest("$bot-control server pause now", "$"),
    /action is invalid/,
  );
  assert.throws(
    () => readDiscordManagementRequest("server-settings", [{
      name: "language-set",
      options: [{ name: "language", value: "ja" }],
    }]),
    /en or ko/,
  );
});

test("bot-control help documents every hidden syntax in English and Korean", () => {
  for (const locale of ["en", "ko"]) {
    const help = formatTextManagementHelp(locale);
    assert.ok(help.length <= 2_000);
    assert.match(help, /\$bot-control help/);
    assert.match(help, /channel language show/);
    assert.match(help, /channel language set en\|ko/);
    assert.match(help, /channel language reset/);
    assert.match(help, /channel disable\|enable/);
    assert.match(help, /server language show/);
    assert.match(help, /server language set en\|ko/);
    assert.match(help, /server language reset/);
    assert.match(help, /server pause\|resume/);
    assert.match(help, />/);
  }
});

test("native Discord management permissions remain scope-specific", () => {
  const interaction = (permissions) => ({
    guild_id: GUILD_ID,
    member: { permissions },
  });
  assert.equal(canManageDiscordSettings(interaction("16"), "channel"), true);
  assert.equal(canManageDiscordSettings(interaction("16"), "guild"), false);
  assert.equal(canManageDiscordSettings(interaction("32"), "guild"), true);
  assert.equal(canManageDiscordSettings(interaction("8"), "channel"), true);
  assert.equal(canManageDiscordSettings(interaction("invalid"), "guild"), false);
});

test("server pause and channel disable gate slash and Modal work before execution", async () => {
  const accessPreferences = new DiscordAccessPreferences();
  const edits = [];
  const defers = [];
  let searches = 0;
  let authorityChecks = 0;
  const bot = new Clearrabot({
    async deferInteraction(_interaction, options) { defers.push(options); },
    async editOriginalInteraction(_applicationId, _token, message) {
      edits.push(message);
      return { attachments: [] };
    },
  }, {
    applicationId: APPLICATION_ID,
    maxConcurrentSearches: 1,
  }, {
    applicationId: APPLICATION_ID,
    accessPreferences,
    botAdministratorAuthority: {
      async allows() {
        authorityChecks += 1;
        return false;
      },
    },
    executor: {
      async execute() {
        searches += 1;
        return { exitCode: 0, stdout: "done", stderr: "" };
      },
    },
  });
  const ingress = new SlashCommandIngress(bot);

  await accessPreferences.disableChannel(CHANNEL_ID, GUILD_ID);
  const path = slash("path", []);
  assert.deepEqual(bot.interactionAccessDecision(path), {
    allowed: false,
    reason: "channel-disabled",
  });
  const blockedModal = ingress.initialResponse(path);
  assert.equal(blockedModal.type, 4);
  assert.equal(blockedModal.data.flags, 64);
  assert.match(blockedModal.data.content, /unavailable in this channel/);
  const submitted = {
    ...path,
    type: 5,
    data: { custom_id: "clearra:search:v3:path", components: [] },
  };
  assert.equal(bot.interactionAccessDecision(submitted).allowed, false);
  const localizedBlockedSubmit = {
    ...submitted,
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
  assert.match(
    ingress.initialResponse(localizedBlockedSubmit).data.content,
    /이 채널에서는 Clearra 명령을 사용할 수 없습니다/u,
  );

  await accessPreferences.pauseGuild(GUILD_ID);
  assert.equal(
    bot.interactionAccessDecision(slash("channel-settings", [{
      name: "enable",
      options: [],
    }])).allowed,
    false,
  );
  assert.equal(
    bot.interactionAccessDecision(slash("server-settings", [{
      name: "resume",
      options: [],
    }])).allowed,
    true,
  );

  await bot.handleInteraction(slash("server-settings", [{
    name: "resume",
    options: [],
  }], "32"));
  assert.equal(accessPreferences.isGuildPaused(GUILD_ID), false);
  assert.equal(accessPreferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
  await bot.handleInteraction(slash("channel-settings", [{
    name: "enable",
    options: [],
  }], "16"));
  assert.equal(accessPreferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), false);
  assert.equal(searches, 0);
  assert.equal(authorityChecks, 0);
  assert.deepEqual(defers, [{ ephemeral: true }, { ephemeral: true }]);
  assert.match(edits[0].payload.content, /available in this server/);
  assert.match(edits[1].payload.content, /enabled in this channel/);
});

test("hidden text recovery bypasses paused admission only for bot administrators", async () => {
  const accessPreferences = new DiscordAccessPreferences();
  await accessPreferences.pauseGuild(GUILD_ID);
  const messages = [];
  let applicationRequests = 0;
  const bot = new Clearrabot({
    async application() {
      applicationRequests += 1;
      throw new Error("configured administrator must remain local");
    },
    async createChannelMessage(channelId, outgoing) {
      messages.push({ channelId, outgoing });
      return { id: "423456789012345678", attachments: [] };
    },
  }, {
    applicationId: APPLICATION_ID,
    discordAdminUserIds: [ADMIN_ID],
    maxConcurrentSearches: 1,
    oracleRenderEnabled: true,
    oracleTextEnabled: true,
    oracleCommandPrefixes: ["$", ">"],
  }, {
    applicationId: APPLICATION_ID,
    accessPreferences,
    executor: { async execute() { throw new Error("unexpected search"); } },
  });
  const ingress = new OracleMessageIngress(bot, {
    oracleRenderEnabled: true,
    oracleTextEnabled: true,
    oracleAllowedChannelIds: [],
    oracleMaxConcurrentMessages: 1,
  }).setBotUserId("523456789012345678");

  const blocked = await ingress.acceptDispatch(
    "MESSAGE_CREATE",
    textMessage("$path *! ..........", "423456789012345678", "message-blocked"),
  );
  assert.deepEqual(blocked, { accepted: false, reason: "guild-paused" });

  const recovered = await ingress.acceptDispatch(
    "MESSAGE_CREATE",
    textMessage("$bot-control server resume", ADMIN_ID, "message-resume"),
  );
  assert.deepEqual(recovered, { accepted: true });
  assert.equal(accessPreferences.isGuildPaused(GUILD_ID), false);
  assert.equal(applicationRequests, 0);
  assert.equal(messages.length, 1);
  assert.match(messages[0].outgoing.payload.content, /available in this server/);
});

test("bot administrators can request hidden syntax help by DM while other users fail silently", async () => {
  const messages = [];
  let authorityChecks = 0;
  const bot = new Clearrabot({
    async createChannelMessage(channelId, outgoing) {
      messages.push({ channelId, outgoing });
      return { id: "423456789012345678", attachments: [] };
    },
  }, {
    applicationId: APPLICATION_ID,
    maxConcurrentSearches: 1,
    oracleTextEnabled: true,
    oracleCommandPrefixes: ["$", ">"],
  }, {
    applicationId: APPLICATION_ID,
    botAdministratorAuthority: {
      async allows(target) {
        authorityChecks += 1;
        return target.user?.id === ADMIN_ID;
      },
    },
    executor: { async execute() { throw new Error("unexpected search"); } },
  });
  const ingress = new OracleMessageIngress(bot, {
    oracleTextEnabled: true,
    oracleAllowedChannelIds: [],
    oracleMaxConcurrentMessages: 1,
  }).setBotUserId("523456789012345678");
  const dm = (content, authorId, id) => {
    const message = textMessage(content, authorId, id);
    delete message.guild_id;
    return message;
  };

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      dm("$bot-control help", "623456789012345678", "help-denied"),
    ),
    { accepted: false, reason: "management-not-authorized" },
  );
  assert.equal(messages.length, 0);

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      dm(">bot-control help", ADMIN_ID, "help-allowed"),
    ),
    { accepted: true },
  );
  assert.equal(messages.length, 1);
  assert.match(messages[0].outgoing.payload.content, /administrator controls/);
  assert.match(messages[0].outgoing.payload.content, /server pause\|resume/);
  assert.equal(authorityChecks, 3);

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      dm("$help path", "623456789012345678", "ordinary-help"),
    ),
    { accepted: true },
  );
  assert.equal(authorityChecks, 3);
  assert.equal(messages.length, 2);
});

test("malformed bot-control text is authenticated before validation details are produced", async () => {
  let handled = 0;
  let authorizationChecks = 0;
  const handler = {
    oracleAccessDecision(message) {
      return {
        allowed: true,
        reason: null,
        management: isTextManagementCandidate(message.content, "$"),
      };
    },
    acceptsOracleMessage() { return true; },
    async authorizeOracleManagementMessage() {
      authorizationChecks += 1;
      return false;
    },
    async handleOracleMessage() { handled += 1; },
  };
  const ingress = new OracleMessageIngress(handler, {
    oracleTextEnabled: true,
  }).setBotUserId("523456789012345678");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage(
        "$bot-control deliberately-invalid",
        "623456789012345678",
        "invalid-control",
      ),
    ),
    { accepted: false, reason: "management-not-authorized" },
  );
  assert.equal(authorizationChecks, 1);
  assert.equal(handled, 0);
});

test("management recovery bypasses heavy-message saturation and user cooldown", async () => {
  let releaseSearch;
  const searchBlocked = new Promise((resolve) => { releaseSearch = resolve; });
  const handled = [];
  const handler = {
    oracleAccessDecision(message) {
      return {
        allowed: true,
        reason: null,
        management: message.content.includes("bot-control"),
      };
    },
    acceptsOracleMessage() { return true; },
    async authorizeOracleManagementMessage() { return true; },
    async handleOracleMessage(message) {
      handled.push(message.id);
      if (!message.content.includes("bot-control")) await searchBlocked;
    },
  };
  const ingress = new OracleMessageIngress(handler, {
    oracleTextEnabled: true,
    oracleMaxConcurrentMessages: 1,
    oracleMaxPendingMessages: 0,
    oracleUserCooldownMs: 60_000,
  }).setBotUserId("523456789012345678");

  const search = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    textMessage("$path", ADMIN_ID, "message-heavy"),
  );
  await Promise.resolve();
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage("$bot-control server resume", ADMIN_ID, "message-control"),
    ),
    { accepted: true },
  );
  assert.deepEqual(handled, ["message-heavy", "message-control"]);
  releaseSearch();
  assert.deepEqual(await search, { accepted: true });
});

test("unauthorized hidden management messages fail silently before entering the recovery lane", async () => {
  let handled = 0;
  let authorizationChecks = 0;
  const handler = {
    oracleAccessDecision(message) {
      return {
        allowed: true,
        reason: null,
        management: message.content.includes("bot-control"),
      };
    },
    acceptsOracleMessage() { return true; },
    async authorizeOracleManagementMessage() {
      authorizationChecks += 1;
      return false;
    },
    async handleOracleMessage() { handled += 1; },
  };
  const ingress = new OracleMessageIngress(handler, {
    oracleTextEnabled: true,
    oracleMaxConcurrentMessages: 1,
    oracleMaxPendingMessages: 0,
  }).setBotUserId("523456789012345678");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage(
        "$bot-control server resume",
        "623456789012345678",
        "unauthorized-control",
      ),
    ),
    { accepted: false, reason: "management-not-authorized" },
  );
  assert.equal(authorizationChecks, 1);
  assert.equal(handled, 0);
});

test("authorized hidden management messages use a bounded recovery queue", async () => {
  const starts = [];
  const releases = [];
  const handler = {
    oracleAccessDecision(message) {
      return { allowed: true, reason: null, management: true };
    },
    acceptsOracleMessage() { return true; },
    async authorizeOracleManagementMessage() { return true; },
    async handleOracleMessage(message) {
      starts.push(message.id);
      await new Promise((resolve) => releases.push(resolve));
    },
  };
  const ingress = new OracleMessageIngress(handler, {
    oracleTextEnabled: true,
  }).setBotUserId("523456789012345678");

  const controls = ["control-1", "control-2", "control-3"].map((id) =>
    ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage("$bot-control server resume", ADMIN_ID, id),
    ));
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage("$bot-control server resume", ADMIN_ID, "control-overflow"),
    ),
    { accepted: false, reason: "management-queue-full" },
  );
  assert.deepEqual(starts, ["control-1"]);

  for (let index = 0; index < controls.length; index += 1) {
    releases.shift()();
    assert.deepEqual(await controls[index], { accepted: true });
    await Promise.resolve();
  }
  assert.deepEqual(starts, ["control-1", "control-2", "control-3"]);
});

test("management candidates are bounded before application-owner authorization", async () => {
  let releaseAuthorization;
  const authorizationBlocked = new Promise((resolve) => {
    releaseAuthorization = resolve;
  });
  let authorizationChecks = 0;
  const handler = {
    oracleAccessDecision() {
      return { allowed: true, reason: null, management: true };
    },
    acceptsOracleMessage() { return true; },
    async authorizeOracleManagementMessage() {
      authorizationChecks += 1;
      await authorizationBlocked;
      return false;
    },
    async handleOracleMessage() {
      throw new Error("unauthorized management must not run");
    },
  };
  const ingress = new OracleMessageIngress(handler, {
    oracleTextEnabled: true,
  }).setBotUserId("523456789012345678");

  const candidates = ["candidate-1", "candidate-2", "candidate-3"].map((id) =>
    ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage("$bot-control server resume", ADMIN_ID, id),
    ));
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      textMessage("$bot-control server resume", ADMIN_ID, "candidate-overflow"),
    ),
    { accepted: false, reason: "management-queue-full" },
  );
  assert.equal(authorizationChecks, 1);

  releaseAuthorization();
  assert.deepEqual(await Promise.all(candidates), [
    { accepted: false, reason: "management-not-authorized" },
    { accepted: false, reason: "management-not-authorized" },
    { accepted: false, reason: "management-not-authorized" },
  ]);
  assert.equal(authorizationChecks, 3);
});

function slash(name, options, permissions = "0") {
  return {
    id: `interaction-${name}-${permissions}`,
    application_id: APPLICATION_ID,
    token: `token-${name}`,
    type: 2,
    data: { type: 1, name, options },
    guild_id: GUILD_ID,
    channel_id: CHANNEL_ID,
    member: {
      permissions,
      user: { id: ADMIN_ID },
    },
  };
}

function textMessage(content, authorId, id) {
  return {
    id,
    guild_id: GUILD_ID,
    channel_id: CHANNEL_ID,
    content,
    attachments: [],
    mentions: [],
    author: { id: authorId, bot: false },
  };
}
