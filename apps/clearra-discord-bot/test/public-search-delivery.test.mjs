import assert from "node:assert/strict";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";
import { platformDeadlineError } from "../src/job-service/platform-deadline.mjs";

function interaction(ageMs = 0) {
  return {
    type: 2,
    id: ((BigInt(Date.now() - ageMs) - 1_420_070_400_000n) << 22n).toString(),
    application_id: "100000000000000001", token: "fixture-token",
    channel_id: "100000000000000002", guild_id: "100000000000000003",
    data: { name: "path", type: 1, options: [] },
  };
}
const outgoing = (content, files = []) => ({ payload: { content, allowed_mentions: { parse: [] } }, files });
const result = { exitCode: 0, stdout: "completed", stderr: "" };
function expired() { return Object.assign(new Error("expired"), { discordCode: 10015, discordStatus: 404 }); }

function createBot(rest, executor = { async execute() { return result; } }, options = {}) {
  return new Clearrabot(rest, { maxConcurrentSearches: 1, searchTimeoutMs: 0 }, {
    executor, logger: { error() {}, warn() {} }, ...options,
  });
}

test("an admitted public search can finish after the interaction token window", async () => {
  const sends = [];
  let executions = 0;
  const bot = createBot({
    async editOriginalInteraction() { assert.fail("expired webhook must not be used"); },
    async createChannelMessage(channel, message) { sends.push({ channel, message }); return { id: "posted" }; },
  }, { async execute(_arguments, options) {
    executions++;
    assert.equal(options.deadlineUnixMs, 0);
    return result;
  } });
  const request = interaction(900_000);
  await bot.runInteractionCommand(request, ["pc", "path"]);
  assert.equal(executions, 1);
  assert.equal(sends.length, 1);
  assert.equal(sends[0].channel, request.channel_id);
  assert.match(sends[0].message.payload.content, /completed/u);
  bot.stop();
});

test("a Cloud boundary failure is delivered once to the public channel after token expiry", async () => {
  const sends = [];
  const bot = createBot({
    async createChannelMessage(_channel, message) { sends.push(message); return { id: "failed" }; },
  }, { async execute() { throw platformDeadlineError(); } });
  await bot.runInteractionCommand(interaction(900_000), ["pc", "path"], null, "ko");
  assert.equal(sends.length, 1);
  assert.match(sends[0].payload.content, /Cloud Run/u);
  assert.match(sends[0].payload.content, /실패/u);
  bot.stop();
});

test("fallback reuploads preview bytes without referencing another message's attachment IDs", async () => {
  const sends = [];
  let edits = 0;
  let finish;
  const pending = new Promise((resolve) => { finish = resolve; });
  const previewFile = { name: "preview.gif", bytes: new Uint8Array([1, 2, 3]) };
  const resultFile = { name: "result.ctk3", bytes: new Uint8Array([4, 5]) };
  const bot = createBot({
    async editOriginalInteraction() {
      edits++;
      if (edits === 1) {
        finish(outgoing("finished", [resultFile]));
        return { attachments: [{ id: "old-attachment", filename: "preview.gif" }] };
      }
      throw expired();
    },
    async createChannelMessage(_channel, message) { sends.push(message); return { id: "new-message" }; },
  });
  const request = interaction();
  bot.beginSearchDelivery(request);
  await bot.deliverInteractionResult(request, pending, Promise.resolve(outgoing("preview", [previewFile])));
  assert.equal(edits, 2);
  assert.equal(sends.length, 1);
  assert.deepEqual(sends[0].files, [previewFile, resultFile]);
  assert.equal(sends[0].payload.attachments, undefined);
  bot.stop();
});

test("private management and access-denial failures never fall back to a public channel", async () => {
  let sends = 0;
  const defers = [];
  const bot = createBot({
    async deferInteraction(_interaction, options) { defers.push(options); },
    async editOriginalInteraction() { throw expired(); },
    async createChannelMessage() { sends++; },
  }, undefined, { accessPreferences: { isGuildPaused: () => true } });
  await assert.rejects(bot.handleInteraction(interaction(900_000)), /expired/u);
  assert.deepEqual(defers, [{ ephemeral: true }]);
  await assert.rejects(bot.handleInteractionFailure(interaction(900_000), new Error("private details")), /expired/u);
  assert.equal(sends, 0);
  bot.stop();
});

test("an explicitly private message and ambiguous delivery error cannot become a public duplicate", async () => {
  let sends = 0;
  const bot = createBot({
    async editOriginalInteraction() { throw expired(); },
    async createChannelMessage() { sends++; },
  });
  const request = interaction(900_000);
  bot.beginSearchDelivery(request);
  const privateMessage = outgoing("private");
  privateMessage.payload.flags = 64;
  await assert.rejects(bot.editSearchInteraction(request, privateMessage), /expired/u);
  assert.equal(sends, 0);
  const ambiguous = createBot({
    async editOriginalInteraction() { throw Object.assign(expired(), { discordAmbiguous: true }); },
    async createChannelMessage() { sends++; },
  });
  const current = interaction();
  ambiguous.beginSearchDelivery(current);
  await assert.rejects(ambiguous.editSearchInteraction(current, outgoing("result")), /expired/u);
  assert.equal(sends, 0);
  bot.stop(); ambiguous.stop();
});

test("late series followups use the admitted channel without rerunning prior work", async () => {
  const sends = [];
  let executions = 0;
  const bot = createBot({
    async createChannelMessage(channel, message) { sends.push({ channel, message }); return { id: `message-${sends.length}` }; },
  }, { async execute() { executions++; return result; } });
  const request = interaction(900_000);
  await bot.runInteractionCommandSeries(request, [["pc", "path", "--lines", "2"], ["pc", "path", "--lines", "4"]]);
  assert.equal(executions, 2);
  assert.equal(sends.length, 2);
  assert.ok(sends.every(({ channel }) => channel === request.channel_id));
  bot.stop();
});
