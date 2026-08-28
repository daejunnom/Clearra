import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const dockerfileUrl = new URL("../Dockerfile.current-job-service", import.meta.url);
const sourceRootUrl = new URL("../", import.meta.url);
const runtimeEntries = Object.freeze([
  "src/clearra/command.mjs",
  "src/job-service/main.mjs",
  "src/job-service/server.mjs",
]);
const appSourcePrefix = "/workspace/apps/clearra-discord-bot/";

const staticModuleSpecifier =
  /\b(?:import|export)\s+(?:(?:[^;]*?)\s+from\s+)?["']([^"']+)["']/gu;
const dynamicModuleSpecifier =
  /\bimport\s*\(\s*["']([^"']+)["']\s*\)/gu;

test("current job image closes and imports its runtime module graph during build", async () => {
  const dockerfile = await readFile(dockerfileUrl, "utf8");
  const copyRules = runtimeCopyRules(dockerfile);
  const closure = await relativeEsmClosure(runtimeEntries);

  assert.match(
    dockerfile,
    /^COPY --from=node-build \/workspace\/apps\/clearra-discord-bot\/src \.\/src$/m,
  );

  for (const modulePath of closure) {
    assert.ok(
      copyRules.some((rule) => copyRulePreservesModulePath(rule, modulePath)),
      `current-job runtime image does not copy transitive ESM dependency ${modulePath}`,
    );
  }

  assert.match(
    dockerfile,
    /^RUN node --input-type=module -e "await import\('\.\/src\/clearra\/command\.mjs'\); await import\('\.\/src\/job-service\/server\.mjs'\)"$/m,
  );
});

async function relativeEsmClosure(entries) {
  const closure = new Set();
  const queue = [...entries];

  while (queue.length > 0) {
    const modulePath = queue.shift();
    if (closure.has(modulePath)) continue;
    closure.add(modulePath);

    const source = await readFile(new URL(modulePath, sourceRootUrl), "utf8");
    for (const specifier of moduleSpecifiers(source)) {
      if (!specifier.startsWith(".")) continue;
      const dependencyPath = path.posix.normalize(
        path.posix.join(path.posix.dirname(modulePath), specifier),
      );
      assert.ok(
        dependencyPath.startsWith("src/"),
        `${modulePath} imports outside the application source tree: ${specifier}`,
      );
      queue.push(dependencyPath);
    }
  }

  return closure;
}

function moduleSpecifiers(source) {
  return [
    ...source.matchAll(staticModuleSpecifier),
    ...source.matchAll(dynamicModuleSpecifier),
  ].map((match) => match[1]);
}

function runtimeCopyRules(dockerfile) {
  const rules = [];
  const copyRule = /^COPY --from=node-build (\S+) (\S+)$/gmu;
  for (const match of dockerfile.matchAll(copyRule)) {
    if (!match[1].startsWith(appSourcePrefix) || !match[2].startsWith("./")) {
      continue;
    }
    rules.push({
      source: match[1].slice(appSourcePrefix.length),
      destination: match[2].slice(2),
    });
  }
  return rules;
}

function copyRulePreservesModulePath(rule, modulePath) {
  if (modulePath === rule.source) return rule.destination === modulePath;
  if (!modulePath.startsWith(`${rule.source}/`)) return false;
  const suffix = modulePath.slice(rule.source.length + 1);
  return path.posix.join(rule.destination, suffix) === modulePath;
}
