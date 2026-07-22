#include "gpu_test_support.h"

void c_gpu_batch_descriptor_preserves_pattern_universe_id(void);
void c_gpu_batch_descriptor_preserves_weight_model_id(void);
void c_gpu_batch_descriptor_preserves_piece_window_exact_count_and_source(void);
void gpu_batch_descriptor_has_piece_source_id(void);
void gpu_batch_descriptor_has_piece_multiset_window(void);
void c_gpu_batch_descriptor_product_source_of_truth_is_source_and_multiset(void);
void c_gpu_batch_descriptor_preserves_active_rows_and_clear_hint(void);
void gpu_batch_descriptor_abi_size_is_stable(void);
void gpu_packing_batch_descriptor_is_primary_abi(void);
void gpu_batch_descriptor_rejects_unsupported_board_shape(void);
void gpu_batch_descriptor_rejects_active_rows_over_board_height(void);
void gpu_batch_descriptor_rejects_clear_hint_over_board_height(void);
void gpu_batch_descriptor_rejects_exact_piece_count_over_window(void);
void gpu_batch_descriptor_rejects_piece_count_exceeding_window(void);
void gpu_batch_descriptor_rejects_mask_outside_active_packing_rows(void);
void gpu_batch_descriptor_rejects_unknown_piece_source_kind(void);
void cpu_packing_reference_uses_multiset_window(void);
void standard_gpu_descriptor_unchanged(void);
void gpu_unavailable_reports_reason(void);
void gpu_backend_selection_defaults_to_unavailable_native(void);
void gpu_backend_native_unavailable_reports_reason(void);
void gpu_backend_registry_excludes_unimplemented_apis(void);
void gpu_backend_rejects_user_provided_shader_path(void);
void gpu_backend_adapter_unavailable_does_not_execute(void);
void gpu_backend_adapter_reports_kernel_unavailable(void);
void gpu_backend_adapter_rejects_user_shader_path(void);
void gpu_context_destroy_releases_memory_context(void);
void gpu_packing_candidate_count_matches_cpu_reference(void);
void gpu_packing_mixed_piece_candidate_count_matches_cpu_reference(void);
void gpu_partial_reference_result_cannot_source_exact_probability(void);
void gpu_result_passes_hash_exact_confirm(void);
void gpu_candidate_requires_cpu_exact_confirm(void);
void gpu_shape_hash_collision_requires_exact_compare(void);
void gpu_cpu_exact_confirm_rejects_operation_shape_or_tiling_key_mismatch(void);
void gpu_strengthening_reports_batch_prefilter_hash_and_compression(void);
void gpu_result_is_deterministic_and_cpu_reference_confirmed(void);
void gpu_result_cpu_reference_matched_before_build_queue(void);
void gpu_shape_union_mask_matches_raw_candidate_shapes(void);
void gpu_candidate_is_not_output_as_solution_before_buildup(void);
void gpu_candidate_is_not_solution_before_buildup(void);
void gpu_raw_candidate_cannot_enter_buildup_queue(void);
void gpu_confirmed_candidate_enters_buildup_queue(void);
void confirmed_candidate_can_enter_buildup_queue(void);
void gpu_confirmed_candidate_is_still_not_solution(void);
void no_backend_fallback_prevents_silent_cpu_fallback(void);
void gpu_backend_fallback_allowed_uses_cpu(void);
void gpu_backend_no_fallback_returns_error(void);
void gpu_product_backend_records_pruning_evidence_and_matches_cpu(void);
void cpu_reference_differs_for_o_only_vs_mixed_piece_batch(void);
void gpu_worker_unavailable_result_is_not_exact(void);
void gpu_worker_unconfirmed_result_cannot_source_exact_probability(void);
void gpu_worker_scheduler_bridge_uses_memory_ticket_and_fence(void);

#define RUN_GPU_TEST(test_fn)        \
    do {                             \
        fputs(#test_fn "\n", stdout); \
        fflush(stdout);              \
        test_fn();                   \
    } while (0)

int main(void) {
    RUN_GPU_TEST(c_gpu_batch_descriptor_preserves_pattern_universe_id);
    RUN_GPU_TEST(c_gpu_batch_descriptor_preserves_weight_model_id);
    RUN_GPU_TEST(c_gpu_batch_descriptor_preserves_piece_window_exact_count_and_source);
    RUN_GPU_TEST(gpu_batch_descriptor_has_piece_source_id);
    RUN_GPU_TEST(gpu_batch_descriptor_has_piece_multiset_window);
    RUN_GPU_TEST(c_gpu_batch_descriptor_product_source_of_truth_is_source_and_multiset);
    RUN_GPU_TEST(c_gpu_batch_descriptor_preserves_active_rows_and_clear_hint);
    RUN_GPU_TEST(gpu_batch_descriptor_abi_size_is_stable);
    RUN_GPU_TEST(gpu_packing_batch_descriptor_is_primary_abi);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_unsupported_board_shape);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_active_rows_over_board_height);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_clear_hint_over_board_height);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_exact_piece_count_over_window);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_piece_count_exceeding_window);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_mask_outside_active_packing_rows);
    RUN_GPU_TEST(gpu_batch_descriptor_rejects_unknown_piece_source_kind);
    RUN_GPU_TEST(cpu_packing_reference_uses_multiset_window);
    RUN_GPU_TEST(standard_gpu_descriptor_unchanged);
    RUN_GPU_TEST(gpu_unavailable_reports_reason);
    RUN_GPU_TEST(gpu_backend_selection_defaults_to_unavailable_native);
    RUN_GPU_TEST(gpu_backend_native_unavailable_reports_reason);
    RUN_GPU_TEST(gpu_backend_registry_excludes_unimplemented_apis);
    RUN_GPU_TEST(gpu_backend_rejects_user_provided_shader_path);
    RUN_GPU_TEST(gpu_backend_adapter_unavailable_does_not_execute);
    RUN_GPU_TEST(gpu_backend_adapter_reports_kernel_unavailable);
    RUN_GPU_TEST(gpu_backend_adapter_rejects_user_shader_path);
    RUN_GPU_TEST(gpu_context_destroy_releases_memory_context);
    RUN_GPU_TEST(gpu_packing_candidate_count_matches_cpu_reference);
    RUN_GPU_TEST(gpu_packing_mixed_piece_candidate_count_matches_cpu_reference);
    RUN_GPU_TEST(gpu_partial_reference_result_cannot_source_exact_probability);
    RUN_GPU_TEST(gpu_result_passes_hash_exact_confirm);
    RUN_GPU_TEST(gpu_candidate_requires_cpu_exact_confirm);
    RUN_GPU_TEST(gpu_shape_hash_collision_requires_exact_compare);
    RUN_GPU_TEST(gpu_cpu_exact_confirm_rejects_operation_shape_or_tiling_key_mismatch);
    RUN_GPU_TEST(gpu_strengthening_reports_batch_prefilter_hash_and_compression);
    RUN_GPU_TEST(gpu_result_is_deterministic_and_cpu_reference_confirmed);
    RUN_GPU_TEST(gpu_result_cpu_reference_matched_before_build_queue);
    RUN_GPU_TEST(gpu_shape_union_mask_matches_raw_candidate_shapes);
    RUN_GPU_TEST(gpu_candidate_is_not_output_as_solution_before_buildup);
    RUN_GPU_TEST(gpu_candidate_is_not_solution_before_buildup);
    RUN_GPU_TEST(gpu_raw_candidate_cannot_enter_buildup_queue);
    RUN_GPU_TEST(gpu_confirmed_candidate_enters_buildup_queue);
    RUN_GPU_TEST(confirmed_candidate_can_enter_buildup_queue);
    RUN_GPU_TEST(gpu_confirmed_candidate_is_still_not_solution);
    RUN_GPU_TEST(no_backend_fallback_prevents_silent_cpu_fallback);
    RUN_GPU_TEST(gpu_backend_fallback_allowed_uses_cpu);
    RUN_GPU_TEST(gpu_backend_no_fallback_returns_error);
    RUN_GPU_TEST(gpu_product_backend_records_pruning_evidence_and_matches_cpu);
    RUN_GPU_TEST(cpu_reference_differs_for_o_only_vs_mixed_piece_batch);
    RUN_GPU_TEST(gpu_worker_unavailable_result_is_not_exact);
    RUN_GPU_TEST(gpu_worker_unconfirmed_result_cannot_source_exact_probability);
    RUN_GPU_TEST(gpu_worker_scheduler_bridge_uses_memory_ticket_and_fence);
    puts("core-c gpu tests passed");
    return 0;
}
