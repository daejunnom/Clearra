import type { ClearraWasmSearchReport } from '../wasm/wasmCommandClient';

export type BuildProbabilityAggregation = 'buildability' | 'tiling' | 'spin';

export type BuildProbabilityAggregationAuthority =
  | {
      state: 'pending';
      requested: BuildProbabilityAggregation;
      reported: null;
      effective: BuildProbabilityAggregation;
      reason: null;
    }
  | {
      state: 'authorized';
      requested: BuildProbabilityAggregation;
      reported: BuildProbabilityAggregation;
      effective: BuildProbabilityAggregation;
      reason: null;
    }
  | {
      state: 'rejected';
      requested: BuildProbabilityAggregation;
      reported: BuildProbabilityAggregation | null;
      effective: null;
      reason:
        | 'missing-or-duplicate-result-aggregation'
        | 'invalid-result-aggregation'
        | 'request-result-aggregation-mismatch';
    };

/**
 * Joins the request-generation snapshot to the executor-owned terminal field.
 *
 * The local snapshot is sufficient only while no report exists. Once a report
 * arrives, its typed `build_probability_aggregation` field must be present
 * exactly once, canonical, and equal to the snapshot that started the run.
 * This keeps an earlier generation from being relabelled with later controls.
 */
export function buildProbabilityAggregationAuthority(
  report: Pick<ClearraWasmSearchReport, 'summary_fields'> | null | undefined,
  requested: BuildProbabilityAggregation
): BuildProbabilityAggregationAuthority {
  if (!report) {
    return {
      state: 'pending',
      requested,
      reported: null,
      effective: requested,
      reason: null
    };
  }

  const values = report.summary_fields
    .filter(([key]) => key === 'build_probability_aggregation')
    .map(([, value]) => value);
  if (values.length !== 1) {
    return {
      state: 'rejected',
      requested,
      reported: null,
      effective: null,
      reason: 'missing-or-duplicate-result-aggregation'
    };
  }

  const reported = parseBuildProbabilityAggregation(values[0]);
  if (reported === null) {
    return {
      state: 'rejected',
      requested,
      reported: null,
      effective: null,
      reason: 'invalid-result-aggregation'
    };
  }
  if (reported !== requested) {
    return {
      state: 'rejected',
      requested,
      reported,
      effective: null,
      reason: 'request-result-aggregation-mismatch'
    };
  }
  return {
    state: 'authorized',
    requested,
    reported,
    effective: reported,
    reason: null
  };
}

function parseBuildProbabilityAggregation(
  value: string | undefined
): BuildProbabilityAggregation | null {
  return value === 'buildability' || value === 'tiling' || value === 'spin'
    ? value
    : null;
}
