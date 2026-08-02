export const DISCORD_RELAY_PROTOCOL = "clearra.discord.relay.v1";

export class DisabledMessageIngress {
  async accept() {
    return { accepted: false, reason: "ordinary-messages-disabled" };
  }
}

export class DiscordRelayIngressAdapter {
  constructor(options) {
    this.slashCommandIngress = options.slashCommandIngress;
    this.predeferredAcknowledger = options.predeferredAcknowledger;
    this.messageIngress = options.messageIngress ?? new DisabledMessageIngress();
  }

  accept(envelope) {
    validateEnvelope(envelope);
    if (envelope.event.kind === "discord.interaction.create") {
      return Promise.resolve({
        accepted: false,
        reason: "relayed-slash-commands-disabled",
      });
    }
    if (envelope.event.kind === "discord.message.create") {
      return this.messageIngress.accept(envelope.event.payload, envelope);
    }
    return Promise.resolve({ accepted: false, reason: "relay-event-disabled" });
  }
}

function validateEnvelope(envelope) {
  if (!envelope || typeof envelope !== "object") {
    throw new TypeError("The Discord relay envelope is invalid.");
  }
  if (envelope.protocol !== DISCORD_RELAY_PROTOCOL) {
    throw new Error("The Discord relay protocol is unsupported.");
  }
  if (!envelope.deliveryId || typeof envelope.deliveryId !== "string") {
    throw new Error("The Discord relay delivery ID is required.");
  }
  if (!envelope.event || typeof envelope.event !== "object") {
    throw new Error("The Discord relay event is required.");
  }
}
