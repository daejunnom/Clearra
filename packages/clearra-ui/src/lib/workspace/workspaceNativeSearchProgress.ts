// SRP rationale: project the Desktop bridge's labeled scalars into shared, job-scoped search telemetry.
import type {
  ClearraSearchProgressCountKey,
  ClearraSearchProgressTelemetry,
  ClearraSearchProgressTelemetryFlags
} from '../wasm/wasmCommandClient';

const unavailable: ClearraSearchProgressTelemetryFlags = {
  geometry_nodes: false,
  candidates_emitted: false,
  geometry_family_count: false,
  candidates_verified: false,
  producer_build_nodes: false,
  producer_coverage_checks: false,
  build_nodes: false,
  coverage_checks: false,
  ready_workers: false,
  active_workers: false,
  worker_count: false,
  oldest_batch_ms: false,
  pass_index: false,
  pass_count: false,
  layer_index: false,
  layer_count: false,
  layer_done: false,
  layer_total: false
};

const emptyTelemetry: ClearraSearchProgressTelemetry = {
  phase: 'searching',
  producer_complete: false,
  geometry_nodes: 0,
  candidates_emitted: 0,
  geometry_family_count: null,
  candidates_verified: 0,
  producer_build_nodes: 0,
  producer_coverage_checks: 0,
  build_nodes: 0,
  coverage_checks: 0,
  ready_workers: 0,
  active_workers: 0,
  worker_count: 0,
  oldest_batch_ms: 0,
  pass_index: 0,
  pass_count: 0,
  layer_index: 0,
  layer_count: 0,
  layer_done: 0,
  layer_total: 0,
  availability: unavailable,
  exactness: unavailable
};

type NativeCounter = Extract<
  ClearraSearchProgressCountKey,
  'geometry_nodes' | 'candidates_emitted' | 'candidates_verified' | 'build_nodes'
>;

export function nativeSearchProgressCounter(
  label: string | undefined,
  total = 0
): NativeCounter | null {
  switch (label) {
    case 'geometry':
    case 'build-geometry':
    case 'wasm-exact-cover':
    case 'forward-search':
      return 'geometry_nodes';
    case 'build-candidates':
    case 'build-probability-candidates':
      return 'candidates_emitted';
    case 'buildup':
      // The legacy materialized BuildUp runner reports completed candidates
      // with a total, unlike the striped solver's node counter.
      return total > 0 ? 'candidates_verified' : 'build_nodes';
    case 'webgpu-buildup':
      return 'candidates_verified';
    case 'build-verification':
      return 'build_nodes';
    default:
      return null;
  }
}

export function isSearchPostprocessLabel(label: string | undefined): boolean {
  return label === 'postprocess' || label === 'setup-finalize'
    || label === 'pc-minimals-finalize' || label === 'pc-score-minimals-finalize'
    || label === 'build-minimals-finalize' || label === 'build-score-minimals-finalize'
    || label === 'spin-structure-cover-minimals-finalize'
    || label?.startsWith('complete-replay-') === true
    || label?.startsWith('execution-') === true
    || label === 'setup-score-reduction'
    || label === 'result-projection' || label === 'result-publish'
    || label === 'cover-minimum' || label === 'recovery-minimum';
}

// Desktop publishes one labeled scalar at a time; retain independent
// counters within its existing job epoch. A label switches the displayed
// counter, not the meaning of earlier observations or the producer's state.
export function mergeNativeSearchProgress(
  previous: ClearraSearchProgressTelemetry | null,
  label: string | undefined,
  done: number | undefined,
  total = 0
): ClearraSearchProgressTelemetry | null {
  if (isSearchPostprocessLabel(label)) {
    return previous && previous.phase !== 'postprocessing'
      ? { ...previous, phase: 'postprocessing', producer_complete: true }
      : previous;
  }
  if (previous?.phase === 'postprocessing') return previous;
  const setupPass = label === 'setup-geometry' ? 0
    : label === 'setup-graph' ? 1
      : label === 'setup-coverage' ? 3 : null;
  if (setupPass !== null) {
    // These native events count pipeline stages, not search nodes. Preserve
    // their phase while leaving work counters unavailable until measured.
    const snapshot = previous ?? emptyTelemetry;
    return {
      ...snapshot,
      pass_index: setupPass,
      pass_count: 4,
      availability: { ...snapshot.availability, pass_index: true, pass_count: true },
      exactness: { ...snapshot.exactness, pass_index: true, pass_count: true }
    };
  }
  if (label === 'forward-search-patterns') {
    if (done === undefined || !Number.isSafeInteger(done) || done < 0) return previous;
    const snapshot = previous ?? emptyTelemetry;
    const totalKnown = Number.isSafeInteger(total) && total > 0;
    return {
      ...snapshot,
      pass_index: done,
      pass_count: totalKnown ? total : 0,
      availability: { ...snapshot.availability, pass_index: true, pass_count: totalKnown },
      exactness: { ...snapshot.exactness,
        pass_index: done < 0xffff_ffff,
        pass_count: totalKnown && total < 0xffff_ffff }
    };
  }
  const counter = nativeSearchProgressCounter(label, total);
  if (
    counter === null || done === undefined || !Number.isSafeInteger(done) || done < 0
  ) return previous;

  const snapshot = previous ?? emptyTelemetry;
  // The native event's legacy u32 projection saturates; never make an exact
  // fraction from a saturated value or from a scalar with an unknown total.
  const exact = done < 0xffff_ffff;
  const familyCount = counter === 'candidates_verified' &&
    Number.isSafeInteger(total) && total > 0 && total < 0xffff_ffff
    ? String(total)
    : snapshot.geometry_family_count;
  const familyKnown = familyCount !== null;
  if (
    snapshot.availability[counter] && snapshot[counter] === done
    && snapshot.exactness[counter] === exact
    && snapshot.geometry_family_count === familyCount
  ) return previous;

  return {
    ...snapshot,
    [counter]: done,
    geometry_family_count: familyCount,
    availability: snapshot.availability[counter] && snapshot.availability.geometry_family_count === familyKnown
      ? snapshot.availability
      : { ...snapshot.availability, [counter]: true, geometry_family_count: familyKnown },
    exactness: snapshot.exactness[counter] === exact && snapshot.exactness.geometry_family_count === familyKnown
      ? snapshot.exactness
      : { ...snapshot.exactness, [counter]: exact, geometry_family_count: familyKnown }
  };
}
