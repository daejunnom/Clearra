import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const FULL_SHA = /^[0-9a-f]{40}$/;
const SAFE_RELEASE_TAG = /^v[0-9][0-9A-Za-z.+-]*$/;

function assertReleaseTag(tag) {
  if (
    typeof tag !== "string" ||
    !SAFE_RELEASE_TAG.test(tag) ||
    tag.includes("..") ||
    tag.endsWith(".") ||
    tag.endsWith(".lock")
  ) {
    throw new Error("release tag is missing or malformed");
  }
}

function assertExpectedCommit(expectedCommit) {
  if (typeof expectedCommit !== "string" || !FULL_SHA.test(expectedCommit)) {
    throw new Error(
      "expected release commit must be a lowercase 40-character SHA",
    );
  }
}

export function validateRemoteAnnotatedTagOutput(output, options) {
  const { tag, expectedCommit } = options;
  assertReleaseTag(tag);
  assertExpectedCommit(expectedCommit);
  if (typeof output !== "string") {
    throw new Error("remote tag response is malformed");
  }

  const tagRef = `refs/tags/${tag}`;
  const peeledRef = `${tagRef}^{}`;
  const lines = output.split(/\r?\n/);
  if (lines.at(-1) === "") lines.pop();
  if (lines.length === 0) {
    throw new Error(`remote release tag ${tag} is missing`);
  }

  const records = new Map();
  for (const line of lines) {
    const match = /^([0-9a-f]{40})\t(\S+)$/.exec(line);
    if (!match) {
      throw new Error("remote tag response is malformed");
    }
    const [, sha, ref] = match;
    if (ref !== tagRef && ref !== peeledRef) {
      throw new Error("remote tag response is ambiguous: unexpected ref");
    }
    if (records.has(ref)) {
      throw new Error("remote tag response is ambiguous: duplicate ref");
    }
    records.set(ref, sha);
  }

  const tagObject = records.get(tagRef);
  const peeledCommit = records.get(peeledRef);
  if (!tagObject && !peeledCommit) {
    throw new Error(`remote release tag ${tag} is missing`);
  }
  if (tagObject && !peeledCommit) {
    throw new Error(`remote release tag ${tag} is lightweight, not annotated`);
  }
  if (!tagObject || !peeledCommit || records.size !== 2) {
    throw new Error("remote annotated tag response is malformed or ambiguous");
  }
  if (tagObject === peeledCommit) {
    throw new Error(
      "remote annotated tag object is indistinguishable from its commit",
    );
  }
  if (peeledCommit !== expectedCommit) {
    throw new Error(
      `remote annotated release tag ${tag} moved or resolves to a different commit`,
    );
  }

  return Object.freeze({ tag, tagObject, peeledCommit });
}

export function verifyRemoteAnnotatedTag(options) {
  const { tag, expectedCommit, runGit = spawnSync } = options;
  assertReleaseTag(tag);
  assertExpectedCommit(expectedCommit);
  const tagRef = `refs/tags/${tag}`;
  const result = runGit(
    "git",
    ["ls-remote", "origin", tagRef, `${tagRef}^{}`],
    {
      encoding: "utf8",
      maxBuffer: 1024 * 1024,
      windowsHide: true,
    },
  );
  if (result.error) {
    throw new Error("remote annotated tag query could not be started");
  }
  if (result.signal || result.status !== 0) {
    throw new Error("remote annotated tag query failed");
  }
  return validateRemoteAnnotatedTagOutput(result.stdout, {
    tag,
    expectedCommit,
  });
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!["--tag", "--expected-commit"].includes(flag) || !value) {
      throw new Error(
        "usage: verify-remote-annotated-tag.mjs --tag <tag> --expected-commit <sha>",
      );
    }
    if (values.has(flag)) {
      throw new Error(`duplicate argument: ${flag}`);
    }
    values.set(flag, value);
  }
  if (values.size !== 2) {
    throw new Error(
      "usage: verify-remote-annotated-tag.mjs --tag <tag> --expected-commit <sha>",
    );
  }
  return {
    tag: values.get("--tag"),
    expectedCommit: values.get("--expected-commit"),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const result = verifyRemoteAnnotatedTag(
      parseArguments(process.argv.slice(2)),
    );
    process.stdout.write(
      `remote annotated release tag ${result.tag} resolves exactly to ${result.peeledCommit}\n`,
    );
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
