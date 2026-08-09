import {
  findApplicationCommand,
  formatSlashCommandHelp,
} from "../discord/slash-command-catalog.mjs";
import {
  buildCommandModalResponse,
  findCommandModalCommand,
} from "../discord/field-modal.mjs";
import { modalErrorText, t, validationErrorText } from "../discord/i18n.mjs";
import { readHelpArgument } from "../discord/slash-command-input.mjs";
import { writeOperationalLog } from "../operational-log.mjs";

const APPLICATION_COMMAND_INTERACTION = 2;
const MODAL_SUBMIT_INTERACTION = 5;
const EPHEMERAL_MESSAGE_FLAG = 1 << 6;
const DEFAULT_INTERACTION_TTL_MS = 16 * 60_000;
const DEFAULT_MAX_REMEMBERED_INTERACTIONS = 4_096;

export class SlashCommandIngress {
  constructor(bot, options = {}) {
    this.bot = bot;
    this.acknowledger = options.acknowledger;
    this.now = options.now ?? Date.now;
    this.interactionTtlMs = options.interactionTtlMs ?? DEFAULT_INTERACTION_TTL_MS;
    this.maxRememberedInteractions =
      options.maxRememberedInteractions ?? DEFAULT_MAX_REMEMBERED_INTERACTIONS;
    this.operationalScope = options.operationalScope ?? null;
    this.logger = options.logger ?? console;
    this.seenInteractions = new Map();
  }

  accepts(interaction) {
    return Boolean(
      findCommandModalCommand(interaction) ||
      (
      interaction?.type === APPLICATION_COMMAND_INTERACTION &&
      findApplicationCommand(
        interaction.data?.type,
        interaction.data?.name,
      ) !== null
      )
    );
  }

  initialResponse(interaction) {
    const decision = this.bot.interactionAccessDecision?.(interaction);
    const locale = this.localeFor(interaction);
    if (decision?.allowed === false) {
      return gatewayInteractionErrorResponse(
        this.bot.accessBlockedText?.(decision, locale) ??
          t(locale, "access.channel_disabled"),
      );
    }
    const command = findCommandModalCommand(interaction) ??
      findApplicationCommand(
        interaction?.data?.type,
        interaction?.data?.name,
      );
    if (command?.kind === "help") {
      try {
        const requestedName = readHelpArgument(interaction.data?.options ?? []);
        return gatewayInteractionMessageResponse(
          formatSlashCommandHelp(requestedName, locale),
        );
      } catch (error) {
        return gatewayInteractionErrorResponse(validationErrorText(error, locale));
      }
    }
    return buildCommandModalResponse(interaction, locale);
  }

  localeFor(interaction) {
    return this.bot.resolveResponseLocale?.(interaction)?.locale ??
      this.bot.resolveLocale?.(interaction)?.locale ?? "en";
  }

  initialResponseError(interaction) {
    return modalErrorText(this.localeFor(interaction));
  }

  unsupportedCommandText(interaction) {
    return t(this.localeFor(interaction), "error.unsupported_command");
  }

  deferredResponse(interaction) {
    const command = findApplicationCommand(
      interaction?.data?.type,
      interaction?.data?.name,
    );
    return command?.kind === "management"
      ? { type: 5, data: { flags: EPHEMERAL_MESSAGE_FLAG } }
      : { type: 5 };
  }

  claim(interaction) {
    return this.#rememberInteraction(interaction?.id);
  }

  releaseClaim(interaction) {
    const id = interaction?.id;
    return typeof id === "string" && id.length > 0
      ? this.seenInteractions.delete(id)
      : false;
  }

  async accept(interaction, options = {}) {
    if (!this.accepts(interaction)) {
      return { accepted: false, reason: "slash-command-disabled" };
    }
    if (options.claimed !== true && !this.claim(interaction)) {
      return { accepted: false, reason: "duplicate-interaction" };
    }
    const acknowledger = options.acknowledger ?? this.acknowledger;
    if (!acknowledger) {
      throw new Error("A Discord interaction acknowledger is required.");
    }
    let deferred = false;
    const deferAwareAcknowledger = {
      async defer(target, deferOptions = {}) {
        await acknowledger.defer(target, deferOptions);
        deferred = true;
      },
    };
    const operationStartedAt = numericClock(this.now);
    let operationStatus = "succeeded";
    try {
      const handled = await this.bot.handleInteraction(interaction, {
        acknowledger: deferAwareAcknowledger,
      });
      if (handled !== true) {
        throw new Error("An accepted slash command did not reach a terminal handler.");
      }
    } catch (error) {
      operationStatus = "failed";
      if (!deferred || typeof this.bot.handleInteractionFailure !== "function") {
        throw error;
      }
      await this.bot.handleInteractionFailure(interaction, error);
    } finally {
      if (this.operationalScope) {
        const completedAt = numericClock(this.now);
        writeOperationalLog(this.logger, {
          scope: this.operationalScope,
          kind: "slash",
          command: interactionCommandPath(interaction),
          status: operationStatus,
          durationMs:
            operationStartedAt === null || completedAt === null
              ? null
              : completedAt - operationStartedAt,
        });
      }
    }
    return { accepted: true };
  }

  async acceptDispatch(type, data, options = {}) {
    if (type !== "INTERACTION_CREATE") {
      return { accepted: false, reason: "gateway-message-events-disabled" };
    }
    if (!this.accepts(data)) {
      return { accepted: false, reason: "slash-command-disabled" };
    }
    if (!this.claim(data)) {
      return { accepted: false, reason: "duplicate-interaction" };
    }

    const acknowledger = options.acknowledger ?? this.acknowledger;
    if (!acknowledger) {
      throw new Error("A Discord interaction acknowledger is required.");
    }

    let initialResponse;
    try {
      initialResponse = this.initialResponse(data);
    } catch (error) {
      initialResponse = gatewayInteractionErrorResponse(
        this.initialResponseError(data),
      );
    }
    if (initialResponse) {
      if (typeof acknowledger.respond !== "function") {
        throw new Error("The Discord interaction acknowledger cannot send a Modal response.");
      }
      await acknowledger.respond(data, initialResponse);
      return { accepted: true };
    }

    return this.accept(data, { acknowledger, claimed: true, admitted: true });
  }

  #rememberInteraction(id) {
    if (typeof id !== "string" || id.length === 0) return true;
    const now = this.now();
    const cutoff = now - this.interactionTtlMs;
    for (const [candidateId, acceptedAt] of this.seenInteractions) {
      if (acceptedAt > cutoff) break;
      this.seenInteractions.delete(candidateId);
    }
    if (this.seenInteractions.has(id)) return false;
    this.seenInteractions.set(id, now);
    while (this.seenInteractions.size > this.maxRememberedInteractions) {
      this.seenInteractions.delete(this.seenInteractions.keys().next().value);
    }
    return true;
  }
}

function interactionCommandPath(interaction) {
  const modalCommand = findCommandModalCommand(interaction);
  const root = interaction?.type === APPLICATION_COMMAND_INTERACTION
    ? interaction?.data?.name
    : modalCommand?.rootName ?? modalCommand?.name;
  if (!safeCommandPart(root)) return null;
  const names = [root];
  if (modalCommand?.subcommand && safeCommandPart(modalCommand.subcommand)) {
    names.push(modalCommand.subcommand);
  }
  let options = interaction?.data?.options;
  while (Array.isArray(options) && options.length === 1) {
    const option = options[0];
    if ((option?.type !== 1 && option?.type !== 2) || !safeCommandPart(option.name)) {
      break;
    }
    names.push(option.name);
    options = option.options;
  }
  return names.slice(0, 3).join(".");
}

function safeCommandPart(value) {
  return typeof value === "string" && /^[a-z0-9][a-z0-9_-]{0,31}$/.test(value);
}

function numericClock(clock) {
  try {
    const value = Number(clock());
    return Number.isFinite(value) ? value : null;
  } catch {
    return null;
  }
}

export function isEnabledSlashCommand(interaction) {
  return (
    (interaction?.type === APPLICATION_COMMAND_INTERACTION &&
      findApplicationCommand(
        interaction.data?.type,
        interaction.data?.name,
      ) !== null) ||
    (interaction?.type === MODAL_SUBMIT_INTERACTION &&
      findCommandModalCommand(interaction) !== null)
  );
}

function gatewayInteractionErrorResponse(message) {
  const response = gatewayInteractionMessageResponse(message);
  response.data.flags = EPHEMERAL_MESSAGE_FLAG;
  return response;
}

function gatewayInteractionMessageResponse(message) {
  return {
    type: 4,
    data: {
      content: String(message).slice(0, 1900),
      allowed_mentions: { parse: [] },
    },
  };
}
