import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import {
  PAGES_IDENTITY_FILE,
  stampAcceptedPagesBuild,
  verifyAcceptedPagesBuild,
} from "./accepted-pages-build.mjs";

const AUTHORITY = Object.freeze({
  sourceCommit: "0123456789abcdef0123456789abcdef01234567",
  acceptedRunId: "33180374868",
  acceptedRunAttempt: "2",
  basePath: "/Clearra",
  version: "0.8.0",
});

test("stamps and verifies a closed accepted Pages build", async () => {
  await withFixture(async (build) => {
    const identity = await stampAcceptedPagesBuild(build, AUTHORITY);
    assert.equal(identity.acceptedRunId, AUTHORITY.acceptedRunId);
    assert.equal(identity.acceptedRunAttempt, AUTHORITY.acceptedRunAttempt);
    assert.equal(identity.basePath, AUTHORITY.basePath);
    assert.equal(identity.files.some((file) => file.path === PAGES_IDENTITY_FILE), false);
    assert.deepEqual(await verifyAcceptedPagesBuild(build, AUTHORITY), identity);
  });
});

test("rejects extra, missing, and mutated accepted Pages files", async () => {
  for (const mutate of [
    (build) => writeFile(join(build, "extra.txt"), "extra", "utf8"),
    (build) => rm(join(build, "_app", "app.js")),
    (build) => writeFile(join(build, "index.html"), "mutated", "utf8"),
  ]) {
    await withFixture(async (build) => {
      await stampAcceptedPagesBuild(build, AUTHORITY);
      await mutate(build);
      await assert.rejects(verifyAcceptedPagesBuild(build, AUTHORITY));
    });
  }
});

test("rejects source, run, attempt, base-path, and version drift", async () => {
  await withFixture(async (build) => {
    await stampAcceptedPagesBuild(build, AUTHORITY);
    for (const drift of [
      { sourceCommit: "89abcdef0123456789abcdef0123456789abcdef" },
      { acceptedRunId: "33180374869" },
      { acceptedRunAttempt: "3" },
      { basePath: "/Other" },
      { version: "0.8.1" },
    ]) {
      await assert.rejects(
        verifyAcceptedPagesBuild(build, { ...AUTHORITY, ...drift }),
        /mismatch|does not prove base path/u,
      );
    }
  });
});

test("rejects fallback, WASM identity, and symlink or reparse drift", async () => {
  await withFixture(async (build) => {
    await writeFile(join(build, "404.html"), "different", "utf8");
    await assert.rejects(
      stampAcceptedPagesBuild(build, AUTHORITY),
      /404 fallback must exactly match/u,
    );
  });
  await withFixture(async (build) => {
    const manifestPath = join(build, "wasm", "clearra_wasm.manifest.json");
    const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
    manifest.build.runtime_identity.source_commit =
      "89abcdef0123456789abcdef0123456789abcdef";
    await writeFile(manifestPath, JSON.stringify(manifest), "utf8");
    await assert.rejects(
      stampAcceptedPagesBuild(build, AUTHORITY),
      /mismatched product identity/u,
    );
  });
  await withFixture(async (build) => {
    const target = join(build, "_app");
    await symlink(target, join(build, "linked-app"), "junction");
    await assert.rejects(
      stampAcceptedPagesBuild(build, AUTHORITY),
      /symlink or reparse point/u,
    );
  });
});

async function withFixture(body) {
  const root = await mkdtemp(join(tmpdir(), "clearra-accepted-pages-"));
  const build = join(root, "build");
  try {
    await mkdir(join(build, "_app"), { recursive: true });
    await mkdir(join(build, "wasm"), { recursive: true });
    const html = '<script src="/Clearra/_app/app.js"></script>';
    const bindings = Buffer.from("bindings");
    const wasm = Buffer.from("wasm");
    const bindingsName = "clearra_wasm.js";
    const wasmName = "clearra_wasm_bg.wasm";
    const manifest = {
      schema_version: 1,
      build: {
        runtime_identity: {
          source_commit: AUTHORITY.sourceCommit,
          engine_build_id: AUTHORITY.sourceCommit,
          contract_schema_version: "clearra.search.contract.v2",
          supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
          artifact_schema_version: "clearra.solution-data.v1",
        },
      },
      bindings: artifact(bindingsName, bindings),
      wasm: artifact(wasmName, wasm),
    };
    await Promise.all([
      writeFile(join(build, "index.html"), html, "utf8"),
      writeFile(join(build, "404.html"), html, "utf8"),
      writeFile(join(build, "_app", "app.js"), "app", "utf8"),
      writeFile(join(build, "wasm", bindingsName), bindings),
      writeFile(join(build, "wasm", wasmName), wasm),
      writeFile(
        join(build, "wasm", "clearra_wasm.manifest.json"),
        JSON.stringify(manifest),
        "utf8",
      ),
    ]);
    await body(build);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
}

function artifact(path, payload) {
  return {
    path,
    bytes: payload.byteLength,
    sha256: createHash("sha256").update(payload).digest("hex"),
  };
}
