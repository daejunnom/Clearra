import assert from "node:assert/strict";
import {
  generateKeyPairSync,
  sign,
} from "node:crypto";
import test from "node:test";

import { DiscordInteractionSignatureVerifier } from "../src/cloud-run/discord-signature.mjs";
import { CloudRunDiscordInteractionAdapter } from "../src/cloud-run/http-adapter.mjs";

test("Discord interaction signatures are verified against the application key", () => {
  const keys = discordSigningKeys();
  const body = Buffer.from('{"type":1}', "utf8");
  const timestamp = "1720000000";
  const signature = sign(
    null,
    Buffer.concat([Buffer.from(timestamp), body]),
    keys.privateKey,
  ).toString("hex");
  const verifier = new DiscordInteractionSignatureVerifier(keys.publicKeyHex, {
    now: () => 1_720_000_000_000,
  });

  assert.equal(verifier.verify(body, signature, timestamp), true);
  assert.equal(
    verifier.verify(Buffer.from('{"type":2}'), signature, timestamp),
    false,
  );
  assert.equal(verifier.verify(body, "00", timestamp), false);
  assert.equal(verifier.verify(body, signature, "1719990000"), false);
});

test("Cloud Run adapter answers Discord PING and defers enabled slash commands", async () => {
  const keys = discordSigningKeys();
  const accepted = [];
  const ingress = {
    accepts(interaction) {
      return interaction?.type === 2 && interaction.data?.name === "path";
    },
    async accept(interaction, options) {
      await options.acknowledger.defer(interaction);
      accepted.push(interaction);
      return { accepted: true };
    },
  };
  const adapter = new CloudRunDiscordInteractionAdapter({
    ingress,
    publicKey: keys.publicKeyHex,
    host: "127.0.0.1",
    port: 0,
    logger: { error() {} },
  });
  const address = await adapter.listen();
  const endpoint = `http://127.0.0.1:${address.port}/interactions`;
  try {
    const health = await fetch(`http://127.0.0.1:${address.port}/health`);
    assert.equal(health.status, 200);
    assert.deepEqual(await health.json(), { status: "ok" });

    const ping = await signedRequest(endpoint, { type: 1 }, keys.privateKey);
    assert.equal(ping.status, 200);
    assert.deepEqual(await ping.json(), { type: 1 });

    const interaction = {
      id: "interaction-id",
      application_id: "application-id",
      token: "interaction-token",
      type: 2,
      data: { type: 1, name: "path", options: [] },
    };
    const deferred = await signedRequest(endpoint, interaction, keys.privateKey);
    assert.equal(deferred.status, 200);
    assert.deepEqual(await deferred.json(), { type: 5 });
    await adapter.drain();
    assert.deepEqual(accepted, [interaction]);

    const disabled = await signedRequest(
      endpoint,
      { ...interaction, data: { type: 1, name: "disabled" } },
      keys.privateKey,
    );
    assert.equal(disabled.status, 200);
    const disabledBody = await disabled.json();
    assert.equal(disabledBody.type, 4);
    assert.match(disabledBody.data.content, /slash commands/iu);

    const invalid = await fetch(endpoint, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-signature-ed25519": "00".repeat(64),
        "x-signature-timestamp": "1720000000",
      },
      body: JSON.stringify({ type: 1 }),
    });
    assert.equal(invalid.status, 401);
  } finally {
    await adapter.close();
  }
});

function discordSigningKeys() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const spki = publicKey.export({ format: "der", type: "spki" });
  return {
    privateKey,
    publicKeyHex: spki.subarray(spki.byteLength - 32).toString("hex"),
  };
}

function signedRequest(endpoint, payload, privateKey) {
  const body = JSON.stringify(payload);
  const timestamp = String(Math.floor(Date.now() / 1000));
  const signature = sign(
    null,
    Buffer.concat([Buffer.from(timestamp), Buffer.from(body)]),
    privateKey,
  ).toString("hex");
  return fetch(endpoint, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-signature-ed25519": signature,
      "x-signature-timestamp": timestamp,
    },
    body,
  });
}
