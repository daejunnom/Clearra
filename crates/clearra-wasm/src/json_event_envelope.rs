use std::fmt::Write;

use clearra_host_contract::{
    AppResponse, AppStatus, BackendReport, CapabilityReport, ContinuationReport, Diagnostic,
    DiagnosticReport, ResourceReport,
};

use crate::{
    BackendStatus, BudgetStatus, JobProgress, MemoryStatus, WasmCommandRuntimeError,
    WasmSearchReport, WasmWorkerJobEvent, WebGpuBackendOutcomeState, WebGpuBackendReport,
    WebGpuLimitsReport, WebGpuMemoryReport, WebGpuReportTrustState, WebGpuShaderReport,
};

pub(crate) fn serialize_worker_events(
    events: &[WasmWorkerJobEvent],
) -> Result<String, WasmCommandRuntimeError> {
    let mut output = String::with_capacity(events.len().saturating_mul(256));
    output.push('[');
    for (index, event) in events.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let mut object = JsonObject::begin(&mut output);
        object.number("schema_version", 1);
        object.string("runtime", "clearra-wasm");
        write_event(&mut object, event);
        object.finish();
    }
    output.push(']');
    Ok(output)
}

fn write_event(object: &mut JsonObject<'_>, event: &WasmWorkerJobEvent) {
    match event {
        WasmWorkerJobEvent::Started { job_id } => {
            object.string("event", "started");
            object.number("job_id", job_id.get());
        }
        WasmWorkerJobEvent::Progress { job_id, progress } => {
            object.string("event", "progress");
            object.number("job_id", job_id.get());
            object.object("progress", |nested| write_progress(nested, progress));
        }
        WasmWorkerJobEvent::Diagnostic { job_id, diagnostic } => {
            object.string("event", "diagnostic");
            object.number("job_id", job_id.get());
            object.object("diagnostic", |nested| write_diagnostic(nested, diagnostic));
        }
        WasmWorkerJobEvent::PartialResult {
            job_id,
            partial,
            label,
            final_result,
        } => {
            object.string("event", "partial_result");
            object.number("job_id", job_id.get());
            object.boolean("partial", *partial);
            object.string("label", label);
            object.boolean("final_result", *final_result);
        }
        WasmWorkerJobEvent::FinalResponse {
            job_id,
            response,
            webgpu_backend,
            search_report,
        } => {
            object.string("event", "final_response");
            object.number("job_id", job_id.get());
            object.object("response", |nested| write_app_response(nested, response));
            object.object("webgpu_backend", |nested| {
                write_webgpu_report(nested, webgpu_backend)
            });
            object.optional_object("search_report", search_report.as_ref(), |nested, report| {
                write_search_report(nested, report)
            });
        }
        WasmWorkerJobEvent::Failed {
            job_id,
            diagnostics,
        } => {
            object.string("event", "failed");
            object.number("job_id", job_id.get());
            object.object("diagnostics", |nested| {
                write_diagnostic_report(nested, diagnostics)
            });
        }
        WasmWorkerJobEvent::Cancelled {
            job_id,
            scope_released,
        } => {
            object.string("event", "cancelled");
            object.number("job_id", job_id.get());
            object.boolean("scope_released", *scope_released);
        }
    }
}

fn write_progress(object: &mut JsonObject<'_>, progress: &JobProgress) {
    object.number("done", progress.done);
    object.number("total", progress.total);
    object.string("label", &progress.label);
    object.object("budget_status", |nested| {
        write_budget_status(nested, &progress.budget_status)
    });
    object.object("backend_status", |nested| {
        write_backend_status(nested, &progress.backend_status)
    });
    object.object("memory_status", |nested| {
        write_memory_status(nested, &progress.memory_status)
    });
}

fn write_budget_status(object: &mut JsonObject<'_>, status: &BudgetStatus) {
    object.string("state", &status.state);
    object.number("used", status.used);
    object.optional_number("limit", status.limit);
}

fn write_backend_status(object: &mut JsonObject<'_>, status: &BackendStatus) {
    object.string("backend_requested", &status.backend_requested);
    object.string("backend_selected", &status.backend_selected);
    object.boolean("fallback_used", status.fallback_used);
    object.optional_string("fallback_reason", status.fallback_reason.as_deref());
}

fn write_memory_status(object: &mut JsonObject<'_>, status: &MemoryStatus) {
    object.string("state", &status.state);
    object.boolean("raw_pointer_exposed", status.raw_pointer_exposed);
}

fn write_app_response(object: &mut JsonObject<'_>, response: &AppResponse) {
    object.optional_string(
        "command",
        response.command().map(|command| command.as_str()),
    );
    object.string(
        "status",
        match response.status() {
            AppStatus::Success => "success",
            AppStatus::ValidationFailed => "validation-failed",
            AppStatus::Unsupported => "unsupported",
            AppStatus::ExecutionFailed => "execution-failed",
        },
    );
    object.optional_object("result", response.result(), |nested, result| {
        nested.string("kind", result.kind())
    });
    object.array("diagnostics", |output| {
        write_object_array(output, response.diagnostics(), write_diagnostic)
    });
    object.object("backend_report", |nested| {
        write_backend_report(nested, response.backend_report())
    });
    object.object("resource_report", |nested| {
        write_resource_report(nested, response.resource_report())
    });
    object.object("capability_report", |nested| {
        write_capability_report(nested, response.capability_report())
    });
    object.optional_object(
        "continuation",
        response.continuation(),
        write_continuation_report,
    );
}

fn write_diagnostic(object: &mut JsonObject<'_>, diagnostic: &Diagnostic) {
    object.string("code", diagnostic.code());
    object.string("severity", diagnostic.severity());
    object.string("message", diagnostic.message());
}

fn write_diagnostic_report(object: &mut JsonObject<'_>, report: &DiagnosticReport) {
    object.array("diagnostics", |output| {
        write_object_array(output, report.diagnostics(), write_diagnostic)
    });
}

fn write_backend_report(object: &mut JsonObject<'_>, report: &BackendReport) {
    object.string("backend_requested", report.backend_requested());
    object.string("backend_selected", report.backend_selected());
    object.boolean("fallback_used", report.fallback_used());
    object.optional_string("fallback_reason", report.fallback_reason());
    object.optional_string("backend_fallback_reason", report.backend_fallback_reason());
    object.optional_string("fallback_backend", report.fallback_backend());
    object.optional_string("gpu_failure_class", report.gpu_failure_class());
    object.optional_string("gpu_failure_stage", report.gpu_failure_stage());
    object.boolean(
        "discarded_partial_gpu_result",
        report.discarded_partial_gpu_result(),
    );
    object.optional_string("gpu_device_requested", report.gpu_device_requested());
    object.optional_number(
        "gpu_device_selected_index",
        report.gpu_device_selected_index(),
    );
    object.optional_string(
        "gpu_device_selected_name",
        report.gpu_device_selected_name(),
    );
    object.optional_string(
        "gpu_device_selected_type",
        report.gpu_device_selected_type(),
    );
    object.optional_string(
        "gpu_device_selected_backend",
        report.gpu_device_selected_backend(),
    );
}

fn write_resource_report(object: &mut JsonObject<'_>, report: &ResourceReport) {
    object.boolean("solver_executed", report.solver_executed());
    object.string("memory_status", report.memory_status());
    object.boolean("truncated", report.truncated);
    object.optional_string("truncation_reason", report.truncation_reason.as_deref());
    object.number("peak_frontier_states", report.peak_frontier_states);
    object.number("peak_candidate_rows", report.peak_candidate_rows);
    object.number("peak_hash_buckets", report.peak_hash_buckets);
    object.number("peak_gpu_bytes", report.peak_gpu_bytes);
    object.number("peak_cpu_bytes", report.peak_cpu_bytes);
    object.number(
        "build_worker_backlog_peak",
        report.build_worker_backlog_peak,
    );
    object.number("coverage_rows_emitted", report.coverage_rows_emitted);
    object.boolean("probability_complete", report.probability_complete);
}

fn write_capability_report(object: &mut JsonObject<'_>, report: &CapabilityReport) {
    object.string("app_request_boundary", report.app_request_boundary());
    object.string("executor_boundary", report.executor_boundary());
    object.optional_object(
        "render_capability",
        report.render_capability(),
        |nested, render| {
            nested.boolean("png_supported", render.png_supported());
            nested.boolean("gif_supported", render.gif_supported());
            nested.boolean("render_exact", render.render_exact());
            nested.optional_string("unsupported_reason", render.unsupported_reason());
        },
    );
}

fn write_continuation_report(object: &mut JsonObject<'_>, report: &ContinuationReport) {
    object.boolean("available", report.available());
    object.optional_string("token", report.token());
}

fn write_webgpu_report(object: &mut JsonObject<'_>, report: &WebGpuBackendReport) {
    object.string(
        "outcome_state",
        match report.outcome_state {
            WebGpuBackendOutcomeState::NotRequested => "NotRequested",
            WebGpuBackendOutcomeState::Connected => "Connected",
            WebGpuBackendOutcomeState::Unavailable => "Unavailable",
        },
    );
    object.boolean("webgpu_available", report.webgpu_available);
    object.string(
        "webgpu_adapter_label_or_redacted",
        &report.webgpu_adapter_label_or_redacted,
    );
    object.object("webgpu_limits", |nested| {
        write_webgpu_limits(nested, &report.webgpu_limits)
    });
    object.object("webgpu_required_limits", |nested| {
        write_webgpu_limits(nested, &report.webgpu_required_limits)
    });
    object.optional_string(
        "webgpu_unavailable_reason",
        report.webgpu_unavailable_reason.as_deref(),
    );
    object.optional_string("expected_digest", report.expected_digest.as_deref());
    object.optional_string("actual_digest", report.actual_digest.as_deref());
    object.object("shader", |nested| {
        write_webgpu_shader(nested, &report.shader)
    });
    object.object("memory", |nested| {
        write_webgpu_memory(nested, &report.memory)
    });
    object.boolean("fallback_used", report.fallback_used);
    object.optional_string("fallback_backend", report.fallback_backend.as_deref());
    object.boolean("gpu_warmup_requested", report.gpu_warmup_requested);
    object.boolean("gpu_warmup_performed", report.gpu_warmup_performed);
    object.boolean("gpu_session_reused", report.gpu_session_reused);
    object.string(
        "gpu_trust_state",
        match report.gpu_trust_state {
            WebGpuReportTrustState::NotUsed => "NotUsed",
            WebGpuReportTrustState::TrustedCpuSampleConfirmed => "TrustedCpuSampleConfirmed",
            WebGpuReportTrustState::Unavailable => "Unavailable",
        },
    );
    object.boolean("cpu_confirmed", report.cpu_confirmed);
    object.boolean(
        "can_source_exact_probability",
        report.can_source_exact_probability,
    );
}

fn write_webgpu_limits(object: &mut JsonObject<'_>, report: &WebGpuLimitsReport) {
    object.number(
        "max_storage_buffer_binding_size",
        report.max_storage_buffer_binding_size,
    );
    object.number(
        "max_compute_workgroup_storage_size",
        report.max_compute_workgroup_storage_size,
    );
    object.number(
        "max_compute_invocations_per_workgroup",
        report.max_compute_invocations_per_workgroup,
    );
}

fn write_webgpu_shader(object: &mut JsonObject<'_>, report: &WebGpuShaderReport) {
    object.string("shader_compile_status", &report.shader_compile_status);
    object.optional_string("shader_hash", report.shader_hash.as_deref());
    object.optional_string("shader_version", report.shader_version.as_deref());
    object.boolean("embedded_reviewed", report.embedded_reviewed);
    object.boolean("user_shader_allowed", report.user_shader_allowed);
    object.boolean(
        "runtime_shader_injection_allowed",
        report.runtime_shader_injection_allowed,
    );
}

fn write_webgpu_memory(object: &mut JsonObject<'_>, report: &WebGpuMemoryReport) {
    object.string("wasm_memory_usage", &report.wasm_memory_usage);
    object.string("wasm_memory_pressure", &report.wasm_memory_pressure);
}

fn write_search_report(object: &mut JsonObject<'_>, report: &WasmSearchReport) {
    object.string("backend_selected", &report.backend_selected);
    object.number("workers_used", report.workers_used);
    object.boolean("cpu_parallel_execution", report.cpu_parallel_execution);
    object.string(
        "cpu_parallel_decision_reason",
        &report.cpu_parallel_decision_reason,
    );
    object.boolean("cpu_warmup_requested", report.cpu_warmup_requested);
    object.boolean("cpu_warmup_performed", report.cpu_warmup_performed);
    object.string("supply_window_resolution", &report.supply_window_resolution);
    object.boolean(
        "projects_unplaced_lookahead",
        report.projects_unplaced_lookahead,
    );
    object.number("source_sequence_length", report.source_sequence_length);
    object.string(
        "total_possible_pattern_count",
        &report.total_possible_pattern_count,
    );
    object.boolean("solution_found", report.solution_found);
    object.number("packing_candidate_count", report.packing_candidate_count);
    object.string(
        "geometry_candidate_family_count",
        &report.geometry_candidate_family_count,
    );
    object.string(
        "packing_candidate_set_digest",
        &report.packing_candidate_set_digest,
    );
    object.array("packing_candidate_keys", |output| {
        write_string_array(output, &report.packing_candidate_keys)
    });
    object.number("unique_solution_count", report.unique_solution_count);
    object.string(
        "normalized_solution_set_hash",
        &report.normalized_solution_set_hash,
    );
    object.array("normalized_solution_keys", |output| {
        write_string_array(output, &report.normalized_solution_keys)
    });
    object.array("solution_probabilities", |output| {
        output.push('[');
        for (index, entry) in report.solution_probabilities.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("solution_key", &entry.solution_key);
            nested.string("probability", &entry.probability);
            nested.number("covered_pattern_count", entry.covered_pattern_count);
            nested.number("pattern_count", entry.pattern_count);
            nested.boolean("probability_complete", entry.probability_complete);
            nested.finish();
        }
        output.push(']');
    });
    object.number("build_variant_count", report.build_variant_count);
    object.string(
        "build_variant_count_exact",
        &report.build_variant_count_exact,
    );
    object.number(
        "materialized_pattern_count",
        report.materialized_pattern_count,
    );
    object.number("covered_pattern_count", report.covered_pattern_count);
    object.string("coverage_probability", &report.coverage_probability);
    object.boolean("probability_complete", report.probability_complete);
    object.boolean("count_complete", report.count_complete);
    object.number("searched_nodes", report.searched_nodes);
    object.number(
        "geometry_domain_pruned_states",
        report.geometry_domain_pruned_states,
    );
    object.number(
        "geometry_hall_pruned_states",
        report.geometry_hall_pruned_states,
    );
    object.number(
        "geometry_column_pruned_states",
        report.geometry_column_pruned_states,
    );
    object.number(
        "geometry_component_compositions",
        report.geometry_component_compositions,
    );
    object.number("peak_frontier_states", report.peak_frontier_states);
    object.number("peak_cpu_bytes", report.peak_cpu_bytes);
    object.number("peak_build_order_nodes", report.peak_build_order_nodes);
    object.number("total_build_order_nodes", report.total_build_order_nodes);
    object.number("coverage_product_words", report.coverage_product_words);
    object.number("coverage_product_states", report.coverage_product_states);
    object.number(
        "coverage_product_edge_checks",
        report.coverage_product_edge_checks,
    );
    object.number(
        "piece_language_coverage_cache_hits",
        report.piece_language_coverage_cache_hits,
    );
    object.number(
        "piece_language_coverage_cache_misses",
        report.piece_language_coverage_cache_misses,
    );
    object.number(
        "standard_bag_symbolic_cache_hits",
        report.standard_bag_symbolic_cache_hits,
    );
    object.number(
        "standard_bag_symbolic_cache_misses",
        report.standard_bag_symbolic_cache_misses,
    );
    object.number("peak_reachability_states", report.peak_reachability_states);
    object.number(
        "total_reachability_states",
        report.total_reachability_states,
    );
    object.number(
        "reachability_lock_queries",
        report.reachability_lock_queries,
    );
    object.number(
        "reachability_harddrop_queries",
        report.reachability_harddrop_queries,
    );
    object.number(
        "reachability_harddrop_hits",
        report.reachability_harddrop_hits,
    );
    object.number(
        "reachability_cache_reachable_hits",
        report.reachability_cache_reachable_hits,
    );
    object.number(
        "reachability_cache_unreachable_hits",
        report.reachability_cache_unreachable_hits,
    );
    object.number(
        "reachability_cache_key_misses",
        report.reachability_cache_key_misses,
    );
    object.number(
        "reachability_partial_searches",
        report.reachability_partial_searches,
    );
    object.number(
        "reachability_exhaustive_searches",
        report.reachability_exhaustive_searches,
    );
    object.number(
        "realization_feasibility_states",
        report.realization_feasibility_states,
    );
    object.number(
        "realization_feasibility_rejected_candidates",
        report.realization_feasibility_rejected_candidates,
    );
    object.boolean("resource_truncated", report.resource_truncated);
    object.string(
        "resource_truncation_reason",
        &report.resource_truncation_reason,
    );
    object.optional_string(
        "representative_candidate_id",
        report.representative_candidate_id.as_deref(),
    );
    object.optional_number(
        "representative_pattern_id",
        report.representative_pattern_id,
    );
    object.array("representative_path", |output| {
        output.push('[');
        for (index, step) in report.representative_path.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("piece", &step.piece);
            nested.number("rotation", step.rotation);
            nested.number("x", step.x);
            nested.number("y", step.y);
            nested.string("hold", &step.hold);
            nested.number("cleared_lines", step.cleared_lines);
            nested.finish();
        }
        output.push(']');
    });
    object.array("summary_fields", |output| {
        output.push('[');
        for (index, (key, value)) in report.summary_fields.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push('[');
            write_json_string(output, key);
            output.push(',');
            write_json_string(output, value);
            output.push(']');
        }
        output.push(']');
    });
    object.optional_string("forward_search_kind", report.forward_search_kind.as_deref());
    object.optional_string(
        "forward_initial_board_mask",
        report.forward_initial_board_mask.as_deref(),
    );
    object.optional_number("maximum_damage", report.maximum_damage);
    object.array("forward_outcomes", |output| {
        output.push('[');
        for (index, outcome) in report.forward_outcomes.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let mut nested = JsonObject::begin(output);
            nested.string("id", &outcome.id);
            nested.number("source_pattern_index", outcome.source_pattern_index);
            nested.string("source_queue", &outcome.source_queue);
            nested.optional_string("group", outcome.group.as_deref());
            nested.string("final_board_mask", &outcome.final_board_mask);
            nested.optional_string("spin_piece", outcome.spin_piece.as_deref());
            nested.boolean("spin_mini", outcome.spin_mini);
            nested.number("spin_lines", outcome.spin_lines);
            nested.number("total_damage", outcome.total_damage);
            nested.array("path", |output| {
                output.push('[');
                for (step_index, step) in outcome.path.iter().enumerate() {
                    if step_index != 0 {
                        output.push(',');
                    }
                    let mut step_object = JsonObject::begin(output);
                    step_object.string("piece", &step.piece);
                    step_object.number("rotation", step.rotation);
                    step_object.number("x", step.x);
                    step_object.number("y", step.y);
                    step_object.string("hold", &step.hold);
                    step_object.number("cleared_lines", step.cleared_lines);
                    step_object.optional_string("spin_piece", step.spin_piece.as_deref());
                    step_object.boolean("spin_mini", step.spin_mini);
                    step_object.number("damage", step.damage);
                    step_object.number("total_damage", step.total_damage);
                    step_object.string("placement_mask", &step.placement_mask);
                    step_object.number("cleared_row_mask", step.cleared_row_mask);
                    step_object.string("board_after_mask", &step.board_after_mask);
                    step_object.finish();
                }
                output.push(']');
            });
            nested.finish();
        }
        output.push(']');
    });
    object.optional_object(
        "setup_report",
        report.setup_report.as_ref(),
        |nested, setup| {
            nested.string("search_mode", &setup.search_mode);
            nested.number("cycle", setup.cycle);
            nested.string("remaining_pieces", &setup.remaining_pieces);
            nested.string("queue_based_pieces", &setup.queue_based_pieces);
            nested.string(
                "next_cycle_remaining_pieces",
                &setup.next_cycle_remaining_pieces,
            );
            nested.boolean("post_cycle_borrow_enabled", setup.post_cycle_borrow_enabled);
            nested.string("coverage_semantics", &setup.coverage_semantics);
            nested.string("geometry_family_count", &setup.geometry_family_count);
            nested.number("partial_build_node_count", setup.partial_build_node_count);
            nested.boolean("complete", setup.complete);
            nested.array("hold_conditions", |output| {
                output.push('[');
                for (condition_index, condition) in setup.hold_conditions.iter().enumerate() {
                    if condition_index != 0 {
                        output.push(',');
                    }
                    let mut condition_object = JsonObject::begin(output);
                    condition_object.string("condition_id", &condition.condition_id);
                    condition_object
                        .optional_string("initial_hold", condition.initial_hold.as_deref());
                    condition_object.string("pattern_expression", &condition.pattern_expression);
                    condition_object.number("pattern_count", condition.pattern_count);
                    condition_object.number("candidate_count", condition.candidate_count);
                    condition_object.boolean("result_truncated", condition.result_truncated);
                    condition_object.boolean("complete", condition.complete);
                    condition_object.array("candidates", |output| {
                        output.push('[');
                        for (candidate_index, candidate) in condition.candidates.iter().enumerate()
                        {
                            if candidate_index != 0 {
                                output.push(',');
                            }
                            let mut candidate_object = JsonObject::begin(output);
                            candidate_object.string("setup_id", &candidate.setup_id);
                            candidate_object.string("board_mask", &candidate.board_mask);
                            candidate_object.number("min_locks", candidate.min_locks);
                            candidate_object.number("max_locks", candidate.max_locks);
                            candidate_object
                                .number("build_covered_patterns", candidate.build_covered_patterns);
                            candidate_object
                                .number("joint_covered_patterns", candidate.joint_covered_patterns);
                            candidate_object
                                .string("build_probability", &candidate.build_probability);
                            candidate_object
                                .string("joint_probability", &candidate.joint_probability);
                            candidate_object.string(
                                "conditional_pc_probability",
                                &candidate.conditional_pc_probability,
                            );
                            candidate_object.array("representative_path", |output| {
                                output.push('[');
                                for (step_index, step) in
                                    candidate.representative_path.iter().enumerate()
                                {
                                    if step_index != 0 {
                                        output.push(',');
                                    }
                                    let mut step_object = JsonObject::begin(output);
                                    step_object.string("piece", &step.piece);
                                    step_object.number("rotation", step.rotation);
                                    step_object.number("x", step.x);
                                    step_object.number("y", step.y);
                                    step_object.string("hold", &step.hold);
                                    step_object.number("cleared_lines", step.cleared_lines);
                                    step_object.finish();
                                }
                                output.push(']');
                            });
                            if candidate.solution_paths_complete {
                                candidate_object
                                    .number("solution_path_count", candidate.solution_path_count);
                                candidate_object.boolean("solution_paths_complete", true);
                                candidate_object.array("solution_paths", |output| {
                                    output.push('[');
                                    for (path_index, path) in
                                        candidate.solution_paths.iter().enumerate()
                                    {
                                        if path_index != 0 {
                                            output.push(',');
                                        }
                                        output.push('[');
                                        for (step_index, step) in path.iter().enumerate() {
                                            if step_index != 0 {
                                                output.push(',');
                                            }
                                            let mut step_object = JsonObject::begin(output);
                                            step_object.string("piece", &step.piece);
                                            step_object.number("rotation", step.rotation);
                                            step_object.number("x", step.x);
                                            step_object.number("y", step.y);
                                            step_object.string("hold", &step.hold);
                                            step_object.number("cleared_lines", step.cleared_lines);
                                            step_object.finish();
                                        }
                                        output.push(']');
                                    }
                                    output.push(']');
                                });
                            }
                            candidate_object.finish();
                        }
                        output.push(']');
                    });
                    condition_object.finish();
                }
                output.push(']');
            });
        },
    );
}

fn write_object_array<T>(
    output: &mut String,
    values: &[T],
    write_value: fn(&mut JsonObject<'_>, &T),
) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        let mut nested = JsonObject::begin(output);
        write_value(&mut nested, value);
        nested.finish();
    }
    output.push(']');
}

fn write_string_array(output: &mut String, values: &[String]) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        write_json_string(output, value);
    }
    output.push(']');
}

struct JsonObject<'a> {
    output: &'a mut String,
    first: bool,
}

impl<'a> JsonObject<'a> {
    fn begin(output: &'a mut String) -> Self {
        output.push('{');
        Self {
            output,
            first: true,
        }
    }

    fn finish(self) {
        self.output.push('}');
    }

    fn key(&mut self, key: &str) {
        if !self.first {
            self.output.push(',');
        }
        self.first = false;
        write_json_string(self.output, key);
        self.output.push(':');
    }

    fn string(&mut self, key: &str, value: &str) {
        self.key(key);
        write_json_string(self.output, value);
    }

    fn optional_string(&mut self, key: &str, value: Option<&str>) {
        self.key(key);
        if let Some(value) = value {
            write_json_string(self.output, value);
        } else {
            self.output.push_str("null");
        }
    }

    fn number(&mut self, key: &str, value: impl std::fmt::Display) {
        self.key(key);
        write!(self.output, "{value}").expect("String writes cannot fail");
    }

    fn optional_number(&mut self, key: &str, value: Option<impl std::fmt::Display>) {
        self.key(key);
        if let Some(value) = value {
            write!(self.output, "{value}").expect("String writes cannot fail");
        } else {
            self.output.push_str("null");
        }
    }

    fn boolean(&mut self, key: &str, value: bool) {
        self.key(key);
        self.output.push_str(if value { "true" } else { "false" });
    }

    fn object(&mut self, key: &str, write_value: impl FnOnce(&mut JsonObject<'_>)) {
        self.key(key);
        let mut nested = JsonObject::begin(self.output);
        write_value(&mut nested);
        nested.finish();
    }

    fn optional_object<T>(
        &mut self,
        key: &str,
        value: Option<&T>,
        write_value: impl FnOnce(&mut JsonObject<'_>, &T),
    ) {
        self.key(key);
        if let Some(value) = value {
            let mut nested = JsonObject::begin(self.output);
            write_value(&mut nested, value);
            nested.finish();
        } else {
            self.output.push_str("null");
        }
    }

    fn array(&mut self, key: &str, write_value: impl FnOnce(&mut String)) {
        self.key(key);
        write_value(self.output);
    }
}

fn write_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' => {
                write!(output, "\\u{:04x}", control as u32).expect("String writes cannot fail");
            }
            other => output.push(other),
        }
    }
    output.push('"');
}
