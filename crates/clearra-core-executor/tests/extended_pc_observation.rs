//! Actual bounded full-height PC policy execution, separate from benchmarks.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::{backend::WasmCpuSearchBackend, CoreExecutionResult};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::{
    builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
    rule_profile::RuleProfile,
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

fn forced_query(height: u8, rule: RuleProfile) -> PcScenarioQuery {
    let starts = if height == 7 {
        vec![0, 3]
    } else {
        (0..u16::from(height)).step_by(4).collect()
    };
    let pieces = starts.len();
    let mut holes = Board256Mask::EMPTY;
    for (column, start) in starts.into_iter().enumerate() {
        for row in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
        }
    }
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(
            u16::from(height),
            Board256Mask::all_cells(u16::from(height) * 10)
                .unwrap()
                .without(holes)
                .words(),
        )
        .unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
        PieceWindow::new(pieces),
    )
    .with_rule(rule)
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    )
}

fn run(query: &PcScenarioQuery) -> CoreExecutionResult {
    let problem = ProblemCompiler::compile_scenario_pc(query).unwrap();
    assert_eq!(
        problem.queue_observation_policy(),
        query.queue_observation_policy()
    );
    WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap()
}

fn assert_policy(result: &CoreExecutionResult) {
    assert_eq!(result.field("queue_knowledge"), Some("visible-7"));
    assert_eq!(
        result.field("coverage_semantics"),
        Some("visible-seven-policy")
    );
    assert_eq!(result.field("visible_piece_count"), Some("7"));
    assert_eq!(
        result.bool_field("observation_policy_evaluated"),
        Some(true)
    );
    assert!(
        result
            .usize_field("observation_policy_action_checks")
            .unwrap()
            > 0
    );
    assert!(result.usize_field("observation_trie_nodes").unwrap() > 0);
    assert!(result.pc_chance_coverage_evidence().is_none());
    assert!(result.pc_full_height_replay_evidence().is_none());
}

#[test]
fn full_height_visible_seven_runs_the_same_physical_family_and_original_denominator() {
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            let base = forced_query(height, rule);
            for (objective, count) in [
                (ObjectivePolicy::all(), PcCountPolicy::CountAll),
                (ObjectivePolicy::unique(), PcCountPolicy::CountUnique),
            ] {
                let query = base
                    .clone()
                    .with_objective(objective)
                    .with_count_policy(count);
                let reference = run(&query);
                let visible =
                    run(&query.with_queue_observation_policy(QueueObservationPolicy::VisibleSeven));
                assert_policy(&visible);
                assert_eq!(
                    visible.normalized_solution_keys(),
                    reference.normalized_solution_keys()
                );
                assert_eq!(
                    visible.coverage_pattern_words(),
                    reference.coverage_pattern_words()
                );
                assert_eq!(visible.field("coverage_probability"), Some("1"));
                assert_eq!(visible.bool_field("count_complete"), Some(true));
                assert_eq!(visible.bool_field("probability_complete"), Some(true));
                assert_eq!(visible.usize_field("coverage_pattern_count"), Some(1));
                assert_eq!(visible.usize_field("workers_requested"), Some(1));
            }
        }
    }
}

#[test]
fn full_height_visible_seven_preserves_explicitly_unavailable_per_solution_policy_reports() {
    let query = forced_query(24, srs_plus())
        .with_objective(ObjectivePolicy::unique())
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
    let result = run(&query);
    assert_policy(&result);
    assert!(result.solution_probabilities().is_empty());
    assert_eq!(
        result.bool_field("solution_probabilities_requested"),
        Some(true)
    );
    assert_eq!(
        result.bool_field("solution_probability_complete"),
        Some(false)
    );
    assert_eq!(
        result.field("solution_probability_basis"),
        Some("unsupported-under-visible-seven-policy")
    );
    assert_eq!(
        result.field("solution_probability_incomplete_reason"),
        Some("per-solution-policy-language-not-materialized")
    );
    assert_eq!(result.bool_field("probability_complete"), Some(true));
}

#[test]
fn full_height_visible_seven_preserves_b2b_filtered_language_and_original_denominator() {
    for height in [7, 8, 12, 24] {
        let query = forced_query(height, srs_plus())
            .with_objective(
                ObjectivePolicy::all().with_back_to_back_preservation(SpinProfileSelection::TSpins),
            )
            .with_count_policy(PcCountPolicy::CountAll);
        let reference = run(&query);
        let visible =
            run(&query.with_queue_observation_policy(QueueObservationPolicy::VisibleSeven));
        assert_eq!(
            visible.bool_field("observation_policy_evaluated"),
            Some(true)
        );
        assert_eq!(visible.field("queue_knowledge"), Some("visible-7"));
        assert_eq!(
            visible.bool_field("execution_constraint_materialized"),
            Some(true)
        );
        assert_eq!(visible.usize_field("coverage_pattern_count"), Some(1));
        assert_eq!(visible.bool_field("probability_complete"), Some(true));
        assert_eq!(
            visible.normalized_solution_keys(),
            reference.normalized_solution_keys()
        );
        assert_eq!(
            visible.coverage_pattern_words(),
            reference.coverage_pattern_words()
        );
        assert_eq!(
            visible.field("coverage_probability"),
            Some(if height == 7 { "0" } else { "1" })
        );
    }
}

#[test]
fn full_height_visible_seven_does_not_gain_oracle_typed_product_authority() {
    use clearra_problem::SearchOutputPolicy;
    let base = forced_query(24, srs_plus())
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
    assert!(ProblemCompiler::compile_scenario_pc(
        &base
            .clone()
            .with_objective(ObjectivePolicy::minimum_cover())
    )
    .is_err());
    let chance = ProblemCompiler::compile_scenario_pc(
        &base
            .clone()
            .with_objective(ObjectivePolicy::unique())
            .with_count_policy(PcCountPolicy::CountUnique),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary)
    .with_pc_chance_probability_v2_evidence();
    let path = ProblemCompiler::compile_scenario_pc(
        &base
            .with_objective(ObjectivePolicy::all())
            .with_count_policy(PcCountPolicy::CountAll),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::Trace)
    .with_pc_path_v2_evidence();
    for problem in [chance, path] {
        let error =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap_err();
        assert_eq!(error.reason(), "extended_pc_family_contract_not_connected");
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_visible_seven_native_workers_transfer_language_not_policy_bits() {
    let hardware = std::thread::available_parallelism().map_or(1, usize::from);
    if hardware < 2 {
        return;
    }
    // Nine I pieces: the last two are outside the initial visible window.
    // The four-by-four base admits several geometry branches, while the tall
    // forced-I tail keeps the fixture bounded instead of enumerating empty24L.
    let mut holes = Board256Mask::EMPTY;
    for row in 0..4 {
        for column in 0..4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
    }
    for row in 4..24 {
        holes = holes.union(Board256Mask::singleton(row * 10 + 5 + (row - 4) / 4).unwrap());
    }
    let pieces = holes.count_ones() as usize / 4;
    assert!(pieces > 7);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(
            24,
            Board256Mask::all_cells(240).unwrap().without(holes).words(),
        )
        .unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
        PieceWindow::new(pieces),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_objective(ObjectivePolicy::unique())
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1),
    );
    for objective in [
        ObjectivePolicy::unique(),
        ObjectivePolicy::unique().with_back_to_back_preservation(SpinProfileSelection::TSpins),
    ] {
        let query = query.clone().with_objective(objective);
        let serial = run(&query);
        let parallel = run(&query.clone().with_execution_policy(
            query
                .execution_policy()
                .clone()
                .with_workers(2)
                .with_use_all_logical_processors(hardware == 2),
        ));
        assert_policy(&parallel);
        assert_eq!(parallel.usize_field("workers_requested"), Some(2));
        assert_eq!(parallel.usize_field("workers_used"), Some(2));
        assert_eq!(
            parallel.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            parallel.coverage_pattern_words(),
            serial.coverage_pattern_words()
        );
        assert_eq!(
            parallel.field("coverage_probability"),
            serial.field("coverage_probability")
        );
        assert_eq!(
            parallel.field("actual_normalized_solution_set_hash"),
            serial.field("actual_normalized_solution_set_hash")
        );
    }
}
