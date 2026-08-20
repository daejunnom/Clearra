import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

import {
  parseExactGitTree,
  verifyExactSourceTar,
} from "./exact-source-tar-contract.mjs";

const helperSources = Object.freeze([
  "create-exact-source-archive.mjs",
  "exact-source-tar-contract.mjs",
]);
const defaultScriptBytes = Buffer.from(
  "#!/bin/sh\nprintf 'archive-ok\\n'\n",
  "utf8",
);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    encoding: options.encoding,
    input: options.input,
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true,
  });
  if (options.allowFailure !== true) {
    assert.equal(result.error, undefined);
    assert.equal(result.signal, null);
    assert.equal(result.status, 0, result.stderr?.toString());
  }
  return result;
}

function git(repository, args, options = {}) {
  return run("git", args, { cwd: repository, ...options });
}

function copyHelpers(repository) {
  const releaseDirectory = join(repository, "scripts", "release");
  mkdirSync(releaseDirectory, { recursive: true });
  for (const name of helperSources) {
    copyFileSync(
      fileURLToPath(new URL(`./${name}`, import.meta.url)),
      join(releaseDirectory, name),
    );
  }
  return join(releaseDirectory, helperSources[0]);
}

function prepareRepository(options = {}) {
  const repository = mkdtempSync(join(tmpdir(), "clearra-exact-archive-"));
  git(repository, ["init", "--quiet"]);
  git(repository, ["config", "user.email", "archive-test@example.invalid"]);
  git(repository, ["config", "user.name", "Clearra Archive Test"]);
  git(repository, ["config", "core.autocrlf", "true"]);

  let helperPath;
  if (options.commitHelpers !== false) helperPath = copyHelpers(repository);
  const scriptBytes = options.scriptBytes ?? defaultScriptBytes;
  const longPath = `${"p".repeat(110)}.txt`;
  writeFileSync(join(repository, "deploy-helper"), scriptBytes);
  writeFileSync(join(repository, "plain.txt"), Buffer.from("plain\n", "utf8"));
  writeFileSync(
    join(repository, longPath),
    Buffer.from("pax-path-content\n", "utf8"),
  );
  if (options.attributes) {
    writeFileSync(
      join(repository, ".gitattributes"),
      Buffer.from(`${options.attributes}\n`, "utf8"),
    );
  }
  git(repository, ["add", "--", "."]);
  git(repository, ["update-index", "--chmod=+x", "--", "deploy-helper"]);

  const linkBlob = git(repository, ["hash-object", "-w", "--stdin"], {
    input: "deploy-helper",
    encoding: "utf8",
  }).stdout.trim();
  git(repository, [
    "update-index",
    "--add",
    "--cacheinfo",
    `120000,${linkBlob},deploy-link`,
  ]);
  const longLinkBlob = git(repository, ["hash-object", "-w", "--stdin"], {
    input: longPath,
    encoding: "utf8",
  }).stdout.trim();
  git(repository, [
    "update-index",
    "--add",
    "--cacheinfo",
    `120000,${longLinkBlob},long-link`,
  ]);
  git(repository, ["commit", "--quiet", "-m", "archive fixture"]);
  const commit = git(repository, ["rev-parse", "HEAD"], {
    encoding: "utf8",
  }).stdout.trim();

  if (options.commitHelpers === false) helperPath = copyHelpers(repository);
  if (options.infoAttributes) {
    const infoDirectory = join(repository, ".git", "info");
    mkdirSync(infoDirectory, { recursive: true });
    writeFileSync(
      join(infoDirectory, "attributes"),
      Buffer.from(`${options.infoAttributes}\n`, "utf8"),
    );
  }
  return { commit, helperPath, longPath, repository, scriptBytes };
}

function runFixtureHelper(fixture, outputPath, sourceCommit = fixture.commit) {
  return run(
    process.execPath,
    [
      fixture.helperPath,
      "--source-commit",
      sourceCommit,
      "--output",
      outputPath,
    ],
    { cwd: fixture.repository, allowFailure: true, encoding: "utf8" },
  );
}

function tarFieldString(header, start, length) {
  const bytes = header.subarray(start, start + length);
  const end = bytes.indexOf(0);
  return bytes.subarray(0, end < 0 ? bytes.length : end).toString("utf8");
}

function tarOctal(header, start, length) {
  return Number.parseInt(tarFieldString(header, start, length).trim(), 8);
}

function tarEntries(rawTar) {
  const entries = new Map();
  let offset = 0;
  while (offset + 512 <= rawTar.length) {
    const header = rawTar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const size = tarOctal(header, 124, 12);
    const type = header[156] === 0 ? "0" : String.fromCharCode(header[156]);
    const name = tarFieldString(header, 0, 100);
    const prefix = tarFieldString(header, 345, 155);
    const path = prefix ? `${prefix}/${name}` : name;
    const contentStart = offset + 512;
    const contentEnd = contentStart + size;
    entries.set(path, {
      content: rawTar.subarray(contentStart, contentEnd),
      headerOffset: offset,
      linkName: tarFieldString(header, 157, 100),
      mode: tarOctal(header, 100, 8),
      type,
    });
    offset = contentStart + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function rewriteTarChecksum(buffer, headerOffset) {
  buffer.fill(32, headerOffset + 148, headerOffset + 156);
  let sum = 0;
  for (let index = headerOffset; index < headerOffset + 512; index += 1) {
    sum += buffer[index];
  }
  const checksum = Buffer.from(
    `${sum.toString(8).padStart(6, "0")}\0 `,
    "ascii",
  );
  checksum.copy(buffer, headerOffset + 148);
}

function expectedTree(fixture) {
  const tree = git(fixture.repository, [
    "ls-tree",
    "-r",
    "-t",
    "-z",
    "--full-tree",
    fixture.commit,
  ]).stdout;
  return parseExactGitTree(tree, "sha1");
}

test("exports every exact commit byte, 0644/0755 mode, safe symlink, and embedded identity with autocrlf enabled", () => {
  const fixture = prepareRepository();
  try {
    const outputPath = join(fixture.repository, "exact-source.tar.gz");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /exact_source_archive=ready/);

    const rawTar = gunzipSync(readFileSync(outputPath));
    assert.ok(
      rawTar.includes(Buffer.from(`path=${fixture.longPath}\n`, "utf8")),
      "long regular path must exercise a PAX path record",
    );
    assert.ok(
      rawTar.includes(Buffer.from(`linkpath=${fixture.longPath}\n`, "utf8")),
      "long symlink target must exercise a PAX linkpath record",
    );
    const entries = tarEntries(rawTar);
    const script = entries.get("deploy-helper");
    assert.ok(script);
    assert.deepEqual(script.content, fixture.scriptBytes);
    assert.equal(script.mode, 0o755);
    assert.equal(script.type, "0");
    assert.equal(entries.get("plain.txt").mode, 0o644);

    const link = entries.get("deploy-link");
    assert.ok(link);
    assert.equal(link.type, "2");
    assert.equal(link.mode, 0o777);
    assert.equal(link.linkName, "deploy-helper");

    const archivedCommit = git(fixture.repository, ["get-tar-commit-id"], {
      input: rawTar,
      encoding: "utf8",
    }).stdout.trim();
    assert.equal(archivedCommit, fixture.commit);
    assert.equal(
      verifyExactSourceTar({
        archive: rawTar,
        expectedEntries: expectedTree(fixture),
        objectFormat: "sha1",
        sourceCommit: fixture.commit,
      }).entryCount,
      expectedTree(fixture).size,
    );
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

for (const regression of [
  {
    title: "rejects committed eol=crlf archive conversion and deletes output",
    options: { attributes: "deploy-helper text eol=crlf" },
    message: /bytes differ from Git blob/,
  },
  {
    title:
      "rejects committed export-ignore archive omission and deletes output",
    options: { attributes: "deploy-helper export-ignore" },
    message: /missing Git tree path/,
  },
  {
    title: "rejects committed export-subst archive mutation and deletes output",
    options: {
      attributes: "deploy-helper export-subst",
      scriptBytes: Buffer.from("$Format:%H$\n", "utf8"),
    },
    message: /bytes differ from Git blob/,
  },
  {
    title:
      "rejects uncommitted info attributes archive omission and deletes output",
    options: { infoAttributes: "deploy-helper export-ignore" },
    message: /missing Git tree path/,
  },
]) {
  test(regression.title, () => {
    const fixture = prepareRepository(regression.options);
    try {
      const outputPath = join(fixture.repository, "rejected.tar.gz");
      const result = runFixtureHelper(fixture, outputPath);
      assert.equal(result.status, 2);
      assert.match(result.stderr, regression.message);
      assert.equal(existsSync(outputPath), false);
    } finally {
      rmSync(fixture.repository, { recursive: true, force: true });
    }
  });
}

test("rejects helper-module drift from the accepted commit before creating output", () => {
  const fixture = prepareRepository();
  try {
    writeFileSync(
      fixture.helperPath,
      `${readFileSync(fixture.helperPath, "utf8")}\n// drift\n`,
    );
    const outputPath = join(fixture.repository, "drifted.tar.gz");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /helper differs from the accepted commit/);
    assert.equal(existsSync(outputPath), false);
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("rejects raw helper drift hidden by an assume-unchanged index flag", () => {
  const fixture = prepareRepository();
  try {
    git(fixture.repository, [
      "update-index",
      "--assume-unchanged",
      "--",
      "scripts/release/create-exact-source-archive.mjs",
    ]);
    writeFileSync(
      fixture.helperPath,
      `${readFileSync(fixture.helperPath, "utf8")}\n// hidden drift\n`,
    );
    const outputPath = join(fixture.repository, "hidden-drift.tar.gz");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /helper differs from the accepted commit/);
    assert.equal(existsSync(outputPath), false);
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("rejects helper modules absent from the accepted commit before creating output", () => {
  const fixture = prepareRepository({ commitHelpers: false });
  try {
    const outputPath = join(fixture.repository, "absent.tar.gz");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /helper is absent from the accepted commit/);
    assert.equal(existsSync(outputPath), false);
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("refuses to overwrite an existing archive path", () => {
  const fixture = prepareRepository();
  try {
    const outputPath = join(fixture.repository, "existing.tar.gz");
    writeFileSync(outputPath, "owned-by-caller");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /archive output already exists/);
    assert.equal(readFileSync(outputPath, "utf8"), "owned-by-caller");
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("rejects a noncanonical source commit before creating output", () => {
  const fixture = prepareRepository();
  try {
    const outputPath = join(fixture.repository, "invalid.tar.gz");
    const result = runFixtureHelper(fixture, outputPath, "HEAD");
    assert.equal(result.status, 2);
    assert.match(result.stderr, /lowercase 40-character Git SHA/);
    assert.equal(existsSync(outputPath), false);
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("ignores local Git replacement refs and archives the canonical accepted object", () => {
  const fixture = prepareRepository();
  try {
    writeFileSync(
      join(fixture.repository, "plain.txt"),
      Buffer.from("replacement-content\n", "utf8"),
    );
    git(fixture.repository, ["add", "--", "plain.txt"]);
    git(fixture.repository, ["commit", "--quiet", "-m", "replacement tree"]);
    const replacementCommit = git(fixture.repository, ["rev-parse", "HEAD"], {
      encoding: "utf8",
    }).stdout.trim();
    git(fixture.repository, ["replace", fixture.commit, replacementCommit]);

    const outputPath = join(fixture.repository, "replacement-safe.tar.gz");
    const result = runFixtureHelper(fixture, outputPath);
    assert.equal(result.status, 0, result.stderr);
    const entries = tarEntries(gunzipSync(readFileSync(outputPath)));
    assert.deepEqual(
      entries.get("plain.txt")?.content,
      Buffer.from("plain\n", "utf8"),
    );
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});

test("production tar verifier rejects duplicate paths, unsupported types, bad checksums, and truncation", () => {
  const fixture = prepareRepository();
  try {
    const outputPath = join(fixture.repository, "parser.tar.gz");
    assert.equal(runFixtureHelper(fixture, outputPath).status, 0);
    const rawTar = gunzipSync(readFileSync(outputPath));
    const entries = tarEntries(rawTar);
    const expectedEntries = expectedTree(fixture);
    const verify = (archive) =>
      verifyExactSourceTar({
        archive,
        expectedEntries,
        objectFormat: "sha1",
        sourceCommit: fixture.commit,
      });

    const duplicate = Buffer.from(rawTar);
    const plainHeader = entries.get("plain.txt").headerOffset;
    duplicate.fill(0, plainHeader, plainHeader + 100);
    Buffer.from("deploy-helper", "utf8").copy(duplicate, plainHeader);
    rewriteTarChecksum(duplicate, plainHeader);
    assert.throws(() => verify(duplicate), /duplicate source tar path/);

    const unsupported = Buffer.from(rawTar);
    unsupported[plainHeader + 156] = "1".charCodeAt(0);
    rewriteTarChecksum(unsupported, plainHeader);
    assert.throws(
      () => verify(unsupported),
      /unsupported source tar member type/,
    );

    const badChecksum = Buffer.from(rawTar);
    badChecksum[148] ^= 1;
    assert.throws(() => verify(badChecksum), /tar header checksum mismatch/);
    assert.throws(
      () => verify(rawTar.subarray(0, rawTar.length - 1)),
      /not block aligned/,
    );
  } finally {
    rmSync(fixture.repository, { recursive: true, force: true });
  }
});
