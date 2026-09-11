// Source-only draft checks. These tests neither build nor run a native product.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import test from 'node:test';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const read = (path) => readFileSync(resolve(root, path), 'utf8');
const literalPattern = /"(?:[^"\\]|\\.)*"/g;
const sourceRoots = [
  'crates/clearra-validation/src/*.rs',
  'crates/clearra-cli/src/args/*.rs',
  'crates/clearra-cli/src/assemble/*.rs',
  'crates/clearra-cli/src/output/*.rs',
  'crates/clearra-output/src/text/*.rs',
  'crates/clearra-output/src/explanation/*.rs',
];
const catalog = read('crates/clearra-i18n/src/catalog/japanese_cli_draft.rs');
const entries = [...catalog.split('\n];')[0].matchAll(
  /\(\s*("(?:[^"\\]|\\.)*")\s*,\s*("(?:[^"\\]|\\.)*")\s*,?\s*\)/g,
)].map((match) => [JSON.parse(match[1]), JSON.parse(match[2])]);
const translations = new Map(entries);
const placeholders = (text) => [...text.matchAll(/\{[^{}]*\}/g)].map(([value]) => value).sort();

test('Japanese source templates have unique sources, Japanese text and exact placeholders', () => {
  assert.equal(entries.length, 408);
  assert.equal(translations.size, entries.length);
  for (const [english, japanese] of entries) {
    assert.match(japanese, /[\u3040-\u30ff\u3400-\u9fff]/u, english);
    assert.notEqual(japanese, english);
    assert.deepEqual(placeholders(japanese), placeholders(english), english);
  }
  assert.match(catalog, /Option<&'static str>/);
  assert.doesNotMatch(catalog, /get_or_fallback/);
});

test('tracked CLI, validation and renderer prose inventory has Japanese source coverage', () => {
  const paths = execFileSync('git', ['ls-files', '--', ...sourceRoots], {
    cwd: root,
    encoding: 'utf8',
  }).trim().split(/\r?\n/).filter((path) => !path.includes('test') && !path.endsWith('japanese_help_draft.rs'));
  let checked = 0;
  for (const path of paths) {
    let source = read(path).split('#[cfg(test)]')[0];
    if (path.endsWith('cli_parser.rs')) source = source.slice(source.indexOf('pub enum CliParseError'));
    for (const [literal] of source.matchAll(literalPattern)) {
      let english;
      try { english = JSON.parse(literal); } catch { continue; }
      // This inventory covers prose templates. Machine keys, command syntax,
      // punctuation-only formats and messages assembled across literals need
      // a separate runtime adapter audit before Japanese can be released.
      if (!/[A-Za-z]{2,} [A-Za-z]{2,}/.test(english) || english.length < 15 || !/^[A-Za-z]/.test(english)) continue;
      if (['//', 'SRP rationale', 'expect('].some((part) => english.includes(part))) continue;
      assert.ok(translations.has(english), `${path}: untranslated source template: ${english}`);
      checked += 1;
    }
  }
  assert.ok(checked >= 373, `Unexpectedly small inventory: ${checked}`);
  assert.ok(translations.has("option '{option}' requires a value"));
});

const parser = read('crates/clearra-cli/src/args/cli_parser.rs');
const draft = read('crates/clearra-cli/src/args/japanese_help_draft.rs');
const prefixPairs = [
  ['usage: ', '使い方: '], ['   or: ', '  または: '],
  ['finesse search: ', 'finesseの検索: '], ['finesse score: ', 'finesseの評価: '],
  ['spin-structure usage: ', 'spin-structureの使い方: '],
  ['common options: ', '共通オプション: '], ['search options: ', '検索オプション: '],
];
const inlinePairs = [
  ['[common options]', '[共通オプション]'], ['[search options]', '[検索オプション]'],
  ['[score options]', '[スコアオプション]'], ['[CPU worker options]', '[CPUワーカーオプション]'],
  ['[backend/resource options]', '[バックエンド・リソースオプション]'],
  ['[Build execution options]', '[Build実行オプション]'], ['[typed Build options]', '[型付きBuildオプション]'],
  ['[legacy positional arguments]', '[従来形式の位置引数]'], ['[legacy-compatible options]', '[従来互換オプション]'],
  ['[options]', '[オプション]'],
];

test('all 33 help topics preserve every command syntax line and option spelling', () => {
  const englishTopics = new Map();
  for (const [enumName, begin, end] of [
    ['CliHelpTopic', 'impl CliHelpTopic', 'impl ProductHelpTopic'],
    ['ProductHelpTopic', 'impl ProductHelpTopic', 'pub enum CliParseError'],
  ]) {
    const source = parser.slice(parser.indexOf(begin), parser.indexOf(end));
    for (const match of source.matchAll(/Self::(\w+)\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")/g)) {
      englishTopics.set(`${enumName}::${match[1]}`, JSON.parse(match[2]));
    }
  }
  const japaneseTopics = new Map([...draft.matchAll(/((?:CliHelpTopic|ProductHelpTopic)::\w+)\s*=>\s*(?:\{\s*)?r#"([\s\S]*?)"#/g)]
    .map((match) => [match[1], match[2]]));
  assert.equal(englishTopics.size, 33);
  assert.deepEqual([...japaneseTopics.keys()].sort(), [...englishTopics.keys()].sort());
  for (const [topic, english] of englishTopics) {
    const japanese = japaneseTopics.get(topic);
    assert.match(japanese, /^使い方: clearra /, topic);
    assert.doesNotMatch(japanese, /usage:|\[options\]/, topic);
    for (const line of english.split('\n')) {
      const prefix = prefixPairs.find(([value]) => line.startsWith(value));
      if (!prefix) continue;
      let expected = prefix[1] + line.slice(prefix[0].length);
      for (const [from, to] of inlinePairs) expected = expected.replaceAll(from, to);
      assert.ok(japanese.split('\n').includes(expected), `${topic}: command syntax drift: ${expected}`);
    }
  }
});

test('Japanese source preparation and runtime catalog open the coordinated release gate', () => {
  const language = read('crates/clearra-i18n/src/language/language_id.rs');
  assert.match(language, /pub const RELEASED: \[Self; 3\] = \[Self::En, Self::Ko, Self::Ja\]/);
  const releasedParser = language.slice(language.indexOf('pub fn parse('), language.indexOf('pub fn parse_known('));
  assert.match(releasedParser, /"ja" \| "ja-jp" => Some\(Self::Ja\)/);
  assert.match(catalog, /pub fn localize_text\(source: &str\) -> String/);
});
