import assert from "node:assert/strict";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";
import { parseClearraTextRequest } from "../src/clearra/text-command.mjs";
import {
  findApplicationCommand,
  findMessageCommand,
  findSlashCommand,
  formatSlashCommandHelp,
  globalCommands,
} from "../src/discord/slash-command-catalog.mjs";

const APPLICATION_ID = "1533373054309371924";
const BOT_USER_ID = "1533373054309371925";
const GUILD_ID = "1476890399426482239";
const CHANNEL_ID = "1533373560201154652";
const REQUESTER_ID = "2533373054309371924";
const OTHER_USER_ID = "3533373054309371924";
const EXPLICIT_MESSAGE_ID = "4533373054309371924";
const OWN_MESSAGE_ID = "5533373054309371924";
const GLOBAL_MESSAGE_ID = "6533373054309371924";
const OLDER_GLOBAL_MESSAGE_ID = "6533373054309371925";
const GIF_BYTES = Uint8Array.from([
  0x47,
  0x49,
  0x46,
  0x38,
  0x39,
  0x61,
  0x00,
]);

test("render is removed while render-file is registered, localized, and documented", () => {
  assert.equal(findSlashCommand("render"), null);
  assert.equal(globalCommands.some(({ name }) => name === "render"), false);

  const command = findSlashCommand("render-file");
  assert.equal(command?.kind, "render-file");
  assert.equal(command?.input, "render-file");
  assert.deepEqual(
    command?.registration.options.map((option) => ({
      type: option.type,
      name: option.name,
      required: option.required,
      max_length: option.max_length,
    })),
    [{ type: 3, name: "image", required: false, max_length: 512 }],
  );

  const registration = globalCommands.find(({ name }) => name === "render-file");
  assert.equal(registration?.name_localizations.ko, "렌더-파일");
  assert.match(registration?.description_localizations.ko, /GIF/);
  const messageCommand = findMessageCommand("Get original GIF");
  assert.equal(messageCommand?.kind, "render-file-message");
  assert.equal(findApplicationCommand(3, "Get original GIF"), messageCommand);
  assert.deepEqual(messageCommand?.registration, {
    type: 3,
    name: "Get original GIF",
    integration_types: [0],
    contexts: [0],
  });
  const messageRegistration = globalCommands.find(({ type }) => type === 3);
  assert.equal(messageRegistration?.name, "Get original GIF");
  assert.equal(messageRegistration?.name_localizations?.ko, "원본 GIF 받기");
  assert.equal(Object.hasOwn(messageRegistration, "description"), false);
  assert.equal(Object.hasOwn(messageRegistration, "options"), false);
  assert.match(formatSlashCommandHelp("render-file", "en"), /message (?:link|ID)/i);
  assert.match(formatSlashCommandHelp("render-file", "ko"), /메시지 (?:ID|링크)/);
  assert.match(formatSlashCommandHelp("render-file", "en"), /Apps.*Get original GIF/);
  assert.match(formatSlashCommandHelp("render-file", "ko"), /답장.*\$render-file/);
  assert.match(formatSlashCommandHelp("", "en"), /`\/render-file`/);
  assert.doesNotMatch(formatSlashCommandHelp("", "en"), /Rendering: `\/render`/);
  assert.match(formatSlashCommandHelp("render", "en"), /Unknown Clearra command/);

  const helpArgument = findSlashCommand("help").registration.options[0];
  assert.equal(helpArgument.type, 3);
  assert.equal(helpArgument.required, false);
  assert.equal(helpArgument.choices, undefined);
  assert.equal(helpArgument.autocomplete, undefined);

  for (const prefix of ["$", ">"] ) {
    assert.equal(parseClearraTextRequest(`${prefix}render anything`, prefix), null);
    const request = parseClearraTextRequest(
      `${prefix}render-file ${EXPLICIT_MESSAGE_ID}`,
      prefix,
    );
    assert.equal(request?.command?.kind, "render-file");
    assert.deepEqual(request?.rawOptions, [{
      name: "image",
      value: EXPLICIT_MESSAGE_ID,
    }]);
  }
});

test("message context command downloads the selected preview without an ID option", async () => {
  const source = renderMessage({
    id: EXPLICIT_MESSAGE_ID,
    ownerId: OTHER_USER_ID,
    attachmentId: "7533373054309371933",
  });
  const fixture = renderFileFixture({ messages: [source] });

  assert.equal(
    await fixture.bot.handleInteraction(messageCommandInteraction(source)),
    true,
  );

  assert.equal(fixture.calls.defers, 1);
  assert.deepEqual(fixture.calls.messageReads, [
    EXPLICIT_MESSAGE_ID,
    EXPLICIT_MESSAGE_ID,
  ]);
  assert.equal(fixture.calls.historyReads.length, 0);
  assert.deepEqual(fixture.calls.downloads, [source.attachments[0].url]);
  assertStandaloneGifFile(fixture.calls.interactionEdits[0]);
  assertNoCompute(fixture.calls);
});

test("message context command rejects a missing resolved target before Discord history", async () => {
  const fixture = renderFileFixture();
  const interaction = messageCommandInteraction(renderMessage({
    id: EXPLICIT_MESSAGE_ID,
    ownerId: REQUESTER_ID,
    attachmentId: "7533373054309371934",
  }));
  interaction.data.resolved.messages = {};

  assert.equal(await fixture.bot.handleInteraction(interaction), true);
  assert.equal(fixture.calls.defers, 1);
  assert.equal(fixture.calls.messageReads.length, 0);
  assert.equal(fixture.calls.historyReads.length, 0);
  assert.match(
    fixture.calls.interactionEdits[0].payload.content,
    /Check the command input/i,
  );
  assertNoCompute(fixture.calls);
});

test("slash render-file resolves an explicit preview message ID into a standalone file component", async () => {
  const source = renderMessage({
    id: EXPLICIT_MESSAGE_ID,
    ownerId: REQUESTER_ID,
    attachmentId: "7533373054309371924",
  });
  const fixture = renderFileFixture({ messages: [source] });

  assert.equal(
    await fixture.bot.handleInteraction(slashInteraction([
      { name: "image", value: EXPLICIT_MESSAGE_ID },
    ])),
    true,
  );

  assert.equal(fixture.calls.defers, 1);
  assert.deepEqual(fixture.calls.messageReads, [
    EXPLICIT_MESSAGE_ID,
    EXPLICIT_MESSAGE_ID,
  ]);
  assert.equal(fixture.calls.historyReads.length, 0);
  assert.deepEqual(fixture.calls.downloads, [source.attachments[0].url]);
  assert.equal(fixture.calls.interactionEdits.length, 1);
  assertStandaloneGifFile(fixture.calls.interactionEdits[0]);
  assertNoCompute(fixture.calls);
});

test("omitted render-file chooses the requester's newest preview before a newer global preview", async () => {
  const newerGlobal = renderMessage({
    id: GLOBAL_MESSAGE_ID,
    ownerId: OTHER_USER_ID,
    attachmentId: "7533373054309371925",
  });
  const requesterPreview = renderMessage({
    id: OWN_MESSAGE_ID,
    ownerId: REQUESTER_ID,
    attachmentId: "7533373054309371926",
  });
  const fixture = renderFileFixture({
    history: [newerGlobal, requesterPreview],
    messages: [newerGlobal, requesterPreview],
  });

  assert.equal(
    await fixture.bot.handleInteraction(slashInteraction([])),
    true,
  );

  assert.deepEqual(fixture.calls.historyReads, [{
    channelId: CHANNEL_ID,
    options: { limit: 100 },
  }]);
  assert.deepEqual(fixture.calls.messageReads, [OWN_MESSAGE_ID]);
  assert.deepEqual(fixture.calls.downloads, [
    requesterPreview.attachments[0].url,
  ]);
  assertStandaloneGifFile(fixture.calls.interactionEdits[0]);
  assertNoCompute(fixture.calls);
});

test("omitted render-file falls back to the newest channel preview when the requester has none", async () => {
  const newest = renderMessage({
    id: GLOBAL_MESSAGE_ID,
    ownerId: OTHER_USER_ID,
    attachmentId: "7533373054309371927",
  });
  const older = renderMessage({
    id: OLDER_GLOBAL_MESSAGE_ID,
    ownerId: "3533373054309371925",
    attachmentId: "7533373054309371928",
  });
  const fixture = renderFileFixture({
    history: [newest, older],
    messages: [newest, older],
  });

  assert.equal(
    await fixture.bot.handleInteraction(slashInteraction([])),
    true,
  );

  assert.deepEqual(fixture.calls.messageReads, [GLOBAL_MESSAGE_ID]);
  assert.deepEqual(fixture.calls.downloads, [newest.attachments[0].url]);
  assertStandaloneGifFile(fixture.calls.interactionEdits[0]);
  assertNoCompute(fixture.calls);
});

test("render-file reports an empty channel and skips an expired implicit candidate", async () => {
  const empty = renderFileFixture();
  assert.equal(await empty.bot.handleInteraction(slashInteraction([])), true);
  assert.match(empty.calls.interactionEdits[0].payload.content, /No recent Clearra GIF/);
  assert.equal(empty.calls.interactionEdits[0].files.length, 0);

  const expiredOwn = renderMessage({
    id: OWN_MESSAGE_ID,
    ownerId: REQUESTER_ID,
    attachmentId: "7533373054309371931",
  });
  const global = renderMessage({
    id: GLOBAL_MESSAGE_ID,
    ownerId: OTHER_USER_ID,
    attachmentId: "7533373054309371932",
  });
  const unavailable = Object.assign(new Error("gone"), { discordStatus: 404 });
  const fallback = renderFileFixture({
    history: [global, expiredOwn],
    messages: [global, expiredOwn],
    downloadErrors: new Map([[expiredOwn.attachments[0].url, unavailable]]),
  });
  assert.equal(await fallback.bot.handleInteraction(slashInteraction([])), true);
  assert.deepEqual(fallback.calls.downloads, [
    expiredOwn.attachments[0].url,
    global.attachments[0].url,
  ]);
  assertStandaloneGifFile(fallback.calls.interactionEdits[0]);
});

test("an explicitly selected expired preview fails without selecting another image", async () => {
  const unavailable = Object.assign(new Error("gone"), { discordStatus: 404 });
  const fixture = renderFileFixture({ missingError: unavailable });
  assert.equal(
    await fixture.bot.handleInteraction(slashInteraction([
      { name: "image", value: EXPLICIT_MESSAGE_ID },
    ])),
    true,
  );
  assert.match(fixture.calls.interactionEdits[0].payload.content, /no longer available/);
  assert.equal(fixture.calls.historyReads.length, 0);
  assert.equal(fixture.calls.downloads.length, 0);
});

test("dollar and greater-than render-file commands send independent file-component messages", async () => {
  for (const [index, prefix] of ["$", ">"].entries()) {
    const source = renderMessage({
      id: EXPLICIT_MESSAGE_ID,
      ownerId: REQUESTER_ID,
      attachmentId: `853337305430937192${index}`,
    });
    const fixture = renderFileFixture({ messages: [source] });
    const commandMessage = {
      id: `953337305430937192${index}`,
      channel_id: CHANNEL_ID,
      guild_id: GUILD_ID,
      content: `${prefix}render-file ${EXPLICIT_MESSAGE_ID}`,
      author: { id: REQUESTER_ID, bot: false },
      attachments: [],
    };

    assert.equal(fixture.bot.acceptsOracleMessage(commandMessage), true);
    assert.equal(await fixture.bot.handleOracleMessage(commandMessage), true);
    assert.equal(fixture.calls.channelMessages.length, 1);
    assert.equal(fixture.calls.channelMessages[0].channelId, CHANNEL_ID);
    assertStandaloneGifFile(fixture.calls.channelMessages[0].outgoing);
    assert.equal(fixture.calls.interactionEdits.length, 0);
    assert.equal(fixture.calls.historyReads.length, 0);
    assertNoCompute(fixture.calls);
  }
});

test("replying with dollar or greater-than render-file selects the replied preview", async () => {
  for (const [index, prefix] of ["$", ">"].entries()) {
    const source = renderMessage({
      id: EXPLICIT_MESSAGE_ID,
      ownerId: OTHER_USER_ID,
      attachmentId: `853337305430937193${index}`,
    });
    const fixture = renderFileFixture({ messages: [source] });
    const commandMessage = {
      id: `953337305430937193${index}`,
      channel_id: CHANNEL_ID,
      guild_id: GUILD_ID,
      content: `${prefix}render-file`,
      author: { id: REQUESTER_ID, bot: false },
      attachments: [],
      message_reference: { message_id: EXPLICIT_MESSAGE_ID },
    };

    assert.equal(fixture.bot.acceptsOracleMessage(commandMessage), true);
    assert.equal(await fixture.bot.handleOracleMessage(commandMessage), true);
    assert.deepEqual(fixture.calls.messageReads, [
      EXPLICIT_MESSAGE_ID,
      EXPLICIT_MESSAGE_ID,
    ]);
    assert.equal(fixture.calls.historyReads.length, 0);
    assert.equal(fixture.calls.channelMessages.length, 1);
    assertStandaloneGifFile(fixture.calls.channelMessages[0].outgoing);
    assert.equal(
      Object.hasOwn(
        fixture.calls.channelMessages[0].outgoing.payload,
        "message_reference",
      ),
      false,
    );
    assertNoCompute(fixture.calls);
  }
});

function renderFileFixture({
  history = [],
  messages = [],
  missingError = null,
  downloadErrors = new Map(),
} = {}) {
  const byId = new Map(messages.map((message) => [message.id, message]));
  const calls = {
    defers: 0,
    historyReads: [],
    messageReads: [],
    downloads: [],
    interactionEdits: [],
    channelMessages: [],
    executor: 0,
    renderer: 0,
    administratorChecks: 0,
  };
  const rest = {
    async deferInteraction() {
      calls.defers += 1;
    },
    async getChannelMessages(channelId, options) {
      calls.historyReads.push({ channelId, options });
      return history;
    },
    async getChannelMessage(channelId, messageId) {
      assert.equal(channelId, CHANNEL_ID);
      calls.messageReads.push(messageId);
      const message = byId.get(messageId);
      if (!message) throw missingError ?? new Error(`missing test message ${messageId}`);
      return message;
    },
    async downloadAttachment(url, maximum) {
      assert.equal(maximum, 1024 * 1024);
      calls.downloads.push(String(url));
      if (downloadErrors.has(String(url))) throw downloadErrors.get(String(url));
      return GIF_BYTES;
    },
    async editOriginalInteraction(_applicationId, _token, outgoing) {
      calls.interactionEdits.push(outgoing);
      return { id: "9533373054309371930", attachments: [] };
    },
    async createChannelMessage(channelId, outgoing) {
      calls.channelMessages.push({ channelId, outgoing });
      return { id: "9533373054309371931", attachments: [] };
    },
  };
  const bot = new Clearrabot(
    rest,
    {
      applicationId: APPLICATION_ID,
      oracleTextEnabled: true,
      oracleRenderEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      maxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      applicationId: APPLICATION_ID,
      botUserId: BOT_USER_ID,
      executor: {
        async execute() {
          calls.executor += 1;
          throw new Error("render-file must not run the Clearra executor");
        },
      },
      gifRenderer: {
        async render() {
          calls.renderer += 1;
          throw new Error("render-file must not invoke the GIF renderer");
        },
        stop() {},
      },
      botAdministratorAuthority: {
        async allows() {
          calls.administratorChecks += 1;
          throw new Error("render-file must not inspect bot administrator authority");
        },
      },
    },
  );
  return { bot, calls };
}

function renderMessage({ id, ownerId, attachmentId }) {
  const filename = "clearra-input-preview.gif";
  return {
    id,
    channel_id: CHANNEL_ID,
    guild_id: GUILD_ID,
    application_id: APPLICATION_ID,
    author: { id: BOT_USER_ID, bot: true },
    interaction_metadata: { user: { id: ownerId } },
    attachments: [{
      id: attachmentId,
      filename,
      content_type: "image/gif",
      size: GIF_BYTES.byteLength,
      url: `https://cdn.discordapp.com/attachments/${CHANNEL_ID}/${attachmentId}/${filename}`,
    }],
  };
}

function slashInteraction(options) {
  return {
    id: "8533373054309371924",
    application_id: APPLICATION_ID,
    token: "interaction-token",
    type: 2,
    guild_id: GUILD_ID,
    channel_id: CHANNEL_ID,
    member: { user: { id: REQUESTER_ID } },
    data: {
      type: 1,
      name: "render-file",
      options,
    },
  };
}

function messageCommandInteraction(source) {
  return {
    id: "8533373054309371926",
    application_id: APPLICATION_ID,
    token: "message-command-token",
    type: 2,
    guild_id: GUILD_ID,
    channel_id: CHANNEL_ID,
    member: { user: { id: REQUESTER_ID } },
    data: {
      type: 3,
      name: "Get original GIF",
      target_id: source.id,
      resolved: { messages: { [source.id]: source } },
    },
  };
}

function assertStandaloneGifFile(outgoing) {
  assert.equal(outgoing.payload.flags, 1 << 15);
  assert.deepEqual(outgoing.payload.allowed_mentions, { parse: [] });
  assert.deepEqual(outgoing.payload.components, [{
    type: 13,
    file: { url: "attachment://clearra-render-original.gif" },
  }]);
  assert.equal(Object.hasOwn(outgoing.payload, "content"), false);
  assert.equal(Object.hasOwn(outgoing.payload, "embeds"), false);
  assert.equal(Object.hasOwn(outgoing.payload, "message_reference"), false);
  assert.equal(outgoing.files.length, 1);
  assert.equal(outgoing.files[0].name, "clearra-render-original.gif");
  assert.equal(outgoing.files[0].contentType, "image/gif");
  assert.deepEqual(outgoing.files[0].bytes, GIF_BYTES);
}

function assertNoCompute(calls) {
  assert.equal(calls.executor, 0);
  assert.equal(calls.renderer, 0);
  assert.equal(calls.administratorChecks, 0);
}
