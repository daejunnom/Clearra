import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { DiscordInputError, matchDiscordLocale } from "../src/discord/i18n.mjs";
import { JAPANESE_DISCORD_MESSAGES } from "../src/discord/japanese-i18n-draft.mjs";
import {
  JAPANESE_INPUT_NAMES, JAPANESE_VALIDATION_MESSAGES, JAPANESE_VALIDATION_PATTERNS,
  japaneseValidationErrorText,
} from "../src/discord/japanese-validation-draft.mjs";

const source = await readFile(new URL("../src/discord/i18n.mjs", import.meta.url), "utf8");
// Extract only the static source-owned translation constants, never execute the
// production adapter or environment loading to enumerate the reference keys.
const constants = source.slice(source.indexOf("const KOREAN_VALIDATION_MESSAGES ="));
const reference = runInNewContext(`${constants}\n({messages:KOREAN_VALIDATION_MESSAGES, patterns:KOREAN_VALIDATION_PATTERNS, names:KOREAN_INPUT_NAMES})`, {}, { timeout: 1000 });
const wrap = (message) => JAPANESE_DISCORD_MESSAGES["error.request"].replace("{message}", message);

test("Japanese validation draft covers every existing exact message, pattern, and input name", () => {
  assert.deepEqual([...JAPANESE_VALIDATION_MESSAGES.keys()].sort(), [...reference.messages.keys()].sort());
  assert.deepEqual(JAPANESE_VALIDATION_PATTERNS.map(([regex]) => regex.source), Array.from(reference.patterns, ([regex]) => regex.source));
  assert.deepEqual(Object.keys(JAPANESE_INPUT_NAMES).sort(), Object.keys(reference.names).sort());
  for (const [english, japanese] of JAPANESE_VALIDATION_MESSAGES) {
    assert.match(japanese, /[\u3040-\u30ff\u3400-\u9fff]/u);
    assert.doesNotMatch(japanese, /[\uac00-\ud7a3]/u);
    assert.notEqual(japanese, english);
    assert.equal(japaneseValidationErrorText(new Error(english)), wrap(japanese));
  }
  assert.equal(matchDiscordLocale("ja-JP"), null);
});

test("Japanese validation preserves numeric limits and recognizable input labels", () => {
  for (const [message, expected] of [
    ["field grid rows must be exactly 10 columns wide.", /フィールド.*10マス/u],
    ["target grid must contain from one through twenty-four rows.", /目標フィールド.*1～24行/u],
    ["field exceeds the 6000-character limit.", /フィールド.*6000文字/u],
    ["lines must be an integer from 1 through 6.", /ライン数.*1から6/u],
    ["field is required in the Clearra command Modal.", /フィールド.*必要/u],
    ["next-cycle-remaining must contain exactly 3 pieces when remaining contains 4.", /4個.*3個/u],
    ["base CTK3 exceeds the 24-row limit.", /既存フィールド.*24行/u],
  ]) assert.match(japaneseValidationErrorText(new Error(message)), expected);
});

test("Japanese validation renders typed input errors and hides unknown internal diagnostics", () => {
  assert.equal(japaneseValidationErrorText(new DiscordInputError("options.setup_qb_bag_capacity")),
    wrap(JAPANESE_DISCORD_MESSAGES["input.options.setup_qb_bag_capacity"]));
  for (const message of [
    "Cloud Run endpoint failed", "C:\\private\\service.json cannot be empty.",
    "runtime worker requires a value.", "unrecognized diagnostic",
  ]) assert.equal(japaneseValidationErrorText(new Error(message)), wrap(JAPANESE_DISCORD_MESSAGES["error.validation"]));
});
