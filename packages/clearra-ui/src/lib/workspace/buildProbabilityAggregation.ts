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

export type BuildProbabilityCoverageAggregation =
  | { state: 'pending'; reason: null }
  | {
      state: 'not-calculated';
      sourceRowCount: number;
      patternCount: number;
      reason: null;
    }
  | {
      state: 'authorized';
      sourceRowCount: number;
      patternCount: number;
      successfulPatternCount: number;
      failedPatternCount: number;
      successProbability: string;
      failedProbability: string;
      complete: boolean;
      reason: null;
    }
  | {
      state: 'rejected';
      reason:
        | 'missing-or-duplicate-coverage-field'
        | 'invalid-coverage-contract'
        | 'coverage-result-mismatch';
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

/**
 * Projects the executor-owned, product-neutral PC/Build coverage aggregation.
 *
 * The presenter does not reconstruct coverage from candidate variants. Every
 * displayed count and probability must agree with the one exact terminal
 * summary and its typed report fields; malformed or cross-generation data is
 * rejected as a unit. The request-side aggregation is joined separately before
 * this projection is allowed to render.
 */
export function buildProbabilityCoverageAggregation(
  report: Pick<
    ClearraWasmSearchReport,
    | 'summary_fields'
    | 'coverage_calculated'
    | 'probability_calculated'
    | 'materialized_pattern_count'
    | 'covered_pattern_count'
    | 'coverage_probability'
    | 'probability_complete'
  > | null | undefined,
  aggregation: BuildProbabilityAggregation
): BuildProbabilityCoverageAggregation {
  if (!report) return { state: 'pending', reason: null };

  const exact = (key: string): string | null => {
    const values = report.summary_fields
      .filter(([field]) => field === key)
      .map(([, value]) => value);
    return values.length === 1 ? values[0] : null;
  };
  const requiredKeys = [
    'coverage_aggregation_contract',
    'coverage_aggregation_availability',
    'coverage_aggregation_complete',
    'coverage_aggregation_source_row_count',
    'materialized_pattern_count',
    'covered_pattern_count',
    'failed_pattern_count',
    'coverage_probability',
    'failed_coverage_probability',
    'materialized_probability_mass',
    'coverage_probability_denominator',
    'probability_complete'
  ] as const;
  const fields = Object.fromEntries(requiredKeys.map((key) => [key, exact(key)]));
  if (Object.values(fields).some((value) => value === null)) {
    return { state: 'rejected', reason: 'missing-or-duplicate-coverage-field' };
  }
  if (
    fields.coverage_aggregation_contract !== 'pattern-coverage-aggregation.v1' ||
    fields.coverage_probability_denominator !== 'full-materialized-pattern-universe'
  ) {
    return { state: 'rejected', reason: 'invalid-coverage-contract' };
  }

  const sourceRowCount = canonicalCount(fields.coverage_aggregation_source_row_count);
  const patternCount = canonicalCount(fields.materialized_pattern_count);
  const successfulPatternCount = canonicalCount(fields.covered_pattern_count);
  if (
    sourceRowCount === null ||
    patternCount === null ||
    patternCount < 1 ||
    successfulPatternCount === null ||
    report.materialized_pattern_count !== patternCount ||
    report.covered_pattern_count !== successfulPatternCount
  ) {
    return { state: 'rejected', reason: 'coverage-result-mismatch' };
  }

  if (aggregation === 'tiling') {
    if (
      fields.coverage_aggregation_availability !== 'not-calculated' ||
      fields.coverage_aggregation_complete !== 'false' ||
      fields.failed_pattern_count !== 'not-calculated' ||
      fields.coverage_probability !== 'not-calculated' ||
      fields.failed_coverage_probability !== 'not-calculated' ||
      fields.probability_complete !== 'false' ||
      report.coverage_calculated ||
      report.probability_calculated ||
      report.probability_complete ||
      successfulPatternCount !== 0 ||
      report.coverage_probability !== 'not-calculated'
    ) {
      return { state: 'rejected', reason: 'coverage-result-mismatch' };
    }
    return { state: 'not-calculated', sourceRowCount, patternCount, reason: null };
  }

  const failedPatternCount = canonicalCount(fields.failed_pattern_count);
  const successProbability = canonicalProbability(fields.coverage_probability);
  const failedProbability = canonicalProbability(fields.failed_coverage_probability);
  const materializedProbabilityMass = canonicalProbability(fields.materialized_probability_mass);
  const complete = canonicalBoolean(fields.coverage_aggregation_complete);
  const probabilityComplete = canonicalBoolean(fields.probability_complete);
  if (
    failedPatternCount === null ||
    successProbability === null ||
    failedProbability === null ||
    materializedProbabilityMass === null ||
    complete === null ||
    probabilityComplete === null ||
    successfulPatternCount + failedPatternCount !== patternCount ||
    complete !== probabilityComplete ||
    report.probability_complete !== complete ||
    report.coverage_calculated !== true ||
    report.probability_calculated !== true ||
    report.coverage_probability !== fields.coverage_probability ||
    fields.coverage_aggregation_availability !== (complete ? 'available' : 'incomplete') ||
    !probabilityPartitionMatches(
      successProbability,
      failedProbability,
      materializedProbabilityMass,
      patternCount
    )
  ) {
    return { state: 'rejected', reason: 'coverage-result-mismatch' };
  }
  return {
    state: 'authorized',
    sourceRowCount,
    patternCount,
    successfulPatternCount,
    failedPatternCount,
    successProbability: fields.coverage_probability!,
    failedProbability: fields.failed_coverage_probability!,
    complete,
    reason: null
  };
}

function canonicalCount(value: string | null): number | null {
  if (value === null || !/^(?:0|[1-9]\d*)$/u.test(value)) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

function canonicalBoolean(value: string | null): boolean | null {
  if (value === 'true') return true;
  if (value === 'false') return false;
  return null;
}

function canonicalProbability(value: string | null): number | null {
  if (
    value === null ||
    !/^(?:0|[1-9]\d*)(?:\.\d+)?(?:e-?\d+)?$/u.test(value)
  ) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 1 ? parsed : null;
}

function probabilityPartitionMatches(
  success: number,
  failed: number,
  materialized: number,
  patternCount: number
): boolean {
  const tolerance = Number.EPSILON * Math.max(patternCount, 1) * 4;
  return Math.abs(success + failed - materialized) <= tolerance;
}

function parseBuildProbabilityAggregation(
  value: string | undefined
): BuildProbabilityAggregation | null {
  return value === 'buildability' || value === 'tiling' || value === 'spin'
    ? value
    : null;
}
