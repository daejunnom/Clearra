// Discord's solutions command reuses the native exact Unique producer. It does
// not introduce an App result family or accept geometry-only/probability output.
export function validDiscordPcSolutionsResult(value) {
  const summary = value?.summary;
  return ["pc", "pc-scenario"].includes(value?.kind) &&
    value.contract?.command?.kind === value.kind &&
    summary?.objective === "unique" &&
    summary.search_output_policy === "trace" &&
    summary.postprocess_pc_path_requested === "false" &&
    summary.packing_candidate_is_solution === "false" &&
    summary.buildability_verified === true &&
    typeof summary.count_complete === "boolean" &&
    typeof summary.solution_count_calculated === "boolean" &&
    typeof summary.solution_set_materialized === "boolean";
}
