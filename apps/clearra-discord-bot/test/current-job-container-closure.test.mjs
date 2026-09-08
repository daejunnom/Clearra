import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const sourceRootUrl = new URL("../", import.meta.url);
const runtimeEntries = Object.freeze([
  "scripts/run-cloud-candidate-smoke-job.mjs",
  "src/clearra/command.mjs",
  "src/job-service/main.mjs",
  "src/job-service/server.mjs",
]);
const appSourcePrefix = "/workspace/apps/clearra-discord-bot/";

const staticModuleSpecifier =
  /\b(?:import|export)\s+(?:(?:[^;]*?)\s+from\s+)?["']([^"']+)["']/gu;
const dynamicModuleSpecifier =
  /\bimport\s*\(\s*["']([^"']+)["']\s*\)/gu;

for (const [file, stage] of [
  ["Dockerfile.current-job-service", "node-build"],
  ["Dockerfile.accepted-job-service", "accepted-inputs"],
]) {
  test(`${file} closes and imports its functional runtime module graph without timing tools`, async () => {
    const dockerfile = await readFile(new URL(file, sourceRootUrl), "utf8");
    const copyRules = runtimeCopyRules(dockerfile);
    const closure = await relativeEsmClosure(runtimeEntries);

    for (const modulePath of ["src", "scripts/run-cloud-candidate-smoke-job.mjs"]) {
      assert.ok(dockerfile.split(/\r?\n/u).includes(
        `COPY --from=${stage} ${appSourcePrefix}${modulePath} ./${modulePath}`,
      ));
    }
    assert.doesNotMatch(dockerfile, /benchmark-cloud-cli-parity/u);

    for (const modulePath of closure) {
      assert.ok(
        copyRules.some((rule) => copyRulePreservesModulePath(rule, modulePath)),
        `${file} does not copy transitive ESM dependency ${modulePath}`,
      );
    }

    const imports = dockerfile.match(/node --input-type=module -e "(await import[^"\r\n]+)"/u)?.[1];
    assert.ok(imports, `${file} must import the runtime closure during packaging`);
    for (const modulePath of [
      "src/clearra/command.mjs", "src/job-service/server.mjs", "scripts/run-cloud-candidate-smoke-job.mjs",
    ]) assert.ok(imports.includes(`await import('./${modulePath}')`));
  });
}

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

test("job service and candidate smoke import only Node builtins and source, without document codecs", async () => {
  const closure = await relativeEsmClosure([
    "scripts/run-cloud-candidate-smoke-job.mjs", "src/job-service/server.mjs",
  ]);
  assert.ok(!closure.has("src/discord/slash-command-input.mjs"));
  assert.ok(closure.has("src/discord/field-limits.mjs"));
  for (const modulePath of closure) {
    const source = await readFile(new URL(modulePath, sourceRootUrl), "utf8");
    for (const specifier of moduleSpecifiers(source)) {
      assert.ok(specifier.startsWith(".") || specifier.startsWith("node:"),
        `${modulePath} pulls a package into the dependency-free job runtime: ${specifier}`);
    }
  }
});

function moduleSpecifiers(source) {
  return [
    ...source.matchAll(staticModuleSpecifier),
    ...source.matchAll(dynamicModuleSpecifier),
  ].map((match) => match[1]);
}

function runtimeCopyRules(dockerfile) {
  const rules = [];
  const copyRule = /^COPY --from=(?:node-build|accepted-inputs) (\S+) (\S+)$/gmu;
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
