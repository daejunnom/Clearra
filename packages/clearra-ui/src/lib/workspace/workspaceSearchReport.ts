import type {
  ExecutionAvailabilityReport,
  ExecutionCompletenessState
} from '../wasm/executionAvailability';
import type { ClearraWasmSearchReport } from '../wasm/wasmCommandClient';

// SRP: the worker/desktop search DTO and the host resource authority are
// deliberately separate contracts. The workspace presenter consumes their
// single, typed projection instead of teaching each result component how to
// join host evidence to solver data.
export type WorkspaceSearchReport = ClearraWasmSearchReport & {
  execution_availability?: ExecutionAvailabilityReport;
  result_completeness?: ExecutionCompletenessState;
};

export function projectWorkspaceSearchReport(
  report: ClearraWasmSearchReport | null,
  executionAvailability: ExecutionAvailabilityReport | null | undefined,
  resultCompleteness: ExecutionCompletenessState | null | undefined
): WorkspaceSearchReport | null {
  if (!report) return null;
  if (!executionAvailability || !resultCompleteness) return report;
  return {
    ...report,
    execution_availability: executionAvailability,
    result_completeness: resultCompleteness
  };
}
