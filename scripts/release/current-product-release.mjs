import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER = /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u;
const DEFAULT_ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)));

export function readCurrentProductRelease(root = DEFAULT_ROOT) {
  const source = readFileSync(resolve(root, "Cargo.toml"), "utf8");
  const lines = source.split(/\r?\n/u);
  const sectionStart = lines.findIndex(
    (line) => line.trim() === "[workspace.package]",
  );
  if (sectionStart < 0) {
    throw new Error("Cargo.toml has no [workspace.package] release authority");
  }
  const sectionEndOffset = lines
    .slice(sectionStart + 1)
    .findIndex((line) => /^\s*\[/u.test(line));
  const sectionEnd = sectionEndOffset < 0
    ? lines.length
    : sectionStart + 1 + sectionEndOffset;
  const body = lines.slice(sectionStart + 1, sectionEnd).join("\n");
  const version = /^version\s*=\s*"([^"]+)"\s*$/mu.exec(body)?.[1] ?? "";
  if (!SEMVER.test(version)) {
    throw new Error("Cargo.toml workspace release version is missing or invalid");
  }
  return Object.freeze({ version, tag: `v${version}` });
}

export function expectedProductArtifactName(role, release = CURRENT_PRODUCT_RELEASE) {
  const suffix = new Map([
    ["linux-cli", `Clearra-CLI-${release.tag}-linux-x86_64`],
    ["windows-cli", `Clearra-CLI-${release.tag}-windows-x86_64.exe`],
    ["windows-gui", `Clearra-GUI-${release.tag}-windows-x86_64.exe`],
  ]).get(role);
  if (suffix === undefined) {
    throw new Error(`unsupported product artifact role: ${String(role)}`);
  }
  return suffix;
}

export const CURRENT_PRODUCT_RELEASE = readCurrentProductRelease();
export const CURRENT_PRODUCT_VERSION = CURRENT_PRODUCT_RELEASE.version;
export const CURRENT_PRODUCT_TAG = CURRENT_PRODUCT_RELEASE.tag;

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const formatIndex = process.argv.indexOf("--format");
  const format = formatIndex < 0 ? "version" : process.argv[formatIndex + 1];
  if (process.argv.length !== (formatIndex < 0 ? 2 : 4)) {
    throw new Error("usage: current-product-release.mjs [--format version|tag|json]");
  }
  if (format === "version") process.stdout.write(`${CURRENT_PRODUCT_VERSION}\n`);
  else if (format === "tag") process.stdout.write(`${CURRENT_PRODUCT_TAG}\n`);
  else if (format === "json") process.stdout.write(`${JSON.stringify(CURRENT_PRODUCT_RELEASE)}\n`);
  else throw new Error("current product release format is invalid");
}
