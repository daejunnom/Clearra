# Base

list(APPEND CLEARRA_CORE_SOURCES src/clearra_core.c)

# Execution

list(APPEND CLEARRA_CORE_SOURCES
    src/execution/execution_control.c
    src/execution/search_stage_profiler.c
)

# Memory

list(APPEND CLEARRA_CORE_SOURCES
    src/memory/clr_mem_context.c
    src/memory/clr_scope.c
    src/memory/clr_allocators.c
    src/memory/clr_release_queue.c
    src/memory/clr_gpu_buffer_lifetime.c
    src/memory/clr_memory_debug.c
)

# Board

list(APPEND CLEARRA_CORE_SOURCES
    src/board/board64.c
    src/board/board_backend_dispatch.c
    src/board/board128.c
    src/board/board256.c
    src/board/standard_pc_extended_board.c
    src/board/wide_board.c
)

# Field

list(APPEND CLEARRA_CORE_SOURCES
    src/field/occupancy_field.c
    src/field/field_text_parser.c
    src/field/field_coordinate.c
)

# Piece

list(APPEND CLEARRA_CORE_SOURCES
    src/piece/tetromino.c
    src/piece/rotation.c
    src/piece/operation.c
    src/piece/operation_table.c
    src/piece/operation_set.c
)

# Rules

list(APPEND CLEARRA_CORE_SOURCES
    src/rules/rule_profile.c
    src/rules/srs_kicks.c
    src/rules/no_kick.c
    src/rules/kick_table.c
    src/rules/spawn_profile.c
)

# Supply

list(APPEND CLEARRA_CORE_SOURCES
    src/supply/queue_view.c
    src/supply/supply_state.c
    src/supply/piece_window.c
    src/supply/piece_source_descriptor.c
    src/supply/hold_automaton.c
    src/supply/standard_bag_automaton.c
)

# Pruning

list(APPEND CLEARRA_CORE_SOURCES
    src/pruning/prune_reason.c
    src/pruning/pruning_proof_ledger.c
    src/pruning/domain_propagation.c
    src/pruning/geometry_bumper_domain.c
    src/pruning/geometry_column_projection.c
    src/pruning/geometry_projection_reachability.c
    src/pruning/geometry_parent_hall_bound.c
)

# Arm-Pair Domain Propagation

list(APPEND CLEARRA_CORE_SOURCES
    src/apdp/geometry_apdp.c
)

# Resource

list(APPEND CLEARRA_CORE_SOURCES
    src/resource/resource_budget.c
    src/resource/resource_report.c
)

# Gpu

list(APPEND CLEARRA_CORE_SOURCES
    src/gpu/gpu_capability.c
    src/gpu/gpu_batch_descriptor.c
    src/gpu/gpu_worker_unavailable.c
)

# Scheduler

list(APPEND CLEARRA_CORE_SOURCES
    src/scheduler/gpu_worker_scheduler_bridge.c
)

# Cache

list(APPEND CLEARRA_CORE_SOURCES
    src/cache/cache_identity.c
    src/cache/cache_key.c
)

# Candidate

list(APPEND CLEARRA_CORE_SOURCES
    src/candidate/candidate_search_dispatch.c
    src/candidate/harddrop_candidate.c
    src/candidate/locked_candidate.c
    src/candidate/candidate_cache.c
)

# Reachability

list(APPEND CLEARRA_CORE_SOURCES
    src/reachability/reachability_checker.c
    src/reachability/reachability_field.c
    src/reachability/harddrop_reachability.c
    src/reachability/locked_reachability.c
    src/reachability/kick_first_success.c
    src/reachability/reachability_frontier.c
    src/reachability/reachable_lock_batch.c
)

# Problem

list(APPEND CLEARRA_CORE_SOURCES
    src/problem/packing_problem.c
    src/problem/buildup_problem.c
    src/problem/problem_defaults.c
)

# Packing

list(APPEND CLEARRA_CORE_SOURCES
    src/packing/placement_candidate.c
    src/packing/target_frame_projection.c
    src/packing/target_frame_geometry_domain.c
    src/packing/geometry_catalog.c
    src/packing/geometry_realization_domain.c
    src/packing/geometry_component_decomposition.c
    src/packing/geometry_component_composer.c
    src/packing/geometry_component_policy.c
    src/packing/geometry_component_solution_table.c
    src/packing/geometry_piece_family_domain.c
    src/packing/geometry_full_placement_domain.c
    src/invariant/geometry_additive_invariant.c
    src/packing/geometry_exact_cover_proof.c
    src/packing/geometry_exact_cover.c
    src/packing/geometry_solution_emitter.c
    src/packing/geometry_residual_memo.c
    src/packing/geometry_solution_family.c
    src/packing/geometry_solution_graph.c
    src/packing/geometry_buildable_stream.c
    src/packing/packing_candidate_materializer.c
    src/packing/packing_candidate_buffer.c
    src/packing/packing_prune_context.c
    src/packing/packing_pruner.c
    src/packing/tiling_key.c
    src/packing/packing_deduper.c
)

# Independent exactness oracle. These sources live outside the product source
# tree and are never linked into clearra_core; an explicit test-only target
# owns them.
set(CLEARRA_CORE_TEST_ORACLE_SOURCES
    tests/oracle/gpu_buffer.c
    tests/oracle/gpu_backend.c
    tests/oracle/gpu_readback_reduce.c
    tests/oracle/gpu_host_confirm.c
    tests/oracle/cpu_packing_reference.c
    tests/oracle/packing_enumerator_cpu.c
)

# Buildup

list(APPEND CLEARRA_CORE_SOURCES
    src/buildup/generic_buildup.c
    src/buildup/buildup_memo.c
    src/buildup/buildup_completion_memo.c
    src/buildup/buildup_search.c
    src/buildup/buildup_operation_source.c
    src/buildup/buildup_operation_variants.c
    src/buildup/buildup_operation_variant_cache.c
    src/buildup/buildup_geometry_transition.c
    src/buildup/buildup_geometry_transition_cache.c
    src/buildup/buildup_geometry_dag.c
    src/buildup/realization_domain_propagation.c
    src/buildup/realization_feasibility.c
    src/buildup/buildup_reachability_result.c
    src/buildup/buildup_reachability_cache.c
    src/buildup/buildup_reachable_lock_cache.c
    src/buildup/buildup_trace.c
    src/buildup/buildup_worker.c
    src/buildup/buildup_workspace.c
    src/buildup/buildup_order_dag.c
    src/buildup/y_adjustment.c
    src/buildup/hold_automaton_bridge.c
    src/buildup/grounded_filter.c
    src/buildup/reachability_bridge.c
    src/buildup/hold_queue_verifier.c
    src/buildup/build_variant_buffer.c
)

# Coverage

list(APPEND CLEARRA_CORE_SOURCES
    src/coverage/pattern_bitset_c.c
    src/coverage/coverage_row_builder.c
    src/coverage/coverage_union.c
    src/coverage/coverage_overlap.c
)

# Scoring Events

list(APPEND CLEARRA_CORE_SOURCES
    src/scoring_events/scoring_events.c
)
