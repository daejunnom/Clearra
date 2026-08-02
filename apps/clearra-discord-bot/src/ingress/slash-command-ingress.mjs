import { findSlashCommand } from "../discord/slash-command-catalog.mjs";

const APPLICATION_COMMAND_INTERACTION = 2;
const CHAT_INPUT_COMMAND = 1;

export class SlashCommandIngress {
  constructor(bot, options = {}) {
    this.bot = bot;
    this.acknowledger = options.acknowledger;
  }

  accepts(interaction) {
    return (
      interaction?.type === APPLICATION_COMMAND_INTERACTION &&
      interaction.data?.type === CHAT_INPUT_COMMAND &&
      findSlashCommand(interaction.data?.name) !== null
    );
  }

  async accept(interaction, options = {}) {
    if (!this.accepts(interaction)) {
      return { accepted: false, reason: "slash-command-disabled" };
    }
    const acknowledger = options.acknowledger ?? this.acknowledger;
    if (!acknowledger) {
      throw new Error("A Discord interaction acknowledger is required.");
    }
    let deferred = false;
    const trackingAcknowledger = {
      async defer(target) {
        await acknowledger.defer(target);
        deferred = true;
      },
    };
    try {
      const handled = await this.bot.handleInteraction(interaction, {
        acknowledger: trackingAcknowledger,
      });
      if (handled !== true) {
        throw new Error("An accepted slash command did not reach a terminal handler.");
      }
    } catch (error) {
      if (!deferred || typeof this.bot.handleInteractionFailure !== "function") {
        throw error;
      }
      await this.bot.handleInteractionFailure(interaction, error);
    }
    return { accepted: true };
  }

  acceptDispatch(type, data, options = {}) {
    return Promise.resolve({
      accepted: false,
      reason:
        type === "INTERACTION_CREATE"
          ? "gateway-slash-commands-disabled"
          : "gateway-message-events-disabled",
    });
  }
}

export function isEnabledSlashCommand(interaction) {
  return (
    interaction?.type === APPLICATION_COMMAND_INTERACTION &&
    interaction.data?.type === CHAT_INPUT_COMMAND &&
    findSlashCommand(interaction.data?.name) !== null
  );
}
