// SRP rationale: enumerate small real product inputs accepted by the existing
// Discord command registry. These are requests, never manufactured results.
export const realCliProjectionProfiles = Object.freeze([
  'srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick',
]);

const pc = ['--lines', '2', '--board-mask', '0', '--height', '2', '--pieces', '5',
  '--queue', 'IIOOO', '--no-hold'];
const pcCases = [
  { name: 'minimum', kind: 'pc-minimum-cover.v2', arguments: ['pc', 'minimals', ...pc] },
  { name: 'score-minimum', kind: 'pc-score-portfolio.v2', arguments: ['pc', 'score-minimals', ...pc] },
  { name: 'replay', kind: 'pc-path-family.v2', arguments: ['pc', 'path', '--lines', '1',
    '--board-mask', '0x3f0', '--height', '1', '--pieces', '1', '--queue', 'I', '--no-hold'] },
];

// No initially full rows: each target row has four empty cells and six
// pre-existing cells. A fixed I queue supplies exactly those missing cells.
// In particular, odd targets do not pretend an empty 1/3/5L field is tileable.
export const realPcScoreTargetCases = Object.freeze(Array.from({ length: 6 }, (_, index) => {
  const height = index + 1;
  let board = 0n;
  for (let row = 0; row < height; row += 1) board |= 0x3f0n << BigInt(row * 10);
  return Object.freeze({
    name: `score-minimum-${height}l`, kind: 'pc-score-portfolio.v2',
    targetLines: height, requiredPieces: height,
    arguments: Object.freeze(['pc', 'score-minimals', '--lines', String(height),
      '--board-mask', `0x${board.toString(16)}`, '--height', String(height),
      '--pieces', String(height), '--queue', 'I'.repeat(height), '--no-hold']),
  });
}));

// Three source pages, two different horizontal I targets, one repeated page.
// Produced by the repository CTK3 codec; no solver result is embedded here.
export const realSetupScoreDocument = 'ctk3_w0kGEPVAACzgA2A9EAAw3A';

export function realCliProductProjectionRequests(profile) {
  if (!realCliProjectionProfiles.includes(profile)) throw new Error('unknown real fixture profile');
  const requests = [];
  for (const [legal, conditioned] of [[false, false], [true, false], [false, true], [true, true]]) {
    for (const input of [...pcCases, ...realPcScoreTargetCases]) requests.push({
      name: input.name, kind: input.kind, policy: `${legal}:${conditioned}`,
      targetLines: input.targetLines, requiredPieces: input.requiredPieces,
      // score-minimals owns CPU/no-fallback itself and rejects overrides.
      arguments: [...input.arguments, ...(input.kind === 'pc-score-portfolio.v2' ? [] : ['--backend', 'cpu']),
        '--rule', profile,
        '--no-tablebase', legal ? '--legal-board' : '--no-legal-board',
        conditioned ? '--conditioned-reachability' : '--no-conditioned-reachability'],
    });
    requests.push({ name: 'setup-score', kind: 'setup-score-ranking.v1',
      policy: `${legal}:${conditioned}`,
      arguments: ['setup', 'score', '--document-format', 'ctk3', '--document', realSetupScoreDocument,
        '--setup-queue', 'I', '--solution-queue', 'OOOI', '--clear', '2', '--no-hold',
        '--score-profile', 'tetrio', '--initial-b2b', '0', '--rule', profile,
        legal ? '--legal-board' : '--no-legal-board',
        conditioned ? '--conditioned-reachability' : '--no-conditioned-reachability'] });
  }
  // Build's closed public registry has no accelerator/TB override authority.
  // Verify the existing installed-data/default path without widening it.
  requests.push({ name: 'build-cover', kind: 'build-coverage-portfolio.v2', policy: 'default',
    arguments: ['build', 'cover', '--base-mask', '0', '--target-mask', '15', '--height', '4',
      '--queue', 'I', '--no-hold', '--queue-knowledge', 'oracle', '--objective', 'min-cover',
      '--backend', 'cpu', '--no-backend-fallback', '--rule', profile] });
  return requests;
}
