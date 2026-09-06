// Development-only observation: never controls scheduling or carries search inputs.
export function isLocalSearchProfileMode(mode: string): boolean {
  return mode === 'local-recovery' || mode === 'local-audit';
}

const stages = ['ready_client_wait', 'prewarm_new', 'prewarm_reuse', 'payload_hash',
  'prepare', 'offered', 'offer_round_trip', 'accepted', 'published', 'posted_to_start_notice',
  'running_commit', 'run_grant_to_reply', 'result_hash', 'result_sealed', 'result_apply',
  'result_applied', 'completed'];
const waveKeys = ['wave', 'query_prepare_ms', 'upstream_gap_ms', 'initialize_all_ready_ms',
  'first_receipt_ms', 'last_receipt_ms', 'remote_admission_wait_ms', 'remote_drain_ms',
  'task_round_trip_max_ms', 'coordinator_compute_ms', 'coordinator_tasks',
  'coordinator_slices', 'remote_tasks', 'sampled_active_min', 'sampled_active_max', 'elapsed_ms'];

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown> : null;
}

function numbers(value: unknown, keys: string[]): Record<string, number | null> {
  const source = record(value);
  const result: Record<string, number | null> = {};
  if (!source) return result;
  for (const key of keys) {
    const item = source[key];
    if (item === null) result[key] = null;
    else if (typeof item === 'number' && Number.isFinite(item) && item >= 0) {
      result[key] = Math.round(item * 1000) / 1000;
    }
  }
  return result;
}

export function localSearchProfileText(event: unknown): string | null {
  const envelope = record(event);
  if (!envelope || !['final_response', 'failed'].includes(String(envelope.event))) return null;
  const profile = record(envelope.search_profile);
  if (!profile) return null;
  const result: Record<string, unknown> = {};
  const host = numbers(profile.host_execution, [
    'product_prepare_ms', 'module_prepare_ms', 'worker_elapsed_to_terminal_ms',
    'run_to_emit_ms', 'source_ms', 'drain_ms', 'verifier_finish_ms', 'finalize_ms',
    'parse_ms', 'source_produce_ms', 'source_produce_calls', 'source_enqueue_ms',
    'source_merge_ms', 'source_merge_calls'
  ]);
  if (Object.keys(host).length > 0) result.host_execution = host;
  const transport = record(record(profile.verifier_transport)?.timings);
  if (transport) {
    const timings: Record<string, unknown> = {};
    for (const operation of ['', 'initialize.', 'consume.', 'finish.']) {
      for (const stage of stages) {
        const key = operation + stage;
        if (record(transport[key])) timings[key] = numbers(transport[key], ['count', 'failed', 'total_ms', 'max_ms']);
      }
    }
    result.verifier_transport = timings;
  }
  const minimum = record(profile.minimum_parallel);
  if (minimum) {
    result.minimum_parallel = {
      ...numbers(minimum, ['wave_count', 'omitted_wave_count']),
      waves: Array.isArray(minimum.waves) ? minimum.waves.slice(0, 128).map((wave) => numbers(wave, waveKeys)) : []
    };
  }
  return Object.keys(result).length > 0 ? JSON.stringify(result, null, 2) : null;
}
