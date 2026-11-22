//! Public query -> native Core -> live App global probability evidence.
//! This focused integration target preserves the one-lock contract cases
//! without linking the unrelated App unit-test suite.

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_core_executor::{
    CoreExecutionResult, PcGlobalProbabilityCompletion, WasmCpuSearchBackend,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_rules::profile::builtin_rules::{srs, srs_plus};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use clearra_app::{
    PcChanceIngressOrigin, PcGlobalProbabilityV1Result, PcGlobalProbabilityValidationError,
};
use clearra_problem::ProblemCompiler;
use std::sync::{Mutex, MutexGuard, PoisonError};

static PRODUCT_ADMISSION: Mutex<()> = Mutex::new(());

fn test_guard() -> MutexGuard<'static, ()> {
    PRODUCT_ADMISSION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn query(piece: PieceKind) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_supply_window_size(SupplyWindowSize::new(1))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_rule(srs_plus())
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_workers(1)
            .with_tablebase_requested(false),
    )
}

fn execute(query: &PcScenarioQuery) -> CoreExecutionResult {
    let problem = ProblemCompiler::compile_scenario_percent(query)
        .unwrap()
        .with_pc_global_probability_v1_evidence();
    WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap()
}

fn project(
    query: &PcScenarioQuery,
    result: &CoreExecutionResult,
) -> Result<PcGlobalProbabilityV1Result, PcGlobalProbabilityValidationError> {
    PcGlobalProbabilityV1Result::from_scenario_execution(
        query,
        PcChanceIngressOrigin::CanonicalPcChance,
        result,
    )
}

#[test]
fn global_positive_probability_does_not_claim_solution_enumeration() {
    let _guard = test_guard();
    let query = query(PieceKind::I);
    let result = execute(&query);
    let report = project(&query, &result).unwrap();
    assert_eq!(report.contract_id(), "pc-global-probability.v1");
    assert_eq!(report.covered_pattern_count(), 1);
    assert_eq!(report.coverage_pattern_words(), &[1]);
    assert_eq!(report.weighted_probability(), "1");
    assert!(report.probability_complete());
    assert!(!report.count_complete());
    assert_eq!(report.solution_count(), None);
    assert_eq!(report.normalized_solution_set_hash(), None);
}

#[test]
fn global_zero_probability_requires_candidate_domain_exhaustion() {
    let _guard = test_guard();
    let query = query(PieceKind::O);
    let result = execute(&query);
    let report = project(&query, &result).unwrap();
    assert_eq!(report.covered_pattern_count(), 0);
    assert_eq!(report.weighted_probability(), "0");
    assert_eq!(
        report.completion_basis(),
        PcGlobalProbabilityCompletion::CandidateDomainExhausted
    );
    assert!(report.enumeration_complete());
    assert!(!report.count_complete());
}

#[test]
fn global_projection_rejects_missing_proof_foreign_rules_and_tampered_bits() {
    let _guard = test_guard();
    let query = query(PieceKind::I);
    let result = execute(&query);
    assert!(project(
        &query,
        &result.clone().without_pc_chance_transient_evidence()
    )
    .is_err());
    assert!(project(&query.clone().with_rule(srs()), &result).is_err());
    assert!(project(&query, &result.clone().with_coverage_pattern_words(vec![0])).is_err());
    assert!(project(
        &query,
        &result.clone().with_coverage_pattern_words(vec![u64::MAX])
    )
    .is_err());
    assert!(project(
        &query,
        &result
            .clone()
            .with_additional_fields(vec![("count_complete".into(), "true".into())])
    )
    .is_err());
}

#[test]
fn ordinary_summary_and_row_probability_cannot_use_global_projection() {
    let _guard = test_guard();
    let query = query(PieceKind::I);
    let bare = ProblemCompiler::compile_scenario_percent(&query).unwrap();
    let global = bare.clone().with_pc_global_probability_v1_evidence();
    for problem in [bare.clone(), bare.with_pc_chance_probability_v2_evidence()] {
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap();
        assert!(project(&query, &result).is_err());
        assert_eq!(result.bool_field("count_complete"), Some(true));
        if problem
            .pc_chance_evidence_policy()
            .retains_pc_probability_v2_evidence()
        {
            let rows = result
                .pc_chance_coverage_evidence()
                .expect("the ordinary V2 execution still retains row authority");
            // This public V2 producer binding is also required by the private
            // V2 App authority: a global request cannot borrow the V2 rows.
            assert!(!rows.problem().matches_search_problem(&global));
        }
    }
}
