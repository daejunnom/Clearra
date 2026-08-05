import { dirname } from "node:path";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";

import {
  DEFAULT_DISCORD_LOCALE,
  isSupportedDiscordLocale,
  matchDiscordLocale,
  normalizeDiscordLocale,
} from "./i18n.mjs";

const STORE_VERSION = 1;
const DISCORD_SNOWFLAKE = /^\d{17,20}$/;
const ADMINISTRATOR_PERMISSION = 1n << 3n;
const MANAGE_CHANNELS_PERMISSION = 1n << 4n;
const MANAGE_GUILD_PERMISSION = 1n << 5n;

export function readDiscordLanguageRequest(rawOptions = []) {
  if (!Array.isArray(rawOptions) || rawOptions.length !== 1) {
    throw new Error("Choose exactly one language action.");
  }
  const actionOption = rawOptions[0];
  const action = actionOption?.name;
  if (!new Set(["show", "set", "reset"]).has(action)) {
    throw new Error("The language action is invalid.");
  }
  const values = new Map();
  for (const option of actionOption.options ?? []) {
    if (typeof option?.name !== "string" || values.has(option.name)) {
      throw new Error("The language command options are invalid.");
    }
    values.set(option.name, option.value);
  }
  if (action === "show") {
    if (values.size !== 0) throw new Error("The show action does not accept options.");
    return Object.freeze({ action, scope: null, locale: null });
  }
  const scope = values.get("scope");
  if (scope !== "channel" && scope !== "guild") {
    throw new Error("Language scope must be channel or guild.");
  }
  if (action === "reset") {
    if (values.size !== 1) throw new Error("The reset action accepts only scope.");
    return Object.freeze({ action, scope, locale: null });
  }
  const locale = values.get("language");
  if (!isSupportedDiscordLocale(locale) || values.size !== 2) {
    throw new Error("Language must be en or ko.");
  }
  return Object.freeze({ action, scope, locale });
}

export function canManageDiscordLocale(interaction, scope) {
  if (!interaction?.guild_id || (scope !== "channel" && scope !== "guild")) {
    return false;
  }
  let permissions;
  try {
    permissions = BigInt(interaction.member?.permissions ?? "0");
  } catch {
    return false;
  }
  if ((permissions & ADMINISTRATOR_PERMISSION) !== 0n) return true;
  const required = scope === "channel"
    ? MANAGE_CHANNELS_PERMISSION
    : MANAGE_GUILD_PERMISSION;
  return (permissions & required) !== 0n;
}

export class DiscordLocalePreferences {
  constructor(options = {}) {
    this.defaultLocale = normalizeDiscordLocale(
      options.defaultLocale,
      DEFAULT_DISCORD_LOCALE,
    );
    this.storePath = options.storePath || null;
    this.fs = options.fs ?? { mkdir, readFile, rename, writeFile };
    this.guilds = new Map();
    this.channels = new Map();
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
    this.guilds = parsed.guilds;
    this.channels = parsed.channels;
    return this;
  }

  resolve(context = {}, explicitLocale = null) {
    if (isSupportedDiscordLocale(explicitLocale)) {
      return Object.freeze({ locale: explicitLocale, source: "explicit" });
    }
    const channelId = optionalSnowflake(context.channelId ?? context.channel_id);
    if (channelId && this.channels.has(channelId)) {
      return Object.freeze({
        locale: this.channels.get(channelId),
        source: "channel",
      });
    }
    const guildId = optionalSnowflake(context.guildId ?? context.guild_id);
    if (guildId && this.guilds.has(guildId)) {
      return Object.freeze({ locale: this.guilds.get(guildId), source: "guild" });
    }
    const interactionLocale = matchDiscordLocale(
      context.interactionLocale ?? context.locale,
    );
    if (interactionLocale) {
      return Object.freeze({ locale: interactionLocale, source: "interaction" });
    }
    return Object.freeze({ locale: this.defaultLocale, source: "global" });
  }

  async setGuild(guildId, locale) {
    const id = requiredSnowflake(guildId, "guild ID");
    const language = requiredLocale(locale);
    await this.#mutate(() => this.guilds.set(id, language));
  }

  async setChannel(channelId, locale) {
    const id = requiredSnowflake(channelId, "channel ID");
    const language = requiredLocale(locale);
    await this.#mutate(() => this.channels.set(id, language));
  }

  async resetGuild(guildId) {
    const id = requiredSnowflake(guildId, "guild ID");
    return this.#mutate(() => this.guilds.delete(id), { persistUnchanged: false });
  }

  async resetChannel(channelId) {
    const id = requiredSnowflake(channelId, "channel ID");
    return this.#mutate(() => this.channels.delete(id), { persistUnchanged: false });
  }

  #mutate(mutation, options = {}) {
    const operation = this.writeChain.then(async () => {
      const guildsBefore = new Map(this.guilds);
      const channelsBefore = new Map(this.channels);
      const result = mutation();
      if (this.storePath && (options.persistUnchanged !== false || result !== false)) {
        try {
          await this.#writeSnapshot();
        } catch (error) {
          this.guilds = guildsBefore;
          this.channels = channelsBefore;
          throw error;
        }
      }
      return result;
    });
    this.writeChain = operation.catch(() => {});
    return operation;
  }

  async #writeSnapshot() {
    const snapshot = JSON.stringify({
      version: STORE_VERSION,
      guilds: Object.fromEntries([...this.guilds].sort()),
      channels: Object.fromEntries([...this.channels].sort()),
    }, null, 2) + "\n";
    await this.fs.mkdir(dirname(this.storePath), { recursive: true, mode: 0o700 });
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
    throw new Error("The Discord locale preference store is not valid JSON.");
  }
  if (!isRecord(value) || value.version !== STORE_VERSION) {
    throw new Error("The Discord locale preference store version is invalid.");
  }
  return {
    guilds: parseEntries(value.guilds, "guild"),
    channels: parseEntries(value.channels, "channel"),
  };
}

function parseEntries(value, label) {
  if (!isRecord(value)) {
    throw new Error(`The Discord locale preference ${label} map is invalid.`);
  }
  const output = new Map();
  for (const [id, locale] of Object.entries(value)) {
    output.set(requiredSnowflake(id, `${label} ID`), requiredLocale(locale));
  }
  return output;
}

function requiredLocale(value) {
  if (!isSupportedDiscordLocale(value)) {
    throw new Error("Discord locale preferences support only en and ko.");
  }
  return value;
}

function requiredSnowflake(value, label) {
  const normalized = String(value ?? "");
  if (!DISCORD_SNOWFLAKE.test(normalized)) {
    throw new Error(`Discord ${label} must be a 17-20 digit snowflake.`);
  }
  return normalized;
}

function optionalSnowflake(value) {
  const normalized = String(value ?? "");
  return DISCORD_SNOWFLAKE.test(normalized) ? normalized : null;
}

function isRecord(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
