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
import { decodeViewerDocument } from "../src/viewer/document.mjs";

test("viewer replies carry an internally rendered GIF and Clearra link", async () => {
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

  await bot.handleInteraction(slashInteraction("view", [
    { name: "document", value: source },
  ]));

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

test("CTK3 slash-command attachments are decoded and rendered without text input", async () => {
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

  await bot.handleInteraction(
    slashInteraction(
      "view",
      [{ name: "file", value: "attachment-1" }],
      {
        attachments: {
          "attachment-1": {
        filename: "field.ctk3",
        content_type: CTK3_FILE_MIME_TYPE,
        size: bytes.byteLength,
        url: "https://cdn.discordapp.com/attachments/a/b/field.ctk3",
          },
        },
      },
    ),
  );

  assert.equal(messages.length, 1);
  assert.equal(messages[0].files[0].contentType, "image/gif");
  assert.match(messages[0].payload.content, /ctk=/);
});

test("search output and viewer image remain separate replies", async () => {
  const edits = [];
  const followups = [];
  const jobIds = [];
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
          jobIds.push(options.jobId);
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
  await bot.handleInteraction(slashInteraction("clearra", [
    { name: "command", value: `pc --lines 2 ${source}` },
  ]));
  assert.equal(edits.length, 1);
  assert.equal(followups.length, 1);
  assert.equal(edits[0].files.length, 0);
  assert.match(edits[0].payload.content, /search result/);
  assert.equal(followups[0].files[0].contentType, "image/gif");
  assert.deepEqual(jobIds, ["discord-interaction-id"]);
});

test("tiling-only search warns before Discord output", async () => {
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

  await bot.handleInteraction(slashInteraction("clearra", [
    { name: "command", value: "pc --lines 2 --tiling-only" },
  ]));

  assert.equal(messages.length, 1);
  const content = messages[0].payload.content;
  assert.ok(content.indexOf("WARNING:") < content.indexOf("tiling result"));
  assert.match(content, /cannot be built/);
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

function slashInteraction(name, options, resolved = undefined) {
  return {
    id: "interaction-id",
    token: "interaction-token",
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
