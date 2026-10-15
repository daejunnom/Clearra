import { realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export function resolveOracleFumenDependency(repositoryRoot) {
  const root = realpathSync(resolve(repositoryRoot));
  // Resolve from the actual consumer. pnpm's isolated installation does not
  // promise a hoisted root node_modules entry, and its entry can be a symlink.
  const consumer = createRequire(join(root, "apps/clearra-discord-bot/package.json"));
  const installed = dirname(realpathSync(consumer.resolve("tetris-fumen/package.json")));
  const local = relative(root, installed);
  if (local === "" || local === ".." || local.startsWith(`..${sep}`) || isAbsolute(local)) {
    throw new Error("The resolved tetris-fumen package is outside the repository installation");
  }
  return installed;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 3) throw new Error("one repository root is required");
    process.stdout.write(`${resolveOracleFumenDependency(process.argv[2])}\n`);
  } catch (error) {
    process.stderr.write(`oracle_runtime_dependency=failed reason=${error.message}\n`);
    process.exitCode = 66;
  }
}
