import assert from "node:assert/strict";
import { createPrivateKey, createPublicKey, sign } from "node:crypto";
import test from "node:test";

import {
  PC4_PUBLIC_KEYRING_SCHEMA,
  canonicalText,
  keyIdForPublicKey,
  publicKeyHex,
} from "./activation-envelope-contract.mjs";
import {
  sealProductionHostGeneration,
  verifyProductionHostGenerationEnvelope,
} from "./seal-production-host-generation.mjs";
import {
  compileProductionRolloutPointer,
  verifyProductionRolloutPointer,
} from "./compile-production-rollout-pointer.mjs";
import { verifyAndLinkProductionActivation } from "./verify-production-activation.mjs";

// RFC 8032 test vector 1. This is public test material and is created only in
// memory; production key loading is intentionally outside these pure modules.
const TEST_SEED = "9d61b19deffd5a60ba844af492ec2cc4" +
  "4449c5697b326919703bac031cae7f60";
const TEST_PKCS8 = `302e020100300506032b657004220420${TEST_SEED}`;
const privateKey = createPrivateKey({
  key: Buffer.from(TEST_PKCS8, "hex"),
  format: "der",
  type: "pkcs8",
});
const publicKey = createPublicKey(privateKey);
const keyId = keyIdForPublicKey(publicKey);
const keyring = {
  schema: PC4_PUBLIC_KEYRING_SCHEMA,
  keys: [{
    algorithm: "ed25519",
    key_id: keyId,
    public_key_hex: publicKeyHex(publicKey),
    status: "active",
  }],
};
const ID = (character) => `sha256:${character.repeat(64)}`;
const COMPATIBILITY = ID("c");

test("Node Ed25519 matches the RFC 8032 empty-message KAT used by Rust", () => {
  assert.equal(sign(null, Buffer.alloc(0), privateKey).toString("hex"),
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155" +
    "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
});

test("seals deterministic canonical bytes and rejects every canonical text mutation", () => {
  const { authority } = fixture();
  const first = seal(authority);
  const second = seal(authority);
  assert.equal(first, second);
  assert.equal(first, canonicalText(JSON.parse(first)));
  assert.equal(verifyProductionHostGenerationEnvelope(first, keyring).generation.profiles.length, 5);

  for (const changed of [
    first.slice(0, -1),
    first.replaceAll("\n", "\r\n"),
    `${first}\n`,
    `\ufeff${first}`,
    first.replace('{"schema":', '{"schema": '),
  ]) {
    assert.throws(() => verifyProductionHostGenerationEnvelope(changed, keyring),
      /canonical|JSON|signature/u);
  }
});

test("one qualified profile links while four typed unavailable profiles remain unavailable", () => {
  const { authority, reader } = fixture();
  const generationEnvelope = seal(authority);
  const rollout = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: generationEnvelope,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "100",
    expiresAtUnixSeconds: "200",
  });
  const linked = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(reader),
    signedGenerationEnvelopeText: generationEnvelope,
    signedRolloutEnvelopeText: rollout,
    publicKeyring: keyring,
    nowUnixSeconds: 150,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });
  assert.equal(linked.replayDecision, "initial");
  assert.deepEqual(linked.profiles.map(({ profile, status }) => [profile, status]), [
    ["srs", "unavailable"],
    ["srs-plus", "unavailable"],
    ["srs-x", "unavailable"],
    ["jstris-180", "ready"],
    ["no-kick", "unavailable"],
  ]);
});

test("reader target receipts cannot mint authority and PC authority cannot enable Setup", () => {
  const { authority, reader } = fixture();
  const generationEnvelope = seal(authority);
  const rollout = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: generationEnvelope,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "100",
    expiresAtUnixSeconds: "200",
  });
  const linked = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(reader),
    signedGenerationEnvelopeText: generationEnvelope,
    signedRolloutEnvelopeText: rollout,
    publicKeyring: keyring,
    nowUnixSeconds: 150,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });
  const active = linked.profiles.find(({ profile }) => profile === "jstris-180");
  assert.deepEqual(active.pc_search_target_lines, [4]);
  assert.deepEqual(active.setup_search_target_lines, []);

  const injected = structuredClone(reader);
  injected.profiles[3].pc_search_target_lines = [4];
  injected.profiles[3].target_qualification_receipts = [pcReceipt("jstris-180")];
  assert.throws(() => verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(injected),
    signedGenerationEnvelopeText: generationEnvelope,
    signedRolloutEnvelopeText: rollout,
    publicKeyring: keyring,
    nowUnixSeconds: 150,
    expectedCompatibilityIdentity: COMPATIBILITY,
  }), /mint target authority/u);

  const withSetup = fixture({ setup: true });
  const setupEnvelope = seal(withSetup.authority);
  const setupRollout = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: setupEnvelope,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "100",
    expiresAtUnixSeconds: "200",
  });
  const setupLinked = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(withSetup.reader),
    signedGenerationEnvelopeText: setupEnvelope,
    signedRolloutEnvelopeText: setupRollout,
    publicKeyring: keyring,
    nowUnixSeconds: 150,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });
  assert.deepEqual(
    setupLinked.profiles.find(({ profile }) => profile === "jstris-180")
      .setup_search_target_lines,
    [4],
  );
});

test("rollout sequence rejects replay and allows only newly signed retained rollback", () => {
  const firstFixture = fixture({ generation: "one" });
  const firstGeneration = seal(firstFixture.authority);
  const firstPointer = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: firstGeneration,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "100",
    expiresAtUnixSeconds: "400",
    rollbackLimit: 2,
  });
  const first = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(firstFixture.reader),
    signedGenerationEnvelopeText: firstGeneration,
    signedRolloutEnvelopeText: firstPointer,
    publicKeyring: keyring,
    nowUnixSeconds: 150,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });

  const secondFixture = fixture({ generation: "two" });
  const secondGeneration = seal(secondFixture.authority);
  const secondPointer = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: secondGeneration,
    previousRolloutEnvelopeText: firstPointer,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "160",
    expiresAtUnixSeconds: "400",
    rollbackLimit: 2,
  });
  const second = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(secondFixture.reader),
    signedGenerationEnvelopeText: secondGeneration,
    signedRolloutEnvelopeText: secondPointer,
    publicKeyring: keyring,
    replayState: first.nextReplayState,
    nowUnixSeconds: 170,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });
  assert.equal(second.replayDecision, "advanced");
  assert.throws(() => verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(firstFixture.reader),
    signedGenerationEnvelopeText: firstGeneration,
    signedRolloutEnvelopeText: firstPointer,
    publicKeyring: keyring,
    replayState: second.nextReplayState,
    nowUnixSeconds: 180,
    expectedCompatibilityIdentity: COMPATIBILITY,
  }), /replay/u);

  const rollbackPointer = compileProductionRolloutPointer({
    selectedGenerationEnvelopeText: firstGeneration,
    previousRolloutEnvelopeText: secondPointer,
    privateKey,
    publicKeyring: keyring,
    issuedAtUnixSeconds: "180",
    expiresAtUnixSeconds: "400",
    rollbackLimit: 2,
  });
  const rollback = verifyAndLinkProductionActivation({
    readerGenerationText: canonicalText(firstFixture.reader),
    signedGenerationEnvelopeText: firstGeneration,
    signedRolloutEnvelopeText: rollbackPointer,
    publicKeyring: keyring,
    replayState: second.nextReplayState,
    nowUnixSeconds: 190,
    expectedCompatibilityIdentity: COMPATIBILITY,
  });
  assert.equal(rollback.replayDecision, "advanced");
  assert.equal(verifyProductionRolloutPointer(rollbackPointer, keyring)
    .statement.rollout_sequence, "3");
});

test("tampered generation, statement, signature, and unpinned key all fail closed", () => {
  const { authority } = fixture();
  const envelope = seal(authority);
  const parsed = JSON.parse(envelope);
  for (const mutate of [
    (value) => { value.statement = value.statement.replace('"channel":"production"', '"channel":"staging"'); },
    (value) => { value.signature_hex = `${value.signature_hex.slice(0, -1)}0`; },
    (value) => { value.statement = value.statement.replace('"generation_id":"one"', '"generation_id":"evil"'); },
  ]) {
    const changed = structuredClone(parsed);
    mutate(changed);
    assert.throws(() => verifyProductionHostGenerationEnvelope(canonicalText(changed), keyring),
      /signature|bind/u);
  }
  const wrongRing = structuredClone(keyring);
  wrongRing.keys[0].key_id = `ed25519-raw-sha256:${"f".repeat(64)}`;
  assert.throws(() => verifyProductionHostGenerationEnvelope(envelope, wrongRing),
    /identity|untrusted/u);
});

function seal(authority) {
  return sealProductionHostGeneration({
    hostGenerationText: canonicalText(authority),
    privateKey,
    publicKeyring: keyring,
    compatibilityIdentity: COMPATIBILITY,
  });
}

function fixture({ setup = false, generation = "one" } = {}) {
  const profiles = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
  const readerProfiles = profiles.map((profile) => profile === "jstris-180"
    ? readerReady(profile)
    : unavailable(profile));
  const authorityProfiles = profiles.map((profile) => profile === "jstris-180"
    ? authorityReady(profile, setup)
    : unavailable(profile));
  return {
    reader: {
      schema: "clearra.pc4.host-generation.v1",
      repository: "muse918/tetris-4lpc-mdp-vstar-policy",
      revision: "a".repeat(40),
      profiles: readerProfiles,
      transferred_bytes: 0,
    },
    authority: {
      schema: "clearra.pc4.host-generation.v1",
      repository: "muse918/tetris-4lpc-mdp-vstar-policy",
      revision: "a".repeat(40),
      admission: {
        generation_id: generation,
        manifest_content_identity: generation === "one" ? ID("d") : ID("e"),
      },
      profiles: authorityProfiles,
      transferred_bytes: 0,
    },
  };
}

function unavailable(profile) {
  return { profile, upstream_complete: false, status: "unavailable", reason: "not-admitted" };
}

function baseReady(profile) {
  return {
    profile,
    upstream_complete: true,
    status: "ready",
    reader_contract: "hydra-jstris-180-complete-graph-v1",
    field_count: 817740,
    target_width: 3,
    target_lines: [4],
    terminal_id: 817739,
    artifacts: {
      fields: { path: "field_hash_to_id.bin", byte_length: 6541936, content_identity: ID("1") },
      offsets: { path: "graph_offsets.bin", byte_length: 6541936, content_identity: ID("2") },
      graph: { path: "graph.bin", byte_length: 511000000, content_identity: ID("3") },
    },
    evidence: { reader: ID("4") },
  };
}

function readerReady(profile) {
  return {
    ...baseReady(profile),
    pc_search_target_lines: [],
    setup_search_target_lines: [],
    target_qualification_receipts: [],
  };
}

function authorityReady(profile, setup) {
  const receipts = [pcReceipt(profile)];
  if (setup) receipts.push(setupReceipt(profile));
  return {
    ...baseReady(profile),
    pc_search_target_lines: [4],
    setup_search_target_lines: setup ? [4] : [],
    target_qualification_receipts: receipts,
    admission: { profile_binding_identity: ID("5") },
  };
}

function pcReceipt(profile) {
  return {
    schema: "clearra.pc4.exact-target-qualification.v1",
    profile,
    use_case: "pc-search",
    target_lines: 4,
  };
}

function setupReceipt(profile) {
  return {
    schema: "clearra.pc4.exact-setup-target-qualification.v1",
    profile,
    use_case: "setup-search",
    target_lines: 4,
  };
}
