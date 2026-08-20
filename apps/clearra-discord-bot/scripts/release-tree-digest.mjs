import { createHash } from "node:crypto";
import {
  lstatSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  realpathSync,
} from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function releaseTreeSha256(rootPath) {
  const root = realpathSync(resolve(String(rootPath ?? "")));
  if (!lstatSync(root).isDirectory()) {
    throw new Error("release tree digest root must be a directory");
  }
  const entries = [];
  collect(root, root, entries);
  entries.sort((left, right) => compareText(left.path, right.path));
  const hash = createHash("sha256");
  hash.update("clearra-release-tree-v1\0", "utf8");
  for (const entry of entries) {
    hash.update(entry.type, "utf8");
    hash.update("\0", "utf8");
    hash.update(entry.path, "utf8");
    hash.update("\0", "utf8");
    if (entry.type === "file") {
      const bytes = readFileSync(entry.absolutePath);
      hash.update(String(bytes.byteLength), "utf8");
      hash.update("\0", "utf8");
      hash.update(bytes);
    } else if (entry.type === "symlink") {
      hash.update(entry.target, "utf8");
    }
    hash.update("\0", "utf8");
  }
  return hash.digest("hex");
}

function collect(root, directory, entries) {
  const children = readdirSync(directory, { withFileTypes: true });
  children.sort((left, right) => compareText(left.name, right.name));
  for (const child of children) {
    const absolutePath = resolve(directory, child.name);
    const path = normalizePath(relative(root, absolutePath));
    const metadata = lstatSync(absolutePath);
    if (metadata.isSymbolicLink()) {
      let resolvedTarget;
      try {
        resolvedTarget = realpathSync(absolutePath);
      } catch {
        throw new Error(`release tree symlink is dangling: ${path}`);
      }
      const targetRelative = relative(root, resolvedTarget);
      if (
        targetRelative === ".." ||
        targetRelative.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) ||
        isAbsolute(targetRelative)
      ) {
        throw new Error(`release tree symlink escapes the immutable root: ${path}`);
      }
      entries.push({ type: "symlink", path, target: readlinkSync(absolutePath) });
    } else if (metadata.isDirectory()) {
      entries.push({ type: "directory", path });
      collect(root, absolutePath, entries);
    } else if (metadata.isFile()) {
      entries.push({ type: "file", path, absolutePath });
    } else {
      throw new Error(`release tree contains unsupported entry type: ${path}`);
    }
  }
}

function normalizePath(value) {
  return value.replaceAll("\\", "/");
}

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    console.log(releaseTreeSha256(process.argv[2]));
  } catch {
    console.error("release_tree_digest=failed");
    process.exitCode = 2;
  }
}
