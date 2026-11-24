//! The all-candidate closure comparison checks the mathematical prefilter;
//! separate small product/concrete references check actual buildability.
//! No geometric family is replaced by a direct dependency solver.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::super::super::{
    geometry::{GeometryAdvance, GeometrySearch, PhysicalPrefixMode},
    packing_hold_projection,
    standard_bag_coverage::StandardBagCoverage,
    with_pc_placement_dependency_reference,
};
use super::*;

fn with_pruning<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CANDIDATE_ILC_OVERRIDE.set(self.0);
        }
    }
    let _restore = Restore(CANDIDATE_ILC_OVERRIDE.replace(Some(enabled)));
    run()
}

fn fixture(
    query: PcScenarioQuery,
    mode: PhysicalPrefixMode,
) -> (SearchProblem, GeometryCatalog, GeometrySearch) {
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    let locks = catalog.required_cells().count_ones() as usize / 4;
    let universe = problem.piece_source().materialized_universe().unwrap();
    let family = universe.packing_multiset_family_for_execution(
        locks,
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        packing_hold_projection(&problem),
    );
    // Mirror the product owner choice. Fixed/other finite sources consume a
    // concrete original-pattern index; only supported standard-bag owners
    // can omit it. A missing index is a fixture error, never a source negative.
    let concrete_pattern_index = !StandardBagCoverage::supports(universe, problem.initial_hold());
    let mut search = GeometrySearch::new(
        universe,
        &family,
        catalog.required_cells(),
        concrete_pattern_index,
    )
    .unwrap();
    search.enable_physical_prefix_pruning(mode);
    (problem, catalog, search)
}

fn mixed_query(initial: u64) -> PcScenarioQuery {
    let locks = (40 - initial.count_ones()) as usize / 4;
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, initial),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(locks),
    )
    .with_rule(clearra_rules::profile::builtin_rules::srs_plus())
    .with_allow_hold(true)
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_count_policy(PcCountPolicy::CountUnique)
}

fn one_o_query() -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0xf3fcf),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
        PieceWindow::new(1),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
}

fn next_candidate(
    catalog: &GeometryCatalog,
    search: &mut GeometrySearch,
) -> Option<GeometryCandidate> {
    for _ in 0..1_000_000 {
        match search.advance(catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => return Some(candidate),
            GeometryAdvance::Complete => return None,
            GeometryAdvance::ResourceIncomplete(reason) => panic!("finite fixture: {reason}"),
        }
    }
    panic!("finite partial-4L fixture must make progress");
}

fn product(
    enabled: bool,
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
) -> (CandidateBuildResult, BuildUpWorkspace) {
    let mut workspace = BuildUpWorkspace::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let result = with_pruning(enabled, || {
        verify_product(
            problem,
            catalog,
            candidate,
            target,
            &mut workspace,
            &mut evaluator,
            CandidateWitnessMode::Disabled,
            true,
            false,
            &ExecutionControl::default(),
            1,
        )
        .unwrap()
    });
    (result, workspace)
}

fn coverage_words(result: &CandidateBuildResult, workspace: &BuildUpWorkspace) -> Vec<u64> {
    if let Some(bits) = &result.covered_patterns {
        bits.to_owned_words()
    } else if let Some(root) = result.symbolic_coverage_root {
        workspace
            .materialize_standard_bag_root(root)
            .unwrap()
            .to_owned_words()
    } else {
        Vec::new()
    }
}

#[test]
fn candidate_ilc_switch_matches_all_completed_candidate_closures() {
    let mut checked = 0;
    let mut rejected = 0;
    for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
        let (_, catalog, mut search) = fixture(mixed_query(initial), PhysicalPrefixMode::Disabled);
        let mut enabled = BuildUpWorkspace::default();
        let mut disabled = BuildUpWorkspace::default();
        let mut fixture_count = 0;
        while let Some(candidate) = next_candidate(&catalog, &mut search) {
            assert!(!candidate.ilc_dependency_checked());
            let expected = RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(
                &catalog, &candidate,
            );
            assert_eq!(
                with_pruning(true, || candidate_ilc_rejects(
                    &catalog,
                    &candidate,
                    &mut enabled
                )),
                expected,
            );
            assert!(!with_pruning(false, || candidate_ilc_rejects(
                &catalog,
                &candidate,
                &mut disabled
            )));
            fixture_count += 1;
            rejected += usize::from(expected);
        }
        assert!(fixture_count > 0);
        assert_eq!(enabled.candidate_ilc_work().checks, fixture_count);
        assert_eq!(enabled.candidate_ilc_work().reused_prefix_certificates, 0);
        assert_eq!(disabled.candidate_ilc_work().checks, 0);
        assert_eq!(disabled.candidate_ilc_work().rejected_candidates, 0);
        assert_eq!(disabled.candidate_ilc_work().reused_prefix_certificates, 0);
        checked += fixture_count;
    }
    assert!(rejected > 0 && rejected < checked);
}

#[test]
fn candidate_ilc_negative_precedes_product_allocation_and_matches_concrete_reference() {
    let (problem, catalog, mut search) =
        fixture(mixed_query(0x3c0f_03c0f), PhysicalPrefixMode::Disabled);
    let candidate = loop {
        let candidate = next_candidate(&catalog, &mut search).expect("ILC negative fixture");
        if RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(&catalog, &candidate) {
            break candidate;
        }
    };
    let target = search.target(candidate.target_index).unwrap();
    let (pruned, workspace) = product(true, &problem, &catalog, &candidate, target);
    let (unpruned, _) = product(false, &problem, &catalog, &candidate, target);
    let mut concrete_workspace = BuildUpWorkspace::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let concrete = with_pc_placement_dependency_reference(|| {
        super::super::verify_candidate(
            &problem,
            &catalog,
            &candidate,
            target,
            &mut concrete_workspace,
            &mut evaluator,
            CandidateWitnessMode::Disabled,
            true,
            1,
            &ExecutionControl::default(),
        )
        .unwrap()
    });
    for result in [&pruned, &unpruned, &concrete] {
        assert!(!result.buildable);
        assert!(result.count_complete);
        assert!(result.covered_patterns.is_none());
        assert!(result.symbolic_coverage_root.is_none());
        assert!(result.witness_pattern_id.is_none());
        assert_eq!(result.build_variant_count, 0);
        assert!(result.representative_path.is_empty());
    }
    assert!(pruned.feasibility_rejected);
    assert_eq!(pruned.graph_nodes, 0);
    assert_eq!(pruned.coverage_product_words, 0);
    assert_eq!(pruned.coverage_product_states, 0);
    assert_eq!(pruned.coverage_product_edge_checks, 0);
    assert_eq!(workspace.reachability_metrics().lock_queries, 0);
    assert!(!workspace.standard_bag_coverage_initialized);
    assert_eq!(
        workspace.piece_order_languages.retained_bytes(),
        BuildUpWorkspace::default()
            .piece_order_languages
            .retained_bytes(),
    );
    assert_eq!(workspace.candidate_ilc_work().checks, 1);
    assert_eq!(workspace.candidate_ilc_work().rejected_candidates, 1);
    assert_eq!(workspace.candidate_ilc_work().reused_prefix_certificates, 0);
}

#[test]
fn candidate_ilc_prefix_certificate_reuses_only_immutable_emitted_rows() {
    let (problem, catalog, mut search) = fixture(one_o_query(), PhysicalPrefixMode::MarkCandidates);
    let candidate = next_candidate(&catalog, &mut search).unwrap();
    assert!(!candidate.physical_prefix_impossible());
    assert!(candidate.ilc_dependency_checked());
    let rebuilt =
        GeometryCandidate::from_rows(&catalog, candidate.target_index, candidate.row_ids())
            .unwrap();
    assert_eq!(candidate.identity, rebuilt.identity);
    assert!(!rebuilt.ilc_dependency_checked());
    let target = search.target(candidate.target_index).unwrap();
    assert!(
        target.pattern_index.is_some(),
        "fixed source owns its concrete index"
    );
    let (certified, reused_workspace) = product(true, &problem, &catalog, &candidate, target);
    let (checked, checked_workspace) = product(true, &problem, &catalog, &rebuilt, target);
    let (disabled, disabled_workspace) = product(false, &problem, &catalog, &candidate, target);
    assert!(certified.buildable);
    for (result, workspace) in [
        (&checked, &checked_workspace),
        (&disabled, &disabled_workspace),
    ] {
        assert_eq!(result.buildable, certified.buildable);
        assert_eq!(result.count_complete, certified.count_complete);
        assert_eq!(result.witness_pattern_id, certified.witness_pattern_id);
        assert_eq!(result.build_variant_count, certified.build_variant_count);
        assert_eq!(result.representative_path, certified.representative_path);
        assert_eq!(
            coverage_words(result, workspace),
            coverage_words(&certified, &reused_workspace)
        );
    }
    assert_eq!(reused_workspace.candidate_ilc_work().checks, 0);
    assert_eq!(
        reused_workspace
            .candidate_ilc_work()
            .reused_prefix_certificates,
        1
    );
    assert_eq!(checked_workspace.candidate_ilc_work().checks, 1);
    assert_eq!(
        checked_workspace
            .candidate_ilc_work()
            .reused_prefix_certificates,
        0
    );
    assert_eq!(disabled_workspace.candidate_ilc_work().checks, 0);
    assert_eq!(
        disabled_workspace
            .candidate_ilc_work()
            .reused_prefix_certificates,
        0
    );
}

#[test]
fn candidate_ilc_cancelled_request_does_not_publish_a_negative() {
    let (problem, catalog, mut search) = fixture(one_o_query(), PhysicalPrefixMode::Disabled);
    let candidate = next_candidate(&catalog, &mut search).unwrap();
    let target = search.target(candidate.target_index).unwrap();
    let control = ExecutionControl::default();
    control.cancellation.handle().cancel();
    let mut workspace = BuildUpWorkspace::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let result = with_pruning(true, || {
        verify_product(
            &problem,
            &catalog,
            &candidate,
            target,
            &mut workspace,
            &mut evaluator,
            CandidateWitnessMode::Disabled,
            false,
            false,
            &control,
            1,
        )
    });
    assert!(matches!(result, Err(WasmExactSearchError::Cancelled)));
    assert_eq!(workspace.candidate_ilc_work().checks, 0);
    assert_eq!(workspace.candidate_ilc_work().rejected_candidates, 0);
    assert_eq!(workspace.candidate_ilc_work().reused_prefix_certificates, 0);
}
