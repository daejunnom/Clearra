const DISCORD_SNOWFLAKE = /^\d{17,20}$/;
const DISCORD_MESSAGE_LINK = /^https:\/\/(?:(?:canary|ptb)\.)?discord(?:app)?\.com\/channels\/(\d{17,20}|@me)\/(\d{17,20})\/(\d{17,20})\/?(?:[?#].*)?$/i;
const RENDER_GIF_FILENAME = /^(?:clearra-input-preview|clearra-view-\d+|clearra-render)\.gif$/i;
const GIF87A = [0x47, 0x49, 0x46, 0x38, 0x37, 0x61];
const GIF89A = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];

export function readRenderFileImage(rawOptions = []) {
  if (!Array.isArray(rawOptions)) {
    throw new Error("Discord supplied invalid /render-file options.");
  }
  let image = null;
  for (const option of rawOptions) {
    if (option?.name !== "image") {
      throw new Error(
        `Discord supplied unsupported /render-file option '${option?.name || "unknown"}'.`,
      );
    }
    if (image !== null) {
      throw new Error("Discord supplied /render-file image more than once.");
    }
    if (typeof option.value !== "string") {
      throw new Error("/render-file image must be a message link or message ID.");
    }
    image = option.value.trim();
    if (!image) throw new Error("/render-file image cannot be empty.");
    if (image.length > 512) {
      throw new Error("/render-file image exceeds the 512-character limit.");
    }
  }
  return image;
}

export function readRenderFileTargetMessageId(interaction) {
  if (interaction?.type !== 2 || interaction.data?.type !== 3) {
    throw new Error("Discord supplied an invalid original-GIF message command.");
  }
  const channelId = snowflake(interaction.channel_id, "current channel ID");
  const guildId = snowflake(interaction.guild_id, "current server ID");
  const targetId = snowflake(
    interaction.data.target_id,
    "target message ID",
  );
  const messages = interaction.data.resolved?.messages;
  if (!messages || typeof messages !== "object" || Array.isArray(messages)) {
    throw new Error("Discord supplied no resolved selected message.");
  }
  const target = messages[targetId];
  if (!target || typeof target !== "object" || Array.isArray(target)) {
    throw new Error("Discord supplied no selected message.");
  }
  if (snowflake(target.id, "resolved message ID") !== targetId) {
    throw new Error("Discord resolved a different message than the selected target.");
  }
  if (
    target.channel_id !== undefined &&
    snowflake(target.channel_id, "resolved message channel ID") !== channelId
  ) {
    throw new Error("Discord supplied a selected message from another channel.");
  }
  if (
    target.guild_id !== undefined &&
    snowflake(target.guild_id, "resolved message server ID") !== guildId
  ) {
    throw new Error("Discord supplied a selected message from another server.");
  }
  return targetId;
}

export function resolveRenderMessageReference(
  value,
  { channelId, guildId = null } = {},
) {
  const currentChannel = snowflake(channelId, "current channel ID");
  const source = String(value ?? "").trim();
  if (DISCORD_SNOWFLAKE.test(source)) {
    return Object.freeze({ channelId: currentChannel, messageId: source });
  }
  const match = source.match(DISCORD_MESSAGE_LINK);
  if (!match) {
    throw new Error(
      "/render-file image must be a Discord message link or message ID.",
    );
  }
  const [, linkedGuild, linkedChannel, messageId] = match;
  if (linkedChannel !== currentChannel) {
    throw new Error("/render-file image must point to the current channel.");
  }
  if (guildId) {
    if (linkedGuild !== String(guildId)) {
      throw new Error("/render-file image must point to the current server.");
    }
  } else if (linkedGuild !== "@me") {
    throw new Error("/render-file image must point to the current direct message.");
  }
  return Object.freeze({ channelId: linkedChannel, messageId });
}

export function renderGifCandidate(message, options = {}) {
  if (!message || typeof message !== "object") return null;
  const applicationId = optionalSnowflake(options.applicationId);
  const botUserId = optionalSnowflake(options.botUserId);
  if (!isClearraAuthoredMessage(message, applicationId, botUserId)) return null;
  const maximum = Number(options.maxBytes);
  if (!Number.isSafeInteger(maximum) || maximum < 1) {
    throw new Error("The render-file size limit is invalid.");
  }
  const attachment = (message.attachments ?? []).find((candidate) =>
    isRenderGifAttachment(candidate, maximum)
  );
  if (!attachment) return null;
  return Object.freeze({
    messageId: String(message.id),
    channelId: String(message.channel_id ?? options.channelId ?? ""),
    ownerId: renderOwnerId(message),
    attachment,
  });
}

export function prioritizeRenderGifCandidates(messages, options = {}) {
  const callerId = snowflake(options.callerId, "requester user ID");
  const own = [];
  const other = [];
  const seen = new Set();
  for (const message of messages ?? []) {
    const candidate = renderGifCandidate(message, options);
    if (!candidate) continue;
    const key = `${candidate.messageId}:${candidate.attachment.id}`;
    if (seen.has(key)) continue;
    seen.add(key);
    if (candidate.ownerId === callerId) own.push(candidate);
    else other.push(candidate);
  }
  return Object.freeze([...own, ...other]);
}

export function assertGifBytes(value) {
  const bytes = value instanceof Uint8Array ? value : new Uint8Array(value ?? []);
  if (!matchesHeader(bytes, GIF87A) && !matchesHeader(bytes, GIF89A)) {
    throw new Error("The selected attachment is not a valid GIF file.");
  }
  return bytes;
}

export function isUnavailableDiscordAttachmentError(error) {
  return error?.discordStatus === 403 || error?.discordStatus === 404;
}

function isClearraAuthoredMessage(message, applicationId, botUserId) {
  const authorId = optionalSnowflake(message.author?.id);
  if (botUserId && authorId === botUserId) return true;
  if (applicationId && authorId === applicationId) return true;
  if (applicationId && optionalSnowflake(message.application_id) === applicationId) {
    return message.author?.bot === true;
  }
  return false;
}

function isRenderGifAttachment(attachment, maximum) {
  if (!attachment || typeof attachment !== "object") return false;
  if (!RENDER_GIF_FILENAME.test(String(attachment.filename ?? ""))) return false;
  if (String(attachment.content_type ?? "").toLowerCase() !== "image/gif") {
    return false;
  }
  const size = Number(attachment.size);
  if (Number.isFinite(size) && (!Number.isSafeInteger(size) || size < 1 || size > maximum)) {
    return false;
  }
  return (
    typeof attachment.id === "string" &&
    typeof attachment.url === "string" &&
    attachment.url.length > 0
  );
}

function renderOwnerId(message, depth = 0) {
  if (!message || depth > 3) return null;
  const interactionUser = optionalSnowflake(
    message.interaction_metadata?.user?.id ?? message.interaction?.user?.id,
  );
  if (interactionUser) return interactionUser;
  const referenced = message.referenced_message;
  if (!referenced || typeof referenced !== "object") return null;
  const nested = renderOwnerId(referenced, depth + 1);
  if (nested) return nested;
  return referenced.author?.bot === true
    ? null
    : optionalSnowflake(referenced.author?.id);
}

function matchesHeader(bytes, header) {
  return bytes.byteLength >= header.length &&
    header.every((value, index) => bytes[index] === value);
}

function snowflake(value, name) {
  const normalized = optionalSnowflake(value);
  if (!normalized) throw new Error(`Discord ${name} must be a 17-20 digit snowflake.`);
  return normalized;
}

function optionalSnowflake(value) {
  const normalized = typeof value === "string" ? value : String(value ?? "");
  return DISCORD_SNOWFLAKE.test(normalized) ? normalized : null;
}
