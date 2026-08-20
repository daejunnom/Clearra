fn option_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

mod opening_coverage_fields {
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;
    use clearra_problem::SearchProblem;
    use clearra_supply::bag::bag_boundary::BagBoundaryReport;

    use crate::{
        buildup::{
            buildup_coverage_bridge::{
                covered_pattern_count_basis_for_problem, verified_pattern_count_for_execution,
            },
            BuildUpRunResult,
        },
        packing::PackingRunResult,
        service::{field, pc_policy_labels::pattern_count},
    };

    pub(super) fn opening_coverage_fields(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        observed_supply_incomplete: bool,
    ) -> Vec<(String, String)> {
        let query = problem.core_query();
        let pattern_count = pattern_count(problem);
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("compiled PC problem has a materialized supply universe");
        let verified_pattern_count = verified_pattern_count_for_execution(
            problem,
            pattern_count,
            packing.count_complete() && buildup.coverage_complete(),
        )
        .unwrap_or(0);
        let covered_pattern_count = buildup.covered_pattern_count();
        let probability_calculated = problem.objective().kind() != ObjectiveKind::Tiling;
        vec![
            field("piece_source_id", problem.piece_source().id().get()),
            field("pattern_universe_id", universe.pattern_universe_id().get()),
            field(
                "pattern_weight_model_id",
                universe.pattern_weight_model_id().get(),
            ),
            field("supply_pattern_count", pattern_count),
            field("coverage_pattern_count", pattern_count),
            field("verified_pattern_count", verified_pattern_count),
            field("materialized_pattern_count", pattern_count),
            field(
                "covered_pattern_count_basis",
                covered_pattern_count_basis_for_problem(problem),
            ),
            field(
                "supply_total_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field("supply_covered_pattern_count", covered_pattern_count),
            field("covered_pattern_count", covered_pattern_count),
            field("supply_weighted_pattern_count", universe.weights().len()),
            field(
                "supply_materialized_probability_mass",
                universe.materialized_probability_mass().get(),
            ),
            field(
                "supply_probability_model",
                if observed_supply_incomplete {
                    "observed-expanded-visible-suffixes"
                } else {
                    "fixed-sequence"
                },
            ),
            field(
                "supply_probability_complete",
                probability_calculated
                    && !observed_supply_incomplete
                    && packing.count_complete()
                    && buildup.coverage_complete(),
            ),
            field(
                "probability_complete",
                probability_calculated
                    && !observed_supply_incomplete
                    && packing.count_complete()
                    && buildup.coverage_complete(),
            ),
            field("probability_calculated", probability_calculated),
            field("supply_expansion_truncated", observed_supply_incomplete),
            field(
                "supply_boundary_candidates",
                supply_boundary_candidate_count(query),
            ),
            field(
                "partitions",
                problem
                    .checkpoint_schedule()
                    .map(|schedule| schedule.partitions().len())
                    .unwrap_or(0),
            ),
            field(
                "checkpoints",
                problem
                    .checkpoint_schedule()
                    .map(|schedule| schedule.checkpoint_count())
                    .unwrap_or(0),
            ),
        ]
    }

    fn supply_boundary_candidate_count(
        query: &clearra_pc_graph::request::PcScenarioQuery,
    ) -> usize {
        match query.remaining_queue() {
            clearra_pc_graph::request::PcQueueInput::Observed(observed) => {
                BagBoundaryReport::analyze_observed_window(
                    observed.pieces(),
                    query.bag().bag_size(),
                )
                .candidates()
                .len()
            }
            clearra_pc_graph::request::PcQueueInput::PatternExpression(expression) => {
                expression.pattern_count()
            }
            clearra_pc_graph::request::PcQueueInput::FixedSequence(_)
            | clearra_pc_graph::request::PcQueueInput::BagAlignedPattern(_)
            | clearra_pc_graph::request::PcQueueInput::Standard7Bag => 1,
        }
    }
}
mod opening_metadata_fields {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_problem::SearchProblem;

    use super::option_usize;
    use crate::service::{field, pc_checkpoint_metadata::checkpoint_partition_labels};

    pub(super) fn opening_metadata_fields(
        problem: &SearchProblem,
        target: PcTarget,
    ) -> Vec<(String, String)> {
        vec![
            field("status", "searched"),
            field("execution_scope", "m17-core-executor"),
            field("executor_flow", "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel"),
            field("problem_layer", "clearra-problem"),
            field("problem_preset", "opening-pc"),
            field("problem_source", problem.scenario().source().as_str()),
            field("compiled_goal", problem.goal().as_str()),
            field("compiled_piece_window", problem.piece_window().max_pieces()),
            field("compiled_exact_pieces", option_usize(problem.exact_pieces())),
            field(
                "compiled_initial_board_mask",
                format!("0x{:016x}", problem.initial_board().occupied_mask()),
            ),
            field("labels", problem.labels().join(",")),
            field("chain_labels", problem.labels().join(",")),
            field("chain_class", problem.chain_class().as_str()),
            field(
                "exact_target_policy",
                format!("{}L-label-clear-to-empty", target.lines()),
            ),
            field("checkpoint_schedule_source", "clearra-pc-graph-labels"),
            field(
                "checkpoint_schedule_label",
                problem
                    .checkpoint_schedule()
                    .map(|schedule| schedule.label().to_owned())
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            field(
                "checkpoint_schedule_partitions",
                checkpoint_partition_labels(problem),
            ),
            field(
                "checkpoint_schedule_checkpoint_count",
                problem
                    .checkpoint_schedule()
                    .map(|schedule| schedule.checkpoint_count())
                    .unwrap_or(0),
            ),
            field("checkpoint_results", "not-executed-label-metadata"),
        ]
    }
}
mod opening_profile_fields {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_problem::SearchProblem;
    use clearra_rules::profile::rule_capability::RuleCapability;

    use crate::{
        buildup::BuildUpRunResult,
        packing::PackingRunResult,
        service::{
            field,
            pc_backend_report_adapter::solver_backend,
            pc_policy_labels::{objective_execution_name, objective_name},
        },
    };

    pub(super) fn opening_profile_fields(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        target: PcTarget,
    ) -> Vec<(String, String)> {
        let query = problem.core_query();
        let pc_query = problem
            .scenario()
            .pc_query()
            .expect("opening preset must preserve PcQuery metadata");
        let capability = RuleCapability::from_rule(query.rule());
        let objective = objective_name(pc_query.objective().kind());
        let objective_execution = objective_execution_name(pc_query.objective().kind());

        vec![
            field("lines", target.lines()),
            field("board_profile", pc_query.board().id().as_str()),
            field("piece_set_profile", query.piece_set().id().as_str()),
            field("bag_profile", query.bag().id().as_str()),
            field("rule_profile", query.rule().id().as_str()),
            field("effective_kick_model", capability.kick_model().as_str()),
            field("queue_mode", query.remaining_queue().mode()),
            field("queue_len", query.remaining_queue().len()),
            field("hold_enabled", query.allow_hold()),
            field("objective", objective),
            field("objective_execution", objective_execution),
            field("objective_search_mode", objective_execution),
            field("objective_applied", "true"),
            field("route", "search-problem-core-executor"),
            field("solver_backend", solver_backend(packing)),
            field(
                "state_count_available",
                packing.backend_report().state_count_available(),
            ),
            field(
                "multiplicity_count_available",
                packing.backend_report().multiplicity_count_available(),
            ),
            field("solution_found", buildup.solution_found()),
        ]
    }
}
mod opening_renderer {
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::BuildUpRunResult,
        core_execution_result::CoreExecutionResult,
        packing::PackingRunResult,
        service::{
            pc_backend_report_adapter::backend_fields,
            pc_continuation_fields::opening_continuation_fields,
            pc_output_model_builder::{
                opening_coverage_fields::opening_coverage_fields,
                opening_metadata_fields::opening_metadata_fields,
                opening_profile_fields::opening_profile_fields,
                opening_search_fields::opening_search_fields,
                resource_summary_fields::resource_summary_fields,
            },
            pc_pipeline_fields::core_pipeline_fields,
            pc_summary_builder::result_count_fields,
        },
    };

    pub(crate) fn render_opening(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
    ) -> CoreExecutionResult {
        let query = problem.core_query();
        let pc_query = problem
            .scenario()
            .pc_query()
            .expect("opening preset must preserve PcQuery metadata");
        let target = problem
            .scenario()
            .exact_target_policy()
            .expect("opening preset must preserve target label policy");
        let observed_supply_incomplete = !problem.piece_source().complete();
        let consumed = if buildup.solution_found() {
            buildup.queue_consumed()
        } else {
            0
        };

        let mut fields = opening_metadata_fields(problem, target);
        fields.extend(core_pipeline_fields(
            &packing.compact_problem(),
            packing,
            buildup,
            target.lines(),
            problem.objective(),
        ));
        fields.extend(backend_fields(
            packing.backend_report(),
            query.execution_policy(),
            packing,
            buildup,
        ));
        fields.extend(opening_profile_fields(problem, packing, buildup, target));
        fields.extend(opening_search_fields(problem, target));
        fields.extend(opening_coverage_fields(
            problem,
            packing,
            buildup,
            observed_supply_incomplete,
        ));
        fields.extend(result_count_fields(
            problem,
            packing,
            buildup,
            !observed_supply_incomplete,
        ));
        fields.extend(resource_summary_fields(
            packing,
            buildup,
            observed_supply_incomplete,
            problem.objective().kind()
                != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling,
        ));
        fields.extend(opening_continuation_fields(
            pc_query,
            crate::packing::packing_queue::fixed_pieces(query.remaining_queue()),
            consumed,
        ));
        fields.extend(super::solution_probability_fields(problem, buildup));

        CoreExecutionResult::new(fields, buildup.path_steps().to_vec())
            .with_normalized_solution_keys(buildup.normalized_solution_keys())
            .with_solution_coverages(buildup.solution_coverages().to_vec())
            .with_solution_probabilities(buildup.solution_probabilities().to_vec())
            .with_postprocess_replay_trace(buildup.sample_replay_trace().cloned())
            .with_postprocess_execution_batch(
                buildup.postprocess_executions().to_vec(),
                buildup.postprocess_execution_complete(),
                buildup.postprocess_pattern_weights().to_vec(),
            )
    }
}
mod opening_search_fields {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_problem::SearchProblem;

    use crate::service::field;

    pub(super) fn opening_search_fields(
        problem: &SearchProblem,
        target: PcTarget,
    ) -> Vec<(String, String)> {
        let pc_query = problem
            .scenario()
            .pc_query()
            .expect("opening preset must preserve PcQuery metadata");
        let two_line_capable =
            target == PcTarget::two_lines() && pc_query.hold_policy().is_enabled();
        vec![
            field("searched_nodes", "0"),
            field("search_nodes", "0"),
            field("budget_exceeded", "false"),
            field("budget_exceeded_count", "0"),
            field("search_unsupported_reason", "none"),
            field("two_line_capable", two_line_capable),
            field("two_line_fast_path_available", "false"),
            field(
                "two_line_fallback_reason",
                if two_line_capable {
                    "two_line_table_unavailable"
                } else if target != PcTarget::two_lines() {
                    "unsupported_target_lines"
                } else {
                    "unsupported_hold_disabled"
                },
            ),
        ]
    }
}
mod resource_summary_fields {
    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn resource_summary_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        observed_supply_incomplete: bool,
        probability_calculated: bool,
    ) -> Vec<(String, String)> {
        let report = packing.resource_report();
        let resource_truncated =
            report.truncated || observed_supply_incomplete || !buildup.count_complete();
        let truncation_reason = if let Some(reason) = report.truncation_reason {
            reason.as_str()
        } else if !buildup.count_complete() {
            buildup.count_truncated_reason()
        } else if observed_supply_incomplete {
            "observed_universe_truncated"
        } else {
            "none"
        };
        let probability_complete = probability_calculated
            && report.probability_complete
            && !resource_truncated
            && buildup.coverage_complete();
        let peak_cpu_bytes = report
            .peak_cpu_bytes
            .saturating_add(buildup.peak_workspace_bytes());

        vec![
            field("resource_truncated", resource_truncated),
            field("resource_truncation_reason", truncation_reason),
            field("resource_peak_frontier_states", report.peak_frontier_states),
            field("resource_peak_candidate_rows", report.peak_candidate_rows),
            field("resource_peak_hash_buckets", report.peak_hash_buckets),
            field("resource_peak_gpu_bytes", report.peak_gpu_bytes),
            field("resource_peak_cpu_bytes", peak_cpu_bytes),
            field(
                "resource_buildup_workspace_bytes",
                buildup.peak_workspace_bytes(),
            ),
            field(
                "resource_build_worker_backlog_peak",
                report.build_worker_backlog_peak,
            ),
            field(
                "resource_coverage_rows_emitted",
                report
                    .coverage_rows_emitted
                    .max(buildup.coverage_row_count()),
            ),
            field("resource_probability_complete", probability_complete),
        ]
    }
}
mod scenario_continuation_fields {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::BuildUpRunResult,
        service::{
            field, pc_continuation_fields::remaining_preview, pc_policy_labels::hold_slot_name,
        },
    };

    pub(super) fn scenario_continuation_fields(
        problem: &SearchProblem,
        buildup: &BuildUpRunResult,
        fixed_pieces: Option<&[PieceKind]>,
        replay_token: &str,
    ) -> Vec<(String, String)> {
        let query = problem.core_query();
        let remaining_queue_len = query
            .remaining_queue()
            .len()
            .saturating_sub(buildup.queue_consumed());
        let replay_token_version = if replay_token == "none" {
            "none"
        } else {
            "sr2"
        };
        vec![
            field("continuation_enough_queue_for_next_pc", "false"),
            field("remaining_queue_len", remaining_queue_len),
            field(
                "remaining_queue_preview",
                remaining_preview(fixed_pieces, buildup.queue_consumed()),
            ),
            field("remaining_hold", hold_slot_name(query.hold_state())),
            field("next_pc_available", "false"),
            field("next_pc_candidate", "none"),
            field("continuation_token_available", "false"),
            field(
                "continuation_token_unavailable_reason",
                "insufficient_remaining_queue",
            ),
            field("continue_available", "false"),
            field("continuation_available", "false"),
            field("continuation_token_version", "none"),
            field("continuation_token", "none"),
            field("continue_hint", "none"),
            field("scenario_replay_token_version", replay_token_version),
            field("scenario_replay_token", replay_token),
            field(
                "replay_hint",
                if replay_token == "none" {
                    "none".to_owned()
                } else {
                    format!("clearra continue {replay_token}")
                },
            ),
            field("continuation_available_complete", "true"),
            field("continuation_basis", "none"),
            field("continuation_queue_consumed", buildup.queue_consumed()),
            field("continuation_exact_pieces_policy", "unset-for-next-state"),
            field("continuation_exact_pieces", "none"),
            field(
                "continuation_min_remaining_queue_policy",
                "recalculated-from-next-state",
            ),
            field("continuation_min_remaining_queue", "0"),
            field("searched_nodes", "0"),
            field("search_nodes", "0"),
            field("budget_exceeded", "false"),
            field("budget_exceeded_count", "0"),
        ]
    }
}
mod scenario_execution_fields {
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::{
            buildup_coverage_bridge::{
                covered_pattern_count_basis_for_problem, verified_pattern_count_for_execution,
            },
            BuildUpRunResult,
        },
        packing::PackingRunResult,
        service::{
            field, pc_backend_report_adapter::solver_backend, pc_policy_labels::pattern_count,
        },
    };

    pub(super) fn scenario_execution_fields(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        observed_supply_incomplete: bool,
    ) -> Vec<(String, String)> {
        let pattern_count = pattern_count(problem);
        let verified_pattern_count = verified_pattern_count_for_execution(
            problem,
            pattern_count,
            packing.count_complete() && buildup.coverage_complete(),
        )
        .unwrap_or(0);
        let probability_calculated = problem.objective().kind() != ObjectiveKind::Tiling;
        vec![
            field("cleared_lines", buildup.cleared_lines()),
            field("route", "search-problem-core-executor"),
            field("solver_backend", solver_backend(packing)),
            field("coverage_pattern_count", pattern_count),
            field("verified_pattern_count", verified_pattern_count),
            field("materialized_pattern_count", pattern_count),
            field(
                "covered_pattern_count_basis",
                covered_pattern_count_basis_for_problem(problem),
            ),
            field("covered_pattern_count", buildup.covered_pattern_count()),
            field(
                "supply_covered_pattern_count",
                buildup.covered_pattern_count(),
            ),
            field(
                "probability_complete",
                probability_calculated
                    && !observed_supply_incomplete
                    && packing.count_complete()
                    && buildup.coverage_complete(),
            ),
            field("probability_calculated", probability_calculated),
            field(
                "state_count_available",
                packing.backend_report().state_count_available(),
            ),
            field(
                "multiplicity_count_available",
                packing.backend_report().multiplicity_count_available(),
            ),
            field("solution_found", buildup.solution_found()),
        ]
    }
}
mod scenario_metadata_fields {
    use clearra_problem::SearchProblem;

    use crate::service::field;

    pub(super) fn scenario_metadata_fields(problem: &SearchProblem) -> Vec<(String, String)> {
        vec![
            field("status", "scenario-searched"),
            field("execution_scope", "m17-core-executor"),
            field("executor_flow", "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel"),
            field("problem_layer", "clearra-problem"),
            field("problem_preset", "scenario-pc"),
            field("chain_labels", problem.labels().join(",")),
            field("chain_class", problem.chain_class().as_str()),
            field("exact_target_policy", "none-scenario-clear-to-empty"),
            field("checkpoint_schedule_source", "none"),
            field("checkpoint_schedule_label", "none"),
            field("checkpoint_schedule_partitions", "none"),
            field("checkpoint_schedule_checkpoint_count", "0"),
            field("checkpoint_results", "none"),
        ]
    }
}
mod scenario_profile_fields {
    use clearra_problem::SearchProblem;
    use clearra_rules::profile::rule_capability::RuleCapability;

    use super::option_usize;
    use crate::service::{
        field,
        pc_policy_labels::{count_policy_name, hold_slot_name},
    };

    pub(super) fn scenario_profile_fields(problem: &SearchProblem) -> Vec<(String, String)> {
        let query = problem.core_query();
        let capability = RuleCapability::from_rule(query.rule());
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("compiled scenario PC problem has a materialized supply universe");
        vec![
            field("completion_goal", query.completion_goal().as_str()),
            field("board_width", query.initial_board().width()),
            field("visible_height", query.initial_board().visible_height()),
            field(
                "initial_board_mask",
                format!("0x{:016x}", query.initial_board().occupied_mask()),
            ),
            field("piece_set_profile", query.piece_set().id().as_str()),
            field("bag_profile", query.bag().id().as_str()),
            field("rule_profile", query.rule().id().as_str()),
            field("effective_kick_model", capability.kick_model().as_str()),
            field("queue_mode", query.remaining_queue().mode()),
            field("queue_len", query.remaining_queue().len()),
            field(
                "supply_window_resolution",
                problem.supply().supply_window_resolution(),
            ),
            field(
                "projects_unplaced_lookahead",
                problem.supply().projects_unplaced_lookahead(),
            ),
            field(
                "projects_standard_bag_lookahead",
                problem.supply().projects_standard_bag_lookahead(),
            ),
            field(
                "source_sequence_length",
                problem.supply().source_sequence_length(),
            ),
            field(
                "total_possible_pattern_count",
                universe.total_possible_pattern_count(),
            ),
            field("hold", hold_slot_name(query.hold_state())),
            field("piece_window", query.piece_window().max_pieces()),
            field("exact_pieces", option_usize(query.exact_pieces())),
            field("min_remaining_queue", query.min_remaining_queue()),
            field("allow_hold", query.allow_hold()),
            field("requires_180", query.requires_180()),
            field("count_policy", count_policy_name(query.count_policy())),
            field("retained_trace_limit", query.retained_trace_limit()),
        ]
    }
}
mod scenario_renderer {
    use clearra_pc_graph::request::PcContinuationTokenCodec;
    use clearra_problem::SearchProblem;

    use crate::{
        buildup::BuildUpRunResult,
        core_execution_result::CoreExecutionResult,
        packing::PackingRunResult,
        service::{
            pc_backend_report_adapter::backend_fields,
            pc_output_model_builder::{
                resource_summary_fields::resource_summary_fields,
                scenario_continuation_fields::scenario_continuation_fields,
                scenario_execution_fields::scenario_execution_fields,
                scenario_metadata_fields::scenario_metadata_fields,
                scenario_profile_fields::scenario_profile_fields,
                scenario_trace_fields::scenario_trace_fields,
            },
            pc_pipeline_fields::core_pipeline_fields,
            pc_summary_builder::result_count_fields,
        },
    };

    pub(crate) fn render_scenario(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
    ) -> CoreExecutionResult {
        let query = problem.core_query();
        let observed_supply_incomplete = !problem.piece_source().complete();
        let replay_token = PcContinuationTokenCodec::encode_scenario_replay(query)
            .unwrap_or_else(|_| "none".to_owned());
        let remaining_queue_len = query
            .remaining_queue()
            .len()
            .saturating_sub(buildup.queue_consumed());

        let mut fields = scenario_metadata_fields(problem);
        fields.extend(core_pipeline_fields(
            &packing.compact_problem(),
            packing,
            buildup,
            0,
            problem.objective(),
        ));
        fields.extend(backend_fields(
            packing.backend_report(),
            query.execution_policy(),
            packing,
            buildup,
        ));
        fields.extend(scenario_profile_fields(problem));
        fields.extend(scenario_execution_fields(
            problem,
            packing,
            buildup,
            observed_supply_incomplete,
        ));
        fields.extend(resource_summary_fields(
            packing,
            buildup,
            observed_supply_incomplete,
            problem.objective().kind()
                != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling,
        ));
        fields.extend(result_count_fields(
            problem,
            packing,
            buildup,
            !observed_supply_incomplete,
        ));
        fields.extend(scenario_trace_fields(packing, buildup, remaining_queue_len));
        fields.extend(scenario_continuation_fields(
            problem,
            buildup,
            crate::packing::packing_queue::fixed_pieces(query.remaining_queue()),
            &replay_token,
        ));
        fields.extend(super::solution_probability_fields(problem, buildup));

        CoreExecutionResult::new(fields, buildup.path_steps().to_vec())
            .with_normalized_solution_keys(buildup.normalized_solution_keys())
            .with_solution_coverages(buildup.solution_coverages().to_vec())
            .with_solution_probabilities(buildup.solution_probabilities().to_vec())
            .with_postprocess_replay_trace(buildup.sample_replay_trace().cloned())
            .with_postprocess_execution_batch(
                buildup.postprocess_executions().to_vec(),
                buildup.postprocess_execution_complete(),
                buildup.postprocess_pattern_weights().to_vec(),
            )
    }
}
mod scenario_trace_fields {
    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn scenario_trace_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        remaining_queue_len: usize,
    ) -> Vec<(String, String)> {
        vec![
            field("min_queue_consumed", buildup.queue_consumed()),
            field("max_queue_consumed", buildup.queue_consumed()),
            field("sample_queue_consumed", buildup.queue_consumed()),
            field("placed_piece_count", buildup.placed_piece_count()),
            field("best_remaining_queue_len", remaining_queue_len),
            field(
                "trace_mode",
                packing.backend_report().solution_trace_mode().as_str(),
            ),
            field("retained_trace_key_count", buildup.retained_trace_count()),
            field(
                "retained_trace_keys",
                if buildup.retained_trace_count() > 0 {
                    buildup.trace_key().unwrap_or("trace-key-unavailable")
                } else {
                    "none"
                },
            ),
            field("retained_trace_key_source", buildup.trace_key_source()),
            field("search_unsupported_reason", "none"),
        ]
    }
}

pub(crate) use opening_renderer::render_opening;
pub(crate) use scenario_renderer::render_scenario;

fn solution_probability_fields(
    problem: &clearra_problem::SearchProblem,
    buildup: &crate::buildup::BuildUpRunResult,
) -> Vec<(String, String)> {
    let requested = problem.solution_probability_policy().requested();
    vec![
        crate::service::field("solution_probabilities_requested", requested),
        crate::service::field(
            "solution_probability_count",
            buildup.solution_probabilities().len(),
        ),
        crate::service::field(
            "solution_probability_complete",
            !requested || buildup.solution_probability_complete(),
        ),
        crate::service::field(
            "solution_probability_basis",
            if requested {
                "normalized-solution-pattern-bitset-or-union"
            } else {
                "not-requested"
            },
        ),
        crate::service::field(
            "solution_probability_incomplete_reason",
            if requested && !buildup.solution_probability_complete() {
                "pattern-specific-coverage-incomplete"
            } else {
                "none"
            },
        ),
    ]
}
