import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { build } from "esbuild";

import {
  keyFromDecodedPage,
  normalizedSetHash,
  runTerminalSupplyCli,
  TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
} from "../../../apps/clearra-discord-bot/scripts/verify-terminal-supply-product.mjs";

const { values } = parseArgs({
  options: {
    clearra: { type: "string" },
  },
  strict: true,
});
assert.ok(values.clearra, "--clearra must name the built release-facing CLI");

const packageRoot = fileURLToPath(new URL("..", import.meta.url));
const bundle = await build({
  bundle: true,
  format: "esm",
  logLevel: "silent",
  platform: "node",
  stdin: {
    contents: `
      export { encodeSolutionKeysForClipboard }
        from './src/lib/workspace/solutionExportAsync.ts';
      export { decodeFieldDocument }
        from './src/lib/workspace/fieldInterchange.ts';
    `,
    resolveDir: packageRoot,
    sourcefile: "terminal-supply-ui-product-entry.ts",
  },
  target: "node22",
  write: false,
});
assert.equal(bundle.outputFiles.length, 1);
const ui = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString("base64")}`
);

const response = runTerminalSupplyCli(values.clearra);
const solutionKeys = response.contract.artifacts.solution_keys;
for (const format of ["ctk", "fumen"]) {
  const encoded = await ui.encodeSolutionKeysForClipboard(solutionKeys, format);
  assert.match(encoded, format === "ctk" ? /^ctk3_/ : /^v115@/);

  const document = ui.decodeFieldDocument(encoded);
  assert.equal(document.width, 10);
  assert.equal(document.pages.length, TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
  const decodedKeys = document.pages.map(keyFromDecodedPage).sort();
  assert.deepEqual(decodedKeys, solutionKeys);
  assert.equal(
    normalizedSetHash(decodedKeys),
    TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  );
}

console.log(
  "[ui-terminal-supply-product] passed" +
    ` | solutions=${solutionKeys.length}` +
    ` | hash=${TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH}` +
    " | formats=ctk,fumen",
);
