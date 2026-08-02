import { createPublicKey, verify } from "node:crypto";

const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const DEFAULT_MAX_SIGNATURE_AGE_MS = 5 * 60_000;
const DEFAULT_MAX_FUTURE_SKEW_MS = 30_000;

export class DiscordInteractionSignatureVerifier {
  constructor(publicKeyHex, options = {}) {
    const publicKey = decodeHex(publicKeyHex, 32, "Discord public key");
    this.publicKey = createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, publicKey]),
      format: "der",
      type: "spki",
    });
    this.now = options.now ?? Date.now;
    this.maxSignatureAgeMs = positiveDuration(
      options.maxSignatureAgeMs,
      DEFAULT_MAX_SIGNATURE_AGE_MS,
    );
    this.maxFutureSkewMs = positiveDuration(
      options.maxFutureSkewMs,
      DEFAULT_MAX_FUTURE_SKEW_MS,
    );
  }

  verify(rawBody, signatureHex, timestamp) {
    if (!Buffer.isBuffer(rawBody)) {
      throw new TypeError("The Discord interaction body must be a Buffer.");
    }
    if (typeof timestamp !== "string" || !/^\d{1,12}$/u.test(timestamp)) {
      return false;
    }
    const timestampMs = Number(timestamp) * 1000;
    if (!Number.isSafeInteger(timestampMs)) return false;
    const ageMs = this.now() - timestampMs;
    if (ageMs > this.maxSignatureAgeMs || ageMs < -this.maxFutureSkewMs) {
      return false;
    }
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

function positiveDuration(value, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("The Discord signature time window is invalid.");
  }
  return parsed;
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
