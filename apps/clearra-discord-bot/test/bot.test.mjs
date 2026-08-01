import assert from "node:assert/strict";
import test from "node:test";

import { decodeCtk3, encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { Clearrabot } from "../src/bot.mjs";
import { decodeViewerDocument } from "../src/viewer/document.mjs";

test("viewer replies carry an internally rendered GIF and Clearra link", async () => {
  const messages = [];
  const rest = {
    createChannelMessage: async (_channelId, message) => {
      messages.push(message);
    },
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      executable: "clearra",
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

  await bot.handleMessage({
    channel_id: "channel",
    author: { bot: false },
    content: `viewer ${source}`,
  });

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

test("search output and viewer image remain separate replies", async () => {
  const messages = [];
  const rest = {
    createChannelMessage: async (_channelId, message) => {
      messages.push(message);
    },
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      executable: "clearra",
      viewerBaseUrl: "https://example.test/Clearra/",
      searchTimeoutMs: 1000,
      maxGifBytes: 1024 * 1024,
      maxConcurrentSearches: 1,
    },
    {
      executor: {
        execute: async () => ({ exitCode: 0, stdout: "search result", stderr: "" }),
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
  await bot.handleMessage({
    channel_id: "channel",
    author: { bot: false },
    content: `!pc --lines 2 ${source}`,
  });
  assert.equal(messages.length, 2);
  assert.equal(messages[0].files.length, 0);
  assert.match(messages[0].payload.content, /search result/);
  assert.equal(messages[1].files[0].contentType, "image/gif");
});

test("tiling-only search warns before Discord output", async () => {
  const messages = [];
  const rest = {
    createChannelMessage: async (_channelId, message) => {
      messages.push(message);
    },
  };
  const bot = new Clearrabot(
    rest,
    {
      prefix: "!",
      executable: "clearra",
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

  await bot.handleMessage({
    channel_id: "channel",
    author: { bot: false },
    content: "!pc --lines 2 --tiling-only",
  });

  assert.equal(messages.length, 1);
  const content = messages[0].payload.content;
  assert.ok(content.indexOf("WARNING:") < content.indexOf("tiling result"));
  assert.match(content, /cannot be built/);
});
