import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { pc4ProductCompatibilityIdentity } from "./product-compatibility.mjs";

const source = await readFile(
  new URL("../../../config/pc4-product-compatibility.v1.json", import.meta.url),
  "utf8",
);

test("checked-in PC4 product compatibility has one stable SHA-256 identity", () => {
  assert.match(pc4ProductCompatibilityIdentity(source), /^sha256:[0-9a-f]{64}$/u);
  assert.equal(
    pc4ProductCompatibilityIdentity(source),
    pc4ProductCompatibilityIdentity(source),
  );
});

test("PC4 product compatibility rejects noncanonical or altered contracts", () => {
  assert.throws(() => pc4ProductCompatibilityIdentity(source.replace(/\n$/u, "")), /canonical/u);
  assert.throws(
    () => pc4ProductCompatibilityIdentity(source.replace("product-host.v1", "product-host.v2")),
    /contract is invalid/u,
  );
});
