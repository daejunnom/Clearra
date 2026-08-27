import assert from "node:assert/strict";
import test from "node:test";

import {
  isOracleMessageDispatch,
  OracleMessageIngress,
  oracleGatewayIntents,
} from "../src/ingress/oracle-message-ingress.mjs";

const RENDER_INTENTS = (1 << 9) | (1 << 12);
const TEXT_INTENTS = RENDER_INTENTS | (1 << 15);

test("Oracle routes only message creates and updates from the Gateway", () => {
  assert.equal(isOracleMessageDispatch("MESSAGE_CREATE"), true);
  assert.equal(isOracleMessageDispatch("MESSAGE_UPDATE"), true);
  assert.equal(isOracleMessageDispatch("INTERACTION_CREATE"), false);
  assert.equal(isOracleMessageDispatch("MESSAGE_DELETE"), false);
});

test("Oracle Gateway requests message intents only when a message feature is enabled", () => {
  assert.equal(oracleGatewayIntents({}), 0);
  assert.equal(
    oracleGatewayIntents({ oracleRenderEnabled: true }),
    RENDER_INTENTS,
  );
  assert.equal(oracleGatewayIntents({ oracleTextEnabled: true }), TEXT_INTENTS);
});

test("Oracle renderer accepts self results everywhere and user documents only by explicit invocation", async () => {
  const handled = [];
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        handled.push(candidate.id);
      },
    },
    { oracleAllowedChannelIds: [] },
  );
  ingress.setBotUserId("clearra-bot");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("self-result", {
        channel_id: "unregistered-channel",
        guild_id: "guild-1",
        author: { id: "clearra-bot", bot: true },
        webhook_id: "interaction-webhook",
      }),
    ),
    { accepted: true },
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("mentioned", {
        guild_id: "guild-1",
        mentions: [{ id: "clearra-bot" }],
      }),
    ),
    { accepted: true },
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("spoofed", {
        guild_id: "guild-1",
        content: "<@clearra-bot> view",
        mentions: [],
      }),
    ),
    { accepted: false, reason: "explicit-invocation-required" },
  );
  assert.deepEqual(handled, ["self-result", "mentioned"]);
});

test("Oracle ingress rejects events outside its bounded message boundary", async () => {
  let acceptanceCalls = 0;
  const ingress = createIngress(
    {
      acceptsOracleMessage() {
        acceptanceCalls += 1;
        return true;
      },
    },
    { oracleMaxInputChars: 4 },
  );

  const cases = [
    ["INTERACTION_CREATE", message("event"), "gateway-message-events-disabled"],
    [
      "MESSAGE_CREATE",
      message("channel", { channel_id: "elsewhere" }),
      "channel-not-allowed",
    ],
    [
      "MESSAGE_CREATE",
      message("long", { content: "12345" }),
      "message-too-long",
    ],
    [
      "MESSAGE_CREATE",
      message("bot", { author: { id: "other-bot", bot: true } }),
      "bot-message",
    ],
    [
      "MESSAGE_CREATE",
      message("webhook", { webhook_id: "webhook-1" }),
      "webhook-message",
    ],
  ];

  for (const [type, candidate, reason] of cases) {
    assert.deepEqual(await ingress.acceptDispatch(type, candidate), {
      accepted: false,
      reason,
    });
  }
  assert.equal(acceptanceCalls, 0);
});

test("Oracle ingress stays disabled unless render or text ingress is enabled", async () => {
  const ingress = new OracleMessageIngress(handler(), {
    oracleAllowedChannelIds: ["allowed"],
  });
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("disabled")),
    { accepted: false, reason: "oracle-message-events-disabled" },
  );
});

test("Oracle ingress delegates only supported messages and deduplicates accepted ids", async () => {
  const handled = [];
  let observedBotUserId;
  const ingress = createIngress({
    acceptsOracleMessage(candidate, context) {
      observedBotUserId = context.botUserId;
      return candidate.content === "render";
    },
    async handleOracleMessage(candidate) {
      handled.push(candidate.id);
    },
  });
  ingress.setBotUserId("clearra-bot");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("unsupported", { content: "ordinary chat" }),
    ),
    { accepted: false, reason: "unsupported-message" },
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("supported", { content: "render" }),
    ),
    { accepted: true },
  );
  assert.equal(observedBotUserId, "clearra-bot");
  assert.deepEqual(handled, ["supported"]);
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("supported", { content: "render" }),
    ),
    { accepted: false, reason: "duplicate-message" },
  );
});

test("Sfinder-man guild ownership delegates search and ambient render before local work", async () => {
  const handled = [];
  const outcomes = [];
  const ingress = createIngress(
    {
      acceptsOracleMessage(candidate) {
        return candidate.content.startsWith("$path") ||
          candidate.content.startsWith("v115@") ||
          candidate.content.startsWith("ctk3_") ||
          candidate.content === "__________" ||
          candidate.attachments.some(
            (attachment) => attachment.filename === "input.ctk3",
          );
      },
      async handleOracleMessage(candidate) {
        handled.push(candidate.id);
      },
      async observeOracleMessageOutcome(candidate, outcome) {
        outcomes.push([candidate.id, outcome]);
      },
    },
    {
      oracleTextEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      oracleSfinderManGuildIds: ["sfinder-guild"],
      oracleMaxConcurrentMessages: 1,
      oracleMaxPendingMessages: 0,
    },
  );

  for (const [id, content] of [
    ["delegated-search", "$path --field __________ --next I"],
    ["delegated-unknown-prefix", "$owned-by-sfinder-man"],
    ["delegated-render", "v115@vhAAgH"],
    ["delegated-ctk3", "ctk3_FAKE"],
    ["delegated-grid", "__________"],
  ]) {
    assert.deepEqual(
      await ingress.acceptDispatch(
        "MESSAGE_CREATE",
        message(id, { guild_id: "sfinder-guild", content }),
      ),
      { accepted: false, reason: "delegated", owner: "sfinder-man" },
    );
  }
  assert.deepEqual(handled, []);
  assert.deepEqual(outcomes, [
    ["delegated-search", {
      handled: false,
      status: "delegated",
      owner: "sfinder-man",
      reason: "guild-owner",
    }],
    ["delegated-unknown-prefix", {
      handled: false,
      status: "delegated",
      owner: "sfinder-man",
      reason: "guild-owner",
    }],
    ["delegated-render", {
      handled: false,
      status: "delegated",
      owner: "sfinder-man",
      reason: "guild-owner",
    }],
    ["delegated-ctk3", {
      handled: false,
      status: "delegated",
      owner: "sfinder-man",
      reason: "guild-owner",
    }],
    ["delegated-grid", {
      handled: false,
      status: "delegated",
      owner: "sfinder-man",
      reason: "guild-owner",
    }],
  ]);

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("delegated-attachment", {
        guild_id: "sfinder-guild",
        content: "",
        attachments: [{ filename: "input.ctk3" }],
      }),
    ),
    { accepted: false, reason: "delegated", owner: "sfinder-man" },
  );
  assert.equal(outcomes.at(-1)[0], "delegated-attachment");
  assert.equal(outcomes.at(-1)[1].status, "delegated");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("delegated-search", {
        guild_id: "sfinder-guild",
        content: "$path --field __________ --next I",
      }),
    ),
    { accepted: false, reason: "duplicate-message" },
  );
  assert.equal(outcomes.length, 6);

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("standalone-render", {
        guild_id: "clearra-guild",
        content: "v115@vhAAgH",
      }),
    ),
    { accepted: true },
  );
  assert.deepEqual(handled, ["standalone-render"]);
  assert.equal(outcomes.at(-1)[1].status, "succeeded");
});

test("Oracle ingress owns one privacy-bounded terminal log for text outcomes", async () => {
  const lines = [];
  let now = 100;
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        if (candidate.id === "failed-log") throw new Error("private detail");
      },
    },
    {
      oracleTextEnabled: true,
      oracleSfinderManGuildIds: ["sfinder-guild"],
    },
    {
      operationalScope: "gateway",
      now: () => now += 5,
      logger: {
        info(value) { lines.push(value); },
        error(value) { lines.push(value); },
      },
    },
  );

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("delegated-log", {
        guild_id: "sfinder-guild",
        content: "$path PRIVATE",
      }),
    ),
    { accepted: false, reason: "delegated", owner: "sfinder-man" },
  );
  await assert.rejects(
    ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("failed-log", { content: "$path PRIVATE" }),
    ),
    /Oracle message handling failed/,
  );

  assert.deepEqual(lines.map((line) => {
    const record = JSON.parse(line);
    return [record.kind, record.command, record.status, record.durationMs];
  }), [
    ["text", "pc.path", "delegated", 5],
    ["text", "pc.path", "failed", 5],
  ]);
  assert.doesNotMatch(lines.join("\n"), /PRIVATE|sfinder-man|guild-owner/i);

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("delegated-render-log", {
        guild_id: "sfinder-guild",
        content: "v115@vhAAgH",
      }),
    ),
    { accepted: false, reason: "delegated", owner: "sfinder-man" },
  );
  const renderRecord = JSON.parse(lines.at(-1));
  assert.equal(renderRecord.kind, "render");
  assert.equal(renderRecord.command, null);
  assert.equal(renderRecord.status, "delegated");
});

test("Sfinder-man guild ownership never delegates Clearra self results", async () => {
  const handled = [];
  const outcomes = [];
  const ingress = createIngress(
    {
      acceptsOracleMessage(candidate) {
        return candidate.attachments.some(
          (attachment) => attachment.filename === "path-result.ctk3",
        );
      },
      async handleOracleMessage(candidate) {
        handled.push(candidate.id);
      },
      async observeOracleMessageOutcome(candidate, outcome) {
        outcomes.push([candidate.id, outcome.status]);
      },
    },
    {
      oracleSfinderManGuildIds: ["sfinder-guild"],
    },
  ).setBotUserId("clearra-bot");

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("self-result", {
        guild_id: "sfinder-guild",
        author: { id: "clearra-bot", bot: true },
        webhook_id: "interaction-webhook",
        content: "",
        attachments: [{ filename: "path-result.ctk3" }],
      }),
    ),
    { accepted: true },
  );
  assert.deepEqual(handled, ["self-result"]);
  assert.deepEqual(outcomes, [["self-result", "succeeded"]]);
});

test("Sfinder-man ownership keeps Clearra-only render-file and management text local", async () => {
  const handled = [];
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        handled.push(candidate.id);
      },
    },
    {
      oracleTextEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      oracleSfinderManGuildIds: ["sfinder-guild"],
    },
  );

  for (const [id, content] of [
    ["render-file", "$render-file"],
    ["management", ">bot-control help"],
  ]) {
    assert.deepEqual(
      await ingress.acceptDispatch(
        "MESSAGE_CREATE",
        message(id, { guild_id: "sfinder-guild", content }),
      ),
      { accepted: true },
    );
  }
  assert.deepEqual(handled, ["render-file", "management"]);
});

test("Oracle ingress admits only complete self-webhook message updates", async () => {
  let acceptanceCalls = 0;
  const ingress = createIngress({
    acceptsOracleMessage(candidate) {
      acceptanceCalls += 1;
      return candidate.attachments.some(
        (attachment) => attachment.filename === "pc-result.ctk3",
      );
    },
  });
  ingress.setBotUserId("clearra-bot");

  const complete = message("updated-result", {
    author: { id: "clearra-bot", bot: true },
    webhook_id: "interaction-webhook",
    attachments: [{ filename: "pc-result.ctk3" }],
  });
  const cases = [
    [
      message("external-update", {
        author: { id: "external-user", bot: false },
        webhook_id: "external-webhook",
        attachments: [{ filename: "pc-result.ctk3" }],
      }),
      "message-update-not-self",
    ],
    [
      message("non-webhook-update", {
        author: { id: "clearra-bot", bot: true },
        attachments: [{ filename: "pc-result.ctk3" }],
      }),
      "message-update-not-webhook",
    ],
    [
      message("attachment-free-update", {
        author: { id: "clearra-bot", bot: true },
        webhook_id: "interaction-webhook",
        attachments: [],
      }),
      "message-update-without-attachments",
    ],
  ];

  for (const [candidate, reason] of cases) {
    assert.deepEqual(await ingress.acceptDispatch("MESSAGE_UPDATE", candidate), {
      accepted: false,
      reason,
    });
  }
  assert.equal(acceptanceCalls, 0);

  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_UPDATE", {
      ...complete,
      id: "non-ctk3-update",
      attachments: [{ filename: "preview.gif" }],
    }),
    { accepted: false, reason: "unsupported-message" },
  );
  assert.equal(acceptanceCalls, 1);
});

test("Oracle ingress hydrates only author-omitted attachment webhook updates", async () => {
  const channelId = "123456789012345678";
  const messageId = "234567890123456789";
  let fetches = 0;
  let acceptanceCalls = 0;
  const ingress = createIngress(
    {
      acceptsOracleMessage() {
        acceptanceCalls += 1;
        return true;
      },
    },
    {},
    {
      async fetchMessage(candidateChannelId, candidateMessageId) {
        fetches += 1;
        assert.equal(candidateChannelId, channelId);
        assert.equal(candidateMessageId, messageId);
        return {
          id: messageId,
          channel_id: channelId,
          content: "",
          author: { id: "external-bot", bot: true },
          webhook_id: "interaction-webhook",
          attachments: [{ filename: "pc-result.ctk3" }],
        };
      },
    },
  );
  ingress.setBotUserId("clearra-bot");

  const partial = {
    id: messageId,
    channel_id: channelId,
    webhook_id: "interaction-webhook",
    attachments: [{ filename: "pc-result.ctk3" }],
  };
  assert.deepEqual(await ingress.acceptDispatch("MESSAGE_UPDATE", partial), {
    accepted: false,
    reason: "message-update-not-self",
  });
  assert.equal(fetches, 1);
  assert.equal(acceptanceCalls, 0);

  for (const incomplete of [
    { ...partial, id: "345678901234567890", webhook_id: undefined },
    { ...partial, id: "456789012345678901", attachments: [] },
  ]) {
    assert.deepEqual(await ingress.acceptDispatch("MESSAGE_UPDATE", incomplete), {
      accepted: false,
      reason: "invalid-message",
    });
  }
  assert.equal(fetches, 1);
});

test("Oracle ingress generalizes message hydration failures", async () => {
  const ingress = createIngress({}, {}, {
    async fetchMessage() {
      throw new Error("sensitive upstream detail");
    },
  });
  ingress.setBotUserId("clearra-bot");

  await assert.rejects(
    ingress.acceptDispatch("MESSAGE_UPDATE", {
      id: "234567890123456789",
      channel_id: "123456789012345678",
      webhook_id: "interaction-webhook",
      attachments: [{ filename: "pc-result.ctk3" }],
    }),
    (error) =>
      error instanceof Error &&
      error.message === "Oracle message update hydration failed.",
  );
});

test("Oracle ingress rate limits users but exempts its own output messages", async () => {
  const handled = [];
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        handled.push(candidate.id);
      },
    },
    { oracleUserCooldownMs: 60_000 },
  );
  ingress.setBotUserId("clearra-bot");

  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("user-1")),
    { accepted: true },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("user-2")),
    { accepted: false, reason: "user-cooldown" },
  );

  for (const id of ["self-1", "self-2"]) {
    assert.deepEqual(
      await ingress.acceptDispatch(
        "MESSAGE_CREATE",
        message(id, {
          author: { id: "clearra-bot", bot: true },
          webhook_id: id === "self-2" ? "interaction-webhook" : undefined,
        }),
      ),
      { accepted: true },
    );
  }
  assert.deepEqual(handled, ["user-1", "self-1", "self-2"]);
});

test("Oracle ingress bounds concurrency and its FIFO pending queue", async () => {
  const starts = [];
  const releases = [];
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        starts.push(candidate.id);
        await new Promise((resolve) => releases.push(resolve));
      },
    },
    {
      oracleMaxConcurrentMessages: 1,
      oracleMaxPendingMessages: 1,
    },
  );

  const first = ingress.acceptDispatch("MESSAGE_CREATE", message("first"));
  const second = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("second", { author: { id: "user-2" } }),
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("overflow", { author: { id: "user-3" } }),
    ),
    { accepted: false, reason: "message-queue-full" },
  );
  assert.deepEqual(starts, ["first"]);

  releases.shift()();
  assert.deepEqual(await first, { accepted: true });
  await Promise.resolve();
  assert.deepEqual(starts, ["first", "second"]);

  releases.shift()();
  assert.deepEqual(await second, { accepted: true });
});

test("Oracle ingress holds its slot until full handling settles when a begin hook exists", async () => {
  const starts = [];
  const releases = [];
  let beginCalls = 0;
  const ingress = createIngress(
    {
      async beginOracleMessage() {
        beginCalls += 1;
      },
      async handleOracleMessage(candidate) {
        starts.push(candidate.id);
        await new Promise((resolve) => releases.push(resolve));
      },
    },
    {
      oracleMaxConcurrentMessages: 1,
      oracleMaxPendingMessages: 1,
    },
  );

  const first = ingress.acceptDispatch("MESSAGE_CREATE", message("first-with-begin"));
  const second = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("second-with-begin", { author: { id: "user-2" } }),
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("overflow-with-begin", { author: { id: "user-3" } }),
    ),
    { accepted: false, reason: "message-queue-full" },
  );
  assert.equal(beginCalls, 0);
  assert.deepEqual(starts, ["first-with-begin"]);

  releases.shift()();
  assert.deepEqual(await first, { accepted: true });
  await Promise.resolve();
  assert.deepEqual(starts, ["first-with-begin", "second-with-begin"]);

  releases.shift()();
  assert.deepEqual(await second, { accepted: true });
});

test("Oracle ingress reserves a bounded priority queue for self result rendering", async () => {
  const starts = [];
  const releases = [];
  const ingress = createIngress(
    {
      async handleOracleMessage(candidate) {
        starts.push(candidate.id);
        await new Promise((resolve) => releases.push(resolve));
      },
    },
    {
      oracleMaxConcurrentMessages: 1,
      oracleMaxPendingMessages: 1,
      oracleMaxPendingSelfMessages: 1,
    },
  );
  ingress.setBotUserId("clearra-bot");

  const activeUser = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("active-user"),
  );
  const pendingUser = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("pending-user", { author: { id: "user-2" } }),
  );
  const pendingSelf = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("pending-self", { author: { id: "clearra-bot", bot: true } }),
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("self-overflow", { author: { id: "clearra-bot", bot: true } }),
    ),
    { accepted: false, reason: "message-queue-full" },
  );

  releases.shift()();
  assert.deepEqual(await activeUser, { accepted: true });
  await Promise.resolve();
  assert.deepEqual(starts, ["active-user", "pending-self"]);

  releases.shift()();
  assert.deepEqual(await pendingSelf, { accepted: true });
  await Promise.resolve();
  assert.deepEqual(starts, ["active-user", "pending-self", "pending-user"]);

  releases.shift()();
  assert.deepEqual(await pendingUser, { accepted: true });
});

test("Oracle ingress releases a worker slot when a handler fails", async () => {
  let calls = 0;
  const ingress = createIngress(
    {
      async handleOracleMessage() {
        calls += 1;
        if (calls === 1) throw new Error("safe handler failure");
      },
    },
    {
      oracleMaxConcurrentMessages: 1,
      oracleMaxPendingMessages: 1,
    },
  );

  const failed = ingress.acceptDispatch("MESSAGE_CREATE", message("failed"));
  const next = ingress.acceptDispatch(
    "MESSAGE_CREATE",
    message("next", { author: { id: "user-2" } }),
  );
  await assert.rejects(failed, /Oracle message handling failed/);
  assert.deepEqual(await next, { accepted: true });
  assert.equal(calls, 2);
});

test("Oracle ingress preserves delegated, failed, and cancelled handler outcomes exactly once", async () => {
  const outcomes = [];
  const ingress = createIngress({
    async handleOracleMessage(candidate) {
      if (candidate.id === "legacy-delegated") return false;
      if (candidate.id === "typed-delegated") {
        return { outcome: "delegated", owner: "sfinder-man" };
      }
      if (candidate.id === "typed-handled") {
        return { outcome: "handled" };
      }
      if (candidate.id === "invalid-typed") {
        return { handled: true, status: "succeeded" };
      }
      if (candidate.id === "cancelled") {
        const error = new Error("cancelled");
        error.name = "AbortError";
        throw error;
      }
      if (candidate.id === "failed") throw new Error("private failure");
      return true;
    },
    async observeOracleMessageOutcome(candidate, outcome) {
      outcomes.push([candidate.id, outcome]);
      if (candidate.id === "observer-failure") {
        throw new Error("observer persistence failed");
      }
    },
  }, {
    oracleMaxConcurrentMessages: 1,
    oracleMaxPendingMessages: 1,
  });

  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("legacy-delegated"),
    ),
    { accepted: false, reason: "delegated" },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("typed-delegated")),
    { accepted: false, reason: "delegated", owner: "sfinder-man" },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("typed-handled")),
    { accepted: true },
  );
  await assert.rejects(
    ingress.acceptDispatch("MESSAGE_CREATE", message("invalid-typed")),
    /Oracle message handling failed/,
  );
  await assert.rejects(
    ingress.acceptDispatch("MESSAGE_CREATE", message("cancelled")),
    /Oracle message handling failed/,
  );
  await assert.rejects(
    ingress.acceptDispatch("MESSAGE_CREATE", message("failed")),
    /Oracle message handling failed/,
  );
  assert.deepEqual(
    await ingress.acceptDispatch(
      "MESSAGE_CREATE",
      message("observer-failure"),
    ),
    { accepted: true },
  );
  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", message("after-observer")),
    { accepted: true },
  );

  assert.deepEqual(
    outcomes.map(([id, outcome]) => [id, outcome.status, outcome.reason ?? null]),
    [
      ["legacy-delegated", "delegated", "handler-delegated"],
      ["typed-delegated", "delegated", "handler-delegated"],
      ["typed-handled", "succeeded", null],
      ["invalid-typed", "failed", "handler-failed"],
      ["cancelled", "cancelled", "handler-cancelled"],
      ["failed", "failed", "handler-failed"],
      ["observer-failure", "succeeded", null],
      ["after-observer", "succeeded", null],
    ],
  );
});

function createIngress(overrides = {}, config = {}, dependencies = {}) {
  return new OracleMessageIngress(handler(overrides), {
    oracleRenderEnabled: true,
    oracleTextEnabled: false,
    oracleAllowedChannelIds: ["allowed"],
    oracleMaxInputChars: 2_000,
    oracleMaxConcurrentMessages: 2,
    oracleMaxPendingMessages: 2,
    oracleMaxPendingSelfMessages: 2,
    oracleUserCooldownMs: 0,
    ...config,
  }, dependencies);
}

function handler(overrides = {}) {
  return {
    acceptsOracleMessage() {
      return true;
    },
    async handleOracleMessage() {},
    ...overrides,
  };
}

function message(id, overrides = {}) {
  return {
    id,
    channel_id: "allowed",
    content: "view",
    author: { id: "user-1" },
    ...overrides,
  };
}
