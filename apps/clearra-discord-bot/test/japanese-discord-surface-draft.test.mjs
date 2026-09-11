import assert from "node:assert/strict";
import test from "node:test";
import {
  japaneseDiscordCatalogReadiness,
  discordCatalogReadiness,
  matchDiscordLocale,
  t,
} from "../src/discord/i18n.mjs";
import { JAPANESE_DISCORD_MESSAGES } from "../src/discord/japanese-i18n-draft.mjs";
import {
  globalCommands,
  slashCommandCatalog,
  messageCommandCatalog,
  assertDiscordRegistrationLimits,
  discordApplicationCommandSize,
  formatSlashCommandHelp,
} from "../src/discord/slash-command-catalog.mjs";
import { buildCommandModalResponse } from "../src/discord/field-modal.mjs";
import { formatTextManagementHelp } from "../src/discord/management-command.mjs";
import {
  japaneseDiscordRegistrationDraft,
  japaneseDiscordFullCommandDraft,
  japaneseDiscordNameDraft,
  japaneseDiscordDescriptionDraft,
  japaneseDiscordChoiceNameDraft,
  japaneseDiscordModalTextDraft,
  formatJapaneseDiscordHelpDraft,
  formatJapaneseTextManagementHelpDraft,
  buildJapaneseCommandModalDraft,
} from "../src/discord/japanese-discord-surface-draft.mjs";

const JAPANESE = /[\u3040-\u30ff\u3400-\u9fff]/u;
const PRODUCT_NAMES = new Set([
  "pc", "ren", "SRS", "SRS-X", "Jstris 180", "T-Spins", "T-Spins+",
  "All-Spin", "All-Spin+", "All-Mini", "All-Mini+", "Guideline", "Jstris Ultra",
  "CTK3", "v115 Fumen", "Fumen v115",
]);
const entries = slashCommandCatalog.flatMap(entry => [entry, ...Object.values(entry.subcommands ?? {})]);
const route = entry => entry.rootName ? `${entry.rootName} ${entry.subcommand}` : entry.name;

function assertJapaneseText(english, japanese, path) {
  assert.equal(typeof japanese, "string", path);
  if (PRODUCT_NAMES.has(english)) return;
  assert.notEqual(japanese, english, `${path} retains English prose`);
  assert.match(japanese, JAPANESE, path);
  assert.doesNotMatch(japanese, /[\uac00-\ud7a3]/u, path);
}

function stripFields(value, ignored) {
  if (Array.isArray(value)) return value.map(item => stripFields(item, ignored));
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value)
    .filter(([key]) => !ignored.has(key))
    .map(([key, child]) => [key, stripFields(child, ignored)]));
}

function assertRegistrationTranslation(node, path, message = false) {
  assertJapaneseText(node.name, node.name_localizations?.ja, `${path}.name`);
  assert.ok([...node.name_localizations.ja].length <= 32, path);
  if (!message) assert.match(node.name_localizations.ja, /^[\p{Ll}\p{Lm}\p{Lo}\p{N}_-]+$/u, path);
  if (node.description !== undefined) {
    assertJapaneseText(node.description, node.description_localizations?.ja, `${path}.description`);
    assert.ok([...node.description_localizations.ja].length <= 100, path);
  }
  const siblings = new Set();
  for (const option of node.options ?? []) {
    assert.ok(!siblings.has(option.name_localizations.ja), `${path}: localized option collision`);
    siblings.add(option.name_localizations.ja);
    assertRegistrationTranslation(option, `${path}.${option.name}`);
  }
  const choices = new Set();
  for (const choice of node.choices ?? []) {
    assertJapaneseText(choice.name, choice.name_localizations?.ja, `${path}[${choice.value}]`);
    assert.ok([...choice.name_localizations.ja].length <= 100, path);
    assert.ok(!choices.has(choice.name_localizations.ja), `${path}: localized choice collision`);
    choices.add(choice.name_localizations.ja);
  }
}

const codeSpans = text => [...text.matchAll(/`([^`]+)`/gu)].map(match => match[1]).sort();

test("Japanese reply draft has exact English keys and placeholders, with no untranslated prose", () => {
  const readiness = japaneseDiscordCatalogReadiness();
  assert.equal(readiness.complete, true);
  assert.equal(readiness.translated, readiness.required);
  assert.deepEqual(readiness.missingKeys, []);
  assert.deepEqual(readiness.unexpectedKeys, []);
  assert.deepEqual(readiness.placeholderMismatches, []);
  for (const [key, japanese] of Object.entries(JAPANESE_DISCORD_MESSAGES)) {
    assertJapaneseText(t("en", key), japanese, key);
  }
  assert.equal(discordCatalogReadiness({ ...JAPANESE_DISCORD_MESSAGES, "result.completed": "完了" }).complete, false);
  assert.equal(discordCatalogReadiness({ ...JAPANESE_DISCORD_MESSAGES, "result.completed": "{kind}{partial}{extra}" }).complete, false);
});

test("every Japanese slash name, option, choice, and description is covered without collisions", () => {
  const localized = japaneseDiscordFullCommandDraft();
  const originals = [...slashCommandCatalog, ...messageCommandCatalog].map(entry => entry.registration);
  assert.deepEqual(stripFields(localized, new Set(["name_localizations", "description_localizations"])),
    stripFields(originals, new Set(["name_localizations", "description_localizations"])));
  const roots = new Set();
  for (const command of localized) {
    const key = `${command.type}:${command.name_localizations.ja}`;
    assert.ok(!roots.has(key), `${key} collides`);
    roots.add(key);
    assertRegistrationTranslation(command, command.name, command.type === 3);
  }
});

test("Japanese registration retains autocomplete and stays within Discord's character budget", () => {
  const draft = japaneseDiscordRegistrationDraft();
  assertDiscordRegistrationLimits(draft);
  assert.equal(draft.length, globalCommands.length);
  for (let index = 0; index < draft.length; index += 1) {
    assertRegistrationTranslation(draft[index], draft[index].name, draft[index].type === 3);
    assert.ok(discordApplicationCommandSize(draft[index]) <= 8000, draft[index].name);
    assert.deepEqual(stripFields(draft[index], new Set(["name_localizations", "description_localizations"])),
      stripFields(globalCommands[index], new Set(["name_localizations", "description_localizations"])));
  }
  const pc = draft.find(command => command.name === "pc");
  assert.ok(pc.options.some(command => command.options.some(option => option.autocomplete === true)));
});

test("all current help pages and objective grammars have Japanese prose and preserve executable examples", () => {
  const targets = ["", ...entries.filter(entry => ["search", "render-file"].includes(entry.kind)).map(route),
    "objective", "objective all", "objective unique", "objective min-cover", "objective minimum-cover",
    "objective tiling", "objective invalid", "unknown-command"];
  for (const target of targets) {
    const original = formatSlashCommandHelp(target, "en");
    const japanese = formatJapaneseDiscordHelpDraft(target);
    assertJapaneseText(original, japanese, target);
    assert.deepEqual(codeSpans(japanese), codeSpans(original), `${target}: executable syntax changed`);
    assert.doesNotMatch(japanese.replace(/`[^`]+`/gu, ""), /Direct syntax|Subcommands:|Note:|Unknown|must |requires |defaults |unavailable/gu, target);
  }
  const admin = formatJapaneseTextManagementHelpDraft();
  assert.match(admin, /管理者/u);
  assert.deepEqual(codeSpans(admin), codeSpans(formatTextManagementHelp("en")));
});

test("every current modal draft preserves input IDs and values and fits component limits", () => {
  let modalCount = 0;
  const display = new Set(["label", "description", "placeholder", "title"]);
  for (const entry of entries.filter(value => !value.subcommands)) {
    const interaction = { type: 2, data: {
      type: 1, name: entry.rootName ?? entry.name,
      options: entry.rootName ? [{ type: 1, name: entry.subcommand, options: [] }] : [],
    } };
    const original = buildCommandModalResponse(interaction, "en");
    const japanese = buildJapaneseCommandModalDraft(interaction);
    if (!original) { assert.equal(japanese, null); continue; }
    modalCount += 1;
    assert.deepEqual(stripFields(japanese, display), stripFields(original, display), route(entry));
    assert.ok(japanese.data.title.length <= 45, route(entry));
    assert.match(japanese.data.title, JAPANESE);
    assert.ok(japanese.data.components.length <= 5);
    for (let index = 0; index < japanese.data.components.length; index += 1) {
      const label = japanese.data.components[index];
      const english = original.data.components[index];
      assertJapaneseText(english.label, label.label, `${route(entry)}.${index}`);
      assert.ok(label.label.length <= 45, label.label);
      assert.ok((label.description?.length ?? 0) <= 100, label.description);
      const component = label.component;
      assert.ok((component.placeholder?.length ?? 0) <= (component.type === 4 ? 100 : 150), component.placeholder);
      const choiceNames = new Set();
      for (const option of component.options ?? []) {
        assert.ok(option.label.length <= 100, option.label);
        assert.ok(!choiceNames.has(option.label), `${route(entry)}: duplicate modal label`);
        choiceNames.add(option.label);
      }
    }
  }
  assert.ok(modalCount > 20, `expected broad modal coverage, got ${modalCount}`);
  const finesse = buildJapaneseCommandModalDraft({ type: 2, data: {
    type: 1, name: "finesse", options: [{ type: 1, name: "search", options: [] }],
  } });
  assert.match(finesse.data.title, /検索/u);
});

test("draft translation failures cannot silently copy new English strings", () => {
  for (const translate of [japaneseDiscordNameDraft, japaneseDiscordDescriptionDraft,
    japaneseDiscordChoiceNameDraft, japaneseDiscordModalTextDraft]) {
    assert.throws(() => translate("New untranslated feature"), /Missing Japanese/u);
  }
});

test("Japanese full-surface fixtures match the released product paths", () => {
  const fixture = japaneseDiscordRegistrationDraft();
  japaneseDiscordFullCommandDraft();
  assert.equal(matchDiscordLocale("ja-JP"), "ja");
  assert.equal(t("ja", "language.name.ja"), "日本語");
  assert.deepEqual(formatSlashCommandHelp("pc path", "ja"), formatJapaneseDiscordHelpDraft("pc path"));
  assert.match(JSON.stringify(globalCommands), /"ja"\s*:/u);
  assert.equal(fixture.length, globalCommands.length);
});
