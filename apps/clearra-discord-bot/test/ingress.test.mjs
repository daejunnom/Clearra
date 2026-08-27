import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { globalCommands } from "../src/discord/slash-command-catalog.mjs";
import {
  isEnabledSlashCommand,
  SlashCommandIngress,
} from "../src/ingress/slash-command-ingress.mjs";
import { DiscordGateway } from "../src/discord/gateway.mjs";
import { Clearrabot } from "../src/bot.mjs";
import { DiscordLocalePreferences } from "../src/discord/locale-preferences.mjs";

test("production startup has one Oracle Gateway interaction ingress", async () => {
  const source = await readFile(new URL("../src/main.mjs", import.meta.url), "utf8");
  assert.match(source, /gateway\s*=\s*new DiscordGateway\(config\.token/u);
  assert.match(source, /slashCommandIngress\.acceptDispatch|ingress\.acceptDispatch/u);
  assert.doesNotMatch(source, /CloudRunDiscordInteractionAdapter|cloudRunAdapter|ingressMode\s*===\s*["']cloud-run/u);
});

test("Gateway fallback requests no privileged or message intents", () => {
  assert.equal(new DiscordGateway("test-token").intents, 0);
});

test("Gateway reconnect resets the heartbeat acknowledgement latch", async () => {
  let socket;
  const gateway = new DiscordGateway("test-token", {
    createWebSocket() {
      socket = new FakeGatewaySocket();
      return socket;
    },
  });
  gateway.heartbeatAcknowledged = false;

  const connection = gateway.connectOnce();
  assert.equal(gateway.heartbeatAcknowledged, true);
  socket.emit("open");
  socket.emit("close", { code: 1006 });
  await connection;
});

test("Gateway stops instead of reconnecting forever after fatal configuration closes", async () => {
  for (const code of [4013, 4014]) {
    let socket;
    const gateway = new DiscordGateway("test-token", {
      createWebSocket() {
        socket = new FakeGatewaySocket();
        return socket;
      },
    });
    const connection = gateway.connectOnce();
    socket.emit("open");
    socket.emit("close", { code });

    await assert.rejects(connection, new RegExp(`\\(${code}\\)`));
    assert.equal(gateway.stopped, true);
  }
});

test("slash ingress rejects ordinary Gateway messages and handles Gateway interactions", async () => {
  let calls = 0;
  let defers = 0;
  const responses = [];
  const ingress = new SlashCommandIngress(
    {
      async handleInteraction(interaction, options) {
        calls += 1;
        await options.acknowledger.defer(interaction);
        return true;
      },
    },
    {
      acknowledger: {
        async defer() { defers += 1; },
        async respond(_interaction, response) { responses.push(response); },
      },
    },
  );

  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", { content: "!pc" }),
    { accepted: false, reason: "gateway-message-events-disabled" },
  );
  assert.equal(calls, 0);

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", {
      type: 2,
      data: { type: 1, name: "path" },
    }),
    { accepted: true },
  );
  assert.equal(calls, 0);
  assert.equal(responses.length, 1);
  assert.equal(responses[0].type, 9);

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", {
      type: 2,
      data: { type: 1, name: "help", options: [] },
    }),
    { accepted: true },
  );
  assert.equal(calls, 0);
  assert.equal(defers, 0);
  assert.equal(responses.length, 2);
  assert.equal(responses[1].type, 4);
  assert.match(responses[1].data.content, /Clearra slash commands/u);
});

test("Gateway Modal submissions are deferred exactly once before execution", async () => {
  let defers = 0;
  let calls = 0;
  const interaction = {
    type: 5,
    data: {
      custom_id: "clearra:search:v2:path",
      components: [],
    },
  };
  const ingress = new SlashCommandIngress(
    {
      async handleInteraction(target, options) {
        calls += 1;
        assert.equal(target, interaction);
        await options.acknowledger.defer(target);
        return true;
      },
    },
    {
      acknowledger: {
        async defer() { defers += 1; },
        async respond() { throw new Error("Modal submit must not open a Modal"); },
      },
    },
  );

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", interaction),
    { accepted: true },
  );
  assert.equal(calls, 1);
  assert.equal(defers, 1);
});

test("Gateway interaction IDs are remembered before the first callback awaits", async () => {
  let releaseResponse;
  const response = new Promise((resolve) => {
    releaseResponse = resolve;
  });
  let responses = 0;
  const ingress = new SlashCommandIngress(
    { async handleInteraction() { throw new Error("Modal must not execute"); } },
    {
      acknowledger: {
        async defer() {},
        async respond() {
          responses += 1;
          await response;
        },
      },
    },
  );
  const interaction = {
    id: "duplicate-interaction",
    type: 2,
    data: { type: 1, name: "path", options: [] },
  };

  const first = ingress.acceptDispatch("INTERACTION_CREATE", interaction);
  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", interaction),
    { accepted: false, reason: "duplicate-interaction" },
  );
  releaseResponse();
  assert.deepEqual(await first, { accepted: true });
  assert.equal(responses, 1);
});

test("slash ingress releases a local claim after a remote claim failure", () => {
  const ingress = new SlashCommandIngress({});
  const interaction = { id: "1533373054309371924" };

  assert.equal(ingress.claim(interaction), true);
  assert.equal(ingress.claim(interaction), false);
  assert.equal(ingress.releaseClaim(interaction), true);
  assert.equal(ingress.claim(interaction), true);
});

test("slash ingress accepts exactly the registered command catalog", () => {
  const ingress = new SlashCommandIngress({});
  assert.deepEqual(
    ingress.deferredResponse({ data: { type: 1, name: "channel-settings" } }),
    { type: 5, data: { flags: 64 } },
  );
  assert.deepEqual(
    ingress.deferredResponse({ data: { type: 1, name: "help" } }),
    { type: 5 },
  );
  assert.deepEqual(
    ingress.deferredResponse({ data: { type: 3, name: "Get original GIF" } }),
    { type: 5 },
  );
  for (const command of globalCommands) {
    const interaction = {
      type: 2,
      data: { type: command.type ?? 1, name: command.name },
    };
    assert.equal(ingress.accepts(interaction), true);
    assert.equal(isEnabledSlashCommand(interaction), true);
  }
  for (const name of ["clearra", "view", "disabled"]) {
    const interaction = { type: 2, data: { type: 1, name } };
    assert.equal(ingress.accepts(interaction), false);
    assert.equal(isEnabledSlashCommand(interaction), false);
  }
  const unknownMessageCommand = {
    type: 2,
    data: { type: 3, name: "Unknown action" },
  };
  assert.equal(ingress.accepts(unknownMessageCommand), false);
  assert.equal(isEnabledSlashCommand(unknownMessageCommand), false);

  const legacyPathModalSubmit = {
    type: 5,
    data: { custom_id: "clearra:board:v1:path", components: [] },
  };
  assert.equal(ingress.accepts(legacyPathModalSubmit), true);
  assert.equal(isEnabledSlashCommand(legacyPathModalSubmit), true);

  const commandModalSubmit = {
    type: 5,
    data: { custom_id: "clearra:search:v2:percent", components: [] },
  };
  assert.equal(ingress.accepts(commandModalSubmit), true);
  assert.equal(isEnabledSlashCommand(commandModalSubmit), true);

  for (const customId of [
    "clearra:board:v2:path",
    "clearra:search:v1:percent",
    "clearra:command:v1:percent",
    "clearra:command:v2:path",
    "clearra:search:v5:percent",
    "clearra:search:v2:disabled",
  ]) {
    const stale = {
      ...commandModalSubmit,
      data: { ...commandModalSubmit.data, custom_id: customId },
    };
    assert.equal(ingress.accepts(stale), false, customId);
    assert.equal(isEnabledSlashCommand(stale), false, customId);
  }
});

test("slash ingress opens bounded command Modals and answers direct help immediately", () => {
  const ingress = new SlashCommandIngress({});
  const expectations = [
    ["path", "clearra:search:v4:path", 5],
    ["percent", "clearra:search:v4:percent", 5],
    ["cover", "clearra:search:v4:cover", 5],
    ["spin-structure", "clearra:search:v4:spin-structure~search", 5, [
      { type: 1, name: "search", options: [] },
    ]],
    ["pc-setup", "clearra:search:v4:pc-setup", 5],
  ];

  for (const [name, customId, rowCount, options = []] of expectations) {
    const modal = ingress.initialResponse({
      type: 2,
      data: { type: 1, name, options },
    });
    assert.equal(modal?.type, 9, name);
    assert.equal(modal.data.custom_id, customId, name);
    assert.equal(modal.data.components.length, rowCount, name);
    assert.ok(modal.data.components.length <= 5, name);
  }

  assert.equal(
    ingress.initialResponse({
      type: 2,
      data: { type: 1, name: "verify", options: [] },
    }) ?? null,
    null,
    "the hidden text diagnostic must not open a slash Modal",
  );

  assert.equal(
    ingress.initialResponse({
      type: 2,
      data: { type: 1, name: "render-file", options: [] },
    }),
    null,
  );

  assert.equal(
    ingress.initialResponse({
      type: 2,
      data: {
        type: 3,
        name: "Get original GIF",
        target_id: "1533373054309371924",
        resolved: { messages: {} },
      },
    }),
    null,
  );

  const help = ingress.initialResponse({
    type: 2,
    data: { type: 1, name: "help", options: [] },
  });
  assert.equal(help.type, 4);
  assert.match(help.data.content, /Clearra slash commands/u);
  assert.deepEqual(help.data.allowed_mentions, { parse: [] });
  assert.equal(help.data.flags, undefined);

  assert.equal(
    ingress.initialResponse({
      data: {
        type: 1,
        name: "path",
        options: [
          { name: "next", value: "I" },
          { name: "field", value: ".........." },
        ],
      },
      type: 2,
    }),
    null,
  );
});

test("slash ingress localizes setup advanced-option Modal loss prevention", () => {
  const interaction = {
    type: 2,
    data: {
      type: 1,
      name: "pc-setup",
      options: [{ name: "setup-length", value: "longer" }],
    },
  };
  const english = new SlashCommandIngress({}).initialResponse(interaction);
  assert.equal(english.type, 4);
  assert.equal(english.data.flags, 64);
  assert.match(
    english.data.content,
    /guided form cannot preserve setup-length.*provide every required.*directly/iu,
  );

  const korean = new SlashCommandIngress({
    resolveResponseLocale() {
      return { locale: "ko" };
    },
  }).initialResponse(interaction);
  assert.equal(korean.type, 4);
  assert.match(korean.data.content, /setup-length.*직접/u);
});

test("Gateway help resolves access and locale before its type 4 response without invoking the bot handler", async () => {
  const events = [];
  const responses = [];
  const interaction = {
    id: "localized-help",
    type: 2,
    locale: "en-US",
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    data: {
      type: 1,
      name: "help",
      options: [{ name: "arguments", type: 3, value: "path" }],
    },
  };
  const ingress = new SlashCommandIngress(
    {
      interactionAccessDecision(target) {
        assert.equal(target, interaction);
        events.push("access");
        return { allowed: true, reason: null };
      },
      resolveResponseLocale(target) {
        assert.equal(target, interaction);
        events.push("locale");
        return { locale: "ko" };
      },
      async handleInteraction() {
        events.push("handle");
        return true;
      },
    },
    {
      acknowledger: {
        async defer() { events.push("defer"); },
        async respond(target, response) {
          assert.equal(target, interaction);
          events.push("respond");
          responses.push(response);
        },
      },
    },
  );

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", interaction),
    { accepted: true },
  );
  assert.deepEqual(events, ["access", "locale", "respond"]);
  assert.equal(responses[0].type, 4);
  assert.match(responses[0].data.content, /직접 입력 문법/u);
});

test("Gateway help cannot bypass a localized channel access block", async () => {
  let handled = 0;
  const responses = [];
  const ingress = new SlashCommandIngress(
    {
      interactionAccessDecision() {
        return { allowed: false, reason: "channel-disabled" };
      },
      resolveResponseLocale() {
        return { locale: "ko" };
      },
      accessBlockedText(_decision, locale) {
        assert.equal(locale, "ko");
        return "이 채널에서는 사용할 수 없습니다.";
      },
      async handleInteraction() {
        handled += 1;
        return true;
      },
    },
    {
      acknowledger: {
        async defer() { throw new Error("blocked help must not defer"); },
        async respond(_interaction, response) { responses.push(response); },
      },
    },
  );

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", {
      id: "blocked-help",
      type: 2,
      data: { type: 1, name: "help", options: [] },
    }),
    { accepted: true },
  );
  assert.equal(handled, 0);
  assert.equal(responses[0].type, 4);
  assert.equal(responses[0].data.flags, 64);
  assert.equal(responses[0].data.content, "이 채널에서는 사용할 수 없습니다.");
});

test("slash ingress localizes Modals from interaction locale behind stored overrides", async () => {
  const localePreferences = new DiscordLocalePreferences();
  const bot = {
    localePreferences,
    resolveLocale: Clearrabot.prototype.resolveLocale,
  };
  const ingress = new SlashCommandIngress(bot);
  const interaction = {
    type: 2,
    guild_id: "123456789012345678",
    channel_id: "223456789012345678",
    locale: "ko",
    data: { type: 1, name: "path", options: [] },
  };

  const korean = ingress.initialResponse(interaction);
  assert.equal(korean.data.title, "경로 탐색 입력");
  assert.equal(
    korean.data.components.find(({ component }) => component.custom_id === "lines")
      .component.options[0].label,
    "자동 — 1–6줄 전체 판정",
  );

  await localePreferences.setGuild(interaction.guild_id, "ko");
  await localePreferences.setChannel(interaction.channel_id, "en");
  assert.equal(ingress.initialResponse(interaction).data.title, "path search form");

  await localePreferences.resetChannel(interaction.channel_id);
  assert.equal(ingress.initialResponse(interaction).data.title, "경로 탐색 입력");
});

test("slash ingress terminates a deferred interaction when its handler throws", async () => {
  const events = [];
  const ingress = new SlashCommandIngress(
    {
      async handleInteraction(interaction, options) {
        await options.acknowledger.defer(interaction);
        events.push("deferred");
        throw new Error("handler failed");
      },
      async handleInteractionFailure(_interaction, error) {
        events.push(`terminal:${error.message}`);
      },
    },
    { acknowledger: { async defer() {} } },
  );

  assert.deepEqual(
    await ingress.accept({ type: 2, data: { type: 1, name: "path" } }),
    { accepted: true },
  );
  assert.deepEqual(events, ["deferred", "terminal:handler failed"]);
});

class FakeGatewaySocket {
  constructor() {
    this.listeners = new Map();
  }

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  emit(type, event = {}) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  close(code = 1000) {
    this.emit("close", { code });
  }

  send() {}
}
