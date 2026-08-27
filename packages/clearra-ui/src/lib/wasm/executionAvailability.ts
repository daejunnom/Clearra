export type ExecutionAvailabilityState =
  | 'available'
  | 'unavailable'
  | 'deferred'
  | 'exhausted'
  | 'cancelled'
  | 'incomplete';

export type ExecutionAvailabilityReason =
  | 'not-executed'
  | 'capability-unavailable'
  | 'pattern-count-address-space-exceeded'
  | 'dense-pattern-representation-unavailable'
  | 'compute-budget-exceeded'
  | 'memory-budget-exceeded'
  | 'shared-resource-contention'
  | 'cancelled-by-caller'
  | 'partial-execution';

export type ExecutionCompletenessState =
  | 'not-executed'
  | 'complete'
  | 'incomplete';

export type ExecutionSurface = 'native' | 'browser-wasm32' | 'unknown';

export type ExecutionAvailabilityReport = Readonly<{
  state: ExecutionAvailabilityState;
  reason: ExecutionAvailabilityReason | null;
  surface: ExecutionSurface;
  descriptor_pattern_count: string | null;
  dense_pattern_count: string | null;
  required_dense_bytes: string | null;
  required_memory_bytes: string | null;
}>;

export function isExecutionAvailabilityReport(
  value: unknown
): value is ExecutionAvailabilityReport {
  if (!value || typeof value !== 'object') return false;
  const report = value as Partial<ExecutionAvailabilityReport>;
  if (!EXECUTION_AVAILABILITY_STATES.has(report.state as ExecutionAvailabilityState)) {
    return false;
  }
  if (!EXECUTION_SURFACES.has(report.surface as ExecutionSurface)) return false;
  if (report.state === 'available') {
    if (report.reason !== null) return false;
  } else if (!EXECUTION_AVAILABILITY_REASONS.has(report.reason as ExecutionAvailabilityReason)) {
    return false;
  }
  if (!reasonIsCompatible(
    report.state as ExecutionAvailabilityState,
    report.reason ?? null
  )) return false;
  const evidence = [
    report.descriptor_pattern_count,
    report.dense_pattern_count,
    report.required_dense_bytes
  ];
  if (!evidence.every((entry) => entry === null || isCanonicalDecimal(entry))) return false;
  const populatedEvidence = evidence.filter((entry) => entry !== null);
  if (populatedEvidence.length !== 0 && populatedEvidence.length !== evidence.length) return false;
  if (report.required_memory_bytes !== null &&
      !isCanonicalDecimal(report.required_memory_bytes)) return false;
  if (populatedEvidence.length === 0) return true;

  const descriptorCount = BigInt(report.descriptor_pattern_count!);
  const denseCount = BigInt(report.dense_pattern_count!);
  const requiredDenseBytes = BigInt(report.required_dense_bytes!);
  const requiredMemoryBytes = report.required_memory_bytes === null
    ? null
    : BigInt(report.required_memory_bytes);
  return denseCount <= descriptorCount &&
    requiredDenseBytes === ((denseCount + 63n) / 64n) * 8n &&
    (requiredMemoryBytes === null || requiredMemoryBytes >= requiredDenseBytes);
}

export function executionResultIsComplete(
  availability: ExecutionAvailabilityReport | null | undefined,
  completeness: ExecutionCompletenessState | null | undefined
): boolean {
  return availability?.state === 'available' && completeness === 'complete';
}

function isCanonicalDecimal(value: unknown): value is string {
  return typeof value === 'string' && /^(?:0|[1-9][0-9]*)$/u.test(value);
}

function reasonIsCompatible(
  state: ExecutionAvailabilityState,
  reason: ExecutionAvailabilityReason | null
): boolean {
  switch (state) {
    case 'available':
      return reason === null;
    case 'unavailable':
      return reason === 'not-executed' ||
        reason === 'capability-unavailable' ||
        reason === 'pattern-count-address-space-exceeded' ||
        reason === 'dense-pattern-representation-unavailable';
    case 'deferred':
      return reason === 'shared-resource-contention';
    case 'exhausted':
      return reason === 'compute-budget-exceeded' || reason === 'memory-budget-exceeded';
    case 'cancelled':
      return reason === 'cancelled-by-caller';
    case 'incomplete':
      return reason === 'partial-execution';
  }
}

const EXECUTION_AVAILABILITY_STATES = new Set<ExecutionAvailabilityState>([
  'available',
  'unavailable',
  'deferred',
  'exhausted',
  'cancelled',
  'incomplete'
]);

const EXECUTION_AVAILABILITY_REASONS = new Set<ExecutionAvailabilityReason>([
  'not-executed',
  'capability-unavailable',
  'pattern-count-address-space-exceeded',
  'dense-pattern-representation-unavailable',
  'compute-budget-exceeded',
  'memory-budget-exceeded',
  'shared-resource-contention',
  'cancelled-by-caller',
  'partial-execution'
]);

const EXECUTION_SURFACES = new Set<ExecutionSurface>([
  'native',
  'browser-wasm32',
  'unknown'
]);
