import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { decodeCtk3 } from 'ctk3';
import { buildCtk3Result, Ctk3ResultError } from '../src/clearra/ctk3-result.mjs';

const cases = readFileSync(new URL('../../../tests/fixtures/contracts/extended_solution_keys.v1.tsv', import.meta.url), 'utf8');
for (const line of cases.split(/\r?\n/u).filter(line => line && !line.startsWith('#'))) {
  const [name, validity, key] = line.split('\t');
  test(`shared CTK2 wire contract: ${name}`, () => {
    const document = { schema_version: 'clearra.solution-data.v1', solution_keys: [key] };
    if (validity !== 'valid') {
      assert.throws(() => buildCtk3Result(document),
        error => error instanceof Ctk3ResultError && error.code === 'invalid-solution-key');
      return;
    }
    const result = buildCtk3Result(document);
    assert.ok(result);
    assert.equal(result.pageCount, 1);
    const page = decodeCtk3(result.source).pages[0];
    const match = /^ctk2\|height=(\d+)\|initial=([0-9a-f]+)\|placements=(.*)$/u.exec(key);
    // CTK3 trims empty top rows, independently of the declared search frame.
    assert.ok(page.height <= Number(match[1]));
    const initial = BigInt(`0x${match[2]}`);
    const placements = match[3] ? match[3].split(',').map(value => {
      const [piece, cells] = value.split(':');
      return { piece, mask: BigInt(`0x${cells}`) };
    }) : [];
    for (let cell = 0; cell < Number(match[1]) * 10; cell += 1) {
      const bit = 1n << BigInt(cell);
      const expected = (initial & bit) !== 0n ? 'G'
        : placements.find(placement => (placement.mask & bit) !== 0n)?.piece ?? null;
      assert.equal(page.cells[cell] ?? null, expected, `cell=${cell}`);
    }
  });
}
