import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const contractUrl = new URL('../fixtures/contracts/search_option_contract.tsv', import.meta.url);

function contractRows() {
  return readFileSync(contractUrl, 'utf8')
    .split(/\r?\n/u)
    .filter((line) => line && !line.startsWith('#'))
    .map((line) => {
      const columns = line.split('\t');
      assert.equal(columns.length, 9, `nine columns: ${line}`);
      return {
        family: columns[0],
        option: columns[1],
        valid: columns[3].split('|'),
        invalid: columns[4] === '-' ? [] : columns[4].split('|'),
        webDefault: columns[5],
        nativeDefault: columns[6],
        discordSurface: columns[7]
      };
    });
}

test('the shared search contract defines each option once across five families', () => {
  const rows = contractRows();
  assert.deepEqual(
    [...new Set(rows.map(({ family }) => family))].sort(),
    ['build', 'damage', 'pc', 'setup', 'spin-finder']
  );
  const identities = rows.map(({ family, option }) => `${family}.${option}`);
  assert.equal(new Set(identities).size, identities.length);
});

test('the shared search contract yields every representative single and ordered pair', () => {
  for (const family of new Set(contractRows().map((row) => row.family))) {
    const rows = contractRows().filter((row) => row.family === family);
    const cases = new Set();
    for (const row of rows) {
      cases.add(`${family}:single:${row.option}:omitted`);
      for (const value of row.valid) cases.add(`${family}:single:${row.option}:${value}`);
      for (const value of row.invalid) cases.add(`${family}:invalid:${row.option}:${value}`);
    }
    let expectedPairCases = 0;
    for (let left = 0; left < rows.length; left += 1) {
      for (let right = left + 1; right < rows.length; right += 1) {
        for (const leftValue of rows[left].valid) {
          for (const rightValue of rows[right].valid) {
            cases.add(`${family}:pair:${rows[left].option}=${leftValue}:${rows[right].option}=${rightValue}:forward`);
            cases.add(`${family}:pair:${rows[right].option}=${rightValue}:${rows[left].option}=${leftValue}:reverse`);
            expectedPairCases += 2;
          }
        }
      }
    }
    assert.equal([...cases].filter((id) => id.includes(':pair:')).length, expectedPairCases);
    assert.equal(
      [...cases].filter((id) => id.includes(':invalid:')).length,
      rows.reduce((count, row) => count + row.invalid.length, 0)
    );
  }
});

test('the shared contract fixes compatibility defaults and Discord exposure', () => {
  const rows = contractRows();
  const row = (family, option) => rows.find((candidate) =>
    candidate.family === family && candidate.option === option
  );
  assert.equal(row('pc', 'lines').webDefault, '4');
  assert.equal(row('pc', 'lines').nativeDefault, '2');
  assert.equal(row('build', 'aggregation').discordSurface, 'sfinder-baked');
  assert.equal(row('pc', 'backend').discordSurface, 'host');
  assert.equal(row('setup', 'mode').discordSurface, 'packed:mode');
  assert.equal(row('damage', 'preserve-b2b').discordSurface, 'packed:preserve-b2b');
});
