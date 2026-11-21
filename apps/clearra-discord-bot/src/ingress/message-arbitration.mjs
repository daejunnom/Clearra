import { classifyClearraTextCommand } from "../clearra/text-command.mjs";
import {
  activeDiscordSearchCapabilities,
  hiddenTextSearchCapabilities,
} from "../discord/capability-registry.mjs";
import { isTextManagementCandidate } from "../discord/management-command.mjs";

// The parser shares slash schemas, so a recognized path alone does not confer
// text ownership. In particular, slash-only searches and settings must not
// turn an otherwise unowned prefixed message into Clearra work.
const CLEARRA_TEXT_IDENTITIES = new Set(["help"]);
for (const capability of [
  ...activeDiscordSearchCapabilities(),
  ...hiddenTextSearchCapabilities(),
]) {
  if (capability.canonical.text) CLEARRA_TEXT_IDENTITIES.add(capability.id);
  for (const route of [capability.canonical, ...capability.aliases]) {
    if (!route.text) continue;
    CLEARRA_TEXT_IDENTITIES.add([route.root, route.subcommand].filter(Boolean).join("."));
  }
}

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
    this.textEnabled = Boolean(config.oracleTextEnabled);
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
      !this.#ownsClearraTextInvocation(message?.content);
  }

  delegatesAcceptedCandidate(message, botUserId, managementMessage) {
    if (!this.#isSfinderManGuildUserMessage(message, botUserId)) return false;
    return !managementMessage &&
      !isTextManagementInvocation(message?.content, this.commandPrefixes) &&
      !this.#ownsClearraTextInvocation(message?.content);
  }

  #ownsClearraTextInvocation(content) {
    return this.commandPrefixes.some((prefix) => {
      const identity = classifyClearraTextCommand(content, prefix);
      // Retain the existing explicit GIF recovery exception. Search/help
      // ownership applies only when the ordinary text feature is enabled.
      return identity === "render-file" ||
        (this.textEnabled && CLEARRA_TEXT_IDENTITIES.has(identity));
    });
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
