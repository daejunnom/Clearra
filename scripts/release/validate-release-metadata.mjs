import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function readReleaseMetadata(root) {
  const rootCargo = readText(root, "Cargo.toml");
  const desktopPackage = JSON.parse(
    readText(root, "apps/clearra-desktop/package.json"),
  );
  const desktopCargo = readText(root, "apps/clearra-desktop/src-tauri/Cargo.toml");
  const tauri = JSON.parse(
    readText(root, "apps/clearra-desktop/src-tauri/tauri.conf.json"),
  );
  const workspaceLock = readText(root, "Cargo.lock");
  const desktopLock = readText(root, "apps/clearra-desktop/src-tauri/Cargo.lock");
  const changelog = readText(root, "CHANGELOG.md");

  const version = cargoSectionVersion(rootCargo, "workspace.package");
  const versions = Object.freeze({
    workspace: version,
    desktopPackage: String(desktopPackage.version ?? ""),
    desktopCargo: cargoSectionVersion(desktopCargo, "package"),
    tauri: String(tauri.version ?? ""),
  });
  const mismatched = Object.entries(versions)
    .filter(([, value]) => value !== version)
    .map(([surface, value]) => `${surface}=${value || "missing"}`);
  if (mismatched.length > 0) {
    throw new Error(
      `release version surfaces differ from ${version}: ${mismatched.join(", ")}`,
    );
  }
  assertClearraLockVersions(workspaceLock, version, "Cargo.lock");
  assertClearraLockVersions(
    desktopLock,
    version,
    "apps/clearra-desktop/src-tauri/Cargo.lock",
  );

  const releaseHeading = new RegExp(
    `^## ${escapeRegex(version)} - \\d{4}-\\d{2}-\\d{2}\\s*$`,
    "m",
  );
  if (!releaseHeading.test(changelog)) {
    throw new Error(`CHANGELOG.md has no dated ${version} release entry`);
  }
  const section = changelog.split(releaseHeading)[1]?.split(/^## /m)[0] ?? "";
  if (!/^\s*-\s+\S/m.test(section)) {
    throw new Error(`CHANGELOG.md ${version} release entry is empty`);
  }
  return Object.freeze({ version, versions });
}

function assertClearraLockVersions(source, expected, label) {
  const packages = source
    .split(/^\[\[package\]\]\s*$/m)
    .map((block) => ({
      name: /^name\s*=\s*"([^"]+)"\s*$/m.exec(block)?.[1] ?? "",
      version: /^version\s*=\s*"([^"]+)"\s*$/m.exec(block)?.[1] ?? "",
    }))
    .filter(({ name }) => /^clearra(?:[-_]|$)/.test(name));
  if (packages.length === 0) {
    throw new Error(`${label} has no Clearra package entries`);
  }
  const mismatched = packages.filter(({ version }) => version !== expected);
  if (mismatched.length > 0) {
    throw new Error(
      `${label} Clearra package versions differ from ${expected}: ${mismatched
        .map(({ name, version }) => `${name}=${version || "missing"}`)
        .join(", ")}`,
    );
  }
}

export function validateReleaseMetadata(root, options = {}) {
  const metadata = readReleaseMetadata(root);
  if (options.tag && options.tag !== `v${metadata.version}`) {
    throw new Error(
      `tag ${options.tag} does not match release version ${metadata.version}`,
    );
  }
  return metadata;
}

function cargoSectionVersion(source, section) {
  const lines = source.split(/\r?\n/);
  const header = `[${section}]`;
  const start = lines.findIndex((line) => line.trim() === header);
  if (start < 0) throw new Error(`${section} version is missing`);
  const body = lines
    .slice(start + 1, nextSectionIndex(lines, start + 1))
    .join("\n");
  const version = /^version\s*=\s*"([^"]+)"\s*$/m.exec(body)?.[1] ?? "";
  if (!version) throw new Error(`${section} version is missing`);
  return version;
}

function nextSectionIndex(lines, start) {
  const relative = lines.slice(start).findIndex((line) => /^\s*\[/.test(line));
  return relative < 0 ? lines.length : start + relative;
}

function readText(root, relativePath) {
  return readFileSync(resolve(root, relativePath), "utf8");
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const tagIndex = process.argv.indexOf("--tag");
  const tag = tagIndex >= 0 ? process.argv[tagIndex + 1] : "";
  try {
    const metadata = validateReleaseMetadata(root, { tag });
    process.stdout.write(`${metadata.version}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
