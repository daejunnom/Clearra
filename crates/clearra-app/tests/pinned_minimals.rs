use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppServices, AppStatus,
    CoveragePortfolioAlternativeSet, PcMinimalsIngressOrigin, PcResultProjection,
    PortfolioAlternativeSetIdentity, ProductCapabilityContract, ScenarioAppCommand,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_host_contract::ProductResultPayloadContent;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn row(patterns: &[u32]) -> PatternBitSet {
    PatternBitSet::from_pattern_indices(3, patterns.to_vec()).unwrap()
}

fn identity() -> PortfolioAlternativeSetIdentity {
    PortfolioAlternativeSetIdentity::new("query", "source", "profile", "universe", "build").unwrap()
}

#[test]
fn mandatory_solution_changes_exact_minimum_without_changing_public_coverage() {
    let keys = ["a", "b", "c", "d"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let rows = vec![row(&[0, 1]), row(&[1, 2]), row(&[0]), row(&[2])];
    let ordinary = CoveragePortfolioAlternativeSet::new_canonical(
        identity(),
        keys.clone(),
        PatternBitSet::all(3),
        rows.clone(),
    )
    .unwrap();
    let pinned = CoveragePortfolioAlternativeSet::new_canonical_with_pinned_keys(
        identity(),
        keys,
        PatternBitSet::all(3),
        rows,
        vec!["c".to_owned(), "a".to_owned()],
    )
    .unwrap();

    assert_eq!(ordinary.optimal_cardinality(), 2);
    assert_eq!(pinned.optimal_cardinality(), 3);
    assert_eq!(pinned.pinned_candidate_ids(), [3, 1]);
    assert_eq!(
        pinned.canonical_candidate_keys_owned().unwrap(),
        ["a", "b", "c"]
    );
    assert_eq!(pinned.required_patterns().pattern_count(), 3);
    assert!(pinned
        .coverage_rows()
        .iter()
        .all(|row| row.pattern_count() == 3));
    assert_ne!(pinned.set_identity_sha256(), ordinary.set_identity_sha256());
    assert_eq!(
        pinned.candidate_map_sha256(),
        ordinary.candidate_map_sha256()
    );
}

fn pc_request(pins: Vec<String>) -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    )
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::minimum_cover());
    let command = ScenarioAppCommand::new(query)
        .with_result_projection(PcResultProjection::MinimumCoverV2(
            PcMinimalsIngressOrigin::CanonicalPcMinimals,
        ))
        .with_pinned_minimum_keys(pins);
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .unwrap()
}

#[test]
fn pc_product_replays_full_source_and_proves_a_pinned_minimum() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let ordinary = context.run(pc_request(Vec::new()));
    assert_eq!(ordinary.status(), AppStatus::Success, "{ordinary:?}");
    let ordinary_report = ordinary
        .product_capability_result()
        .unwrap()
        .pc_minimum_cover_v2()
        .unwrap();
    let key = ordinary_report.portfolio_alternatives().candidates()[0]
        .normalized_key()
        .to_owned();
    let pinned = context.run(pc_request(vec![key.clone()]));
    assert_eq!(pinned.status(), AppStatus::Success, "{pinned:?}");
    let report = pinned
        .product_capability_result()
        .unwrap()
        .pc_minimum_cover_v2()
        .unwrap();
    assert!(report.completeness().complete());
    assert_eq!(
        report.portfolio_alternatives().pinned_candidate_ids().len(),
        1
    );
    assert!(report.selected_solution_keys().contains(&key));
    let payload = pinned
        .product_capability_result()
        .unwrap()
        .public_result_payload()
        .expect("pinned PC product payload");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("pinned PC product must retain its coverage portfolio payload");
    };
    assert_eq!(page.pinned_candidate_keys(), &[key.clone()]);
    assert_eq!(
        report.required_pattern_count(),
        ordinary_report.required_pattern_count()
    );
    assert_eq!(
        report.source_solution_count(),
        ordinary_report.source_solution_count()
    );
}
