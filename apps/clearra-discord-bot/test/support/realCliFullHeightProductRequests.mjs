// These are the same few-piece physical inputs as the native PC24 contract.
// No expected solver response, performance sample, or generated asset is stored.
import { readFileSync } from 'node:fs';
import { realCliProjectionProfiles } from './realCliProductProjectionRequests.mjs';

const fixture = readFileSync(new URL('../../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv',
  import.meta.url), 'utf8');
export const realCliFullHeightInputs = Object.freeze(fixture.split(/\r?\n/u)
  .filter(line => line && !line.startsWith('#')).map(line => {
    const [name, height, mask, pieces, queue] = line.split('\t');
    if (!name || !/^(?:7|8|12|24)$/u.test(height) || !/^[0-9a-f]{64}$/u.test(mask) ||
      !/^[1-6]$/u.test(pieces) || queue !== 'I'.repeat(Number(pieces))) {
      throw new Error('invalid bounded full-height physical fixture');
    }
    return Object.freeze({ name, height: Number(height), mask, pieces: Number(pieces), queue });
  }));

const PRODUCTS = Object.freeze([
  ['', 'pc-scenario'], ['chance', 'pc-probability.v2'],
  ['failed-queue', 'pc-failed-queue.v2'], ['minimals', 'pc-minimum-cover.v2'],
  ['score', 'pc-score-summary.v2'], ['score-minimals', 'pc-score-portfolio.v2'],
  ['score-finder', 'pc-fixed-score-witness.v2'], ['path', 'pc-path-family.v2'],
]);

export function realCliFullHeightProductRequests(profile) {
  if (!realCliProjectionProfiles.includes(profile)) throw new Error('unknown full-height fixture profile');
  return realCliFullHeightInputs.flatMap(input => [
    ...PRODUCTS.map(([product, kind]) => [product, kind, false]),
    // The 7L pair clears 3+4 and therefore has no preserving-B2B execution;
    // 8/12/24L clear only tetrises. Empty is a normal exact result, not an
    // unsupported runtime or a fabricated minimum/replay candidate.
    ['minimals', 'pc-minimum-cover.v2', true], ['path', 'pc-path-family.v2', true],
  ].map(([product, kind, preserveB2b]) => ({
    ...input, profile, product, kind, preserveB2b,
    arguments: ['pc', ...(product ? [product] : []), '--lines', String(input.height),
      '--height', String(input.height), '--board-mask', `0x${input.mask}`,
      '--pieces', String(input.pieces), '--queue', input.queue, '--no-hold', '--rule', profile,
      ...(product === 'score-finder' ? [] : ['--no-tablebase']),
      '--no-legal-board', '--no-conditioned-reachability',
      // The full-height scenario options select the shared CLI grammar:
      // --queue is already fixed, unlike the native legacy opening default.
      // Every closed score product owns CPU execution. Finder additionally
      // excludes TB semantics; do not depend on Discord stripping an invalid
      // fixture option before the native/desktop grammar sees it.
      ...(['score', 'score-minimals', 'score-finder'].includes(product) ? [] : ['--backend', 'cpu']),
      ...(preserveB2b ? ['--preserve-b2b', '--spin-profile', 't-spins'] : []),
      ...(product === 'failed-queue' ? ['--failed-count', '1'] : [])],
  })));
}
