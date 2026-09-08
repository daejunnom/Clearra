import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { findPublicCommandDocViolations, readPublicCommandDocs } from './public-command-docs.mjs';
import { RELEASE_REGRESSION_TEST_FILES } from './run-release-regression-tests.mjs';

test('public READMEs and every Markdown document retain command non-discovery before product builds', () => {
  const documents = readPublicCommandDocs(fileURLToPath(new URL('../..', import.meta.url)));
  assert.deepEqual(findPublicCommandDocViolations(documents), [], 'Public documentation exposes a hidden command');
});

test('all original blocked command spellings and descriptions remain blocked', () => {
  for (const text of ['--diagnostics', '`/verify`', '$verify', '>verify', 'diagnostic.verify',
    'VerifyKicks', '`Verify`', 'clearra verify', 'sfinder verify', 'verify kicks',
    'hidden verify', 'verification scope', 'verification commands', 'reserved diagnostic',
    'hidden diagnostics', 'internal diagnostic probes', 'non-search diagnostic',
    'diagnostic root', 'diagnostic route', 'diagnostic modal', 'diagnostic boundary',
    'diagnostic probe', 'diagnostic probes', 'diagnostic feature', 'diagnostics intentionally']) {
    assert.ok(findPublicCommandDocViolations([{ path: 'fixture.md', text }]).length > 0, text);
    assert.ok(findPublicCommandDocViolations([{ path: 'fixture.md', text: text.toUpperCase() }]).length > 0, text);
  }
});

test('reports every file and exact line without retaining unrelated document content', () => {
  const text = '# CI\r\nCloud evaluation workflow\r\ndiagnostic route\r\n$verify\r\n' + 'z'.repeat(1_100_000);
  const violations = findPublicCommandDocViolations([
    { path: 'docs/research/ci.md', text }, { path: 'README.md', text: '\n`/verify`\n' },
  ]);
  assert.deepEqual(violations, [
    { path: 'docs/research/ci.md', line: 3, matched: 'diagnostic route' },
    { path: 'docs/research/ci.md', line: 4, matched: '$verify' },
    { path: 'README.md', line: 2, matched: '`/verify' },
  ]);
  assert.ok(JSON.stringify(violations).length < 400);
  assert.deepEqual(findPublicCommandDocViolations([
    { path: 'a.md', text: 'diagnostic' }, { path: 'b.md', text: 'route' },
    { path: 'ci.md', text: 'Cloud evaluation workflow; verify an accepted artifact.' },
  ]), []);
});

test('document inventory preserves both READMEs and nested research with one metadata owner', () => {
  const root = mkdtempSync(join(tmpdir(), 'clearra-public-doc-policy-'));
  try {
    for (const path of ['apps/clearra-discord-bot', 'docs/research/nested']) mkdirSync(join(root, path), { recursive: true });
    for (const path of ['README.md', 'apps/clearra-discord-bot/README.md', 'docs/research/nested/ci.md']) {
      writeFileSync(join(root, path), `# ${path}\n`);
    }
    writeFileSync(join(root, 'docs/ignored.txt'), '$verify');
    assert.deepEqual(readPublicCommandDocs(root).map(({ path }) => path),
      ['README.md', 'apps/clearra-discord-bot/README.md', 'docs/research/nested/ci.md']);
    assert.equal(RELEASE_REGRESSION_TEST_FILES.filter(path => path === 'scripts/tools/public-command-docs.test.mjs').length, 1);
    const capabilityTest = readFileSync(new URL('../../apps/clearra-discord-bot/test/capability-registry.test.mjs', import.meta.url), 'utf8');
    assert.doesNotMatch(capabilityTest, /readMarkdownTree|readPublicCommandDocs|findPublicCommandDocViolations/u);
    assert.match(capabilityTest, /globalCommands.some\(\(\{ name \}\) => name === "verify"\)/u);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
