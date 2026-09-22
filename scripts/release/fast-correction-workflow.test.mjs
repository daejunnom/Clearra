import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [fast, pages, discord] = await Promise.all([
  readFile(new URL("../../.github/workflows/fast-correction.yml", import.meta.url), "utf8"),
  readFile(new URL("../../.github/workflows/pages.yml", import.meta.url), "utf8"),
  readFile(new URL("../../.github/workflows/discord-deploy.yml", import.meta.url), "utf8"),
]);

test("fast workflow has no user-selected performance or product bypass switch", () => {
  assert.doesNotMatch(fast, /performance[_ -]?(?:changed|unchanged|bypass)|skip[_ -]?(?:canonical|test)|product[_ -]?scope/iu);
  assert.match(fast, /accepted_base_sha:/u);
  assert.match(fast, /candidate_sha="\$\(git -C candidate-source rev-parse HEAD\)"/u);
  assert.match(fast, /git -C candidate-source merge-base --is-ancestor/u);
  assert.match(fast, /fast-correction-owners\.v1\.json/u);
  assert.match(fast, /fast-correction-plan\.mjs/u);
  assert.doesNotMatch(fast, /actions:\s*write/u);
  assert.doesNotMatch(fast, /gh workflow run .*release-cli/u);
});

test("accepted-base code classifies and seals the candidate before candidate code executes", () => {
  assert.match(fast, /Check out the candidate without executing it/u);
  assert.match(fast, /node accepted-base\/scripts\/release\/fast-correction-plan\.mjs/u);
  assert.match(fast, /git -C candidate-source diff --quiet .*authority_paths/u);
  assert.match(fast, /node accepted-base\/scripts\/release\/canonical-acceptance-run\.mjs/u);
  assert.match(fast, /latest production tag commit/u);
  assert.match(fast, /fast-correction-evidence\.mjs/u);
  assert.match(fast, /retention-days: 90/u);
});

test("focused jobs cannot build or deploy unrelated CLI, GUI, or Discord products", () => {
  assert.match(fast, /npm test --workspace @clearra\/web/u);
  assert.match(fast, /npm run build --workspace @clearra\/web/u);
  assert.match(fast, /actions\/upload-pages-artifact@v3/u);
  assert.match(fast, /actions\/deploy-pages@v4/u);
  assert.doesNotMatch(fast, /cargo build .*clearra-cli|tauri build|gcloud builds submit|discord-path-confirmation/iu);
  assert.match(fast, /needs\.authority\.outputs\.deploy_pages == 'true'/u);
  assert.match(fast, /needs\.authority\.outputs\.decision == 'fast-eligible'/u);
});

test("unknown or high-risk changes stop with sealed full-required evidence and no automatic privileged dispatch", () => {
  assert.match(fast, /needs\.authority\.outputs\.decision == 'full-required'/u);
  assert.match(fast, /--outcome full-required/u);
  assert.match(fast, /Require the full canonical release path/u);
  assert.match(fast, /exit 2/u);
  assert.match(fast, /does not auto-dispatch or approve the privileged full\/deployment path/u);
});

test("Pages follow-up consumes dual authority while reusing the accepted base build", () => {
  for (const marker of [
    "fast_correction_run_id:",
    "fast_correction_run_attempt:",
    "fast-correction-authority.mjs",
    "--required-control pages",
    "accepted-pages-build-${{ inputs.accepted_sha }}-run-",
    "workflow_source_sha",
    "canonical-acceptance-run.mjs",
  ]) assert.ok(pages.includes(marker), marker);
  assert.match(pages, /main_sha.*WORKFLOW_SOURCE_SHA/su);
  assert.match(pages, /AUTHORITY_SHA: \$\{\{ needs\.accepted-source\.outputs\.workflow_source_sha \}\}/u);
  assert.match(pages, /actions\/deploy-pages@v4/u);
});

test("Discord follow-up consumes dual authority without weakening protected environments", () => {
  for (const marker of [
    "fast_correction_run_id:",
    "fast_correction_run_attempt:",
    "fast-correction-authority.mjs",
    "--required-control discord",
    "discord-fast-control-authority-",
    "fast_control=true",
    "deploy_discord=true",
    "environment: discord-path-confirmation",
    "environment: discord-global-command-sync",
  ]) assert.ok(discord.includes(marker), marker);
  assert.match(discord, /source_commit="\$MANUAL_SHA"/u);
  assert.match(discord, /workflow_source_commit="\$current_main"/u);
  assert.match(discord, /canonical-acceptance-run\.mjs[\s\S]*--expected-run-id "\$accepted_run_id"/u);
  assert.doesNotMatch(discord, /environment:\s*discord-runtime-rollback/u);
});
