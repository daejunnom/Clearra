use super::*;

mod case_pc_contract_exposes_backend_execution_report_fields {
    use super::*;

    #[test]
    fn pc_contract_exposes_backend_execution_report_fields() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[
                RenderField::new("requested_backend", RenderFieldValue::string("gpu")),
                RenderField::new(
                    "selected_backend",
                    RenderFieldValue::string("cpu-geometry-exact-cover"),
                ),
                RenderField::new(
                    "selected_model",
                    RenderFieldValue::string("bitset-algorithm-x"),
                ),
                RenderField::new("compute_device", RenderFieldValue::string("cpu")),
                RenderField::new(
                    "search_result_model",
                    RenderFieldValue::string("geometry-candidate-set"),
                ),
                RenderField::new(
                    "backend_selection_reason",
                    RenderFieldValue::string("explicit-fallback-to-cpu-geometry-exact-cover"),
                ),
                RenderField::new("backend_fallback_used", RenderFieldValue::bool(true)),
                RenderField::new("workers_requested", RenderFieldValue::string("auto")),
                RenderField::new("workers_used", RenderFieldValue::number("1")),
                RenderField::new("execution_deterministic", RenderFieldValue::bool(true)),
                RenderField::new("solution_trace_mode", RenderFieldValue::string("none")),
                RenderField::new("state_count_available", RenderFieldValue::bool(false)),
                RenderField::new(
                    "multiplicity_count_available",
                    RenderFieldValue::bool(false),
                ),
                RenderField::new(
                    "backend_fallback_reason",
                    RenderFieldValue::string("gpu_feature_disabled"),
                ),
                RenderField::new(
                    "gpu_unavailable_reason",
                    RenderFieldValue::string("gpu_feature_disabled"),
                ),
                RenderField::new("gpu_device", RenderFieldValue::string("auto")),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let search = object_member(pc, "search");
        let backend = object_member(pc, "backend");
        let gpu = object_member(backend, "gpu");
        let counts = object_member(pc, "counts");
        let trace = object_member(pc, "trace");
        assert_eq!(
            member_value(search, "requested_backend"),
            &JsonValue::string("gpu")
        );
        assert_eq!(
            member_value(search, "selected_backend"),
            &JsonValue::string("cpu-geometry-exact-cover")
        );
        assert_eq!(
            member_value(search, "backend_fallback_reason"),
            &JsonValue::string("gpu_feature_disabled")
        );
        assert_eq!(
            member_value(search, "gpu_unavailable_reason"),
            &JsonValue::string("gpu_feature_disabled")
        );
        assert_eq!(
            member_value(search, "search_result_model"),
            &JsonValue::string("geometry-candidate-set")
        );
        assert_eq!(
            member_value(search, "backend_selection_reason"),
            &JsonValue::string("explicit-fallback-to-cpu-geometry-exact-cover")
        );
        assert_eq!(
            member_value(backend, "requested"),
            &JsonValue::string("gpu")
        );
        assert_eq!(
            member_value(backend, "selected"),
            &JsonValue::string("cpu-geometry-exact-cover")
        );
        assert_eq!(member_value(backend, "compute"), &JsonValue::string("cpu"));
        assert_eq!(
            member_value(backend, "traversal"),
            &JsonValue::string("bitset-algorithm-x")
        );
        assert_eq!(
            member_value(backend, "selection_reason"),
            &JsonValue::string("explicit-fallback-to-cpu-geometry-exact-cover")
        );
        assert_eq!(
            member_value(backend, "fallback_used"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(backend, "fallback_reason"),
            &JsonValue::string("gpu_feature_disabled")
        );
        assert_eq!(member_value(backend, "workers_requested"), &JsonValue::Null);
        assert_eq!(
            member_value(backend, "workers_used"),
            &JsonValue::number("1")
        );
        assert_eq!(
            member_value(backend, "deterministic"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(gpu, "device_selected"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(gpu, "device_label"),
            &JsonValue::string("auto")
        );
        assert_eq!(member_value(gpu, "backend"), &JsonValue::Null);
        assert_eq!(
            member_value(gpu, "unavailable_reason"),
            &JsonValue::string("gpu_feature_disabled")
        );
        assert_eq!(
            member_value(trace, "solution_trace_mode"),
            &JsonValue::string("none")
        );
        assert_eq!(
            member_value(counts, "state_count_available"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(counts, "multiplicity_count_available"),
            &JsonValue::Bool(false)
        );
    }
}

mod case_pc_contract_exposes_r11_result_views_without_collapsing_counts {
    use super::*;

    #[test]
    fn pc_contract_exposes_r11_result_views_without_collapsing_counts() {
        let contract = JsonContract::from_render_message(
            "pc-scenario",
            &[
                RenderField::new(
                    "search_execution_report",
                    RenderFieldValue::string("attached"),
                ),
                RenderField::new("backend_report", RenderFieldValue::string("attached")),
                RenderField::new("backend_requested", RenderFieldValue::string("auto")),
                RenderField::new(
                    "backend_selected",
                    RenderFieldValue::string("cpu-geometry-exact-cover"),
                ),
                RenderField::new("packing_result", RenderFieldValue::string("core-c")),
                RenderField::new("packing_candidate_view", RenderFieldValue::string("core-c")),
                RenderField::new("packing_candidate_count", RenderFieldValue::number("9")),
                RenderField::new("buildup_result", RenderFieldValue::string("core-c")),
                RenderField::new("coverage_result", RenderFieldValue::string("coverage")),
                RenderField::new("coverage_row_view", RenderFieldValue::string("row")),
                RenderField::new("coverage_row_count", RenderFieldValue::number("2")),
                RenderField::new("coverage_pattern_count", RenderFieldValue::number("4")),
                RenderField::new("covered_pattern_count", RenderFieldValue::number("3")),
                RenderField::new("coverage_probability", RenderFieldValue::number("0.75")),
                RenderField::new("probability_complete", RenderFieldValue::Bool(true)),
                RenderField::new("objective_result", RenderFieldValue::string("objective")),
                RenderField::new("replay_trace", RenderFieldValue::string("replay")),
                RenderField::new(
                    "exact_target_policy",
                    RenderFieldValue::string("2L-label-clear-to-empty"),
                ),
                RenderField::new("chain_labels", RenderFieldValue::string("2L")),
                RenderField::new(
                    "checkpoint_results",
                    RenderFieldValue::string("not-executed-label-metadata"),
                ),
                RenderField::new("solution_found", RenderFieldValue::Bool(true)),
                RenderField::new("total_solution_count", RenderFieldValue::number("12")),
                RenderField::new("unique_solution_count", RenderFieldValue::number("8")),
                RenderField::new("retained_trace_count", RenderFieldValue::number("2")),
                RenderField::new("count_complete", RenderFieldValue::Bool(true)),
                RenderField::new("trace_retention_truncated", RenderFieldValue::Bool(true)),
                RenderField::new(
                    "trace_retention_reason",
                    RenderFieldValue::string("retained_trace_limit"),
                ),
                RenderField::new("continuation_available", RenderFieldValue::Bool(true)),
                RenderField::new("continuation_token", RenderFieldValue::string("pc2:test")),
                RenderField::new(
                    "continue_hint",
                    RenderFieldValue::string("clearra continue pc2:test"),
                ),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let execution_report = object_member(pc, "execution_report");
        let search = object_member(pc, "search");
        let packing = object_member(execution_report, "packing");
        let buildup = object_member(execution_report, "buildup");
        let coverage = object_member(pc, "coverage");
        let trace = object_member(pc, "trace");
        let backend = object_member(pc, "backend");
        let remaining = object_member(pc, "remaining");
        let checkpoint_schedule = object_member(pc, "checkpoint_schedule");

        assert_eq!(
            member_value(search, "exact_target_policy"),
            &JsonValue::string("2L-label-clear-to-empty")
        );
        assert_eq!(
            member_value(search, "chain_labels"),
            &JsonValue::string("2L")
        );
        assert_eq!(
            member_value(search, "checkpoint_results"),
            &JsonValue::string("not-executed-label-metadata")
        );
        assert_eq!(
            member_value(checkpoint_schedule, "checkpoint_results"),
            &JsonValue::string("not-executed-label-metadata")
        );

        assert_eq!(
            member_value(packing, "packing_candidate_count"),
            &JsonValue::number("9")
        );
        assert_eq!(
            member_value(buildup, "total_solution_count"),
            &JsonValue::number("12")
        );
        assert_eq!(
            member_value(trace, "retained_trace_count"),
            &JsonValue::number("2")
        );
        assert_eq!(
            member_value(trace, "trace_retention_truncated"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(trace, "retained_trace_count"),
            &JsonValue::number("2")
        );
        assert_ne!(
            member_value(trace, "retained_trace_count"),
            member_value(buildup, "total_solution_count")
        );
        assert_eq!(
            member_value(coverage, "coverage_pattern_count"),
            &JsonValue::number("4")
        );
        assert_eq!(
            member_value(coverage, "covered_pattern_count"),
            &JsonValue::number("3")
        );
        assert_eq!(
            member_value(coverage, "coverage_probability"),
            &JsonValue::number("0.75")
        );
        assert_eq!(
            member_value(coverage, "probability_complete"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(backend, "requested"),
            &JsonValue::string("auto")
        );
        assert_eq!(
            member_value(backend, "selected"),
            &JsonValue::string("cpu-geometry-exact-cover")
        );
        assert_eq!(
            member_value(remaining, "continuation_available"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(remaining, "continuation_token"),
            &JsonValue::string("pc2:test")
        );
        assert_eq!(
            member_value(remaining, "continue_hint"),
            &JsonValue::string("clearra continue pc2:test")
        );
    }
}

mod case_pc_contract_exposes_backend_and_memory_reports {
    use super::*;

    #[test]
    fn pc_contract_exposes_backend_and_memory_reports() {
        let contract = JsonContract::from_render_message(
            "pc",
            &[
                RenderField::new("backend_requested", RenderFieldValue::string("auto")),
                RenderField::new("backend_selected", RenderFieldValue::string("cpu")),
                RenderField::new("backend_fallback_reason", RenderFieldValue::string("none")),
                RenderField::new("gpu_trust_state", RenderFieldValue::string("not-used")),
                RenderField::new("cpu_confirm_required", RenderFieldValue::bool(false)),
                RenderField::new(
                    "deterministic_reference_matched",
                    RenderFieldValue::bool(false),
                ),
                RenderField::new("memory_leak_report_clean", RenderFieldValue::bool(true)),
                RenderField::new("live_scopes", RenderFieldValue::number("0")),
                RenderField::new("live_allocations", RenderFieldValue::number("0")),
                RenderField::new("live_gpu_buffers", RenderFieldValue::number("0")),
                RenderField::new("pending_release_queue", RenderFieldValue::number("0")),
                RenderField::new("pending_gpu_buffer_releases", RenderFieldValue::number("0")),
                RenderField::new("double_releases", RenderFieldValue::number("0")),
                RenderField::new("canary_failures", RenderFieldValue::number("0")),
                RenderField::new("poison_detections", RenderFieldValue::number("0")),
                RenderField::new("memory_pressure_level", RenderFieldValue::string("normal")),
            ],
        );

        let JsonValue::Object(root) = contract.root() else {
            panic!("root object");
        };
        let contract = object_member(&root, "contract");
        let pc = object_member(contract, "pc");
        let backend_report = object_member(pc, "backend_report");
        let memory_report = object_member(pc, "memory_report");

        assert_eq!(
            member_value(backend_report, "backend_requested"),
            &JsonValue::string("auto")
        );
        assert_eq!(
            member_value(backend_report, "backend_selected"),
            &JsonValue::string("cpu")
        );
        assert_eq!(
            member_value(backend_report, "gpu_trust_state"),
            &JsonValue::string("not-used")
        );
        assert_eq!(
            member_value(backend_report, "fallback_reason"),
            &JsonValue::Null
        );
        assert_eq!(
            member_value(backend_report, "cpu_confirm_required"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(backend_report, "deterministic_reference_matched"),
            &JsonValue::Bool(false)
        );
        assert_eq!(
            member_value(memory_report, "memory_leak_report_clean"),
            &JsonValue::Bool(true)
        );
        assert_eq!(
            member_value(memory_report, "live_gpu_buffers"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "pending_release_queue"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "pending_gpu_buffer_releases"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "double_releases"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "canary_failures"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "poison_detections"),
            &JsonValue::number("0")
        );
        assert_eq!(
            member_value(memory_report, "memory_pressure_level"),
            &JsonValue::string("normal")
        );
    }
}
