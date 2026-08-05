import {
  isSupportedDiscordLocale,
  normalizeDiscordLocale,
} from "./i18n.mjs";
import { canManageDiscordLocale } from "./locale-preferences.mjs";

const MANAGEMENT_COMMANDS = new Map([
  ["channel-settings", "channel"],
  ["server-settings", "guild"],
]);

const SCOPE_ACTIONS = Object.freeze({
  channel: new Set(["language-show", "language-set", "language-reset", "disable", "enable"]),
  guild: new Set(["language-show", "language-set", "language-reset", "pause", "resume"]),
});

export function readDiscordManagementRequest(commandName, rawOptions = []) {
  const scope = MANAGEMENT_COMMANDS.get(commandName);
  if (!scope) throw new Error("The management command is invalid.");
  if (!Array.isArray(rawOptions) || rawOptions.length !== 1) {
    throw new Error("Choose exactly one management action.");
  }
  const actionOption = rawOptions[0];
  const action = actionOption?.name;
  if (!SCOPE_ACTIONS[scope].has(action)) {
    throw new Error("The management action is invalid.");
  }
  const options = actionOption.options ?? [];
  if (!Array.isArray(options)) {
    throw new Error("The management command options are invalid.");
  }
  if (action === "language-set") {
    if (
      options.length !== 1 ||
      options[0]?.name !== "language" ||
      !isSupportedDiscordLocale(options[0]?.value)
    ) {
      throw new Error("Language must be en or ko.");
    }
    return Object.freeze({ scope, action, locale: options[0].value });
  }
  if (options.length !== 0) {
    throw new Error("The management action does not accept options.");
  }
  return Object.freeze({ scope, action, locale: null });
}

export function readTextManagementRequest(content, prefix) {
  const source = String(content ?? "").trim();
  if (!isTextManagementCandidate(source, prefix)) return null;
  const body = source.slice(prefix.length).trim();
  const tokens = body.split(/\s+/).map((value) => value.toLowerCase());
  if (tokens.shift() !== "bot-control") return null;
  if (tokens[0] === "help") {
    if (tokens.length !== 1) {
      throw new Error("Bot control help does not accept arguments.");
    }
    return Object.freeze({ scope: null, action: "help", locale: null });
  }
  const scopeToken = tokens.shift();
  const scope = scopeToken === "server"
    ? "guild"
    : scopeToken === "channel"
      ? "channel"
      : null;
  if (!scope) throw new Error("Bot control scope must be channel or server.");

  const firstAction = tokens.shift();
  let action;
  let locale = null;
  if (firstAction === "language") {
    const languageAction = tokens.shift();
    if (languageAction === "show") action = "language-show";
    else if (languageAction === "reset") action = "language-reset";
    else if (languageAction === "set") {
      action = "language-set";
      locale = tokens.shift() ?? null;
      if (!isSupportedDiscordLocale(locale)) {
        throw new Error("Language must be en or ko.");
      }
    }
  } else if (scope === "channel" && ["disable", "enable"].includes(firstAction)) {
    action = firstAction;
  } else if (scope === "guild" && ["pause", "resume"].includes(firstAction)) {
    action = firstAction;
  }
  if (!action || tokens.length !== 0) {
    throw new Error("The bot control action is invalid.");
  }
  return Object.freeze({ scope, action, locale });
}

export function isTextManagementCandidate(content, prefix) {
  if (typeof prefix !== "string" || prefix.length === 0) return false;
  const source = String(content ?? "").trim();
  if (!source.startsWith(prefix)) return false;
  const body = source.slice(prefix.length).trim();
  return /^bot-control(?:\s|$)/i.test(body);
}

export function formatTextManagementHelp(locale = "en") {
  return normalizeDiscordLocale(locale) === "ko"
    ? [
        "**ClearraBot 관리자 명령어**",
        "`$` 대신 `>` 접두사도 사용할 수 있습니다.",
        "`$bot-control help`",
        "`$bot-control channel language show`",
        "`$bot-control channel language set en|ko`",
        "`$bot-control channel language reset`",
        "`$bot-control channel disable|enable`",
        "`$bot-control server language show`",
        "`$bot-control server language set en|ko`",
        "`$bot-control server language reset`",
        "`$bot-control server pause|resume`",
        "이 도움말과 명령은 ClearraBot 관리자만 사용할 수 있습니다.",
      ].join("\n")
    : [
        "**ClearraBot administrator controls**",
        "The `>` prefix can be used instead of `$`.",
        "`$bot-control help`",
        "`$bot-control channel language show`",
        "`$bot-control channel language set en|ko`",
        "`$bot-control channel language reset`",
        "`$bot-control channel disable|enable`",
        "`$bot-control server language show`",
        "`$bot-control server language set en|ko`",
        "`$bot-control server language reset`",
        "`$bot-control server pause|resume`",
        "This help page and these commands are available only to ClearraBot administrators.",
      ].join("\n");
}

export function canManageDiscordSettings(interaction, scope) {
  return canManageDiscordLocale(interaction, scope);
}

export function isServerResumeRequest(request) {
  return request?.scope === "guild" && request.action === "resume";
}

export function isChannelEnableRequest(request) {
  return request?.scope === "channel" && request.action === "enable";
}
