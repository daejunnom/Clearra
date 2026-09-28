// SRP rationale: exercise the exact read-only fixture's dependency parser,
// including multi-binary ldd headings, without Docker, WSL or build output.
import assert from 'node:assert/strict';
import test from 'node:test';
import { collectReadonlyRuntimeLibraries } from './readonly-runtime-libraries.mjs';

test('two binary headings cannot be mistaken for host libraries', () => {
  const output = `/home/runner/work/Clearra/Clearra/_local/artifacts/proof/node:
    linux-vdso.so.1 (0x00007ffe1e915000)
    libstdc++.so.6 => /lib/x86_64-linux-gnu/libstdc++.so.6 (0x00007fe1dc300000)
    libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007fe1dc100000)
    /lib64/ld-linux-x86-64.so.2 (0x00007fe1dcf25000)
/home/runner/work/Clearra/Clearra/_local/artifacts/proof/clearra:
    linux-vdso.so.1 (0x00007ffe1e915000)
    libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007fe1dc100000)
    /lib64/ld-linux-x86-64.so.2 (0x00007fe1dcf25000)
`;
  assert.deepEqual(collectReadonlyRuntimeLibraries(output), [
    '/lib/x86_64-linux-gnu/libc.so.6',
    '/lib/x86_64-linux-gnu/libstdc++.so.6',
    '/lib64/ld-linux-x86-64.so.2',
  ]);
});

test('unresolved, unapproved and traversal paths fail instead of producing a partial fixture', () => {
  for (const line of [
    'libmissing.so => not found',
    '/home/runner/library.so (0x0001)',
    '/lib/../../home/runner/library.so (0x0001)',
    'unexpected ldd diagnostic',
    'statically linked',
    '',
  ]) assert.throws(() => collectReadonlyRuntimeLibraries(line));
});

test('all approved host-library roots and CRLF are parsed without executing any path', () => {
  const libraries = ['/lib/a.so', '/lib64/b.so', '/usr/lib/c.so', '/usr/lib64/d.so'];
  const output = libraries.map(path => `${path} (0xFF)`).join('\r\n');
  assert.deepEqual(collectReadonlyRuntimeLibraries(output), libraries);
});
