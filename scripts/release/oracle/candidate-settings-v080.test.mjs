import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  candidateSettingsAuthorityV080,
  canonicalCandidateOrigin,
  renderCandidateSettingsV080,
} from "./candidate-settings-v080.mjs";

const SOURCE_COMMIT = "0123456789abcdef0123456789abcdef01234567";
const CANDIDATE_URL = "https://candidate.example.test";
const FIXTURE = fileURLToPath(
  new URL("../../../tests/fixtures/contracts/oracle_candidate_settings_v080.v1.txt", import.meta.url),
);
const LAUNCHER = fileURLToPath(
  new URL("./clearra-oracle-release-deploy-v080", import.meta.url),
);
const CLI = fileURLToPath(new URL("./candidate-settings-v080.mjs", import.meta.url));

test("renders the canonical 13-line Oracle candidate settings fixture", async () => {
  const expected = await readFile(FIXTURE);
  const actual = renderCandidateSettingsV080({
    sourceCommit: SOURCE_COMMIT,
    candidateUrl: CANDIDATE_URL,
  });
  assert.deepEqual(actual, expected);
  assert.equal(actual.includes(13), false);
  assert.equal(actual.toString("utf8").trimEnd().split("\n").length, 13);
  assert.deepEqual(candidateSettingsAuthorityV080({
    sourceCommit: SOURCE_COMMIT,
    candidateUrl: CANDIDATE_URL,
  }), {
    lineCount: 13,
    size: 661,
    sha256: "a14111258028ad8d0ec3449720bc803895f346e3a92a5e2d30e9861ff1c5c61e",
  });
});

test("remote launcher emits bytes identical to the canonical fixture", async () => {
  const launcher = await readFile(LAUNCHER, "utf8");
  const match = launcher.match(
    /# candidate-settings-v080: begin\n\/usr\/bin\/cat > "\$temporary_settings" <<CLEARRA_CANDIDATE_SETTINGS_V080\n(?<settings>[\s\S]*?)\nCLEARRA_CANDIDATE_SETTINGS_V080\n    # candidate-settings-v080: end/u,
  );
  assert.ok(match?.groups?.settings, "candidate settings block is unavailable");
  const renderedLauncherBytes = Buffer.from(
    `${match.groups.settings
      .replaceAll("$candidate_url", CANDIDATE_URL)
      .replaceAll("$source_commit", SOURCE_COMMIT)}\n`,
    "utf8",
  );
  assert.deepEqual(renderedLauncherBytes, await readFile(FIXTURE));
});

test("hash-only CLI prints only the canonical SHA-256", () => {
  const expected = candidateSettingsAuthorityV080({
    sourceCommit: SOURCE_COMMIT,
    candidateUrl: CANDIDATE_URL,
  }).sha256;
  const result = spawnSync(
    process.execPath,
    [
      CLI,
      "--source-commit",
      SOURCE_COMMIT,
      "--candidate-url",
      `${CANDIDATE_URL}/`,
      "--hash-only",
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, `${expected}\n`);
  assert.equal(result.stderr, "");
});

test("rejects non-canonical URLs and source commits", () => {
  assert.equal(canonicalCandidateOrigin(`${CANDIDATE_URL}/`), CANDIDATE_URL);
  for (const candidateUrl of [
    "http://candidate.example.test",
    "https://user@candidate.example.test",
    "https://candidate.example.test/jobs",
    "https://CANDIDATE.example.test",
  ]) {
    assert.throws(
      () => renderCandidateSettingsV080({ sourceCommit: SOURCE_COMMIT, candidateUrl }),
      /canonical credential-free HTTPS origin/u,
    );
  }
  assert.throws(
    () => renderCandidateSettingsV080({
      sourceCommit: SOURCE_COMMIT.toUpperCase(),
      candidateUrl: CANDIDATE_URL,
    }),
    /exact lowercase 40-character SHA/u,
  );
});
