// Independently authored local-only boundary experiment, not a Tetris move
// generator or a product pruning authority. No wirelyre source/data is used.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

const canonical = (value) => Array.isArray(value) ? value.map(canonical)
  : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort()
    .map((key) => [key, canonical(value[key])])) : value;
const digest = (value) => createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex');

// Ordered kick attempts are semantic. Only object key order is normalized.
export function generationKey(contract) {
  assert(contract && ['engine', 'width', 'height', 'spawn', 'lock', 'lineClear',
    'initialDomain', 'pieceDomain', 'kicks'].every((key) => Object.hasOwn(contract, key)));
  assert(contract.width > 0 && contract.height > 0);
  return digest({ schema: 'local-legal-board-generation.v1', contract });
}

export async function generateIndex({ contract, states, goals, transitions,
  maxStates = 100_000, maxEdges = 1_000_000, signal }) {
  assert(!process.env.GITHUB_ACTIONS && !process.env.CI, 'local-only experiment');
  const key = generationKey(contract);
  const domain = new Set(states);
  const goalSet = new Set(goals);
  const empty = (reason) => Object.freeze({ key, complete: false, reason,
    classify: () => 'unknown', stateCount: domain.size, edgeCount: 0 });
  if (domain.size !== states.length || domain.size > maxStates || !goalSet.size ||
      [...goalSet].some((goal) => !domain.has(goal))) return empty('invalid-domain');
  const reverse = new Map([...domain].map((state) => [state, []]));
  let edges = 0;
  try {
    for (const state of domain) {
      if (signal?.aborted) return empty('cancelled');
      const result = await transitions(state, contract);
      if (!result || result.complete !== true || !Array.isArray(result.next)) return empty('incomplete-transitions');
      for (const next of new Set(result.next)) {
        // An edge beyond the declared domain cannot silently be discarded.
        if (!domain.has(next)) return empty('domain-not-closed');
        if (++edges > maxEdges) return empty('edge-budget');
        reverse.get(next).push(state);
      }
    }
  } catch { return empty('generation-failed'); }
  if (signal?.aborted) return empty('cancelled');
  const live = new Set(goalSet);
  const pending = [...goalSet];
  for (let head = 0; head < pending.length; head += 1) {
    for (const prior of reverse.get(pending[head])) if (!live.has(prior)) {
      live.add(prior);
      pending.push(prior);
    }
  }
  return Object.freeze({ key, complete: true, stateCount: domain.size, edgeCount: edges,
    // Positive membership is NOT proof of queue coverage or a legal replay.
    classify(state, requestedContract) {
      let requested;
      try { requested = generationKey(requestedContract); } catch { return 'unknown'; }
      return requested === key && domain.has(state) && !live.has(state) ? 'impossible' : 'unknown';
    } });
}
