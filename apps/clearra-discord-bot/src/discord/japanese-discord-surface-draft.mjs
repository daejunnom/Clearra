// Review-only Japanese renderers. No production module imports this file.
// The released locale parser, registration catalog, and persisted preferences
// remain authoritative; these drafts do not release or register Japanese.
import {
  globalCommands,
  slashCommandCatalog,
  messageCommandCatalog,
  formatSlashCommandHelp,
} from "./slash-command-catalog.mjs";
import { buildCommandModalResponse } from "./field-modal.mjs";
import { formatTextManagementHelp } from "./management-command.mjs";
import {
  JAPANESE_NAMES,
  JAPANESE_DESCRIPTIONS,
  JAPANESE_CHOICES,
  JAPANESE_HELP_DESCRIPTIONS,
  JAPANESE_HELP_TEXT,
  JAPANESE_MODAL_TEXT,
} from "./japanese-discord-surface-catalog.mjs";

export function japaneseDiscordNameDraft(name) {
  return requiredTranslation(JAPANESE_NAMES, name, "name");
}

export function japaneseDiscordDescriptionDraft(description) {
  if (Object.hasOwn(JAPANESE_DESCRIPTIONS, description)) return JAPANESE_DESCRIPTIONS[description];
  if (Object.hasOwn(JAPANESE_HELP_DESCRIPTIONS, description)) return JAPANESE_HELP_DESCRIPTIONS[description];
  const compact = /^Set (.+); see \/help for details$/u.exec(description);
  if (compact) {
    const name = japaneseDiscordNameDraft(compact[1].replaceAll(" ", "-"));
    return `${name}の設定。詳細は/helpを参照`;
  }
  throw new Error(`Missing Japanese Discord description draft: ${description}`);
}

export function japaneseDiscordChoiceNameDraft(name) {
  return requiredTranslation(JAPANESE_CHOICES, name, "choice");
}

function localizeRegistration(node) {
  return {
    ...node,
    name_localizations: { ...node.name_localizations, ja: japaneseDiscordNameDraft(node.name) },
    ...(typeof node.description === "string" ? {
      description_localizations: {
        ...node.description_localizations,
        ja: japaneseDiscordDescriptionDraft(node.description),
      },
    } : {}),
    ...(node.options ? { options: node.options.map(localizeRegistration) } : {}),
    ...(node.choices ? {
      choices: node.choices.map(choice => ({
        ...choice,
        name_localizations: {
          ...choice.name_localizations,
          ja: japaneseDiscordChoiceNameDraft(choice.name),
        },
      })),
    } : {}),
  };
}

// Uses the same compacted registration tree that production sends, retaining
// autocomplete conversion and short descriptions needed for the 8,000 limit.
export function japaneseDiscordRegistrationDraft() {
  return globalCommands.map(localizeRegistration);
}

// Also exercises every choice removed from registration in favor of autocomplete.
export function japaneseDiscordFullCommandDraft() {
  return [...slashCommandCatalog, ...messageCommandCatalog]
    .map(entry => localizeRegistration(entry.registration));
}

export function formatJapaneseDiscordHelpDraft(requestedName) {
  return formatSlashCommandHelp(requestedName, "en")
    .split("\n")
    .map(japaneseHelpLine)
    .join("\n");
}

export function formatJapaneseTextManagementHelpDraft() {
  const prose = {
    "**ClearraBot administrator controls**": "**ClearraBot管理者コマンド**",
    "The `>` prefix can be used instead of `$`.": "接頭辞は`$`の代わりに`>`も使えます。",
    "This help page and these commands are available only to ClearraBot administrators.":
      "このヘルプとコマンドはClearraBot管理者のみ利用できます。",
  };
  return formatTextManagementHelp("en").split("\n").map(line => {
    if (Object.hasOwn(prose, line)) return prose[line];
    if (/^`\$bot-control [a-z |\-]+`$/u.test(line)) return line;
    throw new Error(`Missing Japanese management help draft: ${line}`);
  }).join("\n");
}

function japaneseHelpLine(line) {
  if (Object.hasOwn(JAPANESE_HELP_TEXT, line)) return JAPANESE_HELP_TEXT[line];
  const description = /^(.* — )(.+)$/u.exec(line);
  if (description) return `${description[1]}${japaneseDiscordDescriptionDraft(description[2])}`;
  // Command spellings, literal option values, and example syntax stay executable.
  if (line.startsWith("Direct syntax: ")) return `直接入力の書式: ${line.slice("Direct syntax: ".length)}`;
  const group = /^Use (`\/help arguments:.+ <subcommand>`) for each command's input syntax\.$/u.exec(line);
  if (group) return `各コマンドの入力方法は${group[1]}で確認できます。`;
  if (line === "Use `/help arguments:<command> <subcommand>` for exact syntax. Omit a board option to enter a multiline grid in the guided form; direct grids use `grid:top-row/next-row`.") {
    return "正確な書式は`/help arguments:<command> <subcommand>`で確認できます。複数行の格子は盤面オプションを省略してフォームで入力します。直接入力には`grid:top-row/next-row`を使います。";
  }
  const unknownObjective = /^Unknown objective (`.+`)\. Use `\/help arguments:objective` to list objective kinds\.$/u.exec(line);
  if (unknownObjective) return `不明な目標関数${unknownObjective[1]}です。種類の一覧は\`/help arguments:objective\`で確認できます。`;
  const unknownCommand = /^Unknown Clearra command (`.+`)\. Use `\/help` to list commands\.$/u.exec(line);
  if (unknownCommand) return `不明なClearraコマンド${unknownCommand[1]}です。コマンド一覧は\`/help\`で確認できます。`;
  throw new Error(`Missing Japanese Discord help draft: ${line}`);
}

export function japaneseDiscordModalTextDraft(text) {
  if (Object.hasOwn(JAPANESE_MODAL_TEXT, text)) return JAPANESE_MODAL_TEXT[text];
  if (Object.hasOwn(JAPANESE_CHOICES, text)) return JAPANESE_CHOICES[text];
  if (Object.hasOwn(JAPANESE_NAMES, text)) return JAPANESE_NAMES[text];
  if (Object.hasOwn(JAPANESE_DESCRIPTIONS, text)) return JAPANESE_DESCRIPTIONS[text];
  // These are literal input examples, never translated prose or fallback text.
  if (text === "" || /^[\d_\n]+$/u.test(text) || text === "IOTSZJL") return text;
  throw new Error(`Missing Japanese Discord modal draft: ${text}`);
}

export function buildJapaneseCommandModalDraft(interaction) {
  const response = buildCommandModalResponse(interaction, "en");
  if (response === null) return null;
  return translateModalNode(response);
}

function translateModalNode(node) {
  if (Array.isArray(node)) return node.map(translateModalNode);
  if (node === null || typeof node !== "object") return node;
  return Object.fromEntries(Object.entries(node).map(([key, value]) => {
    if (["label", "description", "placeholder"].includes(key) && typeof value === "string") {
      return [key, japaneseDiscordModalTextDraft(value)];
    }
    if (key === "title") {
      const command = value.replace(/ form$/u, "");
      return [key, `${command.split(" ").map(japaneseDiscordNameDraft).join(" ")}の入力`];
    }
    return [key, translateModalNode(value)];
  }));
}

function requiredTranslation(catalog, text, surface) {
  if (Object.hasOwn(catalog, text)) return catalog[text];
  throw new Error(`Missing Japanese Discord ${surface} draft: ${text}`);
}
