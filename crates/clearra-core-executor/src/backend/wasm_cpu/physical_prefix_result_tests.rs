//! End-to-end contracts for candidate-local physical prefix negatives.
//! Serial references keep the concrete BuildUp movement authority; the
//! producer-only tests separately compare every raw leaf and original ordinal.

use std::sync::{Mutex, MutexGuard};

use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::profile::builtin_rules;
use clearra_supply::QueueObservationPolicy;

use super::super::with_pc_placement_dependency_reference;
use super::{
    physical_pc_prefix_mode, CoreExecutionResult, ExactSearchAdvance, ExecutionControl,
    GeometryCatalog, PhysicalPrefixMode, WasmExactSearchSession,
};

static SESSION_TEST_LOCK: Mutex<()> = Mutex::new(());

fn session_test_guard() -> MutexGuard<'static, ()> {
    SESSION_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn execution_policy() -> PcExecutionPolicy {
    // Serial references must remain on the thread holding the reference TLS.
    // This fixture setting does not change the product Auto worker policy.
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_tablebase_requested(false)
        .with_precompute_build_dependencies(false)
}

fn mixed_query(initial: u64) -> PcScenarioQuery {
    let locks = (40 - initial.count_ones()) as usize / 4;
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, initial),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(locks),
    )
    .with_rule(builtin_rules::srs_plus())
    .with_allow_hold(true)
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy())
}

fn compile(query: &PcScenarioQuery, output: SearchOutputPolicy) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(query)
        .expect("finite fixed-field PC fixture")
        .with_output_policy(output)
}

fn serial(problem: &SearchProblem, mode: PhysicalPrefixMode) -> CoreExecutionResult {
    let mut session = WasmExactSearchSession::new(problem).expect("finite serial session");
    session.geometry.enable_physical_prefix_pruning(mode);
    let control = ExecutionControl::default();
    for _ in 0..100_000 {
        match session
            .advance(512, &control)
            .expect("exact serial advance")
        {
            ExactSearchAdvance::Pending => {}
            ExactSearchAdvance::Completed(result) => return result,
            ExactSearchAdvance::Cancelled => panic!("fixture was not cancelled"),
        }
    }
    panic!("bounded partial-4L fixture must complete");
}

fn assert_result_parity(reference: &CoreExecutionResult, pruned: &CoreExecutionResult) {
    assert_eq!(pruned.solution_found(), reference.solution_found());
    assert_eq!(
        pruned.normalized_solution_identities(),
        reference.normalized_solution_identities()
    );
    assert_eq!(
        pruned.normalized_solution_keys(),
        reference.normalized_solution_keys()
    );
    assert_eq!(
        pruned.coverage_pattern_words(),
        reference.coverage_pattern_words()
    );
    assert_eq!(pruned.path_steps(), reference.path_steps());
    assert_eq!(pruned.solution_coverages(), reference.solution_coverages());
    assert_eq!(
        pruned.normalized_solution_coverages(),
        reference.normalized_solution_coverages()
    );
    assert_eq!(
        pruned.solution_probabilities(),
        reference.solution_probabilities()
    );
    for key in [
        "packing_candidate_count",
        "packing_candidate_domain",
        "packing_candidate_set_digest",
        "packing_candidate_set_digest_calculated",
        "packing_count_complete",
        "geometry_candidate_family_count",
        "unique_solution_count",
        "normalized_unique_solution_count",
        "normalized_solution_set_hash",
        "solution_count_calculated",
        "solution_set_materialized",
        "build_variant_count",
        "build_variant_count_exact",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "count_complete",
        "probability_complete",
        "resource_truncated",
        "representative_candidate_id",
        "representative_candidate_ordinal",
        "representative_pattern_id",
    ] {
        assert!(
            reference.field(key).is_some(),
            "missing reference contract: {key}"
        );
        assert_eq!(pruned.field(key), reference.field(key), "{key}");
    }
    for result in [reference, pruned] {
        assert_eq!(result.field("packing_candidate_domain"), Some("raw"));
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
    }
}

#[test]
fn physical_prefix_serial_mark_preserves_exact_pc_results_and_raw_digest() {
    let _guard = session_test_guard();
    let mut certified = 0;
    for (initial, output) in [
        (0x3c0f_03c0f, SearchOutputPolicy::Trace),
        (0x90_747d_83e5, SearchOutputPolicy::Summary),
    ] {
        let problem = compile(&mixed_query(initial), output);
        let catalog = GeometryCatalog::compile(&problem).unwrap();
        assert_eq!(
            physical_pc_prefix_mode(&problem, &catalog),
            PhysicalPrefixMode::MarkCandidates
        );
        let reference = with_pc_placement_dependency_reference(|| {
            serial(&problem, PhysicalPrefixMode::Disabled)
        });
        let marked = serial(&problem, PhysicalPrefixMode::MarkCandidates);
        assert_result_parity(&reference, &marked);
        assert_eq!(
            reference.bool_field("pc_placement_dependency_selected"),
            Some(false)
        );
        #[cfg(feature = "placement-dependency-pc")]
        assert_eq!(
            marked.bool_field("pc_placement_dependency_selected"),
            Some(true)
        );
        assert_eq!(
            marked.bool_field("packing_candidate_set_digest_calculated"),
            Some(true)
        );
        assert_eq!(
            marked.field("physical_prefix_pruning_mode"),
            Some(PhysicalPrefixMode::MarkCandidates.label())
        );
        assert_eq!(
            marked.usize_field("physical_prefix_skipped_candidate_count"),
            Some(0)
        );
        assert_eq!(
            marked.usize_field("physical_prefix_emitted_candidate_count"),
            reference.usize_field("packing_candidate_count")
        );
        certified += marked
            .usize_field("physical_prefix_certified_candidate_count")
            .unwrap();
    }
    assert!(
        certified > 0,
        "the endpoint fixtures must bypass actual impossible candidates"
    );
}

#[test]
fn physical_prefix_serial_skip_preserves_pc_coverage_and_original_counts() {
    let _guard = session_test_guard();
    let mut skipped = 0;
    for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
        let problem = compile(&mixed_query(initial), SearchOutputPolicy::CoverageSummary);
        let catalog = GeometryCatalog::compile(&problem).unwrap();
        assert_eq!(
            physical_pc_prefix_mode(&problem, &catalog),
            PhysicalPrefixMode::SkipSubtrees
        );
        let reference = with_pc_placement_dependency_reference(|| {
            serial(&problem, PhysicalPrefixMode::Disabled)
        });
        let pruned = serial(&problem, PhysicalPrefixMode::SkipSubtrees);
        assert_result_parity(&reference, &pruned);
        assert_eq!(
            pruned.field("packing_candidate_set_digest"),
            Some("not-calculated")
        );
        assert_eq!(
            pruned.bool_field("packing_candidate_set_digest_calculated"),
            Some(false)
        );
        assert_eq!(
            pruned.field("physical_prefix_pruning_mode"),
            Some(PhysicalPrefixMode::SkipSubtrees.label())
        );
        let emitted = pruned
            .usize_field("physical_prefix_emitted_candidate_count")
            .unwrap();
        let omitted = pruned
            .usize_field("physical_prefix_skipped_candidate_count")
            .unwrap();
        assert_eq!(
            emitted + omitted,
            reference.usize_field("packing_candidate_count").unwrap()
        );
        skipped += omitted;
    }
    assert!(
        skipped > 0,
        "the endpoint fixtures must skip an exactly counted subtree"
    );
}

#[cfg(feature = "parallel")]
#[test]
fn physical_prefix_native_parallel_preserves_exact_outputs_and_raw_ordinals() {
    let _guard = session_test_guard();
    // Reuse the existing native terminal-identity fixture, which has multiple
    // branches and more than one legal normalized result.
    let query = mixed_query(0xfc3f_0fc3_f0)
        .with_rule(builtin_rules::jstris_180())
        .with_allow_hold(false)
        .with_supply_window_size(SupplyWindowSize::new(4));
    for (output, mode) in [
        (
            SearchOutputPolicy::Trace,
            PhysicalPrefixMode::MarkCandidates,
        ),
        (
            SearchOutputPolicy::CoverageSummary,
            PhysicalPrefixMode::SkipSubtrees,
        ),
    ] {
        let problem = compile(&query, output);
        let execute = |mode| {
            let mut session = WasmExactSearchSession::new(&problem).unwrap();
            session.geometry.enable_physical_prefix_pruning(mode);
            session
                .execute_parallel_with_identity_storage(2, &ExecutionControl::default(), false)
                .expect("finite native parallel execution")
                .expect("fixture must retain multiple native branches")
        };
        let reference = execute(PhysicalPrefixMode::Disabled);
        let pruned = execute(mode);
        assert_result_parity(&reference, &pruned);
        assert!(reference.solution_found());
        assert!(reference.usize_field("covered_pattern_count").unwrap() > 0);
        assert_eq!(
            pruned.field("physical_prefix_pruning_mode"),
            Some(mode.label())
        );
        assert_eq!(
            pruned.usize_field("workers_used"),
            reference.usize_field("workers_used")
        );
        assert!(reference.usize_field("workers_used").unwrap() >= 2);
        if output == SearchOutputPolicy::Trace {
            assert!(reference.normalized_solution_identities().len() > 1);
            assert!(!reference.path_steps().is_empty());
            // The existing native parallel result does not publish the
            // internal representative rank. Keep its empty field contract;
            // producer seed tests compare every original local ordinal.
            assert_eq!(
                reference.field("representative_candidate_ordinal"),
                Some("")
            );
        }
    }
}

#[test]
fn physical_prefix_gate_preserves_richer_options_and_budget_boundaries() {
    let query = mixed_query(0x90_747d_83e5);
    let base = compile(&query, SearchOutputPolicy::Summary);
    let catalog = GeometryCatalog::compile(&base).unwrap();
    for (output, expected) in [
        (
            SearchOutputPolicy::Trace,
            PhysicalPrefixMode::MarkCandidates,
        ),
        (
            SearchOutputPolicy::Summary,
            PhysicalPrefixMode::MarkCandidates,
        ),
        (
            SearchOutputPolicy::CoverageSummary,
            PhysicalPrefixMode::SkipSubtrees,
        ),
    ] {
        assert_eq!(
            physical_pc_prefix_mode(&base.clone().with_output_policy(output), &catalog),
            expected
        );
    }
    let mut denied = vec![
        (
            "count-all",
            compile(
                &query.clone().with_count_policy(PcCountPolicy::CountAll),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "minimum-cover",
            compile(
                &query
                    .clone()
                    .with_objective(ObjectivePolicy::minimum_cover()),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "score",
            compile(
                &query
                    .clone()
                    .with_objective(ObjectivePolicy::unique().with_score_summary()),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "B2B",
            compile(
                &query.clone().with_objective(
                    ObjectivePolicy::unique()
                        .with_back_to_back_preservation(SpinProfileSelection::TSpins),
                ),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "solution-probabilities",
            compile(
                &query
                    .clone()
                    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "visible-seven",
            compile(
                &query
                    .clone()
                    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
                SearchOutputPolicy::Summary,
            ),
        ),
        (
            "typed-probability",
            base.clone()
                .with_pc_chance_probability_v2_evidence()
                .with_output_policy(SearchOutputPolicy::CoverageSummary),
        ),
        (
            "coverage-rows",
            base.clone()
                .with_output_policy(SearchOutputPolicy::CoverageRows),
        ),
        ("replay-evidence", base.clone().with_build_replay_evidence()),
    ];
    for (name, policy) in [
        (
            "tablebase",
            execution_policy().with_tablebase_requested(true),
        ),
        (
            "dependency-prepass",
            execution_policy().with_precompute_build_dependencies(true),
        ),
        ("node-budget", execution_policy().with_max_nodes(1)),
        (
            "candidate-budget",
            execution_policy().with_max_candidates(1),
        ),
        (
            "frontier-budget",
            execution_policy().with_max_frontier_states(1),
        ),
        (
            "memory-budget",
            execution_policy().with_max_memory_mib(Some(256)),
        ),
    ] {
        denied.push((
            name,
            compile(
                &query.clone().with_execution_policy(policy),
                SearchOutputPolicy::Summary,
            ),
        ));
    }
    for (name, problem) in denied {
        assert_eq!(
            physical_pc_prefix_mode(&problem, &catalog),
            PhysicalPrefixMode::Disabled,
            "{name}"
        );
    }
}

#[cfg(feature = "placement-dependency-pc")]
#[test]
fn summary_boolean_serial_preserves_trace_representative_and_original_ordinals() {
    let _guard = session_test_guard();
    let query = mixed_query(0x3c0f_03c0f);
    let execute = |output| {
        let problem = compile(&query, output);
        let mut session = WasmExactSearchSession::new(&problem).unwrap();
        let control = ExecutionControl::default();
        for _ in 0..100_000 {
            match session.advance(512, &control).unwrap() {
                ExactSearchAdvance::Pending => {}
                ExactSearchAdvance::Completed(result) => {
                    return (
                        result,
                        session.buildup_workspace.standard_bag_memo_accounting(),
                    );
                }
                ExactSearchAdvance::Cancelled => panic!("fixture was not cancelled"),
            }
        }
        panic!("bounded P7 representative fixture must complete");
    };
    let (reference, reference_work) = execute(SearchOutputPolicy::Trace);
    let (summary, summary_work) = execute(SearchOutputPolicy::Summary);
    assert_result_parity(&reference, &summary);
    assert!(summary.solution_found());
    assert!(!summary.path_steps().is_empty());
    assert_eq!(summary.usize_field("covered_pattern_count"), Some(5_040));
    assert_eq!(
        reference_work.boolean_membership_calls, 0,
        "Trace cannot use a first-success Boolean witness"
    );
    assert_eq!(reference_work.boolean_certified_membership_calls, 0);
    // A finite session proves output parity. Whether this fixture reaches Ω
    // before its last useful candidate is recorded, not assumed from its final
    // probability. Primitive tests separately force the exact transition.
    eprintln!(
        "SUMMARY_P7_SERIAL boolean_calls={} certified_calls={} scalar_calls={}",
        summary_work.boolean_membership_calls,
        summary_work.boolean_certified_membership_calls,
        summary_work.first_witness_calls
    );
}

#[cfg(all(feature = "placement-dependency-pc", feature = "parallel"))]
#[test]
fn summary_boolean_native_parallel_preserves_reverified_representative() {
    let _guard = session_test_guard();
    let query = mixed_query(0x3c0f_03c0f);
    let execute = |output| {
        let problem = compile(&query, output);
        let mut session = WasmExactSearchSession::new(&problem).unwrap();
        session
            .execute_parallel_with_identity_storage(2, &ExecutionControl::default(), false)
            .expect("finite native P7 execution")
            .expect("P7 fixture must retain multiple native branches")
    };
    let reference = execute(SearchOutputPolicy::Trace);
    let summary = execute(SearchOutputPolicy::Summary);
    assert_result_parity(&reference, &summary);
    assert!(summary.solution_found());
    assert!(!summary.path_steps().is_empty());
    assert_eq!(summary.usize_field("covered_pattern_count"), Some(5_040));
    assert!(summary.usize_field("workers_used").unwrap() >= 2);
    assert_eq!(
        summary.field("representative_pattern_id"),
        reference.field("representative_pattern_id")
    );
    assert_eq!(
        summary.field("representative_candidate_ordinal"),
        reference.field("representative_candidate_ordinal")
    );
}
