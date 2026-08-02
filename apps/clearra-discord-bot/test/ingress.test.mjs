import assert from "node:assert/strict";
import test from "node:test";

import {
  DiscordRelayIngressAdapter,
  DISCORD_RELAY_PROTOCOL,
} from "../src/ingress/relay-adapter.mjs";
import { globalCommands } from "../src/discord/slash-command-catalog.mjs";
import {
  isEnabledSlashCommand,
  SlashCommandIngress,
} from "../src/ingress/slash-command-ingress.mjs";
import { DiscordGateway } from "../src/discord/gateway.mjs";

test("Gateway fallback requests no privileged or message intents", () => {
  assert.equal(new DiscordGateway("test-token").intents, 0);
});

test("slash ingress rejects ordinary Gateway messages", async () => {
  let calls = 0;
  const ingress = new SlashCommandIngress(
    { async handleInteraction() { calls += 1; } },
    { acknowledger: { async defer() {} } },
  );

  assert.deepEqual(
    await ingress.acceptDispatch("MESSAGE_CREATE", { content: "!pc" }),
    { accepted: false, reason: "gateway-message-events-disabled" },
  );
  assert.equal(calls, 0);

  assert.deepEqual(
    await ingress.acceptDispatch("INTERACTION_CREATE", {
      type: 2,
      data: { type: 1, name: "path" },
    }),
    { accepted: false, reason: "gateway-slash-commands-disabled" },
  );
  assert.equal(calls, 0);
});

test("slash ingress accepts exactly the registered command catalog", () => {
  const ingress = new SlashCommandIngress({});
  for (const command of globalCommands) {
    const interaction = {
      type: 2,
      data: { type: 1, name: command.name },
    };
    assert.equal(ingress.accepts(interaction), true);
    assert.equal(isEnabledSlashCommand(interaction), true);
  }
  for (const name of ["clearra", "view", "render", "disabled"]) {
    const interaction = { type: 2, data: { type: 1, name } };
    assert.equal(ingress.accepts(interaction), false);
    assert.equal(isEnabledSlashCommand(interaction), false);
  }
});

test("slash ingress terminates a deferred interaction when its handler throws", async () => {
  const events = [];
  const ingress = new SlashCommandIngress(
    {
      async handleInteraction(interaction, options) {
        await options.acknowledger.defer(interaction);
        events.push("deferred");
        throw new Error("handler failed");
      },
      async handleInteractionFailure(_interaction, error) {
        events.push(`terminal:${error.message}`);
      },
    },
    { acknowledger: { async defer() {} } },
  );

  assert.deepEqual(
    await ingress.accept({ type: 2, data: { type: 1, name: "path" } }),
    { accepted: true },
  );
  assert.deepEqual(events, ["deferred", "terminal:handler failed"]);
});

test("relay adapter keeps slash and text events disabled by default", async () => {
  const forwarded = [];
  const acknowledger = { async defer() {} };
  const adapter = new DiscordRelayIngressAdapter({
    slashCommandIngress: {
      async accept(interaction, options) {
        forwarded.push({ interaction, acknowledger: options.acknowledger });
        return { accepted: true };
      },
    },
    predeferredAcknowledger: acknowledger,
  });
  const interaction = {
    type: 2,
    data: { type: 1, name: "path" },
  };

  assert.deepEqual(
    await adapter.accept({
      protocol: DISCORD_RELAY_PROTOCOL,
      deliveryId: "delivery-1",
      acknowledgement: "deferred",
      event: { kind: "discord.interaction.create", payload: interaction },
    }),
    { accepted: false, reason: "relayed-slash-commands-disabled" },
  );
  assert.deepEqual(forwarded, []);
  assert.deepEqual(
    await adapter.accept({
      protocol: DISCORD_RELAY_PROTOCOL,
      deliveryId: "delivery-2",
      event: { kind: "discord.message.create", payload: { content: "!pc" } },
    }),
    { accepted: false, reason: "ordinary-messages-disabled" },
  );
});
