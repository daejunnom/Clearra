import { createHash } from "node:crypto";
import { posix } from "node:path";

const utf8 = new TextDecoder("utf-8", { fatal: true });
const SUPPORTED_OBJECT_FORMATS = new Map([
  ["sha1", 40],
  ["sha256", 64],
]);

function decodeUtf8(bytes, label) {
  try {
    return utf8.decode(bytes);
  } catch {
    throw new Error(`${label} is not valid UTF-8`);
  }
}

function assertSafePath(path, label) {
  if (
    !path ||
    path.startsWith("/") ||
    /^[A-Za-z]:/.test(path) ||
    path.includes("\\") ||
    path.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error(`${label} is not a canonical repository-relative path`);
  }
}

function expectedEntryKind(mode, type) {
  if (mode === "040000" && type === "tree") return "directory";
  if (mode === "100644" && type === "blob") return "regular";
  if (mode === "100755" && type === "blob") return "executable";
  if (mode === "120000" && type === "blob") return "symlink";
  throw new Error(`unsupported Git tree entry mode/type: ${mode} ${type}`);
}

export function parseExactGitTree(treeOutput, objectFormat) {
  const oidLength = SUPPORTED_OBJECT_FORMATS.get(objectFormat);
  if (!oidLength) {
    throw new Error(`unsupported Git object format: ${objectFormat}`);
  }
  if (!Buffer.isBuffer(treeOutput)) {
    throw new Error("Git tree response must be bytes");
  }

  const entries = new Map();
  const foldedPaths = new Map();
  let offset = 0;
  while (offset < treeOutput.length) {
    const end = treeOutput.indexOf(0, offset);
    if (end < 0) throw new Error("Git tree response is not NUL terminated");
    if (end === offset)
      throw new Error("Git tree response has an empty record");
    const record = treeOutput.subarray(offset, end);
    const tab = record.indexOf(9);
    if (tab < 0) throw new Error("Git tree response is malformed");
    const header = record.subarray(0, tab).toString("ascii");
    const match = /^(\d{6}) (blob|tree|commit) ([0-9a-f]+)$/.exec(header);
    if (!match || match[3].length !== oidLength) {
      throw new Error("Git tree response has an invalid object record");
    }
    const [, mode, type, oid] = match;
    const path = decodeUtf8(record.subarray(tab + 1), "Git tree path");
    assertSafePath(path, "Git tree path");
    if (entries.has(path)) throw new Error(`duplicate Git tree path: ${path}`);
    const folded = path.toLocaleLowerCase("en-US");
    if (foldedPaths.has(folded)) {
      throw new Error(
        `case-insensitive Git tree path collision: ${foldedPaths.get(folded)} and ${path}`,
      );
    }
    foldedPaths.set(folded, path);
    entries.set(path, {
      kind: expectedEntryKind(mode, type),
      mode,
      oid,
      path,
    });
    offset = end + 1;
  }
  if (entries.size === 0) throw new Error("Git tree is empty");
  return entries;
}

function fieldBytes(header, start, length) {
  const field = header.subarray(start, start + length);
  const terminator = field.indexOf(0);
  return terminator < 0 ? field : field.subarray(0, terminator);
}

function parseOctal(header, start, length, label) {
  const value = fieldBytes(header, start, length).toString("ascii").trim();
  if (!/^[0-7]+$/.test(value)) throw new Error(`${label} is not octal`);
  return Number.parseInt(value, 8);
}

function verifyHeaderChecksum(header) {
  const expected = parseOctal(header, 148, 8, "tar header checksum");
  let actual = 0;
  for (let index = 0; index < 512; index += 1) {
    actual += index >= 148 && index < 156 ? 32 : header[index];
  }
  if (actual !== expected) throw new Error("tar header checksum mismatch");
}

function parsePaxRecords(content, label) {
  const records = new Map();
  let offset = 0;
  while (offset < content.length) {
    const space = content.indexOf(32, offset);
    if (space < 0) throw new Error(`${label} has no record length`);
    const lengthText = content.subarray(offset, space).toString("ascii");
    if (!/^[1-9][0-9]*$/.test(lengthText)) {
      throw new Error(`${label} record length is malformed`);
    }
    const length = Number.parseInt(lengthText, 10);
    const end = offset + length;
    if (end > content.length || content[end - 1] !== 10) {
      throw new Error(`${label} record is truncated`);
    }
    const body = content.subarray(space + 1, end - 1);
    const equals = body.indexOf(61);
    if (equals <= 0) throw new Error(`${label} record has no key/value pair`);
    const key = body.subarray(0, equals).toString("ascii");
    if (!/^[A-Za-z0-9._-]+$/.test(key) || records.has(key)) {
      throw new Error(`${label} has an invalid or duplicate key`);
    }
    records.set(key, body.subarray(equals + 1));
    offset = end;
  }
  return records;
}

function headerPathBytes(header) {
  const name = fieldBytes(header, 0, 100);
  const prefix = fieldBytes(header, 345, 155);
  return prefix.length ? Buffer.concat([prefix, Buffer.from("/"), name]) : name;
}

function gitBlobOid(objectFormat, content) {
  const hash = createHash(objectFormat === "sha256" ? "sha256" : "sha1");
  hash.update(Buffer.from(`blob ${content.length}\0`, "utf8"));
  hash.update(content);
  return hash.digest("hex");
}

function assertSafeSymlink(path, linkBytes, expectedEntries) {
  const target = decodeUtf8(linkBytes, `symlink target for ${path}`);
  if (
    !target ||
    target.startsWith("/") ||
    /^[A-Za-z]:/.test(target) ||
    target.includes("\\")
  ) {
    throw new Error(`symlink target escapes the source archive: ${path}`);
  }
  const resolved = posix.normalize(posix.join(posix.dirname(path), target));
  if (
    resolved === ".." ||
    resolved.startsWith("../") ||
    posix.isAbsolute(resolved) ||
    !expectedEntries.has(resolved)
  ) {
    throw new Error(
      `symlink target is dangling or outside the source tree: ${path}`,
    );
  }
}

function verifyPayload(entry, tarEntry, objectFormat, expectedEntries) {
  const { content, linkBytes, mode, type } = tarEntry;
  if (entry.kind === "directory") {
    if (type !== "5" || mode !== 0o755 || content.length !== 0) {
      throw new Error(`directory tar contract mismatch: ${entry.path}`);
    }
    return;
  }
  if (entry.kind === "symlink") {
    if (type !== "2" || mode !== 0o777 || content.length !== 0) {
      throw new Error(`symlink tar contract mismatch: ${entry.path}`);
    }
    if (gitBlobOid(objectFormat, linkBytes) !== entry.oid) {
      throw new Error(`symlink target differs from Git blob: ${entry.path}`);
    }
    assertSafeSymlink(entry.path, linkBytes, expectedEntries);
    return;
  }
  const expectedMode = entry.kind === "executable" ? 0o755 : 0o644;
  if (type !== "0" || mode !== expectedMode) {
    throw new Error(`regular-file tar contract mismatch: ${entry.path}`);
  }
  if (gitBlobOid(objectFormat, content) !== entry.oid) {
    throw new Error(`regular-file bytes differ from Git blob: ${entry.path}`);
  }
}

export function verifyExactSourceTar(options) {
  const { archive, expectedEntries, objectFormat, sourceCommit } = options;
  if (!Buffer.isBuffer(archive) || archive.length < 1536) {
    throw new Error("source tar is missing or too small");
  }
  if (archive.length % 512 !== 0) {
    throw new Error("source tar is not block aligned");
  }
  const commitLength = SUPPORTED_OBJECT_FORMATS.get(objectFormat);
  if (!commitLength || sourceCommit.length !== commitLength) {
    throw new Error("source tar object format and commit disagree");
  }

  const seen = new Set();
  let globalIdentitySeen = false;
  let pendingPax;
  let offset = 0;
  let terminated = false;
  while (offset + 512 <= archive.length) {
    const header = archive.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      const remainder = archive.subarray(offset);
      if (remainder.length < 1024 || !remainder.every((byte) => byte === 0)) {
        throw new Error("source tar has invalid end padding");
      }
      terminated = true;
      break;
    }
    verifyHeaderChecksum(header);
    const size = parseOctal(header, 124, 12, "tar member size");
    if (!Number.isSafeInteger(size))
      throw new Error("tar member size is unsafe");
    const contentStart = offset + 512;
    const contentEnd = contentStart + size;
    if (contentEnd > archive.length)
      throw new Error("source tar member is truncated");
    const content = archive.subarray(contentStart, contentEnd);
    const type = header[156] === 0 ? "0" : String.fromCharCode(header[156]);
    offset = contentStart + Math.ceil(size / 512) * 512;

    if (type === "g") {
      if (globalIdentitySeen || pendingPax) {
        throw new Error(
          "source tar has duplicate or misplaced global metadata",
        );
      }
      const records = parsePaxRecords(content, "global pax header");
      if (records.size !== 1 || !records.has("comment")) {
        throw new Error("source tar global metadata is not commit-only");
      }
      if (
        decodeUtf8(records.get("comment"), "source tar commit") !== sourceCommit
      ) {
        throw new Error("source tar global commit identity differs");
      }
      globalIdentitySeen = true;
      continue;
    }
    if (type === "x") {
      if (pendingPax) throw new Error("source tar has stacked pax metadata");
      const records = parsePaxRecords(content, "extended pax header");
      for (const key of records.keys()) {
        if (key !== "path" && key !== "linkpath") {
          throw new Error(`unsupported source tar pax key: ${key}`);
        }
      }
      pendingPax = records;
      continue;
    }
    if (!["0", "2", "5"].includes(type)) {
      throw new Error(`unsupported source tar member type: ${type}`);
    }

    const rawPath = pendingPax?.get("path") ?? headerPathBytes(header);
    const rawLink = pendingPax?.get("linkpath") ?? fieldBytes(header, 157, 100);
    pendingPax = undefined;
    let path = decodeUtf8(rawPath, "source tar path");
    if (type === "5") {
      if (!path.endsWith("/")) throw new Error("tar directory lacks a slash");
      path = path.slice(0, -1);
    } else if (path.endsWith("/")) {
      throw new Error("non-directory tar path has a trailing slash");
    }
    assertSafePath(path, "source tar path");
    if (seen.has(path)) throw new Error(`duplicate source tar path: ${path}`);
    const expected = expectedEntries.get(path);
    if (!expected) throw new Error(`source tar has an extra path: ${path}`);
    seen.add(path);
    verifyPayload(
      expected,
      {
        content,
        linkBytes: rawLink,
        mode: parseOctal(header, 100, 8, "tar member mode"),
        type,
      },
      objectFormat,
      expectedEntries,
    );
  }

  if (!terminated || pendingPax || !globalIdentitySeen) {
    throw new Error("source tar is unterminated or missing commit metadata");
  }
  if (seen.size !== expectedEntries.size) {
    const missing = [...expectedEntries.keys()].find((path) => !seen.has(path));
    throw new Error(`source tar is missing Git tree path: ${missing}`);
  }
  return Object.freeze({ entryCount: seen.size });
}
