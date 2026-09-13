//! Shared assertions for the range-produced 1L..4L candidate fixtures.
//! Expected candidates are not copied from an offline result: both providers
//! independently enter the existing App finalizer for the same typed request.

use core::cell::Cell;

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_host_contract::ResourceBudget;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc4_tablebase::{Pc4RuleProfile, QualifiedSnapshotIdentity};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppResponse, AppServices,
    AppStatus, CooperativeAppAdvance, Pc4CandidateProductError, PcCandidateBoundaryError,
    PcCandidatePageGuard, PcCandidateReducerInput, PcCandidateSourceBinding, PcChanceIngressOrigin,
    PcMinimalsIngressOrigin, PcPathIngressOrigin, PcResultProjection, ProductCapabilityContract,
    ScenarioAppCommand,
};

#[derive(Clone, Copy, Debug)]
enum Product {
    All,
    Chance,
    Minimum,
    Replay,
}

fn request(lines: u8, profile: Pc4RuleProfile, initial: u64, product: Product) -> AppRequest {
    let rule = match profile {
        Pc4RuleProfile::Srs => srs(),
        Pc4RuleProfile::SrsPlus => srs_plus(),
        Pc4RuleProfile::SrsX => srs_x(),
        Pc4RuleProfile::Jstris180 => jstris_180(),
        Pc4RuleProfile::NoKick => no_kick(),
    };
    let (count, objective, projection, contract) = match product {
        Product::All => (
            PcCountPolicy::CountAll,
            ObjectivePolicy::all(),
            PcResultProjection::Standard,
            None,
        ),
        Product::Chance => (
            PcCountPolicy::CountUnique,
            ObjectivePolicy::unique(),
            PcResultProjection::ChanceProbabilityV2(PcChanceIngressOrigin::CanonicalPcChance),
            Some(ProductCapabilityContract::PcChance),
        ),
        Product::Minimum => (
            PcCountPolicy::CountUnique,
            ObjectivePolicy::minimum_cover(),
            PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals),
            Some(ProductCapabilityContract::PcMinimals),
        ),
        Product::Replay => (
            PcCountPolicy::CountAll,
            ObjectivePolicy::all(),
            PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
            Some(ProductCapabilityContract::PcPath),
        ),
    };
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(u16::from(lines), initial),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; usize::from(lines)])),
        PieceWindow::new(usize::from(lines)),
    )
    .with_exact_pieces(Some(usize::from(lines)))
    .with_allow_hold(false)
    .with_rule(rule)
    .with_count_policy(count)
    .with_objective(objective)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1),
    );
    let request = AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(projection),
    ));
    match contract {
        Some(contract) => request.with_product_capability_contract(contract).unwrap(),
        None => request,
    }
}

fn ordinary(context: &AppContext, request: AppRequest) -> AppResponse {
    let mut execution = context.start_cooperative_execution(request);
    for _ in 0..4096 {
        match execution.advance(256, &ExecutionControl::default()) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(response) => return response,
            other => panic!("unexpected ordinary product state: {other:?}"),
        }
    }
    panic!("bounded ordinary product fixture did not finish");
}

pub(super) fn assert_product_parity<G: PcCandidatePageGuard>(
    lines: u8,
    profile: Pc4RuleProfile,
    initial: u64,
    input: &PcCandidateReducerInput,
    guard: &G,
) {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for product in [
        Product::All,
        Product::Chance,
        Product::Minimum,
        Product::Replay,
    ] {
        let request = request(lines, profile, initial, product);
        let expected = ordinary(&context, request.clone());
        assert_eq!(
            expected.status(),
            AppStatus::Success,
            "{lines}L {profile:?} {product:?}: {expected:?}"
        );
        let mut actual = context
            .start_pc4_candidate_product(request, input, guard, &ExecutionControl::default())
            .unwrap_or_else(|error| panic!("{lines}L {profile:?} {product:?}: {error:?}"));
        assert_eq!(
            actual.evidence().universe_identity(),
            input.universe_identity()
        );
        let mut response = None;
        for _ in 0..4096 {
            match actual
                .advance(256, guard, &ExecutionControl::default())
                .unwrap()
            {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(completed) => {
                    response = Some(completed);
                    break;
                }
                other => panic!("unexpected online product state: {other:?}"),
            }
        }
        let response = response.expect("bounded online product fixture must complete");
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "{lines}L {profile:?} {product:?}: {response:?}"
        );
        assert!(matches!(
            actual.advance(256, guard, &ExecutionControl::default()),
            Err(Pc4CandidateProductError::AlreadyFinished)
        ));
        let core = response
            .render_model()
            .and_then(crate::AppRenderModel::core_result)
            .unwrap();
        let expected_core = expected
            .render_model()
            .and_then(crate::AppRenderModel::core_result)
            .unwrap();
        assert_eq!(
            core.normalized_solution_identities(),
            expected_core.normalized_solution_identities()
        );
        assert_eq!(
            core.normalized_solution_keys(),
            expected_core.normalized_solution_keys()
        );
        assert_eq!(
            core.solution_coverages(),
            expected_core.solution_coverages()
        );
        assert_eq!(
            core.coverage_pattern_words(),
            expected_core.coverage_pattern_words()
        );
        for field in [
            "unique_solution_count",
            "count_complete",
            "coverage_complete",
            "normalized_solution_set_hash",
        ] {
            assert_eq!(
                core.field(field),
                expected_core.field(field),
                "{lines}L {profile:?} {product:?}: {field}"
            );
        }
        if let Some(expected_product) = expected.product_capability_result() {
            let actual_product = response
                .product_capability_result()
                .expect("typed result must remain typed");
            assert_eq!(actual_product.contract(), expected_product.contract());
            assert_eq!(actual_product.result_kind(), expected_product.result_kind());
            assert_eq!(
                actual_product.resource_evidence(),
                expected_product.resource_evidence()
            );
            if let Some(expected_chance) = expected_product.pc_probability_v2() {
                let chance = actual_product.pc_probability_v2().unwrap();
                assert_eq!(
                    chance.coverage_pattern_words(),
                    expected_chance.coverage_pattern_words()
                );
                assert_eq!(
                    chance.weighted_probability_bits(),
                    expected_chance.weighted_probability_bits()
                );
                assert_eq!(chance.completeness(), expected_chance.completeness());
            }
            if let Some(expected_minimum) = expected_product.pc_minimum_cover_v2() {
                let minimum = actual_product.pc_minimum_cover_v2().unwrap();
                assert!(minimum.completeness().exact_minimum_proven());
                assert_eq!(
                    minimum.selected_solution_keys(),
                    expected_minimum.selected_solution_keys()
                );
                assert_eq!(
                    minimum.selected_solution_count(),
                    expected_minimum.selected_solution_count()
                );
                assert_eq!(
                    minimum.portfolio_alternatives().canonical_page(),
                    expected_minimum.portfolio_alternatives().canonical_page()
                );
            }
            if matches!(product, Product::Replay) {
                assert!(actual_product.pc_path_family_v2().is_some());
                // Public replay paging retains the ordinary source, not the
                // limited graph-observation page used by the input fixture.
                assert_eq!(core.path_steps(), expected_core.path_steps());
            }
        } else {
            assert!(response.product_capability_result().is_none());
        }
    }
    assert_rejections(&context, lines, profile, initial, input, guard);
}

struct RevokingGuard<'a, G> {
    inner: &'a G,
    snapshots_remaining: Cell<usize>,
}

impl<G: PcCandidatePageGuard> PcCandidatePageGuard for RevokingGuard<'_, G> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        self.inner.is_current_source(source)
    }
    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        let remaining = self.snapshots_remaining.get();
        self.snapshots_remaining.set(remaining.saturating_sub(1));
        remaining > 0 && self.inner.is_current_snapshot(snapshot)
    }
}

fn assert_rejections<G: PcCandidatePageGuard>(
    context: &AppContext,
    lines: u8,
    profile: Pc4RuleProfile,
    initial: u64,
    input: &PcCandidateReducerInput,
    guard: &G,
) {
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    let cancelled = ExecutionControl::new(cancellation);
    assert!(matches!(
        context.start_pc4_candidate_product(
            request(lines, profile, initial, Product::All),
            input,
            guard,
            &cancelled
        ),
        Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::Cancelled
        ))
    ));
    let finite = request(lines, profile, initial, Product::All)
        .with_resource_budget(ResourceBudget::new(1, None, Some(64)));
    assert!(matches!(
        context.start_pc4_candidate_product(finite, input, guard, &ExecutionControl::default()),
        Err(Pc4CandidateProductError::FiniteMemoryAuthorityRequired)
    ));
    let mismatched_profile = if profile == Pc4RuleProfile::Srs {
        Pc4RuleProfile::Jstris180
    } else {
        Pc4RuleProfile::Srs
    };
    assert!(matches!(
        context.start_pc4_candidate_product(
            request(lines, mismatched_profile, initial, Product::All),
            input,
            guard,
            &ExecutionControl::default()
        ),
        Err(Pc4CandidateProductError::Compatibility(_))
    ));
    let revoking_after_core = RevokingGuard {
        inner: guard,
        snapshots_remaining: Cell::new(2),
    };
    assert!(matches!(
        context.start_pc4_candidate_product(
            request(lines, profile, initial, Product::All),
            input,
            &revoking_after_core,
            &ExecutionControl::default(),
        ),
        Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::StaleSnapshot
        ))
    ));
    let mut execution = context
        .start_pc4_candidate_product(
            request(lines, profile, initial, Product::Minimum),
            input,
            guard,
            &ExecutionControl::default(),
        )
        .unwrap();
    let revoking = RevokingGuard {
        inner: guard,
        snapshots_remaining: Cell::new(1),
    };
    assert!(matches!(
        execution.advance(256, &revoking, &ExecutionControl::default()),
        Err(Pc4CandidateProductError::Source(
            PcCandidateBoundaryError::StaleSnapshot
        ))
    ));
    // Restoring freshness does not resurrect a rejected finalizer.
    assert!(matches!(
        execution.advance(256, guard, &ExecutionControl::default()),
        Err(Pc4CandidateProductError::AlreadyFinished)
    ));
}
