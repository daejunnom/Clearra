import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const dockerfileUrl = new URL("../Dockerfile.current-job-service", import.meta.url);

test("current job image closes and imports its runtime module graph during build", async () => {
  const dockerfile = await readFile(dockerfileUrl, "utf8");

  for (const modulePath of [
    "src/discord/build-v2-result.mjs",
    "src/discord/typed-product-result.mjs",
  ]) {
    assert.match(
      dockerfile,
      new RegExp(
        String.raw`^COPY --from=node-build /workspace/apps/clearra-discord-bot/${modulePath.replaceAll("/", String.raw`\/`)} \./${modulePath.replaceAll("/", String.raw`\/`)}$`,
        "m",
      ),
    );
  }

  assert.match(
    dockerfile,
    /^RUN node --input-type=module -e "await import\('\.\/src\/clearra\/command\.mjs'\); await import\('\.\/src\/job-service\/server\.mjs'\)"$/m,
  );
});
