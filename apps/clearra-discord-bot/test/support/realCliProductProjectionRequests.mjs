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

export function realCliProductProjectionRequests(profile) {
  if (!realCliProjectionProfiles.includes(profile)) throw new Error('unknown real fixture profile');
  const requests = [];
  for (const [legal, conditioned] of [[false, false], [true, false], [false, true], [true, true]]) {
    for (const input of pcCases) requests.push({
      name: input.name, kind: input.kind, policy: `${legal}:${conditioned}`,
      arguments: [...input.arguments, '--backend', 'cpu', '--rule', profile,
        '--no-tablebase', legal ? '--legal-board' : '--no-legal-board',
        conditioned ? '--conditioned-reachability' : '--no-conditioned-reachability'],
    });
  }
  // Build's closed public registry has no accelerator/TB override authority.
  // Verify the existing installed-data/default path without widening it.
  requests.push({ name: 'build-cover', kind: 'build-coverage-portfolio.v2', policy: 'default',
    arguments: ['build', 'cover', '--base-mask', '0', '--target-mask', '15', '--height', '4',
      '--queue', 'I', '--no-hold', '--queue-knowledge', 'oracle', '--objective', 'min-cover',
      '--backend', 'cpu', '--no-backend-fallback', '--rule', profile] });
  return requests;
}
