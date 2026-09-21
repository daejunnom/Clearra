// SRP: validate the checked-in product compatibility document and derive its
// exact identity. File I/O and release mutation belong to workflow adapters.

import { createHash } from "node:crypto";

import { canonicalJson, requireExactKeys } from "../canonical-release-evidence.mjs";

const EXPECTED = Object.freeze({
  schema: "clearra.pc4.product-compatibility.v1",
  activation_contract: "clearra.pc4.production-activation.v1",
  host_generation_schema: "clearra.pc4.host-generation.v1",
  product_adapter: "clearra.pc4.product-host.v1",
  reader_contract: "hydra-jstris-180-complete-graph-v1",
});

export function pc4ProductCompatibilityIdentity(text) {
  if (
    typeof text !== "string" ||
    Buffer.byteLength(text, "utf8") > 4_096 ||
    text.includes("\r") ||
    !text.endsWith("\n") ||
    text.endsWith("\n\n")
  ) {
    throw new Error("PC4 product compatibility text is not canonical");
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("PC4 product compatibility text is not JSON");
  }
  requireExactKeys(value, Object.keys(EXPECTED), "PC4 product compatibility document");
  if (
    Object.entries(EXPECTED).some(([key, expected]) => value[key] !== expected) ||
    `${canonicalJson(value)}\n` !== text
  ) {
    throw new Error("PC4 product compatibility contract is invalid");
  }
  return `sha256:${createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex")}`;
}
