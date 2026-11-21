// Diagnostic only. These are sampled, potentially nested elapsed spans from
// independent WASM realms; their sum is neither CPU time nor request wall time.
const MAX_STAGE_ROWS = 96;
const STAGE_NAME = /^(?:app|finesse|rust|wasm)(?:\.[a-z][a-z0-9_]*){1,7}$/u;

export type VerifierWasmStage = {
  name: string;
  duration_ns: number;
  invocation_count: number;
  work_item_count: number;
};

function isSafeCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

export function normalizeVerifierWasmStageRows(value: unknown): VerifierWasmStage[] | null {
  if (typeof value !== 'object' || value === null || !('schema_version' in value) ||
      value.schema_version !== 1 || !('stages' in value) || !Array.isArray(value.stages) ||
      value.stages.length > MAX_STAGE_ROWS) return null;

  const names = new Set<string>();
  const observed: VerifierWasmStage[] = [];
  for (const stage of value.stages) {
    if (typeof stage !== 'object' || stage === null) return null;
    const { name, duration_ns, invocation_count, work_item_count } = stage as Record<string, unknown>;
    if (typeof name !== 'string' || name.length > 96 || !STAGE_NAME.test(name) || names.has(name) ||
        !isSafeCount(duration_ns) || !isSafeCount(invocation_count) ||
        !isSafeCount(work_item_count)) return null;
    names.add(name);
    if (invocation_count > 0) {
      observed.push({ name, duration_ns, invocation_count, work_item_count });
    }
  }
  return observed.length > 0 ? observed : null;
}

export class VerifierWasmStageProfile {
  private totals = new Map<string, VerifierWasmStage>();
  private sampledWorkers = 0;
  expectedWorkers = 0;

  append(value: unknown): void {
    const rows = normalizeVerifierWasmStageRows(value);
    if (!rows) return;
    const merged = new Map(this.totals);
    for (const row of rows) {
      const previous = merged.get(row.name);
      const combined = previous
        ? { name: row.name, duration_ns: previous.duration_ns + row.duration_ns,
            invocation_count: previous.invocation_count + row.invocation_count,
            work_item_count: previous.work_item_count + row.work_item_count }
        : row;
      if (!isSafeCount(combined.duration_ns) || !isSafeCount(combined.invocation_count) ||
          !isSafeCount(combined.work_item_count)) return;
      merged.set(row.name, combined);
    }
    if (merged.size > MAX_STAGE_ROWS) return;
    this.totals = merged;
    this.sampledWorkers += 1;
  }

  finish() {
    return {
      schema_version: 1,
      scope: 'finished-geometry-verifiers' as const,
      semantics: 'sampled nested elapsed spans summed across peer WASM realms; not CPU or request wall time',
      sampled_workers: this.sampledWorkers,
      expected_workers: this.expectedWorkers,
      stages: [...this.totals.values()].sort((a, b) => a.name.localeCompare(b.name))
    };
  }
}
