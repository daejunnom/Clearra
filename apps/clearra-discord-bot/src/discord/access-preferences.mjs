import { dirname } from "node:path";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";

const STORE_VERSION = 1;
const DISCORD_SNOWFLAKE = /^\d{17,20}$/;
const STORE_KEYS = Object.freeze([
  "disabledChannels",
  "pausedGuilds",
  "version",
]);

export class DiscordAccessPreferences {
  constructor(options = {}) {
    this.storePath = options.storePath || null;
    this.fs = options.fs ?? { mkdir, readFile, rename, writeFile };
    this.pausedGuilds = new Set();
    this.disabledChannels = new Map();
    this.writeChain = Promise.resolve();
  }

  async load() {
    if (!this.storePath) return this;
    let source;
    try {
      source = await this.fs.readFile(this.storePath, "utf8");
    } catch (error) {
      if (error?.code === "ENOENT") return this;
      throw error;
    }
    const parsed = parseStore(source);
    this.pausedGuilds = parsed.pausedGuilds;
    this.disabledChannels = parsed.disabledChannels;
    return this;
  }

  isGuildPaused(guildId) {
    return this.pausedGuilds.has(requiredSnowflake(guildId, "guild ID"));
  }

  isChannelDisabled(channelId, guildId = undefined) {
    const channel = requiredSnowflake(channelId, "channel ID");
    const ownerGuild = this.disabledChannels.get(channel);
    if (ownerGuild === undefined) return false;
    if (guildId === undefined) return true;
    return ownerGuild === requiredSnowflake(guildId, "guild ID");
  }

  pauseGuild(guildId) {
    const guild = requiredSnowflake(guildId, "guild ID");
    return this.#mutate(() => {
      if (this.pausedGuilds.has(guild)) return false;
      this.pausedGuilds.add(guild);
      return true;
    }, { persistUnchanged: false });
  }

  resumeGuild(guildId) {
    const guild = requiredSnowflake(guildId, "guild ID");
    return this.#mutate(
      () => this.pausedGuilds.delete(guild),
      { persistUnchanged: false },
    );
  }

  disableChannel(channelId, guildId) {
    const channel = requiredSnowflake(channelId, "channel ID");
    const guild = requiredSnowflake(guildId, "guild ID");
    return this.#mutate(() => {
      const existingGuild = this.disabledChannels.get(channel);
      if (existingGuild === guild) return false;
      if (existingGuild !== undefined) {
        throw new Error(
          "Discord access preference channel belongs to a different guild.",
        );
      }
      this.disabledChannels.set(channel, guild);
      return true;
    }, { persistUnchanged: false });
  }

  enableChannel(channelId, guildId) {
    const channel = requiredSnowflake(channelId, "channel ID");
    const guild = requiredSnowflake(guildId, "guild ID");
    return this.#mutate(() => {
      if (this.disabledChannels.get(channel) !== guild) return false;
      return this.disabledChannels.delete(channel);
    }, { persistUnchanged: false });
  }

  #mutate(mutation, options = {}) {
    const operation = this.writeChain.then(async () => {
      const pausedGuildsBefore = new Set(this.pausedGuilds);
      const disabledChannelsBefore = new Map(this.disabledChannels);
      try {
        const result = mutation();
        if (
          this.storePath &&
          (options.persistUnchanged !== false || result !== false)
        ) {
          await this.#writeSnapshot();
        }
        return result;
      } catch (error) {
        this.pausedGuilds = pausedGuildsBefore;
        this.disabledChannels = disabledChannelsBefore;
        throw error;
      }
    });
    this.writeChain = operation.catch(() => {});
    return operation;
  }

  async #writeSnapshot() {
    const snapshot = JSON.stringify({
      version: STORE_VERSION,
      pausedGuilds: [...this.pausedGuilds].sort(),
      disabledChannels: Object.fromEntries([...this.disabledChannels].sort()),
    }, null, 2) + "\n";
    await this.fs.mkdir(dirname(this.storePath), {
      recursive: true,
      mode: 0o700,
    });
    const temporaryPath = `${this.storePath}.${process.pid}.tmp`;
    await this.fs.writeFile(temporaryPath, snapshot, {
      encoding: "utf8",
      mode: 0o600,
    });
    await this.fs.rename(temporaryPath, this.storePath);
  }
}

function parseStore(source) {
  let value;
  try {
    value = JSON.parse(source);
  } catch {
    throw new Error("The Discord access preference store is not valid JSON.");
  }
  if (!isRecord(value)) {
    throw new Error("The Discord access preference store must be an object.");
  }
  const keys = Object.keys(value).sort();
  if (
    keys.length !== STORE_KEYS.length ||
    keys.some((key, index) => key !== STORE_KEYS[index])
  ) {
    throw new Error("The Discord access preference store shape is invalid.");
  }
  if (value.version !== STORE_VERSION) {
    throw new Error("The Discord access preference store version is invalid.");
  }
  if (!Array.isArray(value.pausedGuilds)) {
    throw new Error("Discord paused guilds must be an array.");
  }
  const pausedGuilds = new Set();
  for (const guildId of value.pausedGuilds) {
    const guild = requiredSnowflake(guildId, "paused guild ID");
    if (pausedGuilds.has(guild)) {
      throw new Error("Discord paused guild IDs must be unique.");
    }
    pausedGuilds.add(guild);
  }
  if (!isRecord(value.disabledChannels)) {
    throw new Error("Discord disabled channels must be an object.");
  }
  const disabledChannels = new Map();
  for (const [channelId, guildId] of Object.entries(value.disabledChannels)) {
    disabledChannels.set(
      requiredSnowflake(channelId, "disabled channel ID"),
      requiredSnowflake(guildId, "disabled channel guild ID"),
    );
  }
  return { pausedGuilds, disabledChannels };
}

function requiredSnowflake(value, label) {
  if (typeof value !== "string" || !DISCORD_SNOWFLAKE.test(value)) {
    throw new Error(`Discord ${label} must be a 17-20 digit snowflake.`);
  }
  return value;
}

function isRecord(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
