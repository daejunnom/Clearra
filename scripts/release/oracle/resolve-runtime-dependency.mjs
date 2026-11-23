import { realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const RUNTIME_PACKAGES = new Set(["tetris-fumen", "undici"]);

export function resolveOracleRuntimeDependency(repositoryRoot, packageName) {
  if (!RUNTIME_PACKAGES.has(packageName)) {
    throw new Error("The requested package is outside the closed Oracle runtime set");
  }
  const root = realpathSync(resolve(repositoryRoot));
  // Resolve from the actual consumer. pnpm's isolated installation does not
  // promise a hoisted root node_modules entry, and its entry can be a symlink.
  const consumer = createRequire(join(root, "apps/clearra-discord-bot/package.json"));
  const installed = dirname(realpathSync(consumer.resolve(`${packageName}/package.json`)));
  const local = relative(root, installed);
  if (local === "" || local === ".." || local.startsWith(`..${sep}`) || isAbsolute(local)) {
    throw new Error(`The resolved ${packageName} package is outside the repository installation`);
  }
  return installed;
}

export function resolveOracleFumenDependency(repositoryRoot) {
  return resolveOracleRuntimeDependency(repositoryRoot, "tetris-fumen");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length < 3 || process.argv.length > 4) {
      throw new Error("one repository root and an optional runtime package are required");
    }
    process.stdout.write(`${resolveOracleRuntimeDependency(process.argv[2], process.argv[3] ?? "tetris-fumen")}\n`);
  } catch (error) {
    process.stderr.write(`oracle_runtime_dependency=failed reason=${error.message}\n`);
    process.exitCode = 66;
  }
}
