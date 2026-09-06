#[path = "../src/setup_ranked_family_result.rs"]
// The isolated contract test consumes only the promotion surface from this
// production module, not every diagnostic accessor it exposes to the app.
#[allow(dead_code)]
mod setup_ranked_family_result;
#[path = "../src/setup_ranking_contract.rs"]
mod setup_ranking_contract;
#[path = "../src/setup_ranking_facade.rs"]
mod setup_ranking_facade;
#[path = "../src/spin_structure_search_result.rs"]
// The isolated contract test consumes the typed result surface but not the
// host-only ownership conversion and canonical-id helper.
#[allow(dead_code)]
mod spin_structure_search_result;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{
    CoreExecutionResult, CorePathStep, SetupCandidateReport, SetupFinderReport,
    SetupHoldConditionReport,
};
use clearra_problem::{
    compile_setup_search_conditions, SetupCandidatePriority, SetupCycleResetBorrowPolicy,
    SetupPathDetail, SetupQueueInput, SetupSearchQuery,
};
use clearra_spin_structure_search::{
    PieceInventory, SpinLineRequirement, SpinStructureMode, SpinStructureQuery,
    SpinStructureSearcher, StructureBoard,
};

use setup_ranked_family_result::SetupRankedFamilyResultError;
use setup_ranking_contract::{SetupRankingContract, SetupRankingContractError, SetupRankingKind};
use setup_ranking_facade::SetupRankingFacade;
use spin_structure_search_result::{SpinStructureSearchResult, SpinStructureSearchResultError};

#[test]
fn setup_contract_binds_kind_query_and_all_five_identities() {
    for (kind, priority, schema) in [
        (
            SetupRankingKind::Joint,
            SetupCandidatePriority::All,
            "setup-joint-ranking.v2",
        ),
        (
            SetupRankingKind::Build,
            SetupCandidatePriority::BuildProbabilityFirst,
            "setup-build-ranking.v2",
        ),
        (
            SetupRankingKind::ConditionalPc,
            SetupCandidatePriority::PcProbabilityFirst,
            "setup-pc-ranking.v2",
        ),
    ] {
        let query = SetupSearchQuery::default().with_candidate_priority(priority);
        let contract = SetupRankingContract::bind(kind, &query).expect("bind ranked family");
        assert_eq!(contract.capability_id(), kind.capability_id());
        assert_eq!(contract.result_schema(), schema);
        assert_eq!(contract.identities().query_sha256().len(), 64);
        assert_eq!(contract.identities().supply_sha256().len(), 64);
        assert_eq!(contract.identities().universe_sha256().len(), 64);
        assert!(contract
            .identities()
            .rule_profile()
            .starts_with("setup-rule-profile.v1:"));
        assert!(contract
            .identities()
            .product_build()
            .starts_with("product-build.v1:"));
        contract.validate_query(&query).expect("same exact query");
    }
}

#[test]
fn setup_contract_rejects_priority_path_detail_and_fieldwise_query_mismatch() {
    let query = SetupSearchQuery::default();
    assert_eq!(
        SetupRankingContract::bind(SetupRankingKind::Build, &query),
        Err(SetupRankingContractError::CandidatePriorityMismatch {
            expected: SetupCandidatePriority::BuildProbabilityFirst,
            actual: SetupCandidatePriority::All,
        })
    );

    let detail = SetupPathDetail::new(1, 0, 1, "condition").expect("path detail");
    let detail_query = query.clone().with_path_detail(detail);
    assert_eq!(
        SetupRankingContract::bind(SetupRankingKind::Joint, &detail_query),
        Err(SetupRankingContractError::PathDetailIsNotRankedFamily)
    );

    let contract = SetupRankingContract::bind(SetupRankingKind::Joint, &query).expect("contract");
    let changed = query.clone().with_max_setup_pieces(8);
    assert_eq!(
        contract.validate_query(&changed),
        Err(SetupRankingContractError::QueryIdentityMismatch)
    );
}

#[test]
fn setup_facade_promotes_only_complete_identity_bound_ranked_family() {
    let query = SetupSearchQuery::default();
    let report = complete_setup_report(&query, false, false);
    let core = setup_core_result(&query, report, &[], None);
    let contract = SetupRankingContract::bind(SetupRankingKind::Joint, &query).expect("contract");
    let promoted = SetupRankingFacade::promote(contract, &query, core).expect("promote result");

    assert_eq!(promoted.candidate_count(), 1);
    assert_eq!(promoted.report().hold_conditions().len(), 1);
    assert_eq!(
        promoted.candidate_identities()[0].condition_id(),
        promoted.report().hold_conditions()[0].condition_id()
    );
    assert!(promoted.candidate_identities()[0]
        .candidate_id()
        .starts_with("setup-candidate.v1:"));
    assert!(promoted.candidate_identities()[0]
        .setup_id()
        .starts_with("setup-"));
    let expected_condition_id = promoted.report().hold_conditions()[0]
        .condition_id()
        .to_owned();
    let (core_result, snapshot) = promoted.into_core_result_and_snapshot();
    assert_eq!(core_result.unique_field("count_complete"), Some("true"));
    assert_eq!(snapshot.kind(), SetupRankingKind::Joint);
    assert_eq!(snapshot.capability_id(), "setup.joint");
    assert_eq!(snapshot.result_schema(), "setup-joint-ranking.v2");
    assert_eq!(snapshot.candidate_count(), 1);
    assert_eq!(
        snapshot.candidate_identities()[0].condition_id(),
        expected_condition_id
    );
    assert_eq!(snapshot.identities().query_sha256().len(), 64);
    assert_eq!(snapshot.identities().supply_sha256().len(), 64);
    assert_eq!(snapshot.identities().universe_sha256().len(), 64);
}

#[test]
fn setup_facade_validates_solution_set_hash_independent_of_ranked_presentation_order() {
    let query = SetupSearchQuery::default();
    let condition = compile_setup_search_conditions(&query)
        .expect("compile fixture condition")
        .remove(0);
    let ranked_candidates = vec![
        SetupCandidateReport::new(
            SetupPathDetail::setup_id_for(2, 0, 1).expect("higher-ranked setup id"),
            2,
            1,
            1,
            2,
            2,
            "1.0".to_owned(),
            "1.0".to_owned(),
            "1.0".to_owned(),
            vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
        ),
        SetupCandidateReport::new(
            SetupPathDetail::setup_id_for(1, 0, 1).expect("lower-ranked setup id"),
            1,
            1,
            1,
            2,
            1,
            "1.0".to_owned(),
            "0.5".to_owned(),
            "0.5".to_owned(),
            vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
        ),
    ];
    let report = SetupFinderReport::new(
        query.search_mode(),
        query.queue_observation_policy(),
        query.residue().cycle().unwrap_or_default(),
        pieces_string(query.residue().pieces()),
        queue_based_pieces(query.queue()),
        pieces_string(query.next_cycle_remaining_pieces().unwrap_or(&[])),
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
        "2".to_owned(),
        2,
        true,
        vec![SetupHoldConditionReport::new(
            condition.condition_id().to_owned(),
            condition.initial_hold(),
            condition.pattern_expression().to_owned(),
            2,
            2,
            false,
            true,
            ranked_candidates,
        )],
    );
    let core = setup_core_result(&query, report, &[], None);
    let contract = SetupRankingContract::bind(SetupRankingKind::Joint, &query).expect("contract");

    let promoted = SetupRankingFacade::promote(contract, &query, core)
        .expect("rank order must not change core solution-set identity");

    assert_eq!(promoted.candidate_count(), 2);
    assert_eq!(
        promoted.report().hold_conditions()[0].candidates()[0].board_mask(),
        2
    );
    assert_eq!(
        promoted.report().hold_conditions()[0].candidates()[1].board_mask(),
        1
    );
}

#[test]
fn setup_facade_fails_closed_for_incomplete_truncated_and_core_mismatch() {
    let query = SetupSearchQuery::default();
    let contract = SetupRankingContract::bind(SetupRankingKind::Joint, &query).expect("contract");

    let core = setup_core_result(
        &query,
        complete_setup_report(&query, true, false),
        &[],
        None,
    );
    assert_eq!(
        SetupRankingFacade::promote(contract.clone(), &query, core),
        Err(SetupRankedFamilyResultError::ReportIncomplete)
    );

    let core = setup_core_result(
        &query,
        complete_setup_report(&query, false, true),
        &[],
        None,
    );
    assert_eq!(
        SetupRankingFacade::promote(contract.clone(), &query, core),
        Err(SetupRankedFamilyResultError::HoldConditionTruncated { condition_index: 0 })
    );

    let report = complete_setup_report(&query, false, false);
    let core = setup_core_result(&query, report, &[("resource_truncated", "true")], None);
    assert!(matches!(
        SetupRankingFacade::promote(contract.clone(), &query, core),
        Err(SetupRankedFamilyResultError::CoreFieldMismatch {
            field: "resource_truncated",
            ..
        })
    ));

    let report = complete_setup_report(&query, false, false);
    let core = setup_core_result(&query, report, &[], Some(("count_complete", "true")));
    assert_eq!(
        SetupRankingFacade::promote(contract, &query, core),
        Err(SetupRankedFamilyResultError::CoreFieldDuplicated(
            "count_complete"
        ))
    );
}

#[test]
fn setup_facade_rejects_report_query_identity_mismatch() {
    let query = SetupSearchQuery::default();
    let mut report = complete_setup_report(&query, false, false);
    report = SetupFinderReport::new(
        report.search_mode(),
        report.queue_observation_policy(),
        report.cycle(),
        "wrong".to_owned(),
        report.queue_based_pieces().to_owned(),
        report.next_cycle_remaining_pieces().to_owned(),
        report.post_cycle_borrow_enabled(),
        report.geometry_family_count().to_owned(),
        report.partial_build_node_count(),
        report.complete(),
        report.hold_conditions().to_vec(),
    );
    let core = setup_core_result(&query, report, &[], None);
    let contract = SetupRankingContract::bind(SetupRankingKind::Joint, &query).expect("contract");
    assert_eq!(
        SetupRankingFacade::promote(contract, &query, core),
        Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "remaining_pieces"
        ))
    );
}

#[test]
fn spin_search_promotes_actual_complete_executor_family_with_stable_identities() {
    let query = one_piece_spin_query();
    let report = SpinStructureSearcher::run(query.clone()).expect("actual spin executor");
    assert!(report.outcome_count() > 0);
    let promoted = SpinStructureSearchResult::promote(&query, report).expect("promote search");

    assert_eq!(promoted.result_schema(), "spin-structure-family.v2");
    assert_eq!(
        promoted.candidate_count(),
        promoted.report().outcome_count()
    );
    assert_eq!(promoted.identities().query_sha256().len(), 64);
    assert_eq!(promoted.identities().supply_sha256().len(), 64);
    assert_eq!(promoted.identities().universe_sha256().len(), 64);
    assert!(promoted
        .identities()
        .rule_profile()
        .starts_with("spin-structure-rule-profile.v1:"));
    assert!(promoted
        .identities()
        .spin_profile()
        .starts_with("spin-structure-spin-profile.v1:"));
    assert!(promoted
        .identities()
        .product_build()
        .starts_with("product-build.v1:"));
    assert!(promoted.candidate_identities().iter().all(|candidate| {
        candidate
            .candidate_id()
            .starts_with("spin-structure-candidate.v1:")
            && candidate.placement_count() > 0
    }));
    assert!(promoted
        .candidate_identities()
        .iter()
        .any(|candidate| !candidate.mini()));
}

#[test]
fn spin_search_fails_closed_for_incomplete_stale_and_inconsistent_reports() {
    let query = one_piece_spin_query();
    let report = SpinStructureSearcher::run(query.clone()).expect("actual spin executor");

    let mut incomplete = report.clone();
    incomplete.complete = false;
    assert_eq!(
        SpinStructureSearchResult::promote(&query, incomplete),
        Err(SpinStructureSearchResultError::ReportIncomplete)
    );

    let mut missing_query = report.clone();
    missing_query.query = None;
    assert_eq!(
        SpinStructureSearchResult::promote(&query, missing_query),
        Err(SpinStructureSearchResultError::MissingReportQuery)
    );

    let mut changed_query = query.clone();
    changed_query.minimality = clearra_spin_structure_search::MinimalityPolicy::MinimumPieceCount;
    assert_eq!(
        SpinStructureSearchResult::promote(&changed_query, report.clone()),
        Err(SpinStructureSearchResultError::QueryIdentityMismatch)
    );

    let mut bad_minimum = report.clone();
    bad_minimum.minimum_placements = Some(u8::MAX);
    assert_eq!(
        SpinStructureSearchResult::promote(&query, bad_minimum),
        Err(SpinStructureSearchResultError::MinimumPlacementMismatch)
    );

    let mut duplicate = report;
    let repeated = duplicate.regular[0].clone();
    duplicate.regular.insert(1, repeated);
    assert_eq!(
        SpinStructureSearchResult::promote(&query, duplicate),
        Err(SpinStructureSearchResultError::DuplicateCandidateId)
    );
}

#[test]
fn ranked_family_modules_do_not_depend_on_alternative_set_infrastructure() {
    let sources = concat!(
        include_str!("../src/setup_ranking_contract.rs"),
        include_str!("../src/setup_ranked_family_result.rs"),
        include_str!("../src/setup_ranking_facade.rs"),
        include_str!("../src/spin_structure_search_result.rs"),
    );
    assert!(!sources.contains("CoveragePortfolioAlternative"));
    assert!(!sources.contains("portfolio_alternative_store"));
    assert!(!sources.contains("tie_metadata"));
}

fn complete_setup_report(
    query: &SetupSearchQuery,
    incomplete: bool,
    truncated: bool,
) -> SetupFinderReport {
    let condition = compile_setup_search_conditions(query)
        .expect("compile fixture condition")
        .remove(0);
    let setup_id = SetupPathDetail::setup_id_for(1, 0, 1).expect("canonical setup id");
    let candidate = SetupCandidateReport::new(
        setup_id,
        1,
        1,
        1,
        1,
        1,
        "1.0".to_owned(),
        "1.0".to_owned(),
        "1.0".to_owned(),
        vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
    );
    SetupFinderReport::new(
        query.search_mode(),
        query.queue_observation_policy(),
        query.residue().cycle().unwrap_or_default(),
        pieces_string(query.residue().pieces()),
        queue_based_pieces(query.queue()),
        pieces_string(query.next_cycle_remaining_pieces().unwrap_or(&[])),
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
        "1".to_owned(),
        1,
        !incomplete,
        vec![SetupHoldConditionReport::new(
            condition.condition_id().to_owned(),
            condition.initial_hold(),
            condition.pattern_expression().to_owned(),
            1,
            1,
            truncated,
            true,
            vec![candidate],
        )],
    )
}

fn setup_core_result(
    query: &SetupSearchQuery,
    report: SetupFinderReport,
    overrides: &[(&str, &str)],
    duplicate: Option<(&str, &str)>,
) -> CoreExecutionResult {
    let candidate_count = report
        .hold_conditions()
        .iter()
        .map(|condition| condition.candidates().len())
        .sum::<usize>();
    let set_hash = setup_candidate_set_hash(&report);
    let mut fields = vec![
        ("status".to_owned(), "setup-finder-complete".to_owned()),
        ("count_complete".to_owned(), "true".to_owned()),
        ("probability_complete".to_owned(), "true".to_owned()),
        ("resource_truncated".to_owned(), "false".to_owned()),
        ("resource_truncation_reason".to_owned(), "none".to_owned()),
        (
            "setup_coverage_semantics".to_owned(),
            query
                .queue_observation_policy()
                .coverage_semantics()
                .to_owned(),
        ),
        (
            "queue_knowledge".to_owned(),
            query.queue_observation_policy().keyword().to_owned(),
        ),
        (
            "visible_piece_count".to_owned(),
            query
                .queue_observation_policy()
                .visible_piece_count()
                .map_or_else(|| "all".to_owned(), |count| count.to_string()),
        ),
        (
            "setup_search_mode".to_owned(),
            query.search_mode().keyword().to_owned(),
        ),
        (
            "remaining_pieces".to_owned(),
            report.remaining_pieces().to_owned(),
        ),
        (
            "queue_based_pieces".to_owned(),
            report.queue_based_pieces().to_owned(),
        ),
        (
            "next_cycle_remaining_pieces".to_owned(),
            report.next_cycle_remaining_pieces().to_owned(),
        ),
        ("setup_cycle".to_owned(), report.cycle().to_string()),
        (
            "setup_candidate_priority".to_owned(),
            query.candidate_priority().keyword().to_owned(),
        ),
        (
            "setup_length_preference".to_owned(),
            query.length_preference().keyword().to_owned(),
        ),
        (
            "geometry_candidate_family_count".to_owned(),
            report.geometry_family_count().to_owned(),
        ),
        (
            "partial_build_node_count".to_owned(),
            report.partial_build_node_count().to_string(),
        ),
        (
            "tablebase_requested".to_owned(),
            query.tablebase_requested().to_string(),
        ),
        (
            "normalized_solution_key_algorithm".to_owned(),
            "clearra-setup-candidate-key-v2-exact-partial-state".to_owned(),
        ),
        (
            "normalized_solution_set_hash_algorithm".to_owned(),
            "clearra-setup-candidate-set-fnv64-v1".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            candidate_count.to_string(),
        ),
        (
            "normalized_unique_solution_count".to_owned(),
            candidate_count.to_string(),
        ),
        (
            "solution_found".to_owned(),
            (candidate_count != 0).to_string(),
        ),
        ("normalized_solution_set_hash".to_owned(), set_hash.clone()),
        ("actual_normalized_solution_set_hash".to_owned(), set_hash),
    ];
    for (key, value) in overrides {
        let field = fields
            .iter_mut()
            .find(|(field_key, _)| field_key == key)
            .expect("fixture override field");
        field.1 = (*value).to_owned();
    }
    if let Some((key, value)) = duplicate {
        fields.push((key.to_owned(), value.to_owned()));
    }
    CoreExecutionResult::new(fields, Vec::new()).with_setup_finder_report(report)
}

fn setup_candidate_set_hash(report: &SetupFinderReport) -> String {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut conditions = report.hold_conditions().iter().collect::<Vec<_>>();
    conditions.sort_unstable_by(|left, right| left.condition_id().cmp(right.condition_id()));
    let mut hash = FNV_OFFSET;
    for condition in conditions {
        let mut candidates = condition.candidates().iter().collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|candidate| candidate.board_mask());
        for candidate in candidates {
            for byte in condition
                .condition_id()
                .bytes()
                .chain(core::iter::once(b'|'))
            {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            for shift in (0..10).rev() {
                let nibble = ((candidate.board_mask() >> (shift * 4)) & 0x0f) as u8;
                let byte = if nibble < 10 {
                    b'0' + nibble
                } else {
                    b'a' + nibble - 10
                };
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("css1:{hash:016x}")
}

fn one_piece_spin_query() -> SpinStructureQuery {
    let inventory = PieceInventory::from_pieces([PieceKind::T]).expect("inventory");
    let mut query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
    query.height = 4;
    query.fill_top = 4;
    query.line_requirement = SpinLineRequirement::Any;
    query.max_placements = Some(1);
    query.initial_board = [(4, 2), (6, 2), (4, 0)]
        .into_iter()
        .fold(StructureBoard::EMPTY, |board, (x, y)| {
            board.with_cell(x, y).expect("fixture cell")
        });
    query
}

fn queue_based_pieces(queue: &SetupQueueInput) -> String {
    queue
        .as_fixed_sequence()
        .map(|queue| pieces_string(queue.pieces()))
        .unwrap_or_default()
}

fn pieces_string(pieces: &[PieceKind]) -> String {
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}
