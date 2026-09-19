// Local benchmark output only. Do not print hundreds of thousands of solution
// keys, and do not turn an unavailable/truncated result into a complete count.
export function pc4SearchSummary(report) {
  if (!report || typeof report !== 'object' || Array.isArray(report)) return null;
  const fields = [
    'backend_selected', 'workers_used', 'cpu_parallel_execution',
    'total_possible_pattern_count', 'solution_found', 'packing_candidate_count',
    'geometry_candidate_family_count', 'packing_candidate_set_digest',
    'unique_solution_count', 'solution_count_calculated', 'solution_set_materialized',
    'solution_keys_materialized_count', 'solution_keys_complete', 'solution_page_available',
    'normalized_solution_set_hash', 'buildability_verified', 'count_complete',
    'searched_nodes', 'peak_frontier_states', 'peak_cpu_bytes',
    'resource_truncated', 'resource_truncation_reason',
  ];
  return Object.fromEntries(fields.filter(key => ['string', 'number', 'boolean'].includes(typeof report[key]))
    .map(key => [key, report[key]]));
}
