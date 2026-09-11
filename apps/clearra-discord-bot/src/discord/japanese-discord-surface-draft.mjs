// Full-surface fixtures built from the same Japanese localization helpers used
// by the released Discord product.
import {
  globalCommands,
  slashCommandCatalog,
  messageCommandCatalog,
  formatSlashCommandHelp,
} from "./slash-command-catalog.mjs";
import { buildCommandModalResponse } from "./field-modal.mjs";
import { formatTextManagementHelp } from "./management-command.mjs";
import {
  japaneseDiscordChoiceName as japaneseDiscordChoiceNameDraft,
  japaneseDiscordDescription as japaneseDiscordDescriptionDraft,
  japaneseDiscordModalText as japaneseDiscordModalTextDraft,
  japaneseDiscordName as japaneseDiscordNameDraft,
  formatJapaneseDiscordHelp,
  formatJapaneseTextManagementHelp,
  translateJapaneseModalNode,
} from "./japanese-discord-localization.mjs";

export {
  japaneseDiscordChoiceNameDraft,
  japaneseDiscordDescriptionDraft,
  japaneseDiscordModalTextDraft,
  japaneseDiscordNameDraft,
};

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
  return formatJapaneseDiscordHelp(formatSlashCommandHelp(requestedName, "en"));
}

export function formatJapaneseTextManagementHelpDraft() {
  return formatJapaneseTextManagementHelp(formatTextManagementHelp("en"));
}

export function buildJapaneseCommandModalDraft(interaction) {
  const response = buildCommandModalResponse(interaction, "en");
  if (response === null) return null;
  return translateJapaneseModalNode(response);
}
