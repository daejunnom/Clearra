import { spawnSync } from 'node:child_process';
import { assertManagedBuildTransaction, assertCargoOutputArguments } from './clearra-build-policy.mjs';
try {
  const transaction = assertManagedBuildTransaction();
  const [compiler, ...arguments_] = process.argv.slice(2);
  if (!compiler) throw new Error('Missing Rust compiler argument');
  assertCargoOutputArguments(arguments_, transaction.transaction);
  const result = spawnSync(compiler, arguments_, { stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
} catch (error) {
  console.error(`Clearra build refused: ${error.message}. Use scripts/tools/invoke-clearra-build.ps1.`);
  process.exitCode = 2;
}
