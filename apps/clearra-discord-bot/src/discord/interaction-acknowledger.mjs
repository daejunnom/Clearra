export class RestInteractionAcknowledger {
  constructor(rest) {
    this.rest = rest;
  }

  defer(interaction, options = {}) {
    return this.rest.deferInteraction(interaction, options);
  }

  respond(interaction, response) {
    return this.rest.createInteractionResponse(interaction, response);
  }

  async claimDeferred(interaction, options = {}) {
    try {
      await this.defer(interaction, options);
      return true;
    } catch (error) {
      if (error?.discordCode === 40060) return false;
      // A timeout or server error may have committed remotely. Starting work
      // without a definite callback success would weaken the at-most-once
      // execution boundary, so ambiguous failures deliberately propagate.
      throw error;
    }
  }
}

export class InlineDeferredInteractionAcknowledger {
  async defer() {}
}
