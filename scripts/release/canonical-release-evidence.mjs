import { createHash } from "node:crypto";

const SECRET_KEY =
  /(?:^|_)(?:secret|token|password|credential|api_key|private_key)(?:_|$)/iu;

export function canonicalJson(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("canonical JSON forbids non-finite numbers");
    }
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((entry) => canonicalJson(entry)).join(",")}]`;
  }
  requirePlainObject(value, "canonical JSON value");
  const fields = Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`,
  );
  return `{${fields.join(",")}}`;
}

export function canonicalSha256(value) {
  return createHash("sha256")
    .update(canonicalJson(value), "utf8")
    .digest("hex");
}

export function sealCanonicalReport(unsignedReport) {
  requirePlainObject(unsignedReport, "canonical report");
  if (Object.hasOwn(unsignedReport, "report_sha256")) {
    throw new Error("canonical report must not be pre-sealed");
  }
  rejectSecretMaterial(unsignedReport);
  return Object.freeze({
    ...unsignedReport,
    report_sha256: canonicalSha256(unsignedReport),
  });
}

export function verifyCanonicalReportHash(report, label = "canonical report") {
  requirePlainObject(report, label);
  const { report_sha256: actual, ...unsigned } = report;
  requireSha256(actual, `${label}.report_sha256`);
  rejectSecretMaterial(unsigned, label);
  if (canonicalSha256(unsigned) !== actual) {
    throw new Error(`${label} SHA-256 differs from its canonical content`);
  }
  return actual;
}

export function rejectSecretMaterial(value, path = "release evidence") {
  if (Array.isArray(value)) {
    value.forEach((entry, index) =>
      rejectSecretMaterial(entry, `${path}[${index}]`));
    return;
  }
  if (!isPlainObject(value)) return;
  for (const [key, nested] of Object.entries(value)) {
    if (SECRET_KEY.test(key)) {
      throw new Error(`${path}.${key} is forbidden secret material`);
    }
    rejectSecretMaterial(nested, `${path}.${key}`);
  }
}

export function requireExactKeys(value, expected, label) {
  requirePlainObject(value, label);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new Error(`${label} fields differ from the closed schema`);
  }
}

export function requirePlainObject(value, label) {
  if (!isPlainObject(value)) throw new Error(`${label} must be an object`);
}

export function requireNonEmptyString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be non-empty`);
  }
  return value;
}

export function requireSha256(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/u.test(value)) {
    throw new Error(`${label} must be a lowercase SHA-256`);
  }
  return value;
}

export function requireSourceCommit(value, label = "source commit") {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/u.test(value)) {
    throw new Error(`${label} must be a full lowercase SHA-1 commit`);
  }
  return value;
}

export function canonicalTimestamp(value, label) {
  if (typeof value !== "string") {
    throw new Error(`${label} must be a canonical ISO-8601 timestamp`);
  }
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== value) {
    throw new Error(`${label} must be a canonical ISO-8601 timestamp`);
  }
  return value;
}

export function isPlainObject(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}
