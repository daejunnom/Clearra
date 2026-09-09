import { appendFile, lstat } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { rebindAcceptedWasmBuild } from "./accepted-wasm-build.mjs";

async function main(arguments_) {
  const { values } = parseArgs({
    args: arguments_,
    options: {
      source: { type: "string" },
      destination: { type: "string" },
      "source-commit": { type: "string" },
      "previous-run-id": { type: "string" },
      "previous-run-attempt": { type: "string" },
      "current-run-id": { type: "string" },
      "current-run-attempt": { type: "string" },
      "github-output": { type: "string" },
    },
    strict: true,
  });
  for (const key of [
    "source",
    "destination",
    "source-commit",
    "previous-run-id",
    "previous-run-attempt",
    "current-run-id",
    "current-run-attempt",
    "github-output",
  ]) {
    if (typeof values[key] !== "string" || values[key].length === 0) {
      throw new Error(`accepted WASM reuse requires --${key}`);
    }
  }

  try {
    const result = await rebindAcceptedWasmBuild(
      values.source,
      values.destination,
      values["source-commit"],
      values["previous-run-id"],
      values["previous-run-attempt"],
      values["current-run-id"],
      values["current-run-attempt"],
    );
    await appendFile(
      values["github-output"],
      `reused=true\npayload_sha256=${result.payload_sha256}\n`,
      "utf8",
    );
    console.log(
      `accepted_wasm_reuse=hit prior_run=${result.previous_run_id}/` +
      `${result.previous_run_attempt} payload_sha256=${result.payload_sha256}`,
    );
  } catch {
    if (await pathExists(resolve(values.destination))) {
      throw new Error("accepted WASM reuse failed after publishing its destination");
    }
    await appendFile(values["github-output"], "reused=false\n", "utf8");
    console.log("accepted_wasm_reuse=miss reason=downloaded-candidate-rejected");
  }
}

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
