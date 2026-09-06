import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = resolve(dirname(fileURLToPath(import.meta.url)), 'retain-clearra-debug-builds.ps1');
const quote = (value) => `'${value.replaceAll("'", "''")}'`;

test('debug retention keeps latest per package and target kind, preserving unknown owners',
  { skip: process.platform !== 'win32' }, async (t) => {
    const temporaryRoot = await mkdtemp(join(tmpdir(), 'clearra-retention-test-'));
    const target = join(temporaryRoot, 'cargo-target');
    const debug = join(target, 'debug');
    const deps = join(debug, 'deps');
    await mkdir(deps, { recursive: true });
    await writeFile(join(debug, '.cargo-lock'), '');
    const artifact = async (packageName, hash, kind = 'test-lib', fingerprint = true) => {
      await writeFile(join(deps, `clearra_wasm-${hash}.exe`), 'non-executable test fixture');
      await writeFile(join(deps, `clearra_wasm-${hash}.pdb`), 'fixture symbols');
      if (fingerprint) {
        const directory = join(debug, '.fingerprint', `${packageName}-${hash}`);
        await mkdir(directory, { recursive: true });
        // Marker contents deliberately aren't JSON: retention must use names only.
        await writeFile(join(directory, `${kind}-clearra_wasm.json`), 'not inspected');
      }
    };
    try {
      await artifact('clearra-wasm', '0000000000000001');
      await artifact('clearra-wasm', '0000000000000002');
      await artifact('clearra-wasm-abi', '0000000000000003');
      await artifact('clearra-wasm-abi', '0000000000000004');
      await artifact('clearra-wasm', '0000000000000005', 'test-bin');
      await artifact('unknown', '0000000000000006', 'test-lib', false);
      await artifact('unknown', '0000000000000007', 'test-lib', false);
      const output = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
        `& ${quote(script)} -CargoTargetDirectory ${quote(target)} | ConvertTo-Json -Compress`],
      { encoding: 'utf8', windowsHide: true });
      const report = JSON.parse(output.trim());
      if (report.Status === 'busy') {
        t.skip('retention correctly refuses to race an active Cargo/compiler/test process');
        return;
      }
      assert.equal(report.Status, 'planned');
      assert.equal(report.RetainPerTarget, 1);
      assert.equal(report.RetainedCount, 5, 'two packages, a separate target kind, and two unknown owners');
      assert.equal(report.UnresolvedExecutableCount, 2);
      assert.equal(report.PlannedCount, 4, 'one obsolete executable/PDB pair per package, not cross-package pruning');
      assert.equal(report.DeletedCount, 0, 'regression check is dry-run only');
    } finally {
      // This directory was minted by mkdtemp above and contains only these fixtures.
      const expectedPrefix = resolve(tmpdir()) + '\\clearra-retention-test-';
      assert.ok(resolve(temporaryRoot).startsWith(expectedPrefix));
      await rm(temporaryRoot, { recursive: true, force: true });
    }
  });
