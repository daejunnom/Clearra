import { createPublicKey, verify } from "node:crypto";

const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

export class DiscordInteractionSignatureVerifier {
  constructor(publicKeyHex) {
    const publicKey = decodeHex(publicKeyHex, 32, "Discord public key");
    this.publicKey = createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, publicKey]),
      format: "der",
      type: "spki",
    });
  }

  verify(rawBody, signatureHex, timestamp) {
    if (!Buffer.isBuffer(rawBody)) {
      throw new TypeError("The Discord interaction body must be a Buffer.");
    }
    if (typeof timestamp !== "string" || timestamp.length === 0) return false;
    let signature;
    try {
      signature = decodeHex(signatureHex, 64, "Discord signature");
    } catch {
      return false;
    }
    const signedBody = Buffer.concat([
      Buffer.from(timestamp, "utf8"),
      rawBody,
    ]);
    return verify(null, signedBody, this.publicKey, signature);
  }
}

function decodeHex(value, byteLength, label) {
  if (
    typeof value !== "string" ||
    value.length !== byteLength * 2 ||
    !/^[0-9a-f]+$/iu.test(value)
  ) {
    throw new Error(`${label} is invalid.`);
  }
  return Buffer.from(value, "hex");
}
