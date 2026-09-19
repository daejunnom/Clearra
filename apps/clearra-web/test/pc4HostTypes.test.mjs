import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

test('PC4 host, async pump and common event DTO typecheck without emitted build files', () => {
  const entry = fileURLToPath(new URL('../src/workers/WasmJobRunner.ts', import.meta.url));
  const program = ts.createProgram([entry], {
    noEmit: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler, strict: true, skipLibCheck: true,
    lib: ['lib.es2022.d.ts', 'lib.dom.d.ts'],
  });
  const diagnostics = ts.getPreEmitDiagnostics(program);
  assert.equal(diagnostics.length, 0, ts.formatDiagnostics(diagnostics, {
    getCurrentDirectory: () => process.cwd(), getCanonicalFileName: name => name, getNewLine: () => '\n',
  }));
});
