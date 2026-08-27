export type SolutionSetReportLike = {
  search_output_policy?: string;
  unique_solution_count?: number;
  normalized_solution_keys?: readonly string[];
  normalized_solution_set_hash?: string;
  solution_count_calculated?: boolean;
  solution_set_materialized?: boolean;
  solution_keys_materialized_count?: number;
  solution_keys_complete?: boolean;
  solution_page_available?: boolean;
  count_complete?: boolean;
  execution_availability?: { state?: string };
  result_completeness?: string;
  summary_fields?: ReadonlyArray<readonly [string, string]>;
};

export function workspaceSolutionCountCalculated(
  report: SolutionSetReportLike | null | undefined
): boolean {
  if (!report) return false;
  if (coverageSummaryDisposition(report) !== 'non-coverage') return false;
  if (!reportPermitsCompletedCount(report)) return false;
  if (typeof report.solution_count_calculated === 'boolean') {
    return report.solution_count_calculated;
  }
  const summaryValue = summaryBoolean(report, 'solution_count_calculated');
  if (summaryValue !== null) return summaryValue;
  return false;
}

export function workspaceSolutionCount(
  report: SolutionSetReportLike | null | undefined
): number | null {
  if (!workspaceSolutionCountCalculated(report)) return null;
  const count = report?.unique_solution_count;
  return Number.isSafeInteger(count) && (count ?? -1) >= 0 ? count! : null;
}

export function workspaceSolutionSetMaterialized(
  report: SolutionSetReportLike | null | undefined
): boolean {
  if (!report) return false;
  if (coverageSummaryDisposition(report) !== 'non-coverage') return false;
  if (!reportPermitsCompletedCount(report)) return false;
  if (typeof report.solution_set_materialized === 'boolean') {
    return report.solution_set_materialized;
  }
  return summaryBoolean(report, 'solution_set_materialized') ??
    workspaceSolutionCountCalculated(report);
}

export function workspaceSolutionKeysComplete(
  report: SolutionSetReportLike | null | undefined
): boolean {
  if (!report) return false;
  if (coverageSummaryDisposition(report) !== 'non-coverage') return false;
  if (!reportPermitsCompletedCount(report)) return false;
  if (typeof report.solution_keys_complete === 'boolean') {
    return report.solution_keys_complete;
  }
  return summaryBoolean(report, 'solution_keys_complete') ??
    workspaceSolutionSetMaterialized(report);
}

export function workspaceSolutionPageAvailable(
  report: SolutionSetReportLike | null | undefined
): boolean {
  if (!report) return false;
  if (coverageSummaryDisposition(report) !== 'non-coverage') return false;
  if (!reportPermitsCompletedCount(report)) return false;
  if (typeof report.solution_page_available === 'boolean') {
    return report.solution_page_available;
  }
  return summaryBoolean(report, 'solution_page_available') ?? false;
}

function reportPermitsCompletedCount(report: SolutionSetReportLike): boolean {
  return report.count_complete === true &&
    report.execution_availability?.state === 'available' &&
    report.result_completeness === 'complete';
}

function summaryBoolean(report: SolutionSetReportLike, key: string): boolean | null {
  const values = summaryValues(report, key);
  if (values.length !== 1) return null;
  const [value] = values;
  if (value === 'true') return true;
  if (value === 'false') return false;
  return null;
}

type CoverageSummaryDisposition = 'canonical' | 'invalid' | 'non-coverage';

const COVERAGE_SUMMARY_REQUIRED_FIELDS = Object.freeze([
  ['search_output_policy', 'coverage-summary'],
  ['unique_solution_count', 'not-calculated'],
  ['normalized_unique_solution_count', 'not-calculated'],
  ['solution_count_calculated', 'false'],
  ['solution_set_materialized', 'false'],
  ['solution_keys_materialized_count', '0'],
  ['solution_keys_complete', 'false'],
  ['solution_page_available', 'false'],
  ['normalized_solution_set_hash', 'not-calculated'],
  ['actual_normalized_solution_set_hash', 'not-calculated']
] as const);

const COVERAGE_SUMMARY_OPTIONAL_SENTINELS = Object.freeze([
  'total_solution_count',
  'actual_normalized_unique_solution_count',
  'mirror_unique_solution_count',
  'original_unique_solution_count',
  'mirror_normalized_solution_set_hash'
]);

const NON_COVERAGE_POLICIES = new Set(['summary', 'trace', 'tiling-only', 'coverage-rows']);

function coverageSummaryDisposition(report: SolutionSetReportLike): CoverageSummaryDisposition {
  const policyValues = summaryValues(report, 'search_output_policy');
  const topLevelPolicy = report.search_output_policy;
  const policyCandidates = [
    ...policyValues,
    ...(typeof topLevelPolicy === 'string' ? [topLevelPolicy] : [])
  ];
  const coverageCandidate = policyCandidates.some(isCoverageSummarySpelling) ||
    COVERAGE_SUMMARY_REQUIRED_FIELDS.slice(1).some(([key, expected]) =>
      expected === 'not-calculated' && summaryValues(report, key).includes(expected)
    ) || report.normalized_solution_set_hash === 'not-calculated';

  if (!coverageCandidate) {
    if (policyValues.length > 1) return 'invalid';
    const policy = policyValues[0] ?? topLevelPolicy;
    if (policy !== undefined && !NON_COVERAGE_POLICIES.has(policy)) return 'invalid';
    return 'non-coverage';
  }

  const requiredFieldsAreCanonical = COVERAGE_SUMMARY_REQUIRED_FIELDS.every(
    ([key, expected]) => {
      const values = summaryValues(report, key);
      return values.length === 1 && values[0] === expected;
    }
  );
  const optionalFieldsAreCanonical = COVERAGE_SUMMARY_OPTIONAL_SENTINELS.every((key) => {
    const values = summaryValues(report, key);
    return values.length === 0 || (values.length === 1 && values[0] === 'not-calculated');
  });
  const topLevelPolicyIsCanonical = topLevelPolicy === undefined ||
    topLevelPolicy === 'coverage-summary';
  const projectedFieldsAreCanonical =
    report.solution_count_calculated === false &&
    report.solution_set_materialized === false &&
    report.solution_keys_materialized_count === 0 &&
    report.solution_keys_complete === false &&
    report.solution_page_available === false &&
    report.unique_solution_count === 0 &&
    Array.isArray(report.normalized_solution_keys) &&
    report.normalized_solution_keys.length === 0 &&
    report.normalized_solution_set_hash === 'not-calculated';

  return requiredFieldsAreCanonical &&
    optionalFieldsAreCanonical &&
    topLevelPolicyIsCanonical &&
    projectedFieldsAreCanonical
    ? 'canonical'
    : 'invalid';
}

function summaryValues(report: SolutionSetReportLike, key: string): string[] {
  if (!Array.isArray(report.summary_fields)) return [];
  const values: string[] = [];
  for (const entry of report.summary_fields) {
    if (!Array.isArray(entry) || entry.length !== 2) continue;
    if (entry[0] === key && typeof entry[1] === 'string') values.push(entry[1]);
  }
  return values;
}

function isCoverageSummarySpelling(value: string): boolean {
  return value.trim().toLowerCase().replaceAll('_', '-') === 'coverage-summary';
}
