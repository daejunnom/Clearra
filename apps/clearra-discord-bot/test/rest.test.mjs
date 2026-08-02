import assert from "node:assert/strict";
import test from "node:test";

import { DiscordRestClient } from "../src/discord/rest.mjs";

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
