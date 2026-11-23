import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm, symlink, unlink, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { sealAcceptedCtk3Dist } from "../../tools/accepted-ctk3-dist.mjs";

const script = fileURLToPath(new URL("./create-actions-layers-v080.sh", import.meta.url));
const verifier = fileURLToPath(new URL("../../tools/accepted-ctk3-dist.mjs", import.meta.url));
const source = "a".repeat(40);
const runId = "100";
const attempt = "1";
const linuxPackaging = { skip: process.platform !== "linux", timeout: 30_000 };

async function put(path, contents) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
}

async function fixture(root, layout) {
  const repository = join(root, "repository");
  const accepted = join(root, "accepted-ctk3");
  await put(join(repository, "apps/clearra-discord-bot/package.json"), "{}\n");
  await put(join(repository, "packages/ctk3/dist/index.js"), "repo-local-poison\n");
  await put(join(repository, "node_modules/unrelated-dev/index.js"), "not-runtime\n");
  const copiedVerifier = join(repository, "scripts/tools/accepted-ctk3-dist.mjs");
  await mkdir(dirname(copiedVerifier), { recursive: true });
  await copyFile(verifier, copiedVerifier);
  for (const name of ["decodeWorker.js", "index.cjs", "index.d.ts", "index.js"]) {
    await put(join(accepted, name), `accepted-${name}\n`);
  }
  await sealAcceptedCtk3Dist(accepted, source, runId, attempt);
  const packageRoot = layout === "pnpm"
    ? join(repository, "node_modules/.pnpm/tetris-fumen@1.1.3/node_modules/tetris-fumen")
    : join(repository, "node_modules/tetris-fumen");
  await put(join(packageRoot, "package.json"), '{"name":"tetris-fumen","version":"1.1.3"}\n');
  await put(join(packageRoot, "index.js"), "production-fumen\n");
  const consumerLink = join(repository, "apps/clearra-discord-bot/node_modules/tetris-fumen");
  if (layout === "pnpm") {
    await mkdir(dirname(consumerLink), { recursive: true });
    await symlink(relative(dirname(consumerLink), packageRoot), consumerLink, "dir");
  }
  const undiciRoot = layout === "pnpm"
    ? join(repository, "node_modules/.pnpm/undici@6.28.0/node_modules/undici")
    : join(repository, "node_modules/undici");
  await put(join(undiciRoot, "package.json"), '{"name":"undici","version":"6.28.0","main":"index.js"}\n');
  await put(join(undiciRoot, "index.js"), 'exports.getGlobalDispatcher = () => ({ dispatch: () => "runtime-undici" });\n');
  const undiciLink = join(repository, "apps/clearra-discord-bot/node_modules/undici");
  if (layout === "pnpm") {
    await symlink(relative(dirname(undiciLink), undiciRoot), undiciLink, "dir");
  }
  return { repository, accepted, packageRoot, consumerLink, undiciRoot, undiciLink };
}

async function build(inputs, output) {
  await mkdir(output);
  return invoke(inputs, output);
}

function invoke({ repository, accepted }, output) {
  return spawnSync("bash", [script, repository, accepted, source, runId, attempt, output], {
    encoding: "utf8", windowsHide: true, timeout: 10_000,
  });
}

function tar(args) {
  const result = spawnSync("tar", args, { encoding: "utf8", timeout: 5_000, windowsHide: true });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

for (const layout of ["hoisted", "pnpm"]) {
  test(`Actions runtime layers materialize ${layout} dependencies and preserve accepted bytes`, linuxPackaging, async () => {
    const root = await mkdtemp(join(tmpdir(), "clearra-actions-layers-"));
    try {
      const inputs = await fixture(root, layout);
      const first = join(root, "first");
      const second = join(root, "second");
      for (const output of [first, second]) {
        const result = await build(inputs, output);
        assert.equal(result.status, 0, result.stderr);
      }
      for (const name of ["ctk3-dist.tar", "node_modules.tar"]) {
        assert.deepEqual(await readFile(join(first, name)), await readFile(join(second, name)));
      }
      const dependencyArchive = join(first, "node_modules.tar");
      const leaves = tar(["-tf", dependencyArchive]).trim().split(/\r?\n/u);
      assert.ok(leaves.every((name) => [
        "node_modules/", "node_modules/@clearra/", "node_modules/@clearra/discord-bot", "node_modules/ctk3",
      ].includes(name) || name.startsWith("node_modules/tetris-fumen/") || name.startsWith("node_modules/undici/")));
      assert.equal(tar(["-xOf", dependencyArchive, "node_modules/tetris-fumen/index.js"]), "production-fumen\n");
      assert.equal(tar(["-xOf", join(first, "ctk3-dist.tar"), "packages/ctk3/dist/index.js"]), "accepted-index.js\n");
      const relocated = join(root, "relocated");
      await mkdir(relocated);
      tar(["-xf", dependencyArchive, "-C", relocated]);
      const dispatcherPath = join(relocated, "apps/clearra-discord-bot/src/clearra/job-http-dispatcher.mjs");
      await mkdir(dirname(dispatcherPath), { recursive: true });
      await copyFile(fileURLToPath(new URL("../../../apps/clearra-discord-bot/src/clearra/job-http-dispatcher.mjs", import.meta.url)), dispatcherPath);
      const importCheck = spawnSync(process.execPath, ["--input-type=module", "--eval",
        'const {jobResponseDispatcher} = await import(process.argv[1]); process.stdout.write(jobResponseDispatcher.dispatch({}, {}));', dispatcherPath],
        { encoding: "utf8", timeout: 5_000 });
      assert.equal(importCheck.status, 0, importCheck.stderr);
      assert.equal(importCheck.stdout, "runtime-undici");
      const before = await readFile(dependencyArchive);
      const duplicate = invoke(inputs, first);
      assert.notEqual(duplicate.status, 0);
      assert.deepEqual(await readFile(dependencyArchive), before);
      await writeFile(join(inputs.accepted, "index.js"), "changed after acceptance\n");
      const rejected = join(root, "rejected");
      assert.notEqual((await build(inputs, rejected)).status, 0);
      await assert.rejects(readFile(join(rejected, "ctk3-dist.tar")), { code: "ENOENT" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
}

test("Actions runtime layers reject missing, escaping, and internally linked packages", linuxPackaging, async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-actions-layers-"));
  try {
    for (const packageName of ["tetris-fumen", "undici"]) {
      const caseRoot = join(root, packageName);
      const inputs = await fixture(caseRoot, "pnpm");
      const packageRoot = packageName === "undici" ? inputs.undiciRoot : inputs.packageRoot;
      const consumerLink = packageName === "undici" ? inputs.undiciLink : inputs.consumerLink;
      const output = join(caseRoot, "output");
      await mkdir(output);
      const unsafe = join(packageRoot, "linked-entry");
      await symlink(join(root, "outside.txt"), unsafe);
      assert.equal(invoke(inputs, output).status, 77);
      await unlink(unsafe);
      await unlink(consumerLink);
      assert.equal(invoke(inputs, output).status, 66);
      const outside = join(caseRoot, "outside-package");
      await put(join(outside, "package.json"), JSON.stringify({ name: packageName }));
      await symlink(outside, consumerLink, "dir");
      assert.equal(invoke(inputs, output).status, 66);
      await assert.rejects(readFile(join(output, "node_modules.tar")), { code: "ENOENT" });
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
