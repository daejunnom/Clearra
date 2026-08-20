import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  lstatSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmdirSync,
  unlinkSync,
  writeSync,
} from "node:fs";
import {
  basename,
  dirname,
  isAbsolute,
  relative,
  resolve,
  sep,
} from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync, gzipSync } from "node:zlib";

import {
  parseExactGitTree,
  verifyExactSourceTar,
} from "./exact-source-tar-contract.mjs";

const FULL_SHA = /^[0-9a-f]{40}$/;
const CANONICAL_HELPER_PATHS = Object.freeze([
  "scripts/release/create-exact-source-archive.mjs",
  "scripts/release/exact-source-tar-contract.mjs",
]);

function runGit(args, options = {}) {
  return spawnSync("git", ["--no-replace-objects", ...args], {
    cwd: options.cwd,
    encoding: options.encoding,
    maxBuffer: options.maxBuffer ?? 8 * 1024 * 1024,
    stdio: options.stdio,
    windowsHide: true,
  });
}

function requireSuccessfulGit(result, message) {
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(message);
  }
  return result.stdout;
}

function resolveRepositoryRoot(cwd) {
  const output = requireSuccessfulGit(
    runGit(["rev-parse", "--show-toplevel"], { cwd, encoding: "utf8" }),
    "exact source archive must run inside a Git worktree",
  ).trim();
  if (!output) throw new Error("exact source archive Git root is missing");
  return resolve(output);
}

function verifyCommit(repositoryRoot, sourceCommit) {
  if (typeof sourceCommit !== "string" || !FULL_SHA.test(sourceCommit)) {
    throw new Error("source commit must be a lowercase 40-character Git SHA");
  }
  const resolvedCommit = requireSuccessfulGit(
    runGit(["rev-parse", `${sourceCommit}^{commit}`], {
      cwd: repositoryRoot,
      encoding: "utf8",
    }),
    "source commit could not be resolved",
  ).trim();
  if (resolvedCommit !== sourceCommit) {
    throw new Error("source commit did not resolve exactly");
  }
}

function verifyHelperAuthority(repositoryRoot, sourceCommit, helperUrls) {
  for (let index = 0; index < CANONICAL_HELPER_PATHS.length; index += 1) {
    const expectedPath = CANONICAL_HELPER_PATHS[index];
    const helperPath = resolve(fileURLToPath(helperUrls[index]));
    const relativePath = relative(repositoryRoot, helperPath)
      .split(sep)
      .join("/");
    if (relativePath !== expectedPath) {
      throw new Error(
        "exact source archive helper is outside its canonical path",
      );
    }
    const acceptedBlobResult = runGit(
      ["rev-parse", `${sourceCommit}:${expectedPath}`],
      { cwd: repositoryRoot, encoding: "utf8" },
    );
    const acceptedBlob = acceptedBlobResult.stdout?.trim();
    if (
      acceptedBlobResult.error ||
      acceptedBlobResult.signal ||
      acceptedBlobResult.status !== 0 ||
      !/^[0-9a-f]{40,64}$/.test(acceptedBlob)
    ) {
      throw new Error(
        "exact source archive helper is absent from the accepted commit",
      );
    }
    const executingBlobResult = runGit(
      ["hash-object", "--no-filters", "--", helperPath],
      { cwd: repositoryRoot, encoding: "utf8" },
    );
    const executingBlob = executingBlobResult.stdout?.trim();
    if (
      executingBlobResult.error ||
      executingBlobResult.signal ||
      executingBlobResult.status !== 0 ||
      executingBlob !== acceptedBlob
    ) {
      throw new Error(
        "exact source archive helper differs from the accepted commit",
      );
    }
  }
}

function verifyOutputPath(outputPath) {
  if (
    typeof outputPath !== "string" ||
    !isAbsolute(outputPath) ||
    !outputPath.endsWith(".tar.gz")
  ) {
    throw new Error("archive output must be an absolute .tar.gz path");
  }
  if (existsSync(outputPath)) throw new Error("archive output already exists");
}

function createRawArchive(repositoryRoot, sourceCommit, rawPath) {
  let handle;
  try {
    handle = openSync(rawPath, "wx", 0o600);
    const result = runGit(
      [
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.eol=lf",
        "-c",
        "tar.umask=0022",
        "archive",
        "--format=tar",
        sourceCommit,
      ],
      { cwd: repositoryRoot, stdio: ["ignore", handle, "pipe"] },
    );
    if (result.error || result.signal || result.status !== 0) {
      throw new Error("canonical Git archive creation failed");
    }
  } finally {
    if (handle !== undefined) closeSync(handle);
  }
}

function verifyEmbeddedCommit(repositoryRoot, sourceCommit, rawPath) {
  const handle = openSync(rawPath, "r");
  let result;
  try {
    result = runGit(["get-tar-commit-id"], {
      cwd: repositoryRoot,
      encoding: "utf8",
      stdio: [handle, "pipe", "pipe"],
    });
  } finally {
    closeSync(handle);
  }
  const archivedCommit = requireSuccessfulGit(
    result,
    "canonical Git archive identity could not be read",
  ).trim();
  if (archivedCommit !== sourceCommit) {
    throw new Error("canonical Git archive contains a different commit");
  }
}

function loadExpectedTree(repositoryRoot, sourceCommit) {
  const objectFormat = requireSuccessfulGit(
    runGit(["rev-parse", "--show-object-format"], {
      cwd: repositoryRoot,
      encoding: "utf8",
    }),
    "Git object format could not be read",
  ).trim();
  const tree = requireSuccessfulGit(
    runGit(["ls-tree", "-r", "-t", "-z", "--full-tree", sourceCommit], {
      cwd: repositoryRoot,
      maxBuffer: 64 * 1024 * 1024,
    }),
    "accepted Git tree could not be read",
  );
  return {
    entries: parseExactGitTree(tree, objectFormat),
    objectFormat,
  };
}

function writeVerifiedGzip(outputPath, rawTar) {
  const compressed = gzipSync(rawTar, { level: 9 });
  compressed.writeUInt32LE(0, 4);
  compressed[9] = 255;
  let handle;
  let outputOwned = false;
  try {
    handle = openSync(outputPath, "wx", 0o600);
    outputOwned = true;
    let offset = 0;
    while (offset < compressed.length) {
      const writtenBytes = writeSync(
        handle,
        compressed,
        offset,
        compressed.length - offset,
      );
      if (writtenBytes <= 0) {
        throw new Error("compressed source archive write was incomplete");
      }
      offset += writtenBytes;
    }
    closeSync(handle);
    handle = undefined;
    const written = lstatSync(outputPath);
    if (!written.isFile() || written.isSymbolicLink() || written.size === 0) {
      throw new Error("compressed source archive is not a regular file");
    }
    if (!gunzipSync(readFileSync(outputPath)).equals(rawTar)) {
      throw new Error("compressed source archive round trip failed");
    }
    return written.size;
  } catch (error) {
    if (handle !== undefined) closeSync(handle);
    if (outputOwned && existsSync(outputPath)) unlinkSync(outputPath);
    throw error;
  }
}

export function createExactSourceArchive(options) {
  const { sourceCommit, outputPath, cwd = process.cwd() } = options;
  const repositoryRoot = resolveRepositoryRoot(cwd);
  verifyCommit(repositoryRoot, sourceCommit);
  verifyHelperAuthority(repositoryRoot, sourceCommit, [
    import.meta.url,
    new URL("./exact-source-tar-contract.mjs", import.meta.url),
  ]);
  verifyOutputPath(outputPath);

  const transientRoot = mkdtempSync(
    resolve(dirname(outputPath), `.clearra-exact-archive-${process.pid}-`),
  );
  const rawPath = resolve(
    transientRoot,
    `.${basename(outputPath)}.${randomUUID()}.raw.tar`,
  );
  let byteLength;
  let tarByteLength;
  let entryCount;
  let outputOwned = false;
  try {
    createRawArchive(repositoryRoot, sourceCommit, rawPath);
    const rawTar = readFileSync(rawPath);
    tarByteLength = rawTar.length;
    verifyEmbeddedCommit(repositoryRoot, sourceCommit, rawPath);
    const expected = loadExpectedTree(repositoryRoot, sourceCommit);
    ({ entryCount } = verifyExactSourceTar({
      archive: rawTar,
      expectedEntries: expected.entries,
      objectFormat: expected.objectFormat,
      sourceCommit,
    }));
    byteLength = writeVerifiedGzip(outputPath, rawTar);
    outputOwned = true;
  } catch (error) {
    if (outputOwned && existsSync(outputPath)) unlinkSync(outputPath);
    throw error;
  } finally {
    try {
      if (existsSync(rawPath)) unlinkSync(rawPath);
      rmdirSync(transientRoot);
    } catch (error) {
      if (outputOwned && existsSync(outputPath)) unlinkSync(outputPath);
      throw error;
    }
  }

  return Object.freeze({
    byteLength,
    entryCount,
    outputPath,
    sourceCommit,
    tarByteLength,
  });
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!["--source-commit", "--output"].includes(flag) || !value) {
      throw new Error(
        "usage: create-exact-source-archive.mjs --source-commit <sha> --output <absolute-tar-gz-path>",
      );
    }
    if (values.has(flag)) throw new Error(`duplicate argument: ${flag}`);
    values.set(flag, value);
  }
  if (values.size !== 2) {
    throw new Error(
      "usage: create-exact-source-archive.mjs --source-commit <sha> --output <absolute-tar-gz-path>",
    );
  }
  return {
    outputPath: values.get("--output"),
    sourceCommit: values.get("--source-commit"),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const result = createExactSourceArchive(
      parseArguments(process.argv.slice(2)),
    );
    process.stdout.write(
      `exact_source_archive=ready source_commit=${result.sourceCommit} entries=${result.entryCount} tar_bytes=${result.tarByteLength} gzip_bytes=${result.byteLength}\n`,
    );
  } catch (error) {
    process.stderr.write(
      `exact_source_archive=failed ${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
