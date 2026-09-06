// SRP: attach opt-in numeric host timings without changing execution authority.
import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown> : null;
}

export function withHostExecutionTiming(
  event: ClearraWasmWorkerEvent,
  timings: Readonly<Record<string, number>> | null
): ClearraWasmWorkerEvent {
  if (!timings || (event.event !== 'final_response' && event.event !== 'failed')) return event;
  const previous = (event as ClearraWasmWorkerEvent & { search_profile?: unknown }).search_profile;
  const profile = record(previous) ?? (previous == null ? {} : { core_profile: previous });
  const profiled: ClearraWasmWorkerEvent & { search_profile: Record<string, unknown> } = {
    ...event,
    search_profile: {
      ...profile,
      host_execution: { ...record(profile.host_execution), ...timings }
    }
  };
  return profiled;
}
