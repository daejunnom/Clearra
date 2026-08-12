import { isTextManagementCandidate } from "../discord/management-command.mjs";

/**
 * Owns deterministic cross-bot message ownership. It deliberately classifies
 * only ingress metadata; it never calls a handler, downloads an attachment, or
 * starts render/search work.
 */
export class OracleMessageArbitration {
  constructor(config = {}) {
    this.commandPrefixes = Object.freeze(
      Array.isArray(config.oracleCommandPrefixes)
        ? config.oracleCommandPrefixes.map(String)
        : ["$", ">"],
    );
    this.sfinderManGuildIds = new Set(
      Array.isArray(config.oracleSfinderManGuildIds)
        ? config.oracleSfinderManGuildIds.map(String)
        : [],
    );
  }

  delegatesPrefixedText(message, botUserId) {
    if (!this.#isSfinderManGuildUserMessage(message, botUserId)) return false;
    if (!isNonemptyPrefixedText(message?.content, this.commandPrefixes)) {
      return false;
    }
    return !isTextManagementInvocation(message?.content, this.commandPrefixes) &&
      !isClearraOnlyTextInvocation(message?.content, this.commandPrefixes);
  }

  delegatesAcceptedCandidate(message, botUserId, managementMessage) {
    if (!this.#isSfinderManGuildUserMessage(message, botUserId)) return false;
    return !managementMessage &&
      !isTextManagementInvocation(message?.content, this.commandPrefixes) &&
      !isClearraOnlyTextInvocation(message?.content, this.commandPrefixes);
  }

  #isSfinderManGuildUserMessage(message, botUserId) {
    return message?.author?.id !== botUserId &&
      this.sfinderManGuildIds.has(String(message?.guild_id ?? ""));
  }
}

function isTextManagementInvocation(content, prefixes) {
  return prefixes.some((prefix) => isTextManagementCandidate(content, prefix));
}

function isNonemptyPrefixedText(content, prefixes) {
  const source = String(content ?? "").trimStart();
  return prefixes.some((prefix) =>
    Boolean(prefix) &&
    source.startsWith(prefix) &&
    source.slice(prefix.length).trim().length > 0
  );
}

function isClearraOnlyTextInvocation(content, prefixes) {
  const source = String(content ?? "").trimStart();
  for (const prefix of prefixes) {
    if (!prefix || !source.startsWith(prefix)) continue;
    const command = source
      .slice(prefix.length)
      .trimStart()
      .split(/\s+/u, 1)[0]
      ?.toLowerCase()
      .replaceAll("_", "-");
    return command === "render-file";
  }
  return false;
}
