import { lstat, realpath } from "node:fs/promises";
import { spawn } from "node:child_process";
import { relative, resolve, win32, posix } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEFAULT_REPOSITORY_ROOT = resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);
const GLOB_META = /[*?\[\]{}!]|(?:^|[\\/])[@+]\(/u;
const CONTROL_CHARACTER = /[\u0000-\u001f\u007f]/u;
const HEAVY_PATH_SEGMENTS = new Set([
  "node_modules",
  "dist",
  "dist-server",
  "build",
  "coverage",
  "models",
  "checkpoints",
  ".cache",
]);
const SECRET_PATH_SEGMENTS = new Set([
  ".git",
  ".ssh",
  "credential",
  ".credential",
  "credentials",
  ".credentials",
  "api-keys",
  "ssh-keys",
  "keys",
  ".keys",
  "secret",
  ".secret",
  "secrets",
  ".secrets",
]);

function canonicalRepositoryRelativePath(input) {
  if (typeof input !== "string" || input.length === 0) {
    throw new Error("focused test paths must be non-empty strings");
  }
  if (input !== input.trim()) {
    throw new Error(`focused test path has surrounding whitespace: ${input}`);
  }
  if (CONTROL_CHARACTER.test(input)) {
    throw new Error("focused test paths must not contain control characters");
  }
  if (win32.isAbsolute(input) || posix.isAbsolute(input)) {
    throw new Error(`focused test path must be repository-relative: ${input}`);
  }
  if (GLOB_META.test(input)) {
    throw new Error(`focused test path must not contain glob syntax: ${input}`);
  }

  const normalized = input.replaceAll("\\", "/");
  if (normalized.startsWith("-")) {
    throw new Error(`focused test path must not begin with an option marker: ${input}`);
  }
  const segments = normalized.split("/");
  if (
    segments.some(
      (segment) => segment.length === 0 || segment === "." || segment === "..",
    )
  ) {
    throw new Error(`focused test path must be canonical: ${input}`);
  }

  const lowerSegments = segments.map((segment) => segment.toLowerCase());
  for (const segment of lowerSegments) {
    if (HEAVY_PATH_SEGMENTS.has(segment)) {
      throw new Error(`focused test path enters a heavy directory: ${input}`);
    }
    if (SECRET_PATH_SEGMENTS.has(segment) || segment === ".env" || segment.startsWith(".env.")) {
      throw new Error(`focused test path enters a secret location: ${input}`);
    }
  }

  const basename = lowerSegments.at(-1);
  if (/^(?:id_ed25519|id_rsa)/u.test(basename)) {
    throw new Error(`focused test path names an SSH credential: ${input}`);
  }
  if (
    /^(?:credentials?|secrets?|api[-_]?keys?|ssh[-_]?keys?)(?:[._-]|$)/u.test(
      basename,
    )
  ) {
    throw new Error(`focused test path names a credential or secret file: ${input}`);
  }
  if (!normalized.endsWith(".test.mjs") && !normalized.endsWith(".contract.ts")) {
    throw new Error(
      `focused test path must end in .test.mjs or .contract.ts: ${input}`,
    );
  }
  return normalized;
}

function isOutsideRoot(repositoryRoot, candidatePath) {
  const rootRelative = relative(repositoryRoot, candidatePath);
  return (
    rootRelative === ".." ||
    rootRelative.startsWith(`..${win32.sep}`) ||
    rootRelative.startsWith(`..${posix.sep}`) ||
    win32.isAbsolute(rootRelative) ||
    posix.isAbsolute(rootRelative)
  );
}

function sameRelativePath(left, right) {
  const normalize = (value) => value.replaceAll("\\", "/");
  return normalize(left) === normalize(right);
}

export async function resolveFocusedTestSelection(
  inputs,
  { repositoryRoot = DEFAULT_REPOSITORY_ROOT } = {},
) {
  if (!Array.isArray(inputs) || inputs.length === 0) {
    throw new Error(
      "at least one explicit repository-relative .test.mjs or .contract.ts file is required",
    );
  }

  const root = resolve(repositoryRoot);
  const physicalRoot = await realpath(root);
  const seen = new Set();
  const nodeTests = [];
  const typescriptContracts = [];

  for (const input of inputs) {
    const repositoryRelative = canonicalRepositoryRelativePath(input);
    const requestedPath = resolve(root, ...repositoryRelative.split("/"));
    if (isOutsideRoot(root, requestedPath)) {
      throw new Error(`focused test path escapes the repository: ${input}`);
    }

    let entry;
    try {
      entry = await lstat(requestedPath);
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new Error(`focused test file does not exist: ${repositoryRelative}`);
      }
      throw error;
    }
    if (entry.isSymbolicLink()) {
      throw new Error(`focused test file must not be a symbolic link: ${repositoryRelative}`);
    }
    if (!entry.isFile()) {
      throw new Error(`focused test path must name a regular file: ${repositoryRelative}`);
    }

    const physicalFile = await realpath(requestedPath);
    if (isOutsideRoot(physicalRoot, physicalFile)) {
      throw new Error(`focused test file resolves outside the repository: ${repositoryRelative}`);
    }
    const physicalRelative = relative(physicalRoot, physicalFile);
    if (!sameRelativePath(repositoryRelative, physicalRelative)) {
      throw new Error(
        `focused test path must not traverse a symbolic-link directory: ${repositoryRelative}`,
      );
    }
    if (seen.has(physicalFile)) {
      throw new Error(`focused test path is duplicated: ${repositoryRelative}`);
    }
    seen.add(physicalFile);

    if (repositoryRelative.endsWith(".test.mjs")) {
      nodeTests.push(repositoryRelative);
    } else {
      typescriptContracts.push(repositoryRelative);
    }
  }

  nodeTests.sort((left, right) => left.localeCompare(right, "en"));
  typescriptContracts.sort((left, right) => left.localeCompare(right, "en"));
  return Object.freeze({
    repositoryRoot: physicalRoot,
    nodeTests: Object.freeze(nodeTests),
    typescriptContracts: Object.freeze(typescriptContracts),
  });
}

export function buildFocusedTestCommandGroups(selection) {
  const groups = [];
  if (selection.nodeTests.length > 0) {
    groups.push(
      Object.freeze({
        label: "node-test",
        command: process.execPath,
        args: Object.freeze(["--test", "--", ...selection.nodeTests]),
        fileCount: selection.nodeTests.length,
      }),
    );
  }
  if (selection.typescriptContracts.length > 0) {
    groups.push(
      Object.freeze({
        label: "typescript-contract",
        command: process.execPath,
        args: Object.freeze([
          "scripts/tools/run-typescript-contracts.mjs",
          ...selection.typescriptContracts,
        ]),
        fileCount: selection.typescriptContracts.length,
      }),
    );
  }
  return Object.freeze(groups);
}

async function executeCommandGroup(group, repositoryRoot, spawnImplementation) {
  process.stdout.write(
    `focused_test_group=${group.label} file_count=${group.fileCount}\n`,
  );
  const child = spawnImplementation(group.command, group.args, {
    cwd: repositoryRoot,
    shell: false,
    stdio: "inherit",
    windowsHide: true,
  });
  const result = await new Promise((resolveResult, rejectResult) => {
    child.once("error", rejectResult);
    child.once("exit", (code, signal) => resolveResult({ code, signal }));
  });
  if (result.code !== 0) {
    throw new Error(
      `${group.label} failed with ${
        result.signal === null ? `exit code ${result.code}` : `signal ${result.signal}`
      }`,
    );
  }
}

export async function runFocusedTests(
  inputs,
  {
    repositoryRoot = DEFAULT_REPOSITORY_ROOT,
    spawnImplementation = spawn,
  } = {},
) {
  const selection = await resolveFocusedTestSelection(inputs, { repositoryRoot });
  const groups = buildFocusedTestCommandGroups(selection);
  for (const group of groups) {
    await executeCommandGroup(
      group,
      selection.repositoryRoot,
      spawnImplementation,
    );
  }
  process.stdout.write(
    `focused_tests=passed file_count=${
      selection.nodeTests.length + selection.typescriptContracts.length
    } group_count=${groups.length}\n`,
  );
  return selection;
}

const isMain =
  process.argv[1] !== undefined &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url;

if (isMain) {
  try {
    await runFocusedTests(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`focused_tests=failed reason=${error.message}\n`);
    process.exitCode = 1;
  }
}
