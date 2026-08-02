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
import {
  findSlashCommand,
  globalCommands,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";
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
  "cat-finder",
  "pc-setup",
  "best-setup",
  "dpc-finder",
  "verify",
];

test("slash catalog registers only represented sfinder commands", () => {
  assert.deepEqual(
    slashCommandCatalog.map((command) => command.name),
    ["help", ...REPRESENTED_SFINDER_COMMANDS],
  );
  assert.deepEqual(
    globalCommands.map((command) => command.name),
    ["help", ...REPRESENTED_SFINDER_COMMANDS],
  );
  assert.deepEqual(
    globalCommands[0].options[0].choices.map(({ value }) => value),
    REPRESENTED_SFINDER_COMMANDS,
  );
  for (const command of slashCommandCatalog.filter(({ kind }) => kind === "search")) {
    assert.deepEqual(command.argvPrefix, ["sfinder", command.name]);
    assert.equal(
      command.registration.options.some(({ name }) => name === "arguments"),
      false,
    );
  }
  assert.deepEqual(
    findSlashCommand("path").registration.options.map(({ name }) => name),
    ["field", "next", "options"],
  );
  assert.deepEqual(
    findSlashCommand("cover").registration.options.map(({ name }) => name),
    ["base", "target", "next", "options"],
  );
  assert.deepEqual(
    findSlashCommand("pc-setup").registration.options.map(({ name }) => name),
    ["remaining"],
  );
  assert.equal(globalCommands.some(({ name }) => name === "clearra"), false);
  assert.equal(globalCommands.some(({ name }) => name === "view"), false);
});

test("remote execution inherits the configured Discord output ceiling", () => {
  const bot = new Clearrabot(
    {},
    {
      jobEndpoint: "https://jobs.example.test/jobs",
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
  const bot = new Clearrabot(
    {
      async deferInteraction() {},
      async editOriginalInteraction(_applicationId, _token, message) {
        messages.push(message);
      },
    },
    { maxConcurrentSearches: 1 },
    {
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
  assert.equal(messages.length, 1);
  assert.match(messages[0].payload.content, /Syntax: `\/path field:/);
  assert.match(messages[0].payload.content, /clear=1\.\.6/);
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

test("every registered slash command reaches its fixed sfinder argv prefix", async () => {
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
  assert.deepEqual(
    invocations.map((arguments_) => arguments_.slice(0, 2)),
    REPRESENTED_SFINDER_COMMANDS.map((name) => ["sfinder", name]),
  );
});

test("compute slash output never triggers viewer GIF followups", async () => {
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
  await bot.handleInteraction(slashInteraction("path", [
    { name: "field", value: source },
    { name: "next", value: "*p2" },
    { name: "options", value: "clear=2 hold=avoid" },
  ]));
  assert.equal(edits.length, 1);
  assert.equal(followups.length, 0);
  assert.equal(edits[0].files.length, 0);
  assert.match(edits[0].payload.content, /search result/);
  assert.deepEqual(jobIds, ["discord-interaction-id"]);
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
  ]));

  assert.deepEqual(
    executedArguments.slice(-3),
    ["--format", "json", "--include-solution-data"],
  );
  assert.equal(edits.length, 1);
  assert.equal(edits[0].files.length, 1);
  assert.equal(edits[0].files[0].contentType, CTK3_FILE_MIME_TYPE);
  assert.equal(edits[0].files[0].name, "pc-result.ctk3");
  const document = decodeCtk3(new TextDecoder().decode(edits[0].files[0].bytes));
  assert.deepEqual(document.pages[0].cells.slice(0, 6), [
    "G", "G", "T", "T", "T", "T",
  ]);
  assert.deepEqual(document.pages[0].cells.slice(10, 14), ["I", "I", "I", "I"]);
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
          executions.push(options.jobId);
          if (executions.length === 1) await firstExecution;
          return { exitCode: 0, stdout: options.jobId, stderr: "" };
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

  assert.deepEqual(executions, ["discord-one"]);
  assert.match(
    edits.find(({ token }) => token === "token-three").content,
    /busy/iu,
  );

  releaseFirst();
  await Promise.all([first, second]);
  assert.deepEqual(executions, ["discord-one", "discord-two"]);
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
    /interaction deadline/iu,
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

function validSearchOptions(command, field) {
  switch (command.input) {
    case "pc":
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
    case "remaining":
      return [{ name: "remaining", value: "IOTS" }];
    case "verify":
      return [];
    default:
      throw new Error(`unsupported test command input: ${command.input}`);
  }
}
