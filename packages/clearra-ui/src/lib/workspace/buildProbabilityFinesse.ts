import type {
  ClearraFinessePolicyResult,
  ClearraFinesseReport,
  ClearraFinesseReportInput,
  ClearraFinesseRepresentativeWitness
} from '../wasm/wasmCommandClient';
import type { FinesseWitnessExport } from './solutionExport';

export type BuildProbabilityFinesseMetric = 'off' | 'inputs';
export type BuildProbabilityPatternKnowledge = 'both' | 'oracle' | 'visible-7';

export const DEFAULT_BUILD_PROBABILITY_FINESSE: BuildProbabilityFinesseMetric = 'off';
export const DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE: BuildProbabilityPatternKnowledge =
  'both';
const FINESSE_UNAVAILABLE_VALUES = new Set(['not-calculated', 'unavailable']);
const FINESSE_INPUT_ACTIONS = new Set<ClearraFinesseReportInput>([
  'hold',
  'tap-left',
  'tap-right',
  'das-left',
  'das-right',
  'rotate-clockwise',
  'rotate-counter-clockwise',
  'rotate-180',
  'soft-drop',
  'hard-drop'
]);
const FINESSE_PIECES = new Set(['I', 'O', 'T', 'S', 'Z', 'J', 'L']);

export function formatFinesseInputCount(
  value: string | number | undefined | null,
  locale: 'en' | 'ko'
): string {
  if (value === undefined || value === null ||
    (typeof value === 'string' && FINESSE_UNAVAILABLE_VALUES.has(value))) {
    return '—';
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0
    ? new Intl.NumberFormat(locale, { maximumFractionDigits: 4 }).format(parsed)
    : '—';
}

export function buildProbabilityFinesseCommandArguments(
  finesse: BuildProbabilityFinesseMetric,
  patternKnowledge: BuildProbabilityPatternKnowledge
): string[] {
  return finesse === 'off'
    ? []
    : ['--finesse', finesse, '--pattern-knowledge', patternKnowledge];
}

export function buildProbabilityFinesseDesktopFields(
  finesse: BuildProbabilityFinesseMetric,
  patternKnowledge: BuildProbabilityPatternKnowledge
): {
  finesse: BuildProbabilityFinesseMetric;
  pattern_knowledge: BuildProbabilityPatternKnowledge;
} {
  return { finesse, pattern_knowledge: patternKnowledge };
}

export type BuildProbabilitySolutionFinesse = {
  policy: ClearraFinessePolicyResult['policy'];
  average_inputs: string;
  complete: boolean;
};

export type BuildProbabilityFinesseView = {
  complete: boolean;
  exactTotalInputs: string | null;
  representativeWitness: ClearraFinesseRepresentativeWitness | null;
  policyResults: ClearraFinessePolicyResult[];
  solutionByKey: Record<string, BuildProbabilitySolutionFinesse[]>;
};

export function buildProbabilityFinesseView(
  report: ClearraFinesseReport | null | undefined
): BuildProbabilityFinesseView | null {
  if (!report || report.metric !== 'inputs' || report.mode !== 'search') return null;

  const policyResults = report.policy_results.map((policyResult) => ({
    ...policyResult,
    complete: report.complete && policyResult.complete
  }));
  const solutionByKey: Record<string, BuildProbabilitySolutionFinesse[]> = {};
  for (const policyResult of policyResults) {
    for (const solution of policyResult.solution_averages) {
      (solutionByKey[solution.solution_key] ??= []).push({
        policy: policyResult.policy,
        average_inputs: solution.average_inputs,
        complete: policyResult.complete && solution.complete
      });
    }
  }

  return {
    complete: report.complete,
    exactTotalInputs:
      report.exact_total_inputs === undefined || report.exact_total_inputs === null
        ? null
        : String(report.exact_total_inputs),
    representativeWitness: validRepresentativeWitness(report),
    policyResults,
    solutionByKey
  };
}

export function representativeWitnessExportForSolution(
  witness: ClearraFinesseRepresentativeWitness | null | undefined,
  solutionKey: string,
  solutionFinesse: readonly BuildProbabilitySolutionFinesse[] = []
): FinesseWitnessExport | null {
  if (!witness || witness.solution_key !== solutionKey) return null;
  const annotationInputs = solutionFinesse.find(
    (entry) => entry.policy === witness.policy &&
      Number.isFinite(Number(entry.average_inputs)) && Number(entry.average_inputs) >= 0
  )?.average_inputs;
  return {
    solutionKey,
    totalInputs: witness.total_inputs,
    ...(annotationInputs === undefined ? {} : { annotationInputs }),
    inputSequence: witness.input_sequence,
    placements: witness.placements
  };
}

function validRepresentativeWitness(
  report: ClearraFinesseReport
): ClearraFinesseRepresentativeWitness | null {
  const witness = report.representative_witness;
  if (!witness || !['oracle', 'visible-7'].includes(witness.policy)) return null;
  if (!Number.isSafeInteger(witness.total_inputs) || witness.total_inputs < 0) return null;
  if (!Array.isArray(witness.queue) || witness.queue.length > 1024 ||
    !witness.queue.every((piece) => typeof piece === 'string' && FINESSE_PIECES.has(piece))) {
    return null;
  }
  if (!Array.isArray(witness.input_sequence) ||
    witness.input_sequence.length !== witness.total_inputs ||
    !witness.input_sequence.every((input) => FINESSE_INPUT_ACTIONS.has(input))) {
    return null;
  }
  if (!Array.isArray(witness.placements) || witness.placements.length > 60 ||
    !witness.placements.every((placement) =>
      placement !== null && typeof placement === 'object' &&
      typeof placement.piece === 'string' && FINESSE_PIECES.has(placement.piece) &&
      Number.isInteger(placement.rotation) && placement.rotation >= 0 && placement.rotation <= 3 &&
      Number.isInteger(placement.x) && placement.x >= -32 && placement.x <= 32 &&
      Number.isInteger(placement.y) && placement.y >= -32 && placement.y <= 32
    ) || witness.input_sequence.filter((input) => input === 'hard-drop').length !==
      witness.placements.length) {
    return null;
  }
  if (!Array.isArray(witness.pattern_ids) ||
    !witness.pattern_ids.every((id) => Number.isSafeInteger(id) && id >= 0)) {
    return null;
  }
  if (witness.solution_key !== undefined && witness.solution_key !== null &&
    (typeof witness.solution_key !== 'string' || witness.solution_key.length === 0)) {
    return null;
  }
  if (report.exact_total_inputs !== undefined && report.exact_total_inputs !== null) {
    const exact = Number(report.exact_total_inputs);
    if (!Number.isSafeInteger(exact) || exact !== witness.total_inputs) return null;
  }
  return witness;
}
