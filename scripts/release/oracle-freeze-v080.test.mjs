import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const oracleRoot = fileURLToPath(new URL("./oracle/", import.meta.url));
const layerBuilder = join(oracleRoot, "create-local-layers-v080.sh");
const freezeHelper = join(oracleRoot, "clearra-oracle-freeze-v080");
const acceptedCtk3Verifier = fileURLToPath(
  new URL("../tools/accepted-ctk3-dist.mjs", import.meta.url),
);
const sourceCommit = "0123456789abcdef0123456789abcdef01234567";
const acceptedRunId = "33180374868";
const acceptedRunAttempt = "2";

const overlayEntries = [
  "apps/clearra-discord-bot/src/admin",
  "apps/clearra-discord-bot/src/admin/access-runtime.mjs",
  "apps/clearra-discord-bot/src/admin/command-identity.mjs",
  "apps/clearra-discord-bot/src/admin/discord-history-hydrator.mjs",
  "apps/clearra-discord-bot/src/admin/discord-observer.mjs",
  "apps/clearra-discord-bot/src/admin/document.mjs",
  "apps/clearra-discord-bot/src/admin/identity.mjs",
  "apps/clearra-discord-bot/src/admin/local-publisher.mjs",
  "apps/clearra-discord-bot/src/admin/main.mjs",
  "apps/clearra-discord-bot/src/admin/oracle-telemetry.conf",
  "apps/clearra-discord-bot/src/admin/oracle-usage-tracker.mjs",
  "apps/clearra-discord-bot/src/admin/runtime-extension.mjs",
  "apps/clearra-discord-bot/src/admin/server.mjs",
  "apps/clearra-discord-bot/src/admin/slash-runtime.mjs",
  "apps/clearra-discord-bot/src/admin/TELEMETRY-OPERATIONS.md",
  "apps/clearra-discord-bot/src/admin/usage-store.mjs",
  "apps/clearra-discord-bot/src/admin/telemetry",
  "apps/clearra-discord-bot/src/admin/telemetry/hmac.mjs",
  "apps/clearra-discord-bot/src/admin/telemetry/rate-limiter.mjs",
  "apps/clearra-discord-bot/src/admin/telemetry/schema.mjs",
  "apps/clearra-discord-bot/src/admin/deploy",
  "apps/clearra-discord-bot/src/admin/deploy/ORACLE_GATEWAY.md",
  "apps/clearra-discord-bot/src/admin/deploy/oracle",
  "apps/clearra-discord-bot/src/admin/deploy/oracle/clearra-gateway-vault-run",
  "apps/clearra-discord-bot/src/admin/deploy/oracle/clearra-gateway.service",
];

function writeFixtureFile(root, relative, contents = "fixture\n") {
  const path = join(root, ...relative.split("/"));
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents, { flag: "wx" });
}

function toUnixPath(path) {
  if (process.platform !== "win32") return path;
  const result = spawnSync("wsl.exe", ["-e", "wslpath", "-a", "--", path], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

function runUnix(executable, args, options = {}) {
  if (process.platform === "win32") {
    return spawnSync("wsl.exe", ["-e", executable, ...args], {
      encoding: "utf8",
      ...options,
    });
  }
  return spawnSync(executable, args, { encoding: "utf8", ...options });
}

function inspectTar(path) {
  const script = String.raw`
import json, sys, tarfile
entries = []
with tarfile.open(sys.argv[1], "r:*") as archive:
    for member in archive:
        if member.isfile(): kind = "file"
        elif member.isdir(): kind = "directory"
        elif member.issym(): kind = "symlink"
        else: kind = "unsupported"
        entries.append({"name": member.name.rstrip("/"), "kind": kind,
                        "link": member.linkname if member.issym() else None})
print(json.dumps(entries, separators=(",", ":")))
`;
  const result = runUnix("python3", ["-c", script, toUnixPath(path)]);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

test("v0.8 Oracle local layer builder freezes only the closed runtime set", () => {
  const temporaryRoot = mkdtempSync(join(tmpdir(), "clearra-oracle-layers-test-"));
  const repository = join(temporaryRoot, "repository");
  const acceptedCtk3Dist = join(temporaryRoot, "accepted-ctk3-dist");
  const output = join(temporaryRoot, "output");
  try {
    mkdirSync(output, { recursive: true });
    mkdirSync(acceptedCtk3Dist, { recursive: true });
    writeFixtureFile(repository, "apps/clearra-discord-bot/package.json", "{}\n");
    writeFixtureFile(repository, "packages/ctk3/package.json", "{}\n");
    writeFixtureFile(repository, "packages/ctk3/dist/repo-local-poison.js");
    const repositoryVerifier = join(
      repository,
      "scripts",
      "tools",
      "accepted-ctk3-dist.mjs",
    );
    mkdirSync(dirname(repositoryVerifier), { recursive: true });
    copyFileSync(acceptedCtk3Verifier, repositoryVerifier);
    const acceptedFiles = [
      ["decodeWorker.js", "worker\n"],
      ["index.cjs", "cjs\n"],
      ["index.d.ts", "types\n"],
      ["index.js", "accepted\n"],
    ];
    for (const [relative, contents] of acceptedFiles) {
      writeFixtureFile(acceptedCtk3Dist, relative, contents);
    }
    writeFixtureFile(
      acceptedCtk3Dist,
      "clearra-accepted-ctk3.v2.json",
      `${JSON.stringify(
        {
          contract: "clearra.accepted-ctk3-dist.v2",
          source_commit: sourceCommit,
          run_id: acceptedRunId,
          run_attempt: acceptedRunAttempt,
          files: acceptedFiles.map(([path, contents]) => ({
            path,
            size: Buffer.byteLength(contents),
            sha256: createHash("sha256").update(contents).digest("hex"),
          })),
        },
        null,
        2,
      )}\n`,
    );
    writeFixtureFile(repository, "node_modules/tetris-fumen/package.json", "{}\n");
    writeFixtureFile(repository, "node_modules/tetris-fumen/index.js");
    for (const relative of overlayEntries) {
      if (
        relative.endsWith("/admin") ||
        relative.endsWith("/telemetry") ||
        relative.endsWith("/deploy") ||
        relative.endsWith("/oracle")
      ) {
        mkdirSync(join(repository, ...relative.split("/")), { recursive: true });
      } else {
        writeFixtureFile(repository, relative);
      }
    }
    writeFixtureFile(
      repository,
      "apps/clearra-discord-bot/src/admin/config.mjs",
      "export default 'synthetic-test-only';\n",
    );

    const first = runUnix("bash", [
      toUnixPath(layerBuilder),
      toUnixPath(repository),
      toUnixPath(acceptedCtk3Dist),
      sourceCommit,
      acceptedRunId,
      acceptedRunAttempt,
      toUnixPath(output),
    ]);
    assert.equal(first.status, 0, first.stderr);
    assert.equal(
      first.stdout.trim().split(/\r?\n/u).filter(Boolean).length,
      4,
      first.stdout,
    );
    assert.equal(
      first.stdout.trim().split(/\r?\n/u)[0],
      `oracle_ctk3_authority=accepted source_commit=${sourceCommit} run_id=${acceptedRunId} run_attempt=${acceptedRunAttempt}`,
    );

    const overlay = inspectTar(join(output, "private-overlay-no-config.tar"));
    assert.deepEqual(
      overlay.map(({ name }) => name).sort(),
      [...overlayEntries].sort(),
    );
    assert.ok(!overlay.some(({ name }) => name.endsWith("/config.mjs")));
    assert.ok(overlay.every(({ kind }) => kind === "file" || kind === "directory"));

    const dist = inspectTar(join(output, "ctk3-dist.tar"));
    assert.ok(
      dist.every(
        ({ name }) =>
          name === "packages/ctk3/dist" || name.startsWith("packages/ctk3/dist/"),
      ),
    );
    assert.ok(
      dist.some(({ name }) => name.endsWith("/clearra-accepted-ctk3.v2.json")),
    );
    assert.ok(!dist.some(({ name }) => name.includes("repo-local-poison")));

    const dependencies = inspectTar(join(output, "node_modules.tar"));
    assert.deepEqual(
      Object.fromEntries(
        dependencies
          .filter(({ kind }) => kind === "symlink")
          .map(({ name, link }) => [name, link]),
      ),
      {
        "node_modules/@clearra/discord-bot": "../../apps/clearra-discord-bot",
        "node_modules/ctk3": "../packages/ctk3",
      },
    );
    assert.ok(
      dependencies.every(
        ({ name }) =>
          name === "node_modules" ||
          name === "node_modules/@clearra" ||
          name === "node_modules/@clearra/discord-bot" ||
          name === "node_modules/ctk3" ||
          name === "node_modules/tetris-fumen" ||
          name.startsWith("node_modules/tetris-fumen/"),
      ),
    );

    const duplicate = runUnix("bash", [
      toUnixPath(layerBuilder),
      toUnixPath(repository),
      toUnixPath(acceptedCtk3Dist),
      sourceCommit,
      acceptedRunId,
      acceptedRunAttempt,
      toUnixPath(output),
    ]);
    assert.notEqual(duplicate.status, 0);
    assert.match(duplicate.stderr, /refusing to overwrite frozen layer/u);
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});

test("v0.8 Oracle freeze helper seals uploads and preserves the active authority", () => {
  const text = readFileSync(freezeHelper, "utf8");
  const syntax = runUnix("dash", ["-n", toUnixPath(freezeHelper)]);
  assert.equal(syntax.status, 0, syntax.stderr);

  const seal = text.indexOf('/usr/bin/chown root:root -- "$upload_root"');
  const validation = text.indexOf("layer_counts=$(validate_layers)");
  const baselineCapture = text.indexOf("\ncapture_baseline\n", seal);
  const baselineBeforeLayers = text.indexOf(
    "\nrequire_baseline_unchanged\n",
    baselineCapture,
  );
  assert.ok(seal >= 0 && validation > seal, "uploads must be sealed before validation");
  assert.ok(
    baselineCapture >= 0 &&
      baselineBeforeLayers > baselineCapture &&
      validation > baselineBeforeLayers,
    "the active runtime must remain unchanged before frozen layer validation",
  );
  assert.match(text, /assemble_candidate "\$candidate_root" "\$release_id"\nrequire_baseline_unchanged/u);
  assert.match(text, /apps\/clearra-discord-bot\/src\/admin\/config\.mjs/u);
  assert.match(text, /candidate_files0755" = 8/u);
  assert.match(text, /candidate_symlinks" = 2/u);
  assert.match(text, /oracle_manifest_base64=\$manifest_base64/u);
  assert.doesNotMatch(text, /(?:cat|head|tail|sed|awk).*\$settings_path/u);
  assert.doesNotMatch(text, /(?:cat|head|tail|sed|awk).*\$baseline_config_path/u);
});

test("v0.8 Oracle freeze records only a distinct installed tool as prior authority", () => {
  const text = readFileSync(freezeHelper, "utf8");
  const functionStart = text.indexOf("tool_authority() {");
  const functionEnd = text.indexOf("\ncleanup() {", functionStart);
  assert.ok(functionStart >= 0 && functionEnd > functionStart);

  const temporaryRoot = mkdtempSync(
    join(tmpdir(), "clearra-oracle-tool-authority-test-"),
  );
  try {
    const installedTool = join(temporaryRoot, "installed-tool");
    const probe = join(temporaryRoot, "probe.sh");
    writeFileSync(installedTool, "same tool bytes\n", { flag: "wx" });
    writeFileSync(
      probe,
      [
        "#!/bin/sh",
        "set -eu",
        'fail() { exit "${1:-1}"; }',
        'require_root_regular_mode() { [ -f "$1" ] && [ ! -L "$1" ]; }',
        "sha256_of() { sha256sum -- \"$1\" | cut -d ' ' -f 1; }",
        text.slice(functionStart, functionEnd),
        'tool_authority "$1" "$2"',
        "",
      ].join("\n"),
      { flag: "wx" },
    );

    const installedBytes = readFileSync(installedTool);
    const installedSha256 = createHash("sha256")
      .update(installedBytes)
      .digest("hex");
    const identical = runUnix("dash", [
      toUnixPath(probe),
      installedSha256,
      toUnixPath(installedTool),
    ]);
    assert.equal(identical.status, 0, identical.stderr);
    assert.equal(identical.stdout.trim(), "absent:0");

    const replacementSha256 = "f".repeat(64);
    assert.notEqual(replacementSha256, installedSha256);
    const distinct = runUnix("dash", [
      toUnixPath(probe),
      replacementSha256,
      toUnixPath(installedTool),
    ]);
    assert.equal(distinct.status, 0, distinct.stderr);
    assert.equal(
      distinct.stdout.trim(),
      `${installedSha256}:${installedBytes.length}`,
    );

    const missing = runUnix("dash", [
      toUnixPath(probe),
      replacementSha256,
      toUnixPath(join(temporaryRoot, "missing-tool")),
    ]);
    assert.equal(missing.status, 0, missing.stderr);
    assert.equal(missing.stdout.trim(), "absent:0");
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});
