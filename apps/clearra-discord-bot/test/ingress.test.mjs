import assert from "node:assert/strict";
import test from "node:test";

import {
  DiscordRelayIngressAdapter,
  DISCORD_RELAY_PROTOCOL,
} from "../src/ingress/relay-adapter.mjs";
import { SlashCommandIngress } from "../src/ingress/slash-command-ingress.mjs";
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
});

test("relay adapter accepts predeferred slash commands and blocks text by default", async () => {
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
    data: { type: 1, name: "clearra" },
  };

  assert.deepEqual(
    await adapter.accept({
      protocol: DISCORD_RELAY_PROTOCOL,
      deliveryId: "delivery-1",
      acknowledgement: "deferred",
      event: { kind: "discord.interaction.create", payload: interaction },
    }),
    { accepted: true },
  );
  assert.deepEqual(forwarded, [{ interaction, acknowledger }]);
  assert.deepEqual(
    await adapter.accept({
      protocol: DISCORD_RELAY_PROTOCOL,
      deliveryId: "delivery-2",
      event: { kind: "discord.message.create", payload: { content: "!pc" } },
    }),
    { accepted: false, reason: "ordinary-messages-disabled" },
  );
});
