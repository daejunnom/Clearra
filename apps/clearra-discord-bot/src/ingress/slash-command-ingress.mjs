const APPLICATION_COMMAND_INTERACTION = 2;
const CHAT_INPUT_COMMAND = 1;
const ENABLED_COMMANDS = new Set(["clearra", "view"]);

export class SlashCommandIngress {
  constructor(bot, options = {}) {
    this.bot = bot;
    this.acknowledger = options.acknowledger;
  }

  accepts(interaction) {
    return (
      interaction?.type === APPLICATION_COMMAND_INTERACTION &&
      interaction.data?.type === CHAT_INPUT_COMMAND &&
      ENABLED_COMMANDS.has(interaction.data?.name)
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
    await this.bot.handleInteraction(interaction, { acknowledger });
    return { accepted: true };
  }

  acceptDispatch(type, data, options = {}) {
    if (type !== "INTERACTION_CREATE") {
      return Promise.resolve({
        accepted: false,
        reason: "gateway-message-events-disabled",
      });
    }
    return this.accept(data, options);
  }
}

export function isEnabledSlashCommand(interaction) {
  return (
    interaction?.type === APPLICATION_COMMAND_INTERACTION &&
    interaction.data?.type === CHAT_INPUT_COMMAND &&
    ENABLED_COMMANDS.has(interaction.data?.name)
  );
}
