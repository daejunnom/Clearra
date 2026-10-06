import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { decoder } from 'tetris-fumen';

import { decodeCtk3 } from '../src/lib/workspace/ctk3Codec';
import { parseSolutionKey, SolutionExportError } from '../src/lib/workspace/solutionExport';
import { encodeSolutionKeysForClipboard } from '../src/lib/workspace/solutionExportAsync';

const cases = await readFile('tests/fixtures/contracts/extended_solution_keys.v1.tsv', 'utf8');
for (const line of cases.split(/\r?\n/u).filter(line => line && !line.startsWith('#'))) {
  const [name, validity, key] = line.split('\t');
  const valid = validity === 'valid';
  const page = parseSolutionKey(key);
  assert.equal(page !== null, valid, name);
  if (!page) {
    await assert.rejects(() => encodeSolutionKeysForClipboard([key], 'ctk'),
      (error: unknown) => error instanceof SolutionExportError && error.code === 'invalid-solution-key');
    continue;
  }
  const source = await encodeSolutionKeysForClipboard([key], 'ctk');
  const decoded = decodeCtk3(source).pages[0];
  // CTK3 canonically trims empty top rows; the source key retains target height.
  assert.ok(decoded.height <= page.height, name);
  for (let cell = 0; cell < page.height * 10; cell += 1) {
    const bit = 1n << BigInt(cell);
    const expected = (page.initialMask & bit) !== 0n ? 'G'
      : page.placements.find(placement => (placement.mask & bit) !== 0n)?.piece ?? null;
    assert.equal(decoded.cells[cell] ?? null, expected, `${name} cell=${cell}`);
  }
  if (page.height === 24) {
    await assert.rejects(() => encodeSolutionKeysForClipboard([key], 'fumen'),
      (error: unknown) => error instanceof SolutionExportError && error.code === 'fumen-height-unsupported');
  } else {
    const fumen = await encodeSolutionKeysForClipboard([key], 'fumen');
    const decodedFumen = decoder.decode(fumen);
    assert.equal(decodedFumen.length, 1, name);
    for (let cell = 0; cell < 230; cell += 1) {
      const bit = 1n << BigInt(cell);
      const expected = (page.initialMask & bit) !== 0n ? 'X'
        : page.placements.find(placement => (placement.mask & bit) !== 0n)?.piece ?? '_';
      assert.equal(decodedFumen[0].field.at(cell % 10, Math.floor(cell / 10)), expected,
        `${name} fumen cell=${cell}`);
    }
  }
}

// CTK1 presentation compatibility is deliberately not tightened to CTK2 order.
assert.ok(parseSolutionKey('ctk1|initial=0000000000000000|placements=T:000000000000000f,I:0000000000003c00'));
