import type {
  ClearraBoundaryRecoveryPopulationExamplePayload, ClearraBoundaryRecoveryPopulationPayload,
  ClearraBoundaryRecoveryStepPayload, ClearraProductResultPayload
} from '../wasm/wasmCommandClient.ts';

export function validateBoundaryRecoveryPayload(product: ClearraProductResultPayload): string | null {
  if (product.content.payload_kind !== 'boundary-recovery') return 'invalid boundary recovery payload';
  const report = product.content.payload;
  const population = report.population;
  const pattern = report.knowledge_basis === 'full-pattern-universe';
  return product.contract === 'boundary-recovery.v1' &&
    product.result_kind === 'boundary-recovery' &&
    (pattern
      ? report.placement_role_scope === 'bag-piece-exact-lock-time' &&
        (report.status === (population?.complete ? 'population-complete' : 'population-incomplete')) &&
        validatePopulation(population)
      : report.knowledge_basis === 'full-fixed-queue' &&
        ['occupancy-only', 'exact-lock-time'].includes(report.placement_role_scope) &&
        population === undefined &&
        ['normal', 'pc-preserving-recovery', 'non-pc-recovery', 'no-path-within-declared-scope', 'incomplete'].includes(report.status)) &&
    (report.max_early_placements === 0 || report.max_early_placements === 1) &&
    Number.isInteger(report.borrow_source_index) && report.borrow_source_index >= 0 && report.borrow_source_index < 42 &&
    /^0x[0-9a-f]{1,64}$/u.test(report.borrow_placement_mask) &&
    Number.isSafeInteger(report.normal_states) && report.normal_states >= 0 &&
    Number.isSafeInteger(report.recovery_states) && report.recovery_states >= 0 &&
    validSteps(report.steps) ? null : 'invalid boundary recovery payload';
}

function validSteps(steps: ClearraBoundaryRecoveryStepPayload[]): boolean {
  return Array.isArray(steps) && steps.length <= 42 &&
    steps.every((step) =>
      Number.isInteger(step.source_queue_index) && step.source_queue_index >= 0 && step.source_queue_index < 42 &&
      /^[IJLOSTZ]$/u.test(step.piece) &&
      /^0x[0-9a-f]{1,64}$/u.test(step.placement_mask) &&
      /^0x[0-9a-f]{1,64}$/u.test(step.board_after_mask)
    );
}

function validatePopulation(population: ClearraBoundaryRecoveryPopulationPayload | undefined): boolean {
  if (!population) return false;
  const counts = [population.materialized_pattern_count, population.evaluated_pattern_count,
    population.state_count, population.normal_count, population.pc_preserving_recovery_count,
    population.non_pc_recovery_count, population.no_path_count, population.incomplete_count,
    population.diagram_unavailable_count];
  const probabilities = [population.normal_probability, population.pc_preserving_recovery_probability,
    population.non_pc_recovery_probability, population.additional_recovery_probability,
    population.total_response_probability, population.no_path_probability,
    population.unknown_probability];
  return counts.every((count) => Number.isSafeInteger(count) && count >= 0) &&
    /^\d+$/u.test(population.total_possible_pattern_count) &&
    probabilities.every((value) => typeof value === 'string' && /^0(?:\.\d+)?$|^1(?:\.0+)?$/u.test(value)) &&
    population.evaluated_pattern_count <= population.materialized_pattern_count &&
    population.normal_count + population.pc_preserving_recovery_count + population.non_pc_recovery_count +
      population.no_path_count + population.incomplete_count === population.evaluated_pattern_count &&
    population.diagram_unavailable_count <= population.no_path_count &&
    validExample(population.normal_example) && validExample(population.recovery_example);
}

function validExample(example: ClearraBoundaryRecoveryPopulationExamplePayload | undefined): boolean {
  return example === undefined || (
    Number.isSafeInteger(example.pattern_index) && example.pattern_index >= 0 &&
    /^[IJLOSTZ]{1,42}$/u.test(example.queue) &&
    ['normal', 'pc-preserving-recovery', 'non-pc-recovery'].includes(example.status) &&
    validSteps(example.steps)
  );
}
