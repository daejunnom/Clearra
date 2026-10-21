import assert from 'node:assert/strict';
import test from 'node:test';
import { ACCEPTED_CTK3_MANIFEST } from '../tools/accepted-ctk3-dist.mjs';
import { validateCtk3NpmPackage } from './ctk3-npm-package.mjs';

function fixture() {
  return {
    metadata: { name: 'ctk3', version: '0.1.1', publishConfig: { access: 'public' },
      repository: { url: 'git+https://github.com/daejunnom/Clearra.git', directory: 'packages/ctk3' } },
    accepted: { files: [{ path: 'index.js' }, { path: 'index.cjs' }, { path: 'index.d.ts' }] },
    report: [{ name: 'ctk3', version: '0.1.1', filename: 'ctk3-0.1.1.tgz',
      files: ['package.json', 'README.md', 'LICENSE', `dist/${ACCEPTED_CTK3_MANIFEST}`,
        'dist/index.js', 'dist/index.cjs', 'dist/index.d.ts'].map(path => ({ path })) }],
  };
}

test('packs only the verified distribution and public package metadata', () => {
  const { metadata, report, accepted } = fixture();
  assert.equal(validateCtk3NpmPackage(metadata, report, accepted), 'ctk3-0.1.1.tgz');
});

for (const [name, change] of [
  ['another registry package', f => { f.metadata.name = 'other'; }],
  ['foreign repository', f => { f.metadata.repository.url = 'git+https://github.com/other/fork.git'; }],
  ['private package', f => { f.metadata.publishConfig.access = 'restricted'; }],
  ['changed packed version', f => { f.report[0].version = '0.1.2'; }],
  ['unsafe tarball path', f => { f.report[0].filename = '../ctk3-0.1.1.tgz'; }],
  ['multiple tarballs', f => { f.report.push(f.report[0]); }],
  ['missing payload', f => { f.report[0].files.pop(); }],
  ['unaccepted extra payload', f => { f.report[0].files.push({ path: 'dist/unaccepted.js' }); }],
  ['private configuration', f => { f.report[0].files.push({ path: '.npmrc' }); }],
  ['duplicate payload', f => { f.report[0].files.push(f.report[0].files[0]); }],
  ['missing accepted manifest', f => { f.accepted.files = []; }],
]) {
  test(`rejects ${name}`, () => {
    const f = fixture(); change(f);
    assert.throws(() => validateCtk3NpmPackage(f.metadata, f.report, f.accepted));
  });
}
