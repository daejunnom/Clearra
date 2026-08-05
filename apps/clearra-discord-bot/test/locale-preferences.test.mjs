import assert from "node:assert/strict";
import test from "node:test";

import {
  canManageDiscordLocale,
  DiscordLocalePreferences,
  readDiscordLanguageRequest,
} from "../src/discord/locale-preferences.mjs";

const GUILD_ID = "123456789012345678";
const CHANNEL_ID = "223456789012345678";

test("locale preferences resolve explicit, channel, server, interaction, then English default", async () => {
  const fs = memoryFs();
  const storePath = "C:\\clearra-state\\locale-preferences.json";
  const preferences = await new DiscordLocalePreferences({ storePath, fs }).load();

  assert.deepEqual(
    preferences.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }),
    { locale: "en", source: "global" },
  );
  assert.deepEqual(
    preferences.resolve({
      guildId: GUILD_ID,
      channelId: CHANNEL_ID,
      interactionLocale: "ko-KR",
    }),
    { locale: "ko", source: "interaction" },
  );
  assert.deepEqual(
    preferences.resolve({
      guildId: GUILD_ID,
      channelId: CHANNEL_ID,
      interactionLocale: "ja",
    }),
    { locale: "en", source: "global" },
  );
  await preferences.setGuild(GUILD_ID, "ko");
  assert.deepEqual(
    preferences.resolve({
      guildId: GUILD_ID,
      channelId: CHANNEL_ID,
      interactionLocale: "en-US",
    }),
    { locale: "ko", source: "guild" },
  );
  await preferences.setChannel(CHANNEL_ID, "en");
  assert.deepEqual(
    preferences.resolve({
      guildId: GUILD_ID,
      channelId: CHANNEL_ID,
      interactionLocale: "ko",
    }),
    { locale: "en", source: "channel" },
  );
  assert.deepEqual(
    preferences.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }, "ko"),
    { locale: "ko", source: "explicit" },
  );

  const reloaded = await new DiscordLocalePreferences({ storePath, fs }).load();
  assert.deepEqual(
    reloaded.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }),
    { locale: "en", source: "channel" },
  );
  await reloaded.resetChannel(CHANNEL_ID);
  assert.deepEqual(
    reloaded.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }),
    { locale: "ko", source: "guild" },
  );
});

test("language requests and Discord management permissions are bounded", () => {
  assert.deepEqual(readDiscordLanguageRequest([{ name: "show" }]), {
    action: "show",
    scope: null,
    locale: null,
  });
  assert.deepEqual(readDiscordLanguageRequest([{
    name: "set",
    options: [
      { name: "scope", value: "channel" },
      { name: "language", value: "ko" },
    ],
  }]), { action: "set", scope: "channel", locale: "ko" });
  assert.throws(
    () => readDiscordLanguageRequest([{
      name: "set",
      options: [
        { name: "scope", value: "guild" },
        { name: "language", value: "ja" },
      ],
    }]),
    /en or ko/,
  );

  const interaction = (permissions) => ({
    guild_id: GUILD_ID,
    member: {
      permissions: String(permissions),
      user: { id: "323456789012345678" },
    },
  });
  assert.equal(canManageDiscordLocale(interaction(16), "channel"), true);
  assert.equal(canManageDiscordLocale(interaction(16), "guild"), false);
  assert.equal(canManageDiscordLocale(interaction(32), "guild"), true);
  assert.equal(canManageDiscordLocale(interaction(8), "channel"), true);
  assert.equal(canManageDiscordLocale(interaction(0), "guild"), false);
  assert.equal(canManageDiscordLocale(interaction(0), "channel"), false);
  assert.equal(canManageDiscordLocale({}, "channel"), false);
});

test("a failed locale-store write rolls back memory before later mutations", async () => {
  const fs = memoryFs();
  const storePath = "C:\\clearra-state\\locale-preferences.json";
  const preferences = await new DiscordLocalePreferences({ storePath, fs }).load();

  fs.failNextWrite(new Error("EACCES: permission denied, open 'C:\\private\\locale.json'"));
  await assert.rejects(preferences.setGuild(GUILD_ID, "ko"), /EACCES/);
  assert.deepEqual(
    preferences.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }),
    { locale: "en", source: "global" },
  );

  await preferences.setGuild(GUILD_ID, "ko");
  assert.deepEqual(
    preferences.resolve({ guildId: GUILD_ID, channelId: CHANNEL_ID }),
    { locale: "ko", source: "guild" },
  );
});

function memoryFs() {
  const files = new Map();
  let nextWriteError = null;
  return {
    failNextWrite(error) {
      nextWriteError = error;
    },
    async mkdir() {},
    async readFile(path) {
      if (files.has(path)) return files.get(path);
      const error = new Error("missing");
      error.code = "ENOENT";
      throw error;
    },
    async writeFile(path, source) {
      if (nextWriteError) {
        const error = nextWriteError;
        nextWriteError = null;
        throw error;
      }
      files.set(path, source);
    },
    async rename(from, to) {
      if (!files.has(from)) throw new Error("temporary locale store is missing");
      files.set(to, files.get(from));
      files.delete(from);
    },
  };
}
