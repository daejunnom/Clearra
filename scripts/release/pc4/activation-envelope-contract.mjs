// SRP: canonical public-key envelope mechanics shared by PC4 release tools.
// No file, environment, network, promotion, or product activation I/O belongs here.

import {
  createHash,
  createPublicKey,
  sign,
  verify,
} from "node:crypto";

import {
  canonicalJson,
  requireExactKeys,
} from "../canonical-release-evidence.mjs";

export const PC4_PUBLIC_KEYRING_SCHEMA = "clearra.pc4.activation-keyring.v1";
export const PC4_SIGNATURE_ALGORITHM = "ed25519";
export const PC4_GENERATION_ENVELOPE_SCHEMA =
  "clearra.pc4.signed-generation-envelope.v1";
export const PC4_GENERATION_STATEMENT_SCHEMA =
  "clearra.pc4.production-generation-statement.v1";
export const PC4_ROLLOUT_ENVELOPE_SCHEMA =
  "clearra.pc4.signed-rollout-envelope.v1";
export const PC4_ROLLOUT_STATEMENT_SCHEMA =
  "clearra.pc4.production-rollout-statement.v1";
export const PC4_GENERATION_SIGNATURE_DOMAIN =
  "clearra.pc4.production-generation-statement.v1\0";
export const PC4_ROLLOUT_SIGNATURE_DOMAIN =
  "clearra.pc4.production-rollout-statement.v1\0";
export const PC4_PRODUCTION_CHANNEL = "production";
export const PC4_MAX_ROLLBACK_GENERATIONS = 5;

const LOWER_HEX = /^[0-9a-f]+$/u;
const SHA256_IDENTITY = /^sha256:[0-9a-f]{64}$/u;

export function canonicalText(value) {
  return `${canonicalJson(value)}\n`;
}

export function assertCanonicalText(text, label, maximumBytes = 196_608) {
  if (typeof text !== "string" || text.length === 0 || text.includes("\r") ||
      text.startsWith("\ufeff") || !text.endsWith("\n") || text.endsWith("\n\n") ||
      Buffer.byteLength(text, "utf8") > maximumBytes) {
    throw new Error(`${label} is not canonical UTF-8 JSON with one final LF`);
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error(`${label} is not JSON`);
  }
  if (canonicalText(value) !== text) {
    throw new Error(`${label} differs from its canonical bytes`);
  }
  return value;
}

export function sha256Identity(bytes) {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

export function keyIdForPublicKey(publicKey) {
  const object = publicKey?.type === "public" ? publicKey : createPublicKey(publicKey);
  if (object.asymmetricKeyType !== "ed25519") {
    throw new Error("PC4 activation public key is not Ed25519");
  }
  const jwk = object.export({ format: "jwk" });
  if (jwk.kty !== "OKP" || jwk.crv !== "Ed25519" || typeof jwk.x !== "string") {
    throw new Error("PC4 activation public key cannot be exported as Ed25519 JWK");
  }
  const raw = Buffer.from(jwk.x, "base64url");
  if (raw.length !== 32) throw new Error("PC4 activation public key has the wrong length");
  return `ed25519-raw-sha256:${createHash("sha256").update(raw).digest("hex")}`;
}

export function publicKeyHex(publicKey) {
  const object = publicKey?.type === "public" ? publicKey : createPublicKey(publicKey);
  const jwk = object.export({ format: "jwk" });
  const raw = Buffer.from(jwk.x, "base64url");
  if (raw.length !== 32) throw new Error("PC4 activation public key has the wrong length");
  return raw.toString("hex");
}

export function validatePublicKeyring(value) {
  requireExactKeys(value, ["schema", "keys"], "PC4 activation public keyring");
  if (value.schema !== PC4_PUBLIC_KEYRING_SCHEMA || !Array.isArray(value.keys) ||
      value.keys.length < 1 || value.keys.length > 8) {
    throw new Error("PC4 activation public keyring is invalid");
  }
  const identities = new Set();
  const keys = value.keys.map((entry) => {
    requireExactKeys(entry, ["algorithm", "key_id", "public_key_hex", "status"],
      "PC4 activation public keyring entry");
    if (entry.algorithm !== PC4_SIGNATURE_ALGORITHM ||
        !["active", "retiring"].includes(entry.status) ||
        typeof entry.public_key_hex !== "string" || entry.public_key_hex.length !== 64 ||
        !LOWER_HEX.test(entry.public_key_hex)) {
      throw new Error("PC4 activation public keyring entry is invalid");
    }
    const raw = Buffer.from(entry.public_key_hex, "hex");
    const expected = `ed25519-raw-sha256:${createHash("sha256").update(raw).digest("hex")}`;
    if (entry.key_id !== expected || identities.has(entry.key_id)) {
      throw new Error("PC4 activation public key identity is invalid or duplicated");
    }
    identities.add(entry.key_id);
    const spki = Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      raw,
    ]);
    return { ...entry, publicKey: createPublicKey({ key: spki, format: "der", type: "spki" }) };
  });
  return { schema: value.schema, keys };
}

export function sealStatement({
  envelopeSchema,
  statement,
  statementSchema,
  domain,
  privateKey,
  publicKeyring,
}) {
  if (!privateKey || privateKey.type !== "private" || privateKey.asymmetricKeyType !== "ed25519") {
    throw new Error("PC4 activation sealer requires an in-memory Ed25519 private KeyObject");
  }
  const keyring = validatePublicKeyring(publicKeyring);
  const keyId = keyIdForPublicKey(privateKey);
  const pinned = keyring.keys.find((entry) => entry.key_id === keyId && entry.status === "active");
  if (!pinned) throw new Error("PC4 activation signing key is not an active pinned key");
  requireExactString(statement.key_id, keyId, "PC4 activation statement key ID");
  requireExactString(statement.algorithm, PC4_SIGNATURE_ALGORITHM,
    "PC4 activation statement algorithm");
  requireExactString(statement.schema, statementSchema, "PC4 activation statement schema");
  const statementText = canonicalText(statement);
  const preimage = Buffer.concat([Buffer.from(domain, "utf8"), Buffer.from(statementText, "utf8")]);
  const signature = sign(null, preimage, privateKey);
  if (signature.length !== 64) throw new Error("PC4 activation signature length is invalid");
  return canonicalText({
    schema: envelopeSchema,
    statement: statementText,
    signature_hex: signature.toString("hex"),
  });
}

export function verifyStatementEnvelope({
  envelopeText,
  envelopeSchema,
  statementSchema,
  domain,
  publicKeyring,
}) {
  const envelope = assertCanonicalText(envelopeText, "PC4 activation envelope");
  requireExactKeys(envelope, ["schema", "statement", "signature_hex"],
    "PC4 activation envelope");
  requireExactString(envelope.schema, envelopeSchema, "PC4 activation envelope schema");
  if (typeof envelope.signature_hex !== "string" || envelope.signature_hex.length !== 128 ||
      !LOWER_HEX.test(envelope.signature_hex)) {
    throw new Error("PC4 activation signature encoding is invalid");
  }
  const statement = assertCanonicalText(envelope.statement, "PC4 activation statement");
  requireExactString(statement.schema, statementSchema, "PC4 activation statement schema");
  requireExactString(statement.algorithm, PC4_SIGNATURE_ALGORITHM,
    "PC4 activation statement algorithm");
  const keyring = validatePublicKeyring(publicKeyring);
  const key = keyring.keys.find((entry) => entry.key_id === statement.key_id);
  if (!key) throw new Error("PC4 activation statement uses an untrusted key");
  const preimage = Buffer.concat([Buffer.from(domain, "utf8"), Buffer.from(envelope.statement, "utf8")]);
  if (!verify(null, preimage, key.publicKey, Buffer.from(envelope.signature_hex, "hex"))) {
    throw new Error("PC4 activation signature verification failed");
  }
  return {
    envelope,
    statement,
    statementIdentity: sha256Identity(Buffer.from(envelope.statement, "utf8")),
  };
}

export function requireSha256Identity(value, label) {
  if (typeof value !== "string" || !SHA256_IDENTITY.test(value) || /^sha256:0+$/u.test(value)) {
    throw new Error(`${label} is not an exact SHA-256 identity`);
  }
  return value;
}

export function requireCanonicalUnsignedInteger(value, label) {
  if (typeof value !== "string" || !/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    throw new Error(`${label} is not a canonical unsigned integer`);
  }
  const parsed = BigInt(value);
  if (parsed > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`${label} exceeds the current product integer bound`);
  }
  return Number(parsed);
}

function requireExactString(value, expected, label) {
  if (value !== expected) throw new Error(`${label} is invalid`);
}
