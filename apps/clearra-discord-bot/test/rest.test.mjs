import assert from "node:assert/strict";
import test from "node:test";

import {
  DiscordRestClient,
  fileComponentMessage,
} from "../src/discord/rest.mjs";
import { RestInteractionAcknowledger } from "../src/discord/interaction-acknowledger.mjs";

test("Discord attachment downloads are host-restricted and byte-bounded", async () => {
  const expected = new TextEncoder().encode("ctk3_file");
  const client = new DiscordRestClient("token", async (url, options) => {
    assert.equal(
      String(url),
      "https://cdn.discordapp.com/attachments/channel/file/document.ctk3",
    );
    assert.equal(options.redirect, "error");
    return new Response(expected, {
      headers: { "content-length": String(expected.byteLength) },
    });
  });

  const bytes = await client.downloadAttachment(
    "https://cdn.discordapp.com/attachments/channel/file/document.ctk3",
    1024,
  );
  assert.deepEqual(bytes, expected);
  await assert.rejects(
    client.downloadAttachment("https://example.test/document.ctk3", 1024),
    /not trusted/,
  );
});

test("Discord attachment streaming stops at the configured limit", async () => {
  const client = new DiscordRestClient(
    "token",
    async () => new Response(new Uint8Array(32)),
  );
  await assert.rejects(
    client.downloadAttachment(
      "https://media.discordapp.net/attachments/channel/file/document.ctk3",
      16,
    ),
    /too large/,
  );
});

test("Discord webhook requests do not require a bot token", async () => {
  let authorization;
  const client = new DiscordRestClient(null, async (_url, options) => {
    authorization = options.headers.get("authorization");
    return new Response(null, { status: 204 });
  });

  await client.editOriginalInteraction("application", "interaction", {
    payload: { content: "done" },
    files: [],
  });
  assert.equal(authorization, null);
  await assert.rejects(client.application(), /DISCORD_TOKEN is required/);
});

test("Discord Gateway interactions can send a Modal as the initial REST callback", async () => {
  const interaction = { id: "interaction", token: "interaction-token" };
  const modal = {
    type: 9,
    data: { custom_id: "clearra:search:v2:path", title: "path", components: [] },
  };
  const client = new DiscordRestClient(null, async (url, options) => {
    assert.equal(
      String(url),
      "https://discord.com/api/v10/interactions/interaction/interaction-token/callback",
    );
    assert.equal(options.method, "POST");
    assert.equal(options.headers.get("authorization"), null);
    assert.deepEqual(JSON.parse(options.body), modal);
    return new Response(null, { status: 204 });
  });

  await client.createInteractionResponse(interaction, modal);
});

test("Discord settings interactions defer ephemerally", async () => {
  const interaction = { id: "interaction", token: "interaction-token" };
  const client = new DiscordRestClient(null, async (_url, options) => {
    assert.deepEqual(JSON.parse(options.body), {
      type: 5,
      data: { flags: 64 },
    });
    return new Response(null, { status: 204 });
  });

  await client.deferInteraction(interaction, { ephemeral: true });
});

test("Discord global command reads request complete localization dictionaries", async () => {
  const client = new DiscordRestClient("token", async (url, options) => {
    assert.equal(
      String(url),
      "https://discord.com/api/v10/applications/123456789012345678/commands?with_localizations=true",
    );
    assert.equal(options.method, "GET");
    assert.equal(options.headers.get("authorization"), "Bot token");
    return Response.json([]);
  });

  assert.deepEqual(
    await client.getGlobalCommands("123456789012345678"),
    [],
  );
});

test("Discord channel message edits PATCH retained attachments without reuploading", async () => {
  const client = new DiscordRestClient("token", async (url, options) => {
    assert.equal(
      String(url),
      "https://discord.com/api/v10/channels/channel/messages/preview",
    );
    assert.equal(options.method, "PATCH");
    assert.equal(options.headers.get("authorization"), "Bot token");
    assert.equal(options.headers.get("content-type"), "application/json");
    assert.deepEqual(JSON.parse(options.body), {
      content: "completed",
      allowed_mentions: { parse: [] },
      attachments: [
        {
          id: "attachment-id",
          filename: "clearra-preview.gif",
          description: "Oracle Fumen and CTK3 preview",
        },
      ],
    });
    return Response.json({ id: "preview" });
  });

  const edited = await client.editChannelMessage("channel", "preview", {
    payload: {
      content: "completed",
      allowed_mentions: { parse: [] },
      attachments: [
        {
          id: "attachment-id",
          filename: "clearra-preview.gif",
          description: "Oracle Fumen and CTK3 preview",
        },
      ],
    },
    files: [],
  });
  assert.deepEqual(edited, { id: "preview" });
});

test("Discord multipart edits retain an existing GIF while uploading a CTK3 result", async () => {
  const client = new DiscordRestClient("token", async (_url, options) => {
    assert.ok(options.body instanceof FormData);
    assert.deepEqual(JSON.parse(options.body.get("payload_json")), {
      content: "completed",
      attachments: [
        { id: "gif-id", filename: "clearra-input-preview.gif" },
        {
          id: 0,
          filename: "pc-result.ctk3",
          description: "Clearra CTK3 result",
        },
      ],
    });
    assert.ok(options.body.get("files[0]") instanceof Blob);
    return Response.json({ id: "preview" });
  });

  await client.editChannelMessage("channel", "preview", {
    payload: {
      content: "completed",
      attachments: [
        { id: "gif-id", filename: "clearra-input-preview.gif" },
      ],
    },
    files: [{
      name: "pc-result.ctk3",
      description: "Clearra CTK3 result",
      contentType: "application/x-clearra-ctk3",
      bytes: new Uint8Array([1, 2, 3]),
    }],
  });
});

test("Discord initial callbacks do not retry an ambiguous server failure", async () => {
  let requests = 0;
  const client = new DiscordRestClient(null, async () => {
    requests += 1;
    return new Response("failed", { status: 500 });
  });

  await assert.rejects(
    client.deferInteraction({ id: "interaction", token: "token" }),
    /Discord API 500/,
  );
  assert.equal(requests, 1);
});

test("Discord callback error 40060 loses the distributed defer claim", async () => {
  const client = new DiscordRestClient(null, async () =>
    Response.json(
      { message: "Interaction has already been acknowledged.", code: 40060 },
      { status: 400 },
    )
  );
  const acknowledger = new RestInteractionAcknowledger(client);

  assert.equal(
    await acknowledger.claimDeferred({ id: "interaction", token: "token" }),
    false,
  );
});

test("Discord non-idempotent command and message writes are never replayed", async () => {
  const operations = [
    (client) => client.registerGlobalCommands(
      "1533373054309371924",
      [{ type: 1, name: "help", description: "Help" }],
    ),
    (client) => client.createInteractionFollowup(
      "1533373054309371924",
      "interaction-token",
      { payload: { content: "result" }, files: [] },
    ),
    (client) => client.createChannelMessage(
      "2533373054309371924",
      { payload: { content: "result" }, files: [] },
    ),
  ];

  for (const operation of operations) {
    let requests = 0;
    const client = new DiscordRestClient("token", async () => {
      requests += 1;
      return new Response("ambiguous failure", { status: 500 });
    });
    await assert.rejects(operation(client), /Discord API 500/);
    assert.equal(requests, 1);
  }
});

test("Discord channel message hydration uses an authenticated snowflake-bounded GET", async () => {
  const channelId = "123456789012345678";
  const messageId = "234567890123456789";
  let requests = 0;
  const client = new DiscordRestClient("token", async (url, options) => {
    requests += 1;
    assert.equal(
      String(url),
      `https://discord.com/api/v10/channels/${channelId}/messages/${messageId}`,
    );
    assert.equal(options.method, "GET");
    assert.equal(options.headers.get("authorization"), "Bot token");
    return Response.json({ id: messageId, channel_id: channelId });
  });

  assert.deepEqual(await client.getChannelMessage(channelId, messageId), {
    id: messageId,
    channel_id: channelId,
  });
  await assert.rejects(
    client.getChannelMessage("not-a-snowflake", messageId),
    /channel ID.*snowflake/,
  );
  await assert.rejects(
    client.getChannelMessage(channelId, "123"),
    /message ID.*snowflake/,
  );
  assert.equal(requests, 1);
});

test("Discord channel history uses authenticated bounded before pagination", async () => {
  const channelId = "123456789012345678";
  const before = "234567890123456789";
  let requests = 0;
  const client = new DiscordRestClient("token", async (url, options) => {
    requests += 1;
    assert.equal(
      String(url),
      `https://discord.com/api/v10/channels/${channelId}/messages?limit=25&before=${before}`,
    );
    assert.equal(options.method, "GET");
    assert.equal(options.headers.get("authorization"), "Bot token");
    return Response.json([{ id: before, channel_id: channelId }]);
  });

  assert.deepEqual(
    await client.getChannelMessages(channelId, { limit: 25, before }),
    [{ id: before, channel_id: channelId }],
  );
  await assert.rejects(
    client.getChannelMessages(channelId, { limit: 0 }),
    /limit must be from 1 through 100/,
  );
  await assert.rejects(
    client.getChannelMessages(channelId, { limit: 101 }),
    /limit must be from 1 through 100/,
  );
  await assert.rejects(
    client.getChannelMessages(channelId, { before: "123" }),
    /before message ID.*snowflake/,
  );
  assert.equal(requests, 1);

  const unauthenticated = new DiscordRestClient(null, async () => {
    throw new Error("Unauthenticated history lookup reached the network.");
  });
  await assert.rejects(
    unauthenticated.getChannelMessages(channelId, { limit: 1 }),
    /DISCORD_TOKEN is required/,
  );
});

test("Discord GIF file components upload an explicit attachment-only payload", async () => {
  const bytes = new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39, 0x61]);
  const client = new DiscordRestClient(null, async (_url, options) => {
    assert.equal(options.method, "PATCH");
    assert.ok(options.body instanceof FormData);

    const payload = JSON.parse(options.body.get("payload_json"));
    assert.equal(payload.flags, 32768);
    assert.deepEqual(payload.components, [{
      type: 13,
      file: { url: "attachment://clearra-render.gif" },
    }]);
    assert.equal(Object.hasOwn(payload, "content"), false);
    assert.equal(Object.hasOwn(payload, "embeds"), false);
    assert.equal(Object.hasOwn(payload, "message_reference"), false);
    assert.deepEqual(payload.attachments, [{
      id: 0,
      filename: "clearra-render.gif",
      description: "Clearra GIF render",
    }]);

    const upload = options.body.get("files[0]");
    assert.ok(upload instanceof Blob);
    assert.equal(upload.type, "image/gif");
    assert.equal(upload.size, bytes.byteLength);
    assert.deepEqual(new Uint8Array(await upload.arrayBuffer()), bytes);
    return Response.json({ id: "render-file" });
  });

  await client.editOriginalInteraction(
    "1533373054309371924",
    "interaction-token",
    fileComponentMessage({
      name: "clearra-render.gif",
      description: "Clearra GIF render",
      contentType: "image/gif",
      bytes,
    }),
  );
});

test("Discord requests fail closed on a bounded network timeout", async () => {
  const client = new DiscordRestClient(
    null,
    async (_url, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener(
        "abort",
        () => reject(options.signal.reason),
        { once: true },
      );
    }),
    { requestTimeoutMs: 5 },
  );

  await assert.rejects(
    client.editOriginalInteraction("application", "interaction", {
      payload: { content: "done" },
      files: [],
    }),
    /timed out/,
  );
});
