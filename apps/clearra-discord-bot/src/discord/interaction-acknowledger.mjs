export class RestInteractionAcknowledger {
  constructor(rest) {
    this.rest = rest;
  }

  defer(interaction) {
    return this.rest.deferInteraction(interaction);
  }
}

export class InlineDeferredInteractionAcknowledger {
  async defer() {}
}
