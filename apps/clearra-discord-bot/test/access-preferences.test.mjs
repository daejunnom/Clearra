import assert from "node:assert/strict";
import test from "node:test";

import { DiscordAccessPreferences } from "../src/discord/access-preferences.mjs";

const GUILD_ID = "123456789012345678";
const OTHER_GUILD_ID = "223456789012345678";
const CHANNEL_ID = "323456789012345678";
const OTHER_CHANNEL_ID = "423456789012345678";
const STORE_PATH = "C:\\clearra-state\\access-preferences.json";

test("access preferences persist paused guilds and guild-owned disabled channels", async () => {
  const fs = memoryFs();
  const preferences = await new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  }).load();

  assert.equal(preferences.isGuildPaused(GUILD_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID), false);
  assert.equal(await preferences.pauseGuild(GUILD_ID), true);
  assert.equal(await preferences.disableChannel(CHANNEL_ID, GUILD_ID), true);
  assert.equal(preferences.isGuildPaused(GUILD_ID), true);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID), true);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
  assert.equal(
    preferences.isChannelDisabled(CHANNEL_ID, OTHER_GUILD_ID),
    false,
  );

  assert.deepEqual(JSON.parse(fs.files.get(STORE_PATH)), {
    version: 1,
    pausedGuilds: [GUILD_ID],
    disabledChannels: { [CHANNEL_ID]: GUILD_ID },
  });
  assert.deepEqual(fs.lastMkdirOptions, { recursive: true, mode: 0o700 });
  assert.deepEqual(fs.lastWriteOptions, { encoding: "utf8", mode: 0o600 });

  const reloaded = await new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  }).load();
  assert.equal(reloaded.isGuildPaused(GUILD_ID), true);
  assert.equal(reloaded.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
  assert.equal(await reloaded.resumeGuild(GUILD_ID), true);
  assert.equal(await reloaded.enableChannel(CHANNEL_ID, GUILD_ID), true);
  assert.equal(reloaded.isGuildPaused(GUILD_ID), false);
  assert.equal(reloaded.isChannelDisabled(CHANNEL_ID), false);
});

test("read-only access checks perform no filesystem or authority work", async () => {
  const fs = memoryFs();
  const preferences = new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  });
  fs.resetCalls();

  assert.equal(preferences.isGuildPaused(GUILD_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), false);
  assert.deepEqual(fs.calls, {
    mkdir: 0,
    readFile: 0,
    rename: 0,
    writeFile: 0,
  });
});

test("access mutations serialize without losing concurrent changes", async () => {
  const fs = memoryFs({ delayWrites: true });
  const preferences = new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  });

  await Promise.all([
    preferences.pauseGuild(GUILD_ID),
    preferences.pauseGuild(OTHER_GUILD_ID),
    preferences.disableChannel(CHANNEL_ID, GUILD_ID),
    preferences.disableChannel(OTHER_CHANNEL_ID, OTHER_GUILD_ID),
  ]);

  assert.equal(preferences.isGuildPaused(GUILD_ID), true);
  assert.equal(preferences.isGuildPaused(OTHER_GUILD_ID), true);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
  assert.equal(
    preferences.isChannelDisabled(OTHER_CHANNEL_ID, OTHER_GUILD_ID),
    true,
  );
  assert.deepEqual(JSON.parse(fs.files.get(STORE_PATH)), {
    version: 1,
    pausedGuilds: [GUILD_ID, OTHER_GUILD_ID],
    disabledChannels: {
      [CHANNEL_ID]: GUILD_ID,
      [OTHER_CHANNEL_ID]: OTHER_GUILD_ID,
    },
  });
});

test("failed access-store writes roll back both maps and later mutations recover", async () => {
  const fs = memoryFs();
  const preferences = new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  });

  fs.failNextWrite(new Error("EACCES: access store is read-only"));
  await assert.rejects(preferences.pauseGuild(GUILD_ID), /EACCES/);
  assert.equal(preferences.isGuildPaused(GUILD_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID), false);

  await preferences.disableChannel(CHANNEL_ID, GUILD_ID);
  fs.failNextRename(new Error("EIO: atomic rename failed"));
  await assert.rejects(preferences.pauseGuild(GUILD_ID), /EIO/);
  assert.equal(preferences.isGuildPaused(GUILD_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);

  assert.equal(await preferences.pauseGuild(GUILD_ID), true);
  assert.equal(preferences.isGuildPaused(GUILD_ID), true);
});

test("unchanged and cross-guild mutations neither rewrite nor remove state", async () => {
  const fs = memoryFs();
  const preferences = new DiscordAccessPreferences({
    storePath: STORE_PATH,
    fs,
  });
  await preferences.pauseGuild(GUILD_ID);
  await preferences.disableChannel(CHANNEL_ID, GUILD_ID);
  fs.resetCalls();

  assert.equal(await preferences.pauseGuild(GUILD_ID), false);
  assert.equal(await preferences.disableChannel(CHANNEL_ID, GUILD_ID), false);
  assert.equal(
    await preferences.enableChannel(CHANNEL_ID, OTHER_GUILD_ID),
    false,
  );
  assert.equal(await preferences.resumeGuild(OTHER_GUILD_ID), false);
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
  assert.equal(fs.calls.writeFile, 0);
  await assert.rejects(
    preferences.disableChannel(CHANNEL_ID, OTHER_GUILD_ID),
    /different guild/,
  );
  assert.equal(preferences.isChannelDisabled(CHANNEL_ID, GUILD_ID), true);
});

test("access store v1 parsing rejects malformed and ambiguous state", async () => {
  const valid = {
    version: 1,
    pausedGuilds: [GUILD_ID],
    disabledChannels: { [CHANNEL_ID]: GUILD_ID },
  };
  const invalidStores = [
    "not JSON",
    JSON.stringify([]),
    JSON.stringify({ ...valid, version: 2 }),
    JSON.stringify({ version: 1, pausedGuilds: [] }),
    JSON.stringify({ ...valid, unexpected: true }),
    JSON.stringify({ ...valid, pausedGuilds: GUILD_ID }),
    JSON.stringify({ ...valid, pausedGuilds: [GUILD_ID, GUILD_ID] }),
    JSON.stringify({ ...valid, pausedGuilds: [123456789012345678] }),
    JSON.stringify({ ...valid, pausedGuilds: ["invalid"] }),
    JSON.stringify({ ...valid, disabledChannels: [] }),
    JSON.stringify({
      ...valid,
      disabledChannels: { invalid: GUILD_ID },
    }),
    JSON.stringify({
      ...valid,
      disabledChannels: { [CHANNEL_ID]: "invalid" },
    }),
  ];

  for (const source of invalidStores) {
    const fs = memoryFs({ initialSource: source });
    await assert.rejects(
      new DiscordAccessPreferences({ storePath: STORE_PATH, fs }).load(),
      /Discord .*access|Discord paused|Discord disabled/,
    );
  }
});

test("public access methods reject non-string and malformed snowflakes", async () => {
  const preferences = new DiscordAccessPreferences();
  assert.throws(() => preferences.isGuildPaused("bad"), /snowflake/);
  assert.throws(() => preferences.isChannelDisabled(123), /snowflake/);
  assert.throws(() => preferences.pauseGuild("1"), /snowflake/);
  assert.throws(
    () => preferences.disableChannel(CHANNEL_ID, "bad"),
    /snowflake/,
  );
  assert.throws(
    () => preferences.enableChannel("bad", GUILD_ID),
    /snowflake/,
  );
});

function memoryFs(options = {}) {
  const files = new Map();
  if (options.initialSource !== undefined) {
    files.set(STORE_PATH, options.initialSource);
  }
  let nextWriteError = null;
  let nextRenameError = null;
  const calls = {
    mkdir: 0,
    readFile: 0,
    rename: 0,
    writeFile: 0,
  };
  return {
    files,
    calls,
    lastMkdirOptions: null,
    lastWriteOptions: null,
    failNextWrite(error) {
      nextWriteError = error;
    },
    failNextRename(error) {
      nextRenameError = error;
    },
    resetCalls() {
      for (const key of Object.keys(calls)) calls[key] = 0;
    },
    async mkdir(_path, mkdirOptions) {
      calls.mkdir += 1;
      this.lastMkdirOptions = mkdirOptions;
    },
    async readFile(path) {
      calls.readFile += 1;
      if (files.has(path)) return files.get(path);
      const error = new Error("missing");
      error.code = "ENOENT";
      throw error;
    },
    async writeFile(path, source, writeOptions) {
      calls.writeFile += 1;
      this.lastWriteOptions = writeOptions;
      if (nextWriteError) {
        const error = nextWriteError;
        nextWriteError = null;
        throw error;
      }
      if (options.delayWrites) await Promise.resolve();
      files.set(path, source);
    },
    async rename(from, to) {
      calls.rename += 1;
      if (nextRenameError) {
        const error = nextRenameError;
        nextRenameError = null;
        throw error;
      }
      if (!files.has(from)) throw new Error("temporary access store is missing");
      files.set(to, files.get(from));
      files.delete(from);
    },
  };
}
