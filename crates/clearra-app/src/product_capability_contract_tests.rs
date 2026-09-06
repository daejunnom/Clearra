// SRP rationale: this test module has one behavior-level change reason: verifying product capability validation and query-bound authority invariants.

use std::sync::{Arc, Mutex};

use clearra_core_domain::{
    execution_cancellation::{ExecutionProgress, ProgressSink},
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind,
    resource::ResourceReport as CoreResourceReport,
};
use clearra_core_executor::PercentService;
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend};
use clearra_host_contract::{
    ExecutionAvailabilityReason, ExecutionAvailabilityReport, ExecutionCompletenessState,
    ExecutionSurface, ProductResultPayloadContent, QueryEnvelope, ResourceReport,
};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy,
    score_objective_policy::{ScoreProfileSelection, SpinProfileSelection},
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{PcChanceEvidencePolicy, ProblemCompiler, SearchProblem};
use clearra_replay::ExactScoringExecutionBatch;
use clearra_rules::profile::{
    builtin_rules::{srs, srs_plus},
    rule_profile::RuleProfile,
};
use clearra_supply::{
    pattern_universe::{MaterializedPatternUniverseStructure, PatternUniverseMaterializer},
    queue::{
        fixed_sequence::FixedSequence, observed_queue::ObservedQueue,
        queue_pattern_expression::QueuePatternExpression,
    },
    QueueObservationPolicy,
};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{
    app_command::RunnableAppCommand,
    app_response::{AppResponse, AppStatus},
    app_services::AppPcFailedQueueExecutionError,
    pc_allspin_result::project_pc_allspin_result,
    pc_failed_queue_result::validate_failed_examples,
    pc_score_summary_result::{PcScoreCompiledAuthority, PcScoreCompiledAuthorityError},
    product_capability_contract::{PcScoreQueryBinding, ValidatedProductCapabilityContract},
    AppCommand, AppCommandKind, AppContext, AppCoreExecutorService, AppError, AppErrorCode,
    AppExecutionContext, AppRequest, AppServices, CooperativeAppAdvance,
    DistributedSearchPreparation, ExecutionCancellationToken, ExecutionControl, PcAppCommand,
    PcChanceIngressOrigin, PcChanceQuerySnapshot, PcFailedQueueIngressOrigin,
    PcMinimalsIngressOrigin, PcMinimumCoverQuerySnapshot, PcPathIngressOrigin, PcResultProjection,
    PcScoreIngressOrigin, PcScoreQuerySnapshot, PcTilingIngressOrigin, PcTilingQuerySnapshot,
    PercentAppCommand, ProductCapabilityContract, ProductCapabilityContractError,
    ProductCapabilityResultKind, ScenarioAppCommand, PC_SCORE_MAX_PATTERNS,
};

fn pc_score_execution_policy() -> PcExecutionPolicy {
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(1)
        .with_allow_backend_fallback(false)
        .with_max_patterns(PC_SCORE_MAX_PATTERNS)
}

fn failed_queue_query(piece: PieceKind) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique())
}

fn failed_queue_command(
    origin: PcFailedQueueIngressOrigin,
    piece: PieceKind,
    failed_pattern_limit: usize,
) -> PercentAppCommand {
    PercentAppCommand::pc_failed_queue(failed_queue_query(piece), origin)
        .with_failed_pattern_limit(failed_pattern_limit)
}

fn failed_queue_request(
    origin: PcFailedQueueIngressOrigin,
    piece: PieceKind,
    failed_pattern_limit: usize,
) -> AppRequest {
    AppRequest::new(AppCommand::Percent(failed_queue_command(
        origin,
        piece,
        failed_pattern_limit,
    )))
    .with_product_capability_contract(ProductCapabilityContract::PcFailedQueue)
    .expect("valid typed pc failed-queue product capability")
}

#[test]
fn pc_failed_queue_examples_rebind_to_the_exact_first_uncovered_universe_rows() {
    let explicit_expression = QueuePatternExpression::parse("[IO]", 2).expect("two-pattern queue");
    let factorized_expression =
        QueuePatternExpression::parse("P7", 5_040).expect("factorized seven-bag queue");
    let universes = [
        (
            PatternUniverseMaterializer::queue_pattern_expression(&explicit_expression, 0x101)
                .expect("explicit universe"),
            MaterializedPatternUniverseStructure::Explicit,
        ),
        (
            PatternUniverseMaterializer::standard_7_bag(1, 0, 0x102)
                .expect("standard seven-bag universe"),
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len: 1 },
        ),
        (
            PatternUniverseMaterializer::observed(&ObservedQueue::default(), 1, 0, 0x103)
                .expect("observed seven-bag universe"),
            MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
                sequence_len: 1,
                observed_len: 0,
                boundary_candidate_count: 7,
            },
        ),
        (
            PatternUniverseMaterializer::queue_pattern_expression(&factorized_expression, 0x104)
                .expect("factorized universe"),
            MaterializedPatternUniverseStructure::FactorizedQueueExpression { sequence_len: 7 },
        ),
    ];

    for (universe, expected_structure) in universes {
        assert_eq!(universe.structure(), expected_structure);
        assert!(universe.complete());
        assert!(universe.pattern_count() >= 2);
        let first = universe.sequence_at(0).into_owned();
        let second = universe.sequence_at(1).into_owned();
        let exact_first_two = [(0_usize, first), (1_usize, second)];
        validate_failed_examples(
            &universe,
            &[0_u64],
            universe.pattern_count(),
            exact_first_two
                .iter()
                .map(|(index, pieces)| (*index, pieces.as_slice())),
            2,
        )
        .expect("the exact first two uncovered patterns for every universe structure");
    }

    let universe =
        PatternUniverseMaterializer::queue_pattern_expression(&explicit_expression, 0x105)
            .expect("explicit rejection universe");
    let first = universe.sequence_at(0).into_owned();
    let second = universe.sequence_at(1).into_owned();

    let missing = [(0_usize, first.clone())];
    let missing_error = validate_failed_examples(
        &universe,
        &[0_u64],
        2,
        missing
            .iter()
            .map(|(index, pieces)| (*index, pieces.as_slice())),
        2,
    )
    .expect_err("missing examples must be rejected");
    assert_eq!(
        missing_error.reason(),
        "failed-queue example count does not match the requested limit"
    );

    let extra = [(0_usize, first.clone()), (1_usize, second.clone())];
    let extra_error = validate_failed_examples(
        &universe,
        &[0_u64],
        2,
        extra
            .iter()
            .map(|(index, pieces)| (*index, pieces.as_slice())),
        1,
    )
    .expect_err("extra examples must be rejected");
    assert_eq!(
        extra_error.reason(),
        "failed-queue example count does not match the requested limit"
    );

    let covered_but_in_range = [(0_usize, first.clone())];
    let covered_error = validate_failed_examples(
        &universe,
        &[1_u64],
        2,
        covered_but_in_range
            .iter()
            .map(|(index, pieces)| (*index, pieces.as_slice())),
        1,
    )
    .expect_err("a covered pattern cannot be a failed example");
    assert_eq!(
        covered_error.reason(),
        "failed-queue examples are not the exact first uncovered patterns"
    );

    let wrong_pieces = [(0_usize, second.clone())];
    let pieces_error = validate_failed_examples(
        &universe,
        &[0_u64],
        2,
        wrong_pieces
            .iter()
            .map(|(index, pieces)| (*index, pieces.as_slice())),
        1,
    )
    .expect_err("example pieces must come from the compiled universe row");
    assert_eq!(
        pieces_error.reason(),
        "failed-queue example pieces do not match the compiled pattern universe"
    );

    let skipped_first_failure = [(1_usize, second)];
    let order_error = validate_failed_examples(
        &universe,
        &[0_u64],
        2,
        skipped_first_failure
            .iter()
            .map(|(index, pieces)| (*index, pieces.as_slice())),
        1,
    )
    .expect_err("a later failure cannot replace the first uncovered pattern");
    assert_eq!(
        order_error.reason(),
        "failed-queue examples are not the exact first uncovered patterns"
    );
}

fn chance_command() -> PcAppCommand {
    chance_command_for_target(PcTarget::two_lines())
}

fn chance_command_for_target(target: PcTarget) -> PcAppCommand {
    let profile = SpinProfileSelection::AllSpinPlus;
    let queue = PcQueueInput::pattern_expression(
        QueuePatternExpression::parse("[TIOSZ]!", 120).expect("five-piece pattern queue"),
    );
    let query = OpeningPcSearchQuery::new(target)
        .with_queue(queue)
        .with_objective(ObjectivePolicy::unique().with_back_to_back_preservation(profile));
    PcAppCommand::new(query)
        .with_result_projection(PcResultProjection::AllSpinPreservationChance(profile))
}

fn uncontracted_chance_request() -> AppRequest {
    AppRequest::new(AppCommand::Pc(chance_command()))
}

fn chance_request() -> AppRequest {
    uncontracted_chance_request()
        .with_product_capability_contract(ProductCapabilityContract::PcAllSpinPreservationChance)
        .expect("valid typed product capability")
}

fn probability_command(origin: PcChanceIngressOrigin) -> ScenarioAppCommand {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique());
    ScenarioAppCommand::new(query)
        .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin))
}

fn uncontracted_probability_request(origin: PcChanceIngressOrigin) -> AppRequest {
    AppRequest::new(AppCommand::Scenario(probability_command(origin)))
}

fn probability_request(origin: PcChanceIngressOrigin) -> AppRequest {
    uncontracted_probability_request(origin)
        .with_product_capability_contract(ProductCapabilityContract::PcChance)
        .expect("valid pc chance product capability")
}

fn minimals_command() -> ScenarioAppCommand {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::minimum_cover());
    ScenarioAppCommand::new(query).with_result_projection(PcResultProjection::MinimumCoverV2(
        PcMinimalsIngressOrigin::CanonicalPcMinimals,
    ))
}

fn minimals_request() -> AppRequest {
    AppRequest::new(AppCommand::Scenario(minimals_command()))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .expect("valid pc minimals product capability")
}

fn multi_candidate_minimals_request() -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        PieceWindow::new(5),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(5))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::minimum_cover());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals),
    );
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .expect("valid multi-candidate pc minimals product capability")
}

fn multi_candidate_minimals_probability_request() -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        PieceWindow::new(5),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(5))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_objective(ObjectivePolicy::minimum_cover());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals),
    );
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .expect("valid probability-bearing pc minimals product capability")
}

fn path_request() -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
    );
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcPath)
        .expect("valid pc path product capability")
}

fn paged_path_request(pattern: &str, board: u64, pieces: usize, hold: bool) -> AppRequest {
    paged_path_request_at_height(4, pattern, board, pieces, hold)
}

fn paged_path_request_at_height(
    height: u16,
    pattern: &str,
    board: u64,
    pieces: usize,
    hold: bool,
) -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, board),
        PcQueueInput::pattern_expression(QueuePatternExpression::parse(pattern, 5040).unwrap()),
        PieceWindow::new(pieces),
    )
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(hold)
    .with_execution_policy(pc_score_execution_policy())
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all());
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(PcResultProjection::PathFamilyV2(
            PcPathIngressOrigin::CanonicalPcPath,
        )),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcPath)
    .unwrap()
}

fn run_paged_path(request: AppRequest) -> AppResponse {
    let context =
        AppContext::new(AppServices::default().with_core_executor(
            AppCoreExecutorService::wasm_cpu().with_product_retention_budget(
                crate::ProductRetentionBudget::new(64 * 1024 * 1024),
            ),
        ));
    let mut execution = context.start_cooperative_execution(request);
    let control = ExecutionControl::default();
    for _ in 0..100_000 {
        match execution.advance(8192, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(response) => return response,
            other => panic!("unexpected replay advance: {other:?}"),
        }
    }
    panic!("replay fixture did not complete within its test-only advance budget")
}

#[test]
fn cooperative_pc_replay_pages_preserve_pattern_family_and_full_copy() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = paged_path_request_at_height(2, "[OI]O", 0xfc3f0, 2, false);
    let eager_context =
        AppContext::new(AppServices::default().with_core_executor(
            AppCoreExecutorService::wasm_cpu().with_product_retention_budget(
                crate::ProductRetentionBudget::new(512 * 1024 * 1024),
            ),
        ));
    let eager = eager_context.run(request.clone());
    assert_eq!(eager.status(), AppStatus::Success, "{:?}", eager.error());
    let response = run_paged_path(request);
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "{:?}",
        response.error()
    );
    let eager_report = eager
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    let report = response
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    assert_eq!(report.witness_count(), eager_report.witness_count());
    let source = report.page_source().expect("lazy replay source");
    assert_eq!(
        source.checked_retained_capacity_bytes(),
        source.checked_full_capacity_recount()
    );
    let mut store = crate::PcReplayPageStore::new(Arc::clone(source));
    let mut copied = Vec::new();
    for geometry in 1..=source.geometry_count() {
        let first = store
            .page(geometry, 1, &ExecutionControl::default())
            .unwrap();
        let member_pages: usize = first.metadata.member_page_count.parse().unwrap();
        for member in 1..=member_pages {
            let page = store
                .page(geometry, member, &ExecutionControl::default())
                .unwrap();
            assert_eq!(
                page.metadata.page_source_identity_sha256,
                source.identity_sha256()
            );
            assert!(page.witnesses.len() <= 100);
            copied.extend(page.witnesses.into_iter().map(|w| {
                (
                    w.candidate_id().to_owned(),
                    w.pattern_id().to_owned(),
                    w.trace_identity().to_owned(),
                )
            }));
        }
    }
    let expected = eager_report
        .witnesses()
        .iter()
        .map(|w| {
            (
                w.candidate_id().to_string(),
                w.pattern_id().to_string(),
                w.trace_identity().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        copied, expected,
        "copy includes every member of every selected geometry"
    );
    let cancelled = ExecutionCancellationToken::new();
    cancelled.handle().cancel();
    assert_eq!(
        store.page(1, 1, &ExecutionControl::new(cancelled)),
        Err("complete_replay_cancelled")
    );
}

#[test]
fn cooperative_pc_replay_pages_keep_same_pattern_operation_order_identity() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = replay_collision_path_request();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let eager = context.run(request.clone());
    let paged = run_paged_path(request);
    assert_eq!(eager.status(), AppStatus::Success, "{:?}", eager.error());
    assert_eq!(paged.status(), AppStatus::Success, "{:?}", paged.error());
    let eager = eager
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    let paged = paged
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    assert!(
        eager.witnesses().windows(2).any(|pair| {
            pair[0].candidate_id() == pair[1].candidate_id()
                && pair[0].pattern_id() == pair[1].pattern_id()
                && pair[0].trace_identity() != pair[1].trace_identity()
        }),
        "fixture must retain distinct operation orders for one candidate and pattern"
    );
    let source = paged.page_source().unwrap();
    let mut store = crate::PcReplayPageStore::new(Arc::clone(source));
    let mut actual = Vec::new();
    for geometry in 1..=source.geometry_count() {
        let first = store
            .page(geometry, 1, &ExecutionControl::default())
            .unwrap();
        let count: usize = first.metadata.member_page_count.parse().unwrap();
        for member in 1..=count {
            let page = store
                .page(geometry, member, &ExecutionControl::default())
                .unwrap();
            actual.extend(page.witnesses.into_iter().map(|w| {
                (
                    w.candidate_id().to_owned(),
                    w.pattern_id().to_owned(),
                    w.normalized_trace_key().to_owned(),
                    w.trace_identity().to_owned(),
                )
            }));
        }
    }
    let expected: Vec<_> = eager
        .witnesses()
        .iter()
        .map(|w| {
            (
                w.candidate_id().to_string(),
                w.pattern_id().to_string(),
                w.normalized_trace_key().to_owned(),
                w.trace_identity().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        actual, expected,
        "page slicing must preserve the complete canonical four-key ordering"
    );
}

#[test]
fn cooperative_pc_replay_p7_ctk3_finishes_without_eager_whole_family_retention() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let response = run_paged_path(paged_path_request("[TIOSZJL]!", 0x3c0f03c0f, 6, true));
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "{:?}",
        response.error()
    );
    let report = response
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    let source = report.page_source().unwrap();
    assert_eq!(
        source.checked_retained_capacity_bytes(),
        source.checked_full_capacity_recount()
    );
    assert_eq!(source.materialized_pattern_count(), 5040);
    assert!(source.witness_count() > 100);
    assert!(source.geometry_count() > 1);
    assert!(!report.witnesses().is_empty() && report.witnesses().len() <= 100);
    assert!(source.checked_retained_capacity_bytes().unwrap() < 32 * 1024 * 1024);
    eprintln!(
        "pc-replay P7 geometries={} witnesses={} source_bytes={}",
        source.geometry_count(),
        source.witness_count(),
        source.checked_retained_capacity_bytes().unwrap()
    );
}

#[test]
fn pc_path_eager_identity_alias_preserves_normalization_while_lazy_requires_canonical_identity() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = replay_collision_path_request();
    let AppCommand::Scenario(command) = request.command() else {
        panic!("scenario fixture");
    };
    let query = Arc::new(command.query().clone());
    let problem = crate::pc_path_result::PcPathQueryBinding::Scenario(&query)
        .compile_expected()
        .unwrap()
        .with_pc_path_v2_evidence();
    let result =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap();
    let (materialized, _) = clearra_postprocess::ExactScoringExecutionMaterializer::materialize_complete_replay_cell_with_limits(
        &result.exact_scoring_execution_batches()[0], 0, 0,
        &ExecutionControl::default(),
        clearra_postprocess::ExactReplayMaterializationLimits::new(10_000, 256, 64 * 1024 * 1024),
    ).unwrap();
    let aggregate = &materialized.aggregates()[0];
    let member = &aggregate.executions()[0];
    let (_, _, trace) = member.clone().into_parts();
    let canonical = trace.canonical_key();
    let execution = clearra_core_executor::CorePostProcessExecution::new(
        aggregate.candidate_id(),
        0,
        "external-nonempty-identity".to_owned(),
        trace,
    );
    let projection = crate::pc_path_result::PcPathProjectionContext::from_problem(&problem);
    let eager = crate::pc_path_result::project_execution_with_context(projection, &execution, 1, 1)
        .unwrap();
    assert_eq!(eager.trace_identity(), "external-nonempty-identity");
    assert_eq!(eager.normalized_trace_key(), canonical);
    crate::pc_path_result::validate_execution_with_context(projection, &execution, 1).unwrap();
    assert_eq!(
        crate::pc_path_result::project_canonical_execution_with_context(
            projection, &execution, 1, 1
        ),
        Err("pc path canonical trace identity is invalid"),
    );
}

#[test]
fn cooperative_pc_replay_manifest_cancellation_never_publishes_a_partial_family() {
    struct CancelAtManifest(ExecutionCancellationToken);
    impl ProgressSink for CancelAtManifest {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "complete-replay-pattern" {
                self.0.handle().cancel();
            }
        }
    }
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context =
        AppContext::new(AppServices::default().with_core_executor(
            AppCoreExecutorService::wasm_cpu().with_product_retention_budget(
                crate::ProductRetentionBudget::new(64 * 1024 * 1024),
            ),
        ));
    let mut execution = context.start_cooperative_execution(replay_collision_path_request());
    let cancellation = ExecutionCancellationToken::new();
    let control = ExecutionControl::new(cancellation.clone())
        .with_progress_sink(Arc::new(CancelAtManifest(cancellation.clone())));
    for _ in 0..10_000 {
        match execution.advance(8192, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => {
                assert!(cancellation.is_cancelled());
                return;
            }
            other => panic!("a cancelled partial manifest must not publish: {other:?}"),
        }
    }
    panic!("tiny replay cancellation fixture exceeded its test-only advance budget")
}

fn replay_collision_path_request() -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0xfc3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 2])),
        PieceWindow::new(2),
    )
    .with_allow_hold(false)
    .with_execution_policy(pc_score_execution_policy())
    .with_exact_pieces(Some(2))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
    );
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcPath)
        .expect("valid replay-collision pc path product capability")
}

#[test]
fn direct_wasm_pc_path_returns_every_query_bound_replay_with_supply_and_line_clear_evidence() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(path_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let wrapper = response
        .product_capability_result()
        .expect("typed pc path wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcPath);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcPathFamilyV2
    );
    let report = wrapper
        .pc_path_family_v2()
        .expect("pc-path-family.v2 report");
    assert_eq!(report.contract_id(), "pc-path-family.v2");
    assert_eq!(report.witness_contract(), "pc-path-witness.v2");
    assert!(report.completeness().complete());
    assert!(!report.witnesses().is_empty());
    assert!(report.witnesses().windows(2).all(|pair| {
        (
            pair[0].candidate_id(),
            pair[0].pattern_id(),
            pair[0].normalized_trace_key(),
        ) <= (
            pair[1].candidate_id(),
            pair[1].pattern_id(),
            pair[1].normalized_trace_key(),
        )
    }));
    let canonical_witness = report
        .canonical_witness()
        .expect("pc.path core-owned canonical witness");
    assert_eq!(Some(canonical_witness), report.witnesses().first());
    assert!(report
        .witnesses()
        .iter()
        .all(|witness| witness.candidate_id() >= canonical_witness.candidate_id()));
    let witness = &report.witnesses()[0];
    assert_eq!(witness.pattern_id(), 0);
    assert_eq!(witness.steps().len(), 1);
    let step = &witness.steps()[0];
    assert_eq!(step.active_piece(), PieceKind::I);
    assert_eq!(step.input_cursor(), 0);
    assert_eq!(step.output_cursor(), 1);
    assert_eq!(step.cleared_lines(), 1);
    assert_eq!(step.cleared_row_mask(), 1);
    assert_eq!(step.board_after_line_clear_mask(), 0);

    let payload = wrapper
        .public_result_payload()
        .expect("public pc path payload");
    assert_eq!(payload.contract(), "pc.path");
    assert_eq!(payload.result_kind(), "pc-path-family.v2");
    let ProductResultPayloadContent::PcPathFamily(payload) = payload.content() else {
        panic!("expected ordinary pc path family payload")
    };
    assert!(payload.complete());
    assert_eq!(
        payload.witness_count(),
        payload.witnesses().len().to_string()
    );
    assert_eq!(payload.canonical_selection(), report.canonical_selection());
    assert_eq!(payload.canonical_witness(), payload.witnesses().first());
}

#[test]
fn pc_path_keeps_distinct_operation_orders_after_their_dag_states_merge() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(replay_collision_path_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let public_core = response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("pc path render model");
    assert!(public_core.postprocess_executions().is_empty());
    assert!(public_core.exact_scoring_execution_batches().is_empty());
    let report = response
        .product_capability_result()
        .and_then(|wrapper| wrapper.pc_path_family_v2())
        .expect("complete replay-collision path family");
    assert!(report.completeness().complete());

    let repeated_candidate = report.witnesses().iter().find_map(|left| {
        report.witnesses().iter().find(|right| {
            left.producer_candidate_id() == right.producer_candidate_id()
                && left.trace_identity() != right.trace_identity()
        })
    });
    assert!(
        repeated_candidate.is_some(),
        "two operation orders that merge at one terminal state must both survive"
    );
    assert!(report.witnesses().iter().all(|witness| {
        witness.consumed_piece_count() == 2
            && witness.terminal_hold_piece().is_none()
            && witness
                .steps()
                .last()
                .is_some_and(|step| step.board_after_line_clear_mask() == 0)
    }));
}

#[test]
fn direct_wasm_pc_minimals_returns_one_query_bound_exact_minimum_cover_wrapper() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let expected_query =
        PcMinimumCoverQuerySnapshot::Scenario(Arc::new(minimals_command().query().clone()));
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(minimals_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let wrapper = response
        .product_capability_result()
        .expect("typed pc minimum-cover wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcMinimals);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcMinimumCoverV2
    );
    assert_eq!(wrapper.validation_count(), 1);
    assert!(wrapper.resource_evidence().probability_complete());

    let report = wrapper
        .pc_minimum_cover_v2()
        .expect("pc-minimum-cover.v2 report");
    assert_eq!(report.contract_id(), "pc-minimum-cover.v2");
    assert_eq!(report.problem_contract_id(), "pc-clear-to-empty.v2");
    assert_eq!(report.input_contract_id(), "pc-pattern.v2");
    assert_eq!(
        report.origin(),
        PcMinimalsIngressOrigin::CanonicalPcMinimals
    );
    assert_eq!(report.query(), &expected_query);
    assert!(report.completeness().complete());
    assert!(report.completeness().exact_minimum_proven());
    assert!(report.completeness().query_bound());
    assert_eq!(report.selected_solution_count(), 1);
    assert_eq!(report.selected_solution_keys().len(), 1);
    let alternatives = report.portfolio_alternatives();
    assert_eq!(alternatives.contract_id(), "portfolio-alternative-set.v1");
    assert_eq!(
        alternatives.candidates().len(),
        report.source_solution_count()
    );
    assert_eq!(
        alternatives
            .canonical_page()
            .portfolio()
            .candidate_ids()
            .len(),
        report.selected_solution_count()
    );
    assert!(alternatives
        .identity()
        .source_identity()
        .ends_with(alternatives.identity().query_identity()));
    assert!(alternatives.checked_retained_capacity_bytes().is_some());

    let (canonical_candidate_id, canonical_solution_key) = report
        .canonical_candidate()
        .expect("pc.minimals App-owned canonical witness");
    let payload = wrapper
        .public_result_payload()
        .expect("public pc.minimals payload");
    let ProductResultPayloadContent::CoveragePortfolio(payload) = payload.content() else {
        panic!("expected pc.minimals coverage portfolio payload")
    };
    assert_eq!(
        payload.canonical_selection(),
        Some(report.canonical_selection())
    );
    let canonical_witness = payload
        .canonical_witness()
        .expect("pc.minimals payload canonical witness");
    assert_eq!(
        canonical_witness.candidate_id(),
        canonical_candidate_id.to_string()
    );
    assert_eq!(
        canonical_witness.normalized_solution_key(),
        canonical_solution_key
    );
    assert_eq!(Some(canonical_witness), payload.members().first());

    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("public pc minimals Core result");
    assert!(core.pc_chance_coverage_evidence().is_none());
    assert_eq!(core.bool_field("minimum_cover_complete"), Some(false));
    assert_eq!(core.bool_field("minimum_cover_proven_minimum"), Some(false));
    assert_eq!(
        core.field("minimum_cover_incomplete_reason"),
        Some("deferred-to-coordinator")
    );
    assert_eq!(
        core.normalized_solution_keys(),
        report.selected_solution_keys()
    );
    assert_eq!(
        core.field("normalized_solution_set_hash"),
        Some(report.normalized_solution_set_hash())
    );
}

#[test]
fn direct_wasm_pc_minimals_enumerates_every_iiooo_single_member_tie() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(multi_candidate_minimals_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let result = response
        .product_capability_result()
        .expect("typed multi-candidate pc minimum-cover wrapper");
    let report = result
        .pc_minimum_cover_v2()
        .expect("multi-candidate pc-minimum-cover.v2 report");
    assert!(report.completeness().complete());
    assert_eq!(report.selected_solution_count(), 1);
    assert_eq!(report.source_solution_count(), 4);
    assert_eq!(report.portfolio_alternatives().candidates().len(), 4);
    assert!(report.selected_solution_probabilities().is_empty());
    assert!(result.public_page_source_owner().is_some());

    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("deferred four-row pc.minimals Core result");
    assert_eq!(core.bool_field("minimum_cover_complete"), Some(false));
    assert_eq!(
        core.field("minimum_cover_incomplete_reason"),
        Some("deferred-to-coordinator")
    );
    assert_eq!(core.normalized_solution_keys().len(), 4);
    assert_eq!(core.normalized_solution_coverages().len(), 4);
    assert_eq!(
        core.usize_field("minimum_cover_selected_solution_count"),
        Some(4)
    );
    assert_ne!(
        core.field("normalized_solution_set_hash"),
        Some(report.normalized_solution_set_hash()),
        "the deferred Core hash describes all four rows, while the typed report describes the canonical singleton"
    );

    let mut store = report
        .portfolio_alternatives()
        .open_store()
        .expect("IIOOO exact alternative store");
    let mut portfolios = vec![report
        .portfolio_alternatives()
        .canonical_page()
        .portfolio()
        .candidate_ids()
        .to_vec()];
    for _ in 0..4 {
        let advance = store
            .next_page(u64::MAX, &mut || false)
            .expect("next IIOOO exact alternative");
        if let Some(page) = advance.page() {
            portfolios.push(page.portfolio().candidate_ids().to_vec());
        }
        if advance.checkpoint().enumeration_complete() {
            break;
        }
    }
    assert_eq!(portfolios, [vec![1], vec![2], vec![3], vec![4]]);
}

#[test]
fn pc_minimals_derives_selected_metadata_and_probabilities_from_the_app_canonical_page() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(multi_candidate_minimals_probability_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let report = response
        .product_capability_result()
        .and_then(|result| result.pc_minimum_cover_v2())
        .expect("probability-bearing App-owned minimum-cover report");
    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("deferred probability-bearing Core result");
    assert_eq!(report.source_solution_count(), 4);
    assert_eq!(report.selected_solution_count(), 1);
    assert_eq!(core.normalized_solution_keys().len(), 4);
    assert_eq!(core.solution_probabilities().len(), 4);
    assert_eq!(report.selected_solution_probabilities().len(), 1);
    assert_eq!(
        report.selected_solution_probabilities()[0].solution_key(),
        report.selected_solution_keys()[0]
    );
    let source_probability = core
        .solution_probabilities()
        .iter()
        .find(|probability| {
            probability.solution_key() == report.selected_solution_keys()[0].as_str()
        })
        .expect("canonical selection is projected from the complete source probabilities");
    assert_eq!(
        &report.selected_solution_probabilities()[0],
        source_probability
    );
    assert_eq!(
        report
            .portfolio_alternatives()
            .canonical_page()
            .portfolio()
            .candidate_ids(),
        &[1]
    );
}

#[test]
fn pc_minimals_deferred_boundary_rejects_status_and_full_source_tampering() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = Arc::new(
        match multi_candidate_minimals_probability_request().command() {
            AppCommand::Scenario(command) => command.query().clone(),
            _ => panic!("fixture is a scenario command"),
        },
    );
    let problem = ProblemCompiler::compile_scenario_pc(query.as_ref())
        .expect("deferred minimum-cover problem")
        .with_pc_minimum_cover_v2_evidence();
    let result = WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
        .expect("deferred minimum-cover producer");
    let validate = |candidate: &CoreExecutionResult| {
        crate::pc_minimum_cover_result::validate_pc_minimum_cover_v2_result(
            crate::pc_minimum_cover_result::PcMinimumCoverQueryBinding::Scenario(&query),
            PcMinimalsIngressOrigin::CanonicalPcMinimals,
            candidate,
        )
    };
    validate(&result).expect("complete deferred source validates once in App");

    let wrong_status = result
        .clone()
        .with_replaced_fields(vec![field("minimum_cover_incomplete_reason", "none")]);
    assert!(validate(&wrong_status).is_err());

    let mut missing_keys = result.normalized_solution_keys().to_vec();
    missing_keys.pop();
    let incomplete_source = result.clone().with_normalized_solution_keys(missing_keys);
    assert!(validate(&incomplete_source).is_err());

    let mut wrong_probabilities = result.solution_probabilities().to_vec();
    wrong_probabilities.pop();
    let incomplete_probabilities = result.with_solution_probabilities(wrong_probabilities);
    assert!(validate(&incomplete_probabilities).is_err());
}

#[test]
fn pc_minimals_unique_execution_preserves_the_v074_count_all_public_field_identity() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let unique_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        PieceWindow::new(5),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(5))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::minimum_cover());
    let count_all_query = unique_query
        .clone()
        .with_count_policy(PcCountPolicy::CountAll);
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let legacy = context.run(AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(count_all_query),
    )));
    let canonical = context.run(multi_candidate_minimals_request());
    let legacy_core = legacy
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("v0.7.4-style count-all minimum-cover public result");
    let report = canonical
        .product_capability_result()
        .and_then(|result| result.pc_minimum_cover_v2())
        .expect("canonical pc.minimals product report");
    let alternatives = report.portfolio_alternatives();
    assert_eq!(alternatives.optimal_cardinality(), 1);
    let candidate_keys = alternatives
        .candidates()
        .iter()
        .map(|candidate| candidate.normalized_key())
        .collect::<Vec<_>>();
    assert_eq!(candidate_keys, legacy_core.normalized_solution_keys());
    assert_eq!(
        candidate_keys,
        vec![
            "ctk1|initial=0000000000000000|placements=I:000000000000000f,I:0000000000003c00,O:000000000000c030,O:00000000000300c0,O:00000000000c0300",
            "ctk1|initial=0000000000000000|placements=I:000000000000003c,I:000000000000f000,O:0000000000000c03,O:00000000000300c0,O:00000000000c0300",
            "ctk1|initial=0000000000000000|placements=I:00000000000000f0,I:000000000003c000,O:0000000000000c03,O:000000000000300c,O:00000000000c0300",
            "ctk1|initial=0000000000000000|placements=I:00000000000003c0,I:00000000000f0000,O:0000000000000c03,O:000000000000300c,O:000000000000c030",
        ]
    );
    assert_eq!(alternatives.coverage_rows().len(), 4);
    assert!(alternatives
        .coverage_rows()
        .iter()
        .all(|row| row == alternatives.required_patterns()));
    let mut store = alternatives
        .open_store()
        .expect("canonical exact alternative enumerator");
    let mut portfolios = vec![alternatives
        .canonical_page()
        .portfolio()
        .candidate_ids()
        .to_vec()];
    loop {
        let advance = store
            .next_page(u64::MAX, &mut || false)
            .expect("next canonical exact alternative");
        if let Some(page) = advance.page() {
            portfolios.push(page.portfolio().candidate_ids().to_vec());
        }
        if advance.checkpoint().enumeration_complete() {
            assert_eq!(advance.checkpoint().known_alternative_count_decimal(), "4");
            break;
        }
    }
    assert_eq!(portfolios, [vec![1], vec![2], vec![3], vec![4]]);
}

#[test]
fn pc_minimals_rejects_unaccounted_caps_and_binds_both_distributed_and_cooperative_workers() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let base = minimals_command().query().clone();
    let capped = base.clone().with_execution_policy(
        base.execution_policy()
            .clone()
            .with_max_memory_mib(Some(64)),
    );
    let command = ScenarioAppCommand::new(capped).with_result_projection(
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals),
    );
    let cap_error = AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .expect_err("pc minimals exact replay scratch is outside an explicit Core memory cap");
    assert!(matches!(
        cap_error,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("exact replay scratch is accounted")
    ));

    let DistributedSearchPreparation::Search(distributed) =
        AppContext::default().prepare_distributed_search(minimals_request())
    else {
        panic!("typed pc minimals must bind its distributed product identity");
    };
    assert!(!distributed.is_pc_score());
    assert_eq!(distributed.problem().exact_pieces(), base.exact_pieces());
    drop(distributed);

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cooperative = context.start_cooperative_execution(minimals_request());
    let control = ExecutionControl::default();
    let mut observed_yield = false;
    let mut completed = None;
    for _ in 0..10_000 {
        match cooperative.advance(100_000, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {
                observed_yield = true;
            }
            CooperativeAppAdvance::Completed(response) => {
                completed = Some(response);
                break;
            }
            advance => {
                panic!(
                    "typed pc minimals must complete through the cooperative worker producer: {advance:?}"
                )
            }
        }
    }
    let cooperative = completed.expect("cooperative pc.minimals completes within a bounded loop");
    assert!(observed_yield, "pc.minimals must yield before final replay");
    assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
    let result = cooperative
        .product_capability_result()
        .expect("cooperative worker result retains the typed product wrapper");
    let report = result
        .pc_minimum_cover_v2()
        .expect("cooperative worker result retains the exact minimum-cover report");
    assert!(result.public_page_source_owner().is_some());
    assert_eq!(
        report
            .portfolio_alternatives()
            .canonical_page()
            .portfolio()
            .candidate_ids()
            .len(),
        report.selected_solution_count()
    );

    let direct = context.run(minimals_request());
    assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
    let direct_report = direct
        .product_capability_result()
        .and_then(|result| result.pc_minimum_cover_v2())
        .expect("blocking pc.minimals report");
    assert_eq!(
        report, direct_report,
        "blocking and cooperative product coordinators must drive the same preparation authority"
    );
}

#[test]
fn cooperative_pc_minimals_local_shard_memory_envelope_tracks_live_owner_lifecycle() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(multi_candidate_minimals_request());
    let control = ExecutionControl::default();
    assert!(execution.minimum_parallel_memory_envelope().is_none());
    let mut observed_owner = false;
    for _ in 0..10_000 {
        if let Some((cap, bytes)) = execution.minimum_parallel_memory_envelope() {
            assert_eq!(
                cap, None,
                "unlimited request is not a fabricated host admission"
            );
            assert!(bytes > core::mem::size_of_val(&execution) as u128);
            observed_owner = true;
        }
        match execution.advance(16, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(response) => {
                assert_eq!(
                    response.status(),
                    AppStatus::Success,
                    "{:?}",
                    response.error()
                );
                assert!(
                    observed_owner,
                    "minimum finalizer must expose an admitted-owner projection"
                );
                assert!(execution.minimum_parallel_memory_envelope().is_none());
                return;
            }
            other => panic!("unexpected minimum advance: {other:?}"),
        }
    }
    panic!("tiny minimum fixture exceeded its test-only advance budget")
}

#[derive(Default)]
struct PcMinimalsProgressRecorder {
    events: Mutex<Vec<ExecutionProgress>>,
}

#[test]
fn cooperative_pc_minimals_external_memory_guard_rejects_before_query_work() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(multi_candidate_minimals_request());
    let control = ExecutionControl::default();
    let mut base_bytes = None;
    for _ in 0..10_000 {
        if let Some((_, bytes)) = execution.minimum_parallel_memory_envelope() {
            base_bytes = Some(bytes);
            break;
        }
        assert!(matches!(
            execution.advance(4_096, &control),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress
        ));
    }
    let base_bytes = base_bytes.expect("tiny fixture reaches minimum preparation");
    let mut calls = 0;
    let mut observed_peak = 0;
    let advance = execution.advance_with_minimum_memory_guard(16, &control, &mut |bytes| {
        calls += 1;
        observed_peak = observed_peak.max(bytes);
        Err(clearra_coverage::cover::ExactMinimumCoverError::MemoryGuardRejected)
    });
    assert!(
        calls > 0,
        "the control-only host must admit preparation work"
    );
    assert!(
        observed_peak >= base_bytes,
        "callback must include the whole retained App owner"
    );
    let CooperativeAppAdvance::Completed(response) = advance else {
        panic!("rejected memory admission must be terminal, not a pending proof: {advance:?}")
    };
    assert_eq!(response.status(), AppStatus::ExecutionFailed);
    assert!(response.product_capability_result().is_none());
    assert!(execution.minimum_parallel_memory_envelope().is_none());
}

impl ProgressSink for PcMinimalsProgressRecorder {
    fn report(&self, progress: ExecutionProgress) {
        self.events.lock().expect("progress lock").push(progress);
    }
}

#[test]
fn cooperative_pc_minimals_finalizer_preserves_zero_budget_and_honors_cancellation() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cooperative = context.start_cooperative_execution(minimals_request());
    let recorder = Arc::new(PcMinimalsProgressRecorder::default());
    let cancellation = ExecutionCancellationToken::new();
    let control = ExecutionControl::new(cancellation.clone()).with_progress_sink(recorder.clone());

    let entered_finalizer = (0..10_000).any(|_| {
        match cooperative.advance(100_000, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            advance => panic!("pc.minimals must reach its finalizer before terminal: {advance:?}"),
        }
        recorder
            .events
            .lock()
            .expect("progress lock")
            .last()
            .is_some_and(|event| event.stage == "pc-minimals-finalize")
    });
    assert!(entered_finalizer, "pc.minimals finalizer was not entered");
    let before_zero = *recorder
        .events
        .lock()
        .expect("progress lock")
        .last()
        .expect("finalizer progress");
    assert!(matches!(
        cooperative.advance(0, &control),
        CooperativeAppAdvance::Progress
    ));
    let after_zero = *recorder
        .events
        .lock()
        .expect("progress lock")
        .last()
        .expect("zero-budget progress");
    assert_eq!(after_zero.stage, "pc-minimals-finalize");
    assert_eq!(after_zero.completed, before_zero.completed);

    cancellation.handle().cancel();
    assert!(matches!(
        cooperative.advance(1, &control),
        CooperativeAppAdvance::Cancelled
    ));
}

fn tiling_command(origin: PcTilingIngressOrigin) -> PcAppCommand {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_allow_backend_fallback(false)
                .with_max_candidates(5_000),
        )
        .with_objective(ObjectivePolicy::tiling());
    PcAppCommand::new(query).with_result_projection(PcResultProjection::TilingFamilyV1(origin))
}

fn uncontracted_tiling_request(origin: PcTilingIngressOrigin) -> AppRequest {
    AppRequest::new(AppCommand::Pc(tiling_command(origin)))
}

fn tiling_request(origin: PcTilingIngressOrigin) -> AppRequest {
    uncontracted_tiling_request(origin)
        .with_product_capability_contract(ProductCapabilityContract::PcTiling)
        .expect("valid pc tiling product capability")
}

fn score_command(origin: PcScoreIngressOrigin, piece: PieceKind) -> ScenarioAppCommand {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    ScenarioAppCommand::new(query)
        .with_result_projection(PcResultProjection::ScoreSummaryV2(origin))
}

fn score_request(origin: PcScoreIngressOrigin, piece: PieceKind) -> AppRequest {
    AppRequest::new(AppCommand::Scenario(score_command(origin, piece)))
        .with_product_capability_contract(ProductCapabilityContract::PcScore)
        .expect("valid pc score product capability")
}

fn score_finder_request(piece: PieceKind) -> AppRequest {
    let origin = PcScoreIngressOrigin::CanonicalPcScoreFinder;
    let base = score_command(origin, piece);
    let score = base
        .query()
        .objective()
        .score()
        .with_profile(ScoreProfileSelection::JstrisUltra)
        .with_spin_profile(SpinProfileSelection::TSpins)
        .with_initial_b2b(1);
    let command = ScenarioAppCommand::new(
        base.query()
            .clone()
            .with_objective(ObjectivePolicy::all().with_score_policy(score)),
    )
    .with_result_projection(PcResultProjection::ScoreSummaryV2(origin));
    AppRequest::new(AppCommand::Scenario(command))
        .with_product_capability_contract(ProductCapabilityContract::PcScoreFinder)
        .expect("valid fixed-queue pc score-finder product capability")
}

fn multi_candidate_score_minimals_command() -> ScenarioAppCommand {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])),
        PieceWindow::new(5),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(5))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
    ScenarioAppCommand::new(query).with_score_minimals_result()
}

fn multi_candidate_score_minimals_request() -> AppRequest {
    AppRequest::new(AppCommand::Scenario(
        multi_candidate_score_minimals_command(),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcScoreMinimals)
    .expect("valid pc score-minimals product capability")
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn parallel_score_minimals_request(execution_policy: PcExecutionPolicy) -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xf03c_0f03_c0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::S,
            PieceKind::J,
            PieceKind::L,
            PieceKind::Z,
        ])),
        PieceWindow::new(6),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(6))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy)
    .with_objective(ObjectivePolicy::minimum_cover().with_score_summary());
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_score_minimals_result(),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcScoreMinimals)
    .expect("valid parallel pc score-minimals product capability")
}

fn opening_score_request(origin: PcScoreIngressOrigin) -> AppRequest {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::J,
            PieceKind::L,
        ])))
        .with_execution_policy(pc_score_execution_policy())
        .with_objective(ObjectivePolicy::all().with_score_summary());
    let command =
        PcAppCommand::new(query).with_result_projection(PcResultProjection::ScoreSummaryV2(origin));
    AppRequest::new(AppCommand::Pc(command))
        .with_product_capability_contract(ProductCapabilityContract::PcScore)
        .expect("valid opening pc score product capability")
}

struct RawScoreExecution {
    problem: Arc<SearchProblem>,
    authority: PcScoreCompiledAuthority,
    result: CoreExecutionResult,
}

fn raw_score_execution(piece: PieceKind, rule: RuleProfile) -> RawScoreExecution {
    let query = score_command(PcScoreIngressOrigin::CanonicalPcScore, piece)
        .query()
        .clone()
        .with_rule(rule);
    let authority =
        PcScoreCompiledAuthority::compile_scenario(query, PcScoreIngressOrigin::CanonicalPcScore)
            .expect("tiny score authority compiles once");
    let problem = authority.problem_arc();
    let result = WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
        Arc::clone(&problem),
        authority
            .checked_external_retained_upper_bound_bytes(0)
            .expect("raw score authority base fits the closed product envelope"),
        authority.terminal_resource_authority(),
        &ExecutionControl::default(),
        |result, terminal_authority| {
            let result = result.expect("tiny score search executes");
            terminal_authority
                .expect("successful score search retains terminal authority")
                .validate_public_result_memory(&result)
                .expect("raw score result fits the request-level authority");
            result
        },
    );
    RawScoreExecution {
        problem,
        authority,
        result,
    }
}

fn scoring_batch_with_profile_ids(
    batch: &ExactScoringExecutionBatch,
    kick_table_id: u64,
    rule_profile_id: u64,
) -> ExactScoringExecutionBatch {
    ExactScoringExecutionBatch::new(
        batch.layout(),
        batch.initial_occupied(),
        batch.patterns().to_vec(),
        batch.initial_cursor(),
        batch.initial_hold(),
        batch.hold_enabled(),
        batch.projects_unplaced_lookahead(),
        batch.projects_standard_bag_lookahead(),
        kick_table_id,
        rule_profile_id,
        batch.graphs().to_vec(),
        batch.complete(),
    )
}

fn source_chance_result() -> CoreExecutionResult {
    CoreExecutionResult::new(
        vec![
            field("problem_preset", "opening-pc"),
            field("compiled_goal", "clear-to-empty"),
            field("count_complete", true),
            field("objective_complete", true),
            field("execution_constraint_preserve_b2b", true),
            field("execution_constraint_materialized", true),
            field("execution_constraint_spin_profile", "all-spin-plus"),
            field("postprocess_scoring_requested", false),
            field("score_objective_mode", "disabled"),
            field("b2b_preservation_selection", "existential"),
            field(
                "b2b_preservation_denominator_semantics",
                "original-materialized-queue",
            ),
            field(
                "b2b_preservation_evaluation_basis",
                "candidate-pattern-existence",
            ),
            field("b2b_preservation_path_multiplicity_counted", false),
            field("b2b_preservation_pattern_universe_count", 120),
            field("b2b_preserving_pattern_count", 60),
            field("b2b_preservation_probability", "0.5"),
            field("b2b_preservation_count_complete", true),
            field("b2b_preservation_probability_complete", true),
        ],
        Vec::new(),
    )
}

fn projected_chance_result() -> CoreExecutionResult {
    let command = chance_command();
    let validated = command
        .validated_result_projection()
        .expect("valid All-Spin projection");
    project_pc_allspin_result(source_chance_result(), validated)
}

fn success_response() -> AppResponse {
    AppResponse::success(crate::AppRenderModel::Pc(projected_chance_result()))
}

fn field(key: &str, value: impl ToString) -> (String, String) {
    (key.to_owned(), value.to_string())
}

fn assert_request_validation_contract_rejected(request: &AppRequest, expected_message: &str) {
    let command_validation = request.command().validate();
    let validation = AppContext::default().validate_request(request);
    let diagnostics = validation.validation().diagnostics();
    assert_eq!(
        &diagnostics[..command_validation.diagnostics().len()],
        command_validation.diagnostics(),
        "command diagnostics must retain their existing order"
    );
    assert_eq!(
        diagnostics.len(),
        command_validation.diagnostics().len() + 1,
        "the binding rejection must append exactly one diagnostic"
    );
    let rejection = diagnostics.last().expect("binding rejection diagnostic");
    assert_eq!(
        rejection.code(),
        DiagnosticCode::EFrontendTypedRequestRequired
    );
    assert!(rejection.message().contains(expected_message));
    assert!(validation.has_errors());
}

fn finalize_direct(request: AppRequest, response: AppResponse) -> AppResponse {
    let context = AppContext::default();
    let command_kind = request.command_kind();
    let (_, output_policy, _, _, _, contract) = request
        .into_execution_parts()
        .expect("valid product capability execution parts");
    context.finalize_response_with_product_capability(
        response,
        command_kind,
        &output_policy,
        contract,
    )
}

#[test]
fn app_request_identity_and_valid_result_are_preserved_through_direct_finalization_once() {
    let request = chance_request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinPreservationChance)
    );

    let raw = success_response();
    let context = AppContext::default();
    let command_kind = request.command_kind();
    let (_, output_policy, _, _, _, contract) = request
        .clone()
        .into_execution_parts()
        .expect("valid product capability execution parts");
    let baseline = context.finalize_response(raw.clone(), command_kind, &output_policy);
    let finalized = context.finalize_response_with_product_capability(
        raw,
        command_kind,
        &output_policy,
        contract,
    );

    assert_eq!(finalized.status(), AppStatus::Success);
    let result = finalized
        .product_capability_result()
        .expect("validated target result wrapper");
    assert_eq!(
        result.contract(),
        ProductCapabilityContract::PcAllSpinPreservationChance
    );
    assert_eq!(
        result.result_kind(),
        ProductCapabilityResultKind::PcB2bPreservationProbabilityV1
    );
    assert_eq!(result.query(), &QueryEnvelope::PcOpening);
    assert!(result.resource_evidence().solver_executed());
    assert_eq!(result.validation_count(), 1);
    assert_eq!(baseline.command(), finalized.command());
    assert_eq!(baseline.status(), finalized.status());
    assert_eq!(baseline.result(), finalized.result());
    assert_eq!(baseline.diagnostics(), finalized.diagnostics());
    assert_eq!(baseline.backend_report(), finalized.backend_report());
    assert_eq!(baseline.resource_report(), finalized.resource_report());
    assert_eq!(baseline.capability_report(), finalized.capability_report());
    assert_eq!(baseline.continuation(), finalized.continuation());
    assert_eq!(baseline.render_model(), finalized.render_model());
    assert_eq!(baseline.effects(), finalized.effects());
    assert_eq!(baseline.exit_code_hint(), finalized.exit_code_hint());
    assert_eq!(baseline.error(), finalized.error());
}

#[test]
fn app_request_rejects_wrong_identity_and_query_envelope_before_execution() {
    let wrong_identity = uncontracted_chance_request()
        .with_product_capability_contract(ProductCapabilityContract::PcAllSpinSolution)
        .expect_err("chance projection cannot claim the exact-witness contract");
    assert!(matches!(
        wrong_identity,
        ProductCapabilityContractError::ProjectionMismatch { .. }
    ));

    let wrong_query = uncontracted_chance_request()
        .with_query_envelope_for_test(QueryEnvelope::PcScenario)
        .with_product_capability_contract(ProductCapabilityContract::PcAllSpinPreservationChance)
        .expect_err("stored query envelope must equal the actual command query");
    assert!(matches!(
        wrong_query,
        ProductCapabilityContractError::QueryEnvelopeMismatch { .. }
    ));

    let profile = SpinProfileSelection::AllSpinPlus;
    let invalid_objective = PcAppCommand::new(
        OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("[TIOSZ]!", 120).expect("five-piece pattern queue"),
            ))
            .with_objective(ObjectivePolicy::unique()),
    )
    .with_result_projection(PcResultProjection::AllSpinPreservationChance(profile));
    let invalid_objective = AppRequest::new(AppCommand::Pc(invalid_objective))
        .with_product_capability_contract(ProductCapabilityContract::PcAllSpinPreservationChance)
        .expect_err("typed identity must invoke the fieldwise objective validator");
    assert!(matches!(
        invalid_objective,
        ProductCapabilityContractError::RequestContractRejected(_)
    ));
}

#[test]
fn missing_contract_fails_closed_at_direct_cooperative_and_distributed_boundaries() {
    let direct = AppContext::default()
        .run(uncontracted_chance_request().with_output_policy(crate::AppOutputPolicy::new(false)));
    assert_typed_request_rejected(&direct, "requires its matching product capability contract");

    let cooperative_context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cooperative =
        cooperative_context.start_cooperative_execution(uncontracted_chance_request());
    let CooperativeAppAdvance::Completed(cooperative) =
        cooperative.advance(1, &ExecutionControl::default())
    else {
        panic!("missing cooperative contract must fail before a search session starts");
    };
    assert_typed_request_rejected(
        &cooperative,
        "requires its matching product capability contract",
    );

    let DistributedSearchPreparation::Ready(distributed) =
        AppContext::default().prepare_distributed_search(uncontracted_chance_request())
    else {
        panic!("missing distributed contract must fail before search preparation");
    };
    assert_typed_request_rejected(
        &distributed,
        "requires its matching product capability contract",
    );
}

#[test]
fn pc_chance_missing_contract_fails_before_every_execution_seam() {
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let direct = AppContext::default().run(uncontracted_probability_request(origin));
    assert_typed_request_rejected(
        &direct,
        "pc.chance result projection requires its matching product capability contract",
    );

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cooperative =
        context.start_cooperative_execution(uncontracted_probability_request(origin));
    let CooperativeAppAdvance::Completed(cooperative) =
        cooperative.advance(1, &ExecutionControl::default())
    else {
        panic!("missing pc chance contract must fail before cooperative compilation");
    };
    assert_typed_request_rejected(
        &cooperative,
        "pc.chance result projection requires its matching product capability contract",
    );

    let DistributedSearchPreparation::Ready(distributed) =
        AppContext::default().prepare_distributed_search(uncontracted_probability_request(origin))
    else {
        panic!("missing pc chance contract must fail before distributed preparation");
    };
    assert_typed_request_rejected(
        &distributed,
        "pc.chance result projection requires its matching product capability contract",
    );
}

#[test]
fn pc_tiling_missing_contract_fails_closed_before_execution() {
    let response = AppContext::default().run(uncontracted_tiling_request(
        PcTilingIngressOrigin::CanonicalPcTiling,
    ));
    assert_typed_request_rejected(
        &response,
        "pc.tiling result projection requires its matching product capability contract",
    );
}

#[test]
fn direct_wasm_pc_tiling_returns_one_complete_pageable_family_wrapper() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcTilingIngressOrigin::CanonicalPcTiling;
    let expected_query =
        PcTilingQuerySnapshot::Opening(Arc::new(tiling_command(origin).query().clone()));
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(tiling_request(origin));
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.pc_tiling_execution_evidence().is_none());

    let wrapper = response
        .product_capability_result()
        .expect("typed pc tiling wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcTiling);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcTilingFamilyV1
    );
    assert_eq!(wrapper.validation_count(), 1);
    assert!(!wrapper.resource_evidence().probability_complete());

    let family = wrapper
        .pc_tiling_family_v1()
        .expect("pc-tiling-family.v1 report");
    assert_eq!(family.contract_id(), "pc-tiling-family.v1");
    assert_eq!(family.origin(), origin);
    assert_eq!(family.query(), &expected_query);
    assert!(family.completeness().family_complete());
    assert!(family.completeness().initial_page_complete());
    assert_eq!(family.completeness().incomplete_reason(), "none");
    assert!(!family.initial_page_keys().is_empty());
    let initial_page = family
        .page_keys(0, family.initial_page_keys().len())
        .expect("initial page remains available without rerunning search");
    assert_eq!(initial_page.as_slice(), family.initial_page_keys());

    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("public pc tiling Core result");
    assert_eq!(core.normalized_solution_keys(), family.initial_page_keys());
    assert_eq!(
        core.usize_field("normalized_unique_solution_count"),
        Some(family.normalized_solution_count())
    );
    assert_eq!(
        core.field("normalized_solution_set_hash"),
        Some(family.normalized_solution_set_hash())
    );
    assert!(core.tiling_solution_page_store().is_some());
}

#[test]
fn cooperative_wasm_pc_tiling_returns_the_validated_family_wrapper() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcTilingIngressOrigin::CanonicalPcTiling;
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(tiling_request(origin));
    let control = ExecutionControl::default();
    let response = (0..512)
        .find_map(|_| match execution.advance(4_096, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
            CooperativeAppAdvance::Completed(response) => Some(response),
            CooperativeAppAdvance::CompletedGoverned(_) => {
                panic!("non-Build cooperative tiling returned a governed Build response")
            }
            CooperativeAppAdvance::FailedFinite(error) => {
                panic!("non-Build cooperative tiling failed as finite: {error:?}")
            }
            CooperativeAppAdvance::Cancelled => panic!("uncancelled pc tiling was cancelled"),
        })
        .expect("cooperative pc tiling must complete within the bounded work budget");

    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.pc_tiling_execution_evidence().is_none());
    let family = response
        .product_capability_result()
        .and_then(|result| result.pc_tiling_family_v1())
        .expect("validated cooperative pc tiling family");
    assert_eq!(family.origin(), origin);
    assert!(family.completeness().family_complete());
    assert!(family.completeness().initial_page_complete());
    assert_eq!(
        family
            .page_keys(0, family.initial_page_keys().len())
            .expect("cooperative family page")
            .as_slice(),
        family.initial_page_keys()
    );
}

#[test]
fn pc_chance_proof_binds_the_full_query_and_closed_ingress_origin() {
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let attached = probability_request(origin);
    assert!(!AppContext::default()
        .validate_request(&attached)
        .has_errors());

    let stale_origin =
        attached
            .clone()
            .with_command_for_product_capability_test(AppCommand::Scenario(probability_command(
                PcChanceIngressOrigin::CompatibilityChance,
            )));
    assert_request_validation_contract_rejected(
        &stale_origin,
        "pc.chance product capability proof is stale for the current command",
    );

    let stale_board_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x1c80),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique());
    let stale_board = attached.with_command_for_product_capability_test(AppCommand::Scenario(
        ScenarioAppCommand::new(stale_board_query)
            .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin)),
    ));
    assert_request_validation_contract_rejected(
        &stale_board,
        "pc.chance product capability proof is stale for the current command",
    );
}

#[test]
fn pc_chance_explicit_memory_caps_fail_closed_without_inheriting_to_percent() {
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let opening_base = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("[TIOSZ]!", 120).expect("five-piece pattern queue"),
        ))
        .with_objective(ObjectivePolicy::unique());
    let opening_policy = opening_base
        .execution_policy()
        .clone()
        .with_max_memory_mib(Some(64));
    let opening = PcAppCommand::new(opening_base.with_execution_policy(opening_policy))
        .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin));
    let opening_rejection = AppRequest::new(AppCommand::Pc(opening))
        .with_product_capability_contract(ProductCapabilityContract::PcChance)
        .expect_err("typed opening chance must reject an unaccounted memory cap");
    assert!(matches!(
        opening_rejection,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("transient proof memory is accounted")
    ));

    let scenario_base = probability_command(origin).query().clone();
    let scenario_policy = scenario_base
        .execution_policy()
        .clone()
        .with_max_memory_mib(Some(64));
    let capped_scenario = scenario_base.with_execution_policy(scenario_policy);
    let scenario = ScenarioAppCommand::new(capped_scenario.clone())
        .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin));
    let scenario_rejection = AppRequest::new(AppCommand::Scenario(scenario))
        .with_product_capability_contract(ProductCapabilityContract::PcChance)
        .expect_err("typed scenario chance must reject an unaccounted memory cap");
    assert!(matches!(
        scenario_rejection,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("transient proof memory is accounted")
    ));

    let ordinary_percent =
        AppRequest::new(AppCommand::Percent(PercentAppCommand::new(capped_scenario)));
    assert_eq!(ordinary_percent.product_capability_contract(), None);
    assert!(!AppContext::default()
        .validate_request(&ordinary_percent)
        .has_errors());
}

#[test]
fn pc_chance_visible_seven_fails_before_direct_or_cooperative_execution() {
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let opening_query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("[TIOSZ]!", 120).expect("five-piece pattern queue"),
        ))
        .with_objective(ObjectivePolicy::unique())
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
    let opening = PcAppCommand::new(opening_query)
        .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin));
    let opening_rejection = AppRequest::new(AppCommand::Pc(opening))
        .with_product_capability_contract(ProductCapabilityContract::PcChance)
        .expect_err("typed opening chance must reject visible-seven semantics");
    assert!(matches!(
        opening_rejection,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("full-queue oracle")
    ));

    let scenario_query = probability_command(origin)
        .query()
        .clone()
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
    let scenario = ScenarioAppCommand::new(scenario_query.clone())
        .with_result_projection(PcResultProjection::ChanceProbabilityV2(origin));
    let scenario_rejection = AppRequest::new(AppCommand::Scenario(scenario.clone()))
        .with_product_capability_contract(ProductCapabilityContract::PcChance)
        .expect_err("typed scenario chance must reject visible-seven semantics");
    assert!(matches!(
        scenario_rejection,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("full-queue oracle")
    ));

    let stale = probability_request(origin)
        .with_command_for_product_capability_test(AppCommand::Scenario(scenario));
    let direct = AppContext::default().run(stale.clone());
    assert_typed_request_rejected(&direct, "pc chance requires full-queue oracle knowledge");

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cooperative = context.start_cooperative_execution(stale);
    let CooperativeAppAdvance::Completed(cooperative) =
        cooperative.advance(1, &ExecutionControl::default())
    else {
        panic!("visible-seven pc chance must fail before cooperative compilation");
    };
    assert_typed_request_rejected(
        &cooperative,
        "pc chance requires full-queue oracle knowledge",
    );

    let ordinary_percent =
        AppRequest::new(AppCommand::Percent(PercentAppCommand::new(scenario_query)));
    assert_eq!(ordinary_percent.product_capability_contract(), None);
    assert!(!AppContext::default()
        .validate_request(&ordinary_percent)
        .has_errors());
}

#[test]
fn stored_wrong_or_stale_product_identity_is_rejected_by_checked_extraction() {
    let profile = SpinProfileSelection::AllSpinPlus;
    let wrong_projection = PcAppCommand::new(chance_command().query().clone())
        .with_result_projection(PcResultProjection::AllSpinSolution(profile));
    let wrong_identity =
        chance_request().with_command_for_product_capability_test(AppCommand::Pc(wrong_projection));
    assert_typed_request_rejected(
        &AppContext::default().run(wrong_identity),
        "cannot use pc.allspin-pres-chance product capability proof",
    );

    let stale_query = chance_request().with_query_envelope_for_test(QueryEnvelope::PcScenario);
    assert_typed_request_rejected(
        &AppContext::default().run(stale_query),
        "query envelope mismatch",
    );

    let stale_command = chance_request().with_command_for_product_capability_test(AppCommand::Pc(
        chance_command_for_target(PcTarget::four_lines()),
    ));
    assert_typed_request_rejected(
        &AppContext::default().run(stale_command),
        "product capability proof is stale for the current command",
    );
}

#[test]
fn validate_request_reuses_checked_product_binding_without_consuming_the_request() {
    let missing = uncontracted_chance_request();
    assert_request_validation_contract_rejected(
        &missing,
        "requires its matching product capability contract",
    );
    assert_eq!(
        missing.product_capability_contract(),
        None,
        "validation must not consume or attach identity"
    );

    let stale_query = chance_request().with_query_envelope_for_test(QueryEnvelope::PcScenario);
    assert_request_validation_contract_rejected(&stale_query, "query envelope mismatch");

    let stale_command = chance_request().with_command_for_product_capability_test(AppCommand::Pc(
        chance_command_for_target(PcTarget::four_lines()),
    ));
    assert_request_validation_contract_rejected(
        &stale_command,
        "product capability proof is stale for the current command",
    );

    let profile = SpinProfileSelection::AllSpinPlus;
    let wrong_projection = PcAppCommand::new(chance_command().query().clone())
        .with_result_projection(PcResultProjection::AllSpinSolution(profile));
    let wrong_identity =
        chance_request().with_command_for_product_capability_test(AppCommand::Pc(wrong_projection));
    assert_request_validation_contract_rejected(
        &wrong_identity,
        "cannot use pc.allspin-pres-chance product capability proof",
    );

    let standard_with_stored_proof = chance_request().with_command_for_product_capability_test(
        AppCommand::Pc(PcAppCommand::new(chance_command().query().clone())),
    );
    assert_request_validation_contract_rejected(
        &standard_with_stored_proof,
        "standard result projection cannot inherit",
    );

    let attached = chance_request();
    let command_validation = attached.command().validate();
    let validation = AppContext::default().validate_request(&attached);
    assert_eq!(validation.validation(), &command_validation);
    assert!(!validation.has_errors());
    assert_eq!(
        attached.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinPreservationChance),
        "validation must preserve the attached typed identity"
    );
}

#[test]
fn standard_non_contract_request_and_ordinary_failure_semantics_are_unchanged() {
    let ordinary_request = || {
        AppRequest::new(AppCommand::Pc(PcAppCommand::new(
            OpeningPcSearchQuery::new(PcTarget::two_lines()),
        )))
    };
    let raw_success = success_response();
    let success = finalize_direct(ordinary_request(), raw_success);
    assert_eq!(success.status(), AppStatus::Success);
    assert!(success.product_capability_result().is_none());
    let (_, _, _, _, _, ordinary_contract) = ordinary_request()
        .into_execution_parts()
        .expect("ordinary Standard request stays admitted");
    assert!(ordinary_contract.is_none());

    let failure = AppResponse::failed(
        AppStatus::Unsupported,
        AppError::new(AppErrorCode::Unsupported, "ordinary focused failure"),
    );
    let context = AppContext::default();
    let request = chance_request();
    let command_kind = request.command_kind();
    let (_, output_policy, _, _, _, contract) = request
        .into_execution_parts()
        .expect("valid product capability execution parts");
    let baseline = context.finalize_response(failure.clone(), command_kind, &output_policy);
    let contracted = context.finalize_response_with_product_capability(
        failure,
        command_kind,
        &output_policy,
        contract,
    );
    assert_eq!(contracted, baseline);
}

#[test]
fn duplicate_direct_finalization_is_rejected_instead_of_replacing_the_wrapper() {
    let context = AppContext::default();
    let first_request = chance_request();
    let command_kind = first_request.command_kind();
    let (_, output_policy, _, _, _, first_contract) = first_request
        .into_execution_parts()
        .expect("valid product capability execution parts");
    let first = context.finalize_response_with_product_capability(
        success_response(),
        command_kind,
        &output_policy,
        first_contract,
    );
    assert_eq!(
        first
            .product_capability_result()
            .expect("first wrapper")
            .validation_count(),
        1
    );

    let (_, _, _, _, _, duplicate_contract) = chance_request()
        .into_execution_parts()
        .expect("valid duplicate product capability execution parts");
    let duplicate = context.finalize_response_with_product_capability(
        first,
        command_kind,
        &output_policy,
        duplicate_contract,
    );
    assert_eq!(duplicate.status(), AppStatus::ExecutionFailed);
    assert!(duplicate.product_capability_result().is_none());
    assert!(duplicate
        .error()
        .expect("duplicate-finalization error")
        .message()
        .contains("response already has a product capability wrapper"));
}

#[test]
fn result_kind_render_and_projected_contract_mismatches_fail_closed() {
    let context = AppContext::default();
    let request = chance_request();
    let (_, output_policy, _, _, _, contract) = request
        .into_execution_parts()
        .expect("valid product capability execution parts");
    let wrong_command = context.finalize_response_with_product_capability(
        success_response(),
        AppCommandKind::Setup,
        &output_policy,
        contract,
    );
    assert_eq!(wrong_command.status(), AppStatus::ExecutionFailed);
    assert!(wrong_command
        .error()
        .expect("command mismatch error")
        .message()
        .contains("response command kind mismatch"));

    let wrong_result_kind = success_response().with_result_kind_for_test("pc-scenario");
    assert_contract_rejected(wrong_result_kind, "response result kind mismatch");

    let missing_render = success_response().without_render_model();
    assert_contract_rejected(missing_render, "response render model is missing");

    let wrong_contract = projected_chance_result().with_replaced_fields(vec![field(
        "pc_allspin_result_contract",
        "pc-b2b-preserving-witness.v1",
    )]);
    assert_contract_rejected(
        AppResponse::success(crate::AppRenderModel::Pc(wrong_contract)),
        "target result contract mismatch",
    );

    let false_projected_completeness = projected_chance_result()
        .with_replaced_fields(vec![field("pc_allspin_probability_complete", false)]);
    assert_contract_rejected(
        AppResponse::success(crate::AppRenderModel::Pc(false_projected_completeness)),
        "target result contract mismatch",
    );

    let wrong_preset = projected_chance_result().with_replaced_fields(vec![
        field("problem_preset", "scenario-pc"),
        field("pc_allspin_problem_preset", "scenario-pc"),
    ]);
    assert_contract_rejected(
        AppResponse::success(crate::AppRenderModel::Pc(wrong_preset)),
        "target result problem preset mismatch",
    );
}

#[test]
fn availability_completeness_and_truncation_are_independent_fail_closed_gates() {
    assert_contract_rejected(
        success_response().with_resource_report(ResourceReport::default()),
        "solver was not executed",
    );

    let mut unavailable = ResourceReport::new(true, "reported").with_execution_availability(
        ExecutionAvailabilityReport::unavailable(
            ExecutionSurface::current(),
            ExecutionAvailabilityReason::CapabilityUnavailable,
        ),
    );
    unavailable.set_result_completeness(ExecutionCompletenessState::Complete);
    assert_contract_rejected(
        success_response().with_resource_report(unavailable),
        "execution is not available",
    );

    assert_contract_rejected(
        success_response().with_resource_report(ResourceReport::new(true, "reported")),
        "result is not complete",
    );

    let mut truncated = ResourceReport::new(true, "reported").with_truncation("test-limit");
    truncated.set_result_completeness(ExecutionCompletenessState::Complete);
    assert_contract_rejected(
        success_response().with_resource_report(truncated),
        "result is truncated",
    );

    let mut stray_reason = ResourceReport::new(true, "reported");
    stray_reason.set_result_completeness(ExecutionCompletenessState::Complete);
    stray_reason.truncation_reason = Some("stray-reason".to_owned());
    assert_contract_rejected(
        success_response().with_resource_report(stray_reason),
        "non-truncated result carries a truncation reason",
    );
}

#[test]
fn cooperative_search_seam_carries_and_consumes_the_validated_contract_once() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let execution = context.start_cooperative_execution(chance_request());
    let response =
        execution.finalize_search_response_for_product_capability_test(success_response());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert_eq!(
        response
            .product_capability_result()
            .expect("cooperative target result")
            .contract(),
        ProductCapabilityContract::PcAllSpinPreservationChance
    );
    assert_eq!(
        response
            .product_capability_result()
            .expect("cooperative target result")
            .validation_count(),
        1
    );
}

#[test]
fn cooperative_cancellation_with_a_contract_remains_cancelled_and_unwrapped() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(chance_request());
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    let control = ExecutionControl::new(cancellation);

    assert_eq!(
        execution.advance(1, &control),
        CooperativeAppAdvance::Cancelled
    );
}

#[test]
fn pc_chance_cooperative_cancellation_remains_cancelled_and_unwrapped() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(probability_request(
        PcChanceIngressOrigin::CanonicalPcChance,
    ));
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    let control = ExecutionControl::new(cancellation);

    assert_eq!(
        execution.advance(1, &control),
        CooperativeAppAdvance::Cancelled
    );
}

#[cfg(feature = "native-c-core")]
#[test]
fn direct_pc_chance_returns_one_recomputed_wrapper_and_no_transient_authority() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let expected_query =
        PcChanceQuerySnapshot::Scenario(probability_command(origin).query().clone());
    let response = AppContext::default().run(probability_request(origin));
    assert_pc_probability_success(&response, origin, &expected_query);
}

#[test]
fn direct_wasm_pc_chance_returns_one_recomputed_wrapper_and_no_transient_authority() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcChanceIngressOrigin::CanonicalPcChance;
    let expected_query =
        PcChanceQuerySnapshot::Scenario(probability_command(origin).query().clone());
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(probability_request(origin));
    assert_pc_probability_success(&response, origin, &expected_query);
}

#[cfg(feature = "native-c-core")]
#[test]
fn pc_chance_finalizer_rejects_incomplete_resource_evidence_and_strips_transients() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::default();
    let request = probability_request(PcChanceIngressOrigin::CanonicalPcChance);
    let command_kind = request.command_kind();
    let (command, output_policy, resource_budget, _, _, contract) = request
        .into_execution_parts()
        .expect("valid pc chance execution parts");
    let control = ExecutionControl::default();
    let execution_context = AppExecutionContext {
        services: context.services(),
        language: context.language(),
        file_policy: context.file_policy(),
        output_policy: &output_policy,
        resource_budget: &resource_budget,
        execution_control: &control,
        pc_score_external_retained_context_bytes: None,
    };
    let raw = command.run(&execution_context);
    assert!(raw.pc_chance_execution_evidence().is_some());
    assert!(raw
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .is_some_and(|result| result.pc_chance_coverage_evidence().is_some()));
    let mut resources = raw.resource_report().clone();
    resources.probability_complete = false;
    let rejected = context.finalize_response_with_product_capability(
        raw.with_resource_report(resources),
        command_kind,
        &output_policy,
        contract,
    );
    assert_eq!(rejected.status(), AppStatus::ExecutionFailed);
    assert!(rejected.product_capability_result().is_none());
    assert!(rejected.pc_chance_execution_evidence().is_none());
    assert!(rejected.render_model().is_none());
    assert!(rejected
        .error()
        .expect("resource-incomplete rejection")
        .message()
        .contains("resource probability result is incomplete"));
}

#[test]
fn cooperative_pc_chance_finalizes_once_and_strips_all_transient_authority() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcChanceIngressOrigin::CompatibilityChance;
    let expected_query =
        PcChanceQuerySnapshot::Scenario(probability_command(origin).query().clone());
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut execution = context.start_cooperative_execution(probability_request(origin));
    let control = ExecutionControl::default();
    let mut response = None;
    for _ in 0..256 {
        match execution.advance(4_096, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(completed) => {
                response = Some(completed);
                break;
            }
            CooperativeAppAdvance::CompletedGoverned(_) => {
                panic!("non-Build cooperative chance returned a governed Build response")
            }
            CooperativeAppAdvance::FailedFinite(error) => {
                panic!("non-Build cooperative chance failed as finite: {error:?}")
            }
            CooperativeAppAdvance::Cancelled => {
                panic!("uncancelled cooperative pc chance was cancelled")
            }
        }
    }
    let response = response.expect("tiny cooperative pc chance must complete within the bound");
    assert_pc_probability_success(&response, origin, &expected_query);

    let CooperativeAppAdvance::Completed(duplicate) = execution.advance(1, &control) else {
        panic!("finished cooperative owner must reject a second finalization immediately");
    };
    assert_eq!(duplicate.status(), AppStatus::ExecutionFailed);
    assert!(duplicate.product_capability_result().is_none());
    assert!(duplicate.pc_chance_execution_evidence().is_none());
}

#[test]
fn pc_score_raw_authority_rejects_foreign_problem_and_replay_profile_swaps() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let RawScoreExecution {
        problem: foreign_rule_problem,
        authority: foreign_rule_authority,
        result: foreign_rule_result,
    } = raw_score_execution(PieceKind::I, srs());
    drop(foreign_rule_authority);
    let RawScoreExecution {
        problem: _,
        authority: foreign_problem_authority,
        result: foreign_problem_result,
    } = raw_score_execution(PieceKind::O, srs_plus());
    drop(foreign_problem_authority);

    let expected = raw_score_execution(PieceKind::I, srs_plus());
    expected
        .authority
        .validate_raw_wasm_execution(&expected.problem, &expected.result)
        .expect("producer-owned score evidence matches its executed problem");
    let evidence = expected
        .result
        .pc_score_problem_evidence()
        .expect("raw score result owns executed-problem evidence");
    let batch = expected
        .result
        .exact_scoring_execution_batch()
        .expect("raw score result owns one exact scoring batch");
    assert_eq!(batch.kick_table_id(), evidence.kick_table_id());
    assert_eq!(batch.rule_profile_id(), evidence.rule_profile_id());

    assert_eq!(
        expected.problem.problem_id(),
        foreign_rule_problem.problem_id(),
        "the corroborative problem id intentionally omits the rule distinction"
    );
    assert_eq!(
        expected
            .authority
            .validate_raw_wasm_execution(&expected.problem, &foreign_rule_result)
            .expect_err("foreign-rule producer evidence must fail closed")
            .component(),
        "pc_score_executed_problem_evidence_mismatch"
    );

    assert_eq!(
        expected
            .authority
            .validate_raw_wasm_execution(&expected.problem, &foreign_problem_result)
            .expect_err("foreign-problem producer evidence must fail closed")
            .component(),
        "pc_score_executed_problem_evidence_mismatch"
    );

    let wrong_kick = expected
        .result
        .clone()
        .with_exact_scoring_execution_batch(Some(scoring_batch_with_profile_ids(
            batch,
            batch.kick_table_id().checked_add(1).expect("test kick id"),
            batch.rule_profile_id(),
        )));
    assert_eq!(
        expected
            .authority
            .validate_raw_wasm_execution(&expected.problem, &wrong_kick)
            .expect_err("foreign kick-table header must fail closed")
            .component(),
        "pc_score_exact_wasm_batch_problem_mismatch"
    );

    let wrong_rule = expected
        .result
        .clone()
        .with_exact_scoring_execution_batch(Some(scoring_batch_with_profile_ids(
            batch,
            batch.kick_table_id(),
            batch
                .rule_profile_id()
                .checked_add(1)
                .expect("test rule id"),
        )));
    assert_eq!(
        expected
            .authority
            .validate_raw_wasm_execution(&expected.problem, &wrong_rule)
            .expect_err("foreign rule-profile header must fail closed")
            .component(),
        "pc_score_exact_wasm_batch_problem_mismatch"
    );
}

#[test]
fn pc_score_canonical_compile_keeps_contract_rejections_inside_the_authority() {
    let incompatible_profile = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone();
    let error = PcScoreCompiledAuthority::compile_scenario(
        incompatible_profile,
        PcScoreIngressOrigin::CompatibilityScore,
    )
    .expect_err("compatibility authority must reject a non-Jstris score profile");
    assert!(matches!(
        error,
        PcScoreCompiledAuthorityError::Contract(error)
            if error.component() == "pc_score_request_contract_rejected"
    ));

    let base = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone();
    let capped = base.clone().with_execution_policy(
        base.execution_policy()
            .clone()
            .with_max_memory_mib(Some(64)),
    );
    let error =
        PcScoreCompiledAuthority::compile_scenario(capped, PcScoreIngressOrigin::CanonicalPcScore)
            .expect_err("canonical authority must reject an unaccounted explicit memory cap");
    assert!(matches!(
        error,
        PcScoreCompiledAuthorityError::Contract(error)
            if error.component() == "pc_score_request_contract_rejected"
    ));

    let disabled_hold_with_piece =
        score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
            .query()
            .clone()
            .with_hold_piece(Some(PieceKind::T))
            .with_allow_hold(false);
    let disabled_hold_command = ScenarioAppCommand::new(disabled_hold_with_piece.clone())
        .with_result_projection(PcResultProjection::ScoreSummaryV2(
            PcScoreIngressOrigin::CanonicalPcScore,
        ));
    assert_eq!(
        disabled_hold_command
            .validate_result_projection()
            .expect_err("disabled hold cannot retain a public initial piece"),
        "pc score does not accept an occupied hold slot when hold is disabled"
    );
    let error = PcScoreCompiledAuthority::compile_scenario(
        disabled_hold_with_piece,
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect_err("score authority must reject a retained hold piece before compilation");
    assert!(matches!(
        error,
        PcScoreCompiledAuthorityError::Contract(error)
            if error.component() == "pc_score_request_contract_rejected"
    ));
}

#[test]
fn pc_score_compiled_authority_retains_exclusive_parent_capacity_until_drop() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone();
    let first = PcScoreCompiledAuthority::compile_scenario(
        query.clone(),
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("first score authority owns the full-capacity parent");

    assert!(matches!(
        PcScoreCompiledAuthority::compile_scenario(
            query.clone(),
            PcScoreIngressOrigin::CanonicalPcScore,
        ),
        Err(PcScoreCompiledAuthorityError::ResourceAdmission(_))
    ));

    drop(first);
    let reacquired =
        PcScoreCompiledAuthority::compile_scenario(query, PcScoreIngressOrigin::CanonicalPcScore)
            .expect("dropping the score authority releases the full-capacity parent");
    assert!(
        reacquired
            .terminal_resource_authority()
            .memory_capacity_bytes()
            > 0
    );
}

#[test]
fn pc_score_compiled_authority_binds_the_purpose_separated_evidence_policy() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let portfolio = PcScoreCompiledAuthority::compile_score_minimals_scenario(
        multi_candidate_score_minimals_command().query_arc(),
        crate::PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
    )
    .expect("score-minimals authority installs its private evidence policy");
    assert_eq!(
        portfolio.problem_arc().pc_chance_evidence_policy(),
        PcChanceEvidencePolicy::PcScorePortfolioV2
    );
    drop(portfolio);

    let summary = PcScoreCompiledAuthority::compile_scenario(
        score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I).query_arc(),
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("score-summary authority keeps the generic score evidence contract");
    assert_eq!(
        summary.problem_arc().pc_chance_evidence_policy(),
        PcChanceEvidencePolicy::Disabled
    );
}

#[test]
fn pc_score_external_authority_base_is_fieldwise_and_phase_additions_fail_closed() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let tiny_query = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone();
    let tiny = PcScoreCompiledAuthority::compile_scenario(
        tiny_query,
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("tiny score authority");
    let tiny_base = tiny.external_retained_base_bytes();
    assert!(tiny_base > core::mem::size_of::<PcScoreCompiledAuthority>() as u128);
    drop(tiny);

    let mut retained_queue = Vec::with_capacity(512 * 1024);
    retained_queue.push(PieceKind::I);
    let high_capacity_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(retained_queue)),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    let high_capacity = PcScoreCompiledAuthority::compile_scenario(
        high_capacity_query,
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("accepted high-capacity query has a measured authority base");
    assert!(high_capacity.external_retained_base_bytes() > tiny_base);

    let product_cap = crate::pc_result_projection::PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES;
    let exact_remaining = product_cap
        .checked_sub(high_capacity.external_retained_base_bytes())
        .expect("accepted base fits the product envelope");
    assert_eq!(
        high_capacity
            .checked_external_retained_upper_bound_bytes(exact_remaining)
            .expect("the exact envelope boundary is accepted"),
        product_cap
    );
    assert_eq!(
        high_capacity
            .checked_external_retained_upper_bound_bytes(exact_remaining + 1)
            .expect_err("one byte above the combined envelope fails closed")
            .component(),
        "pc_score_external_retained_envelope_exceeded"
    );
    assert_eq!(
        high_capacity
            .checked_external_retained_upper_bound_bytes(u128::MAX)
            .expect_err("checked addition overflow fails closed")
            .component(),
        "pc_score_external_retained_projection_unavailable"
    );
}

#[test]
fn pc_score_factorized_billion_pattern_authority_retains_symbolic_storage() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    const LOGICAL_PATTERNS: usize = 1_066_867_200;
    let expression =
        QueuePatternExpression::parse("P7P7P2", LOGICAL_PATTERNS).expect("factorized source");
    assert!(expression.is_factorized());
    assert_eq!(expression.pattern_count(), LOGICAL_PATTERNS);
    let query = OpeningPcSearchQuery::new(PcTarget::six_lines())
        .with_queue(PcQueueInput::pattern_expression(expression))
        .with_execution_policy(pc_score_execution_policy())
        .with_objective(ObjectivePolicy::all().with_score_summary());
    let authority =
        PcScoreCompiledAuthority::compile_opening(query, PcScoreIngressOrigin::CanonicalPcScore)
            .expect("the complete six-line factorized domain remains symbolic");
    assert!(authority.external_retained_base_bytes() < LOGICAL_PATTERNS as u128);
    assert!(authority
        .checked_external_retained_upper_bound_bytes(0)
        .is_ok());
}

#[test]
fn pc_score_opening_authority_counts_its_extra_compiled_query_and_checkpoint_owners() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let scenario = PcScoreCompiledAuthority::compile_scenario(
        score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
            .query()
            .clone(),
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("scenario authority");
    let scenario_base = scenario.external_retained_base_bytes();
    drop(scenario);

    let opening_request = opening_score_request(PcScoreIngressOrigin::CanonicalPcScore);
    let opening_query = match opening_request.command() {
        AppCommand::Pc(command) => command.query_arc(),
        _ => unreachable!("opening score helper builds a PC command"),
    };
    let opening = PcScoreCompiledAuthority::compile_opening(
        opening_query,
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("opening authority");
    assert!(opening.external_retained_base_bytes() > scenario_base);
}

#[test]
fn pc_score_direct_outer_envelope_counts_live_context_and_shares_one_query_pointee() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = score_request(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I);
    let (command, output_policy, _, _, _, contract) = request
        .into_execution_parts()
        .expect("typed score execution parts");
    let command_query = match &command {
        AppCommand::Scenario(command) => command.query_arc(),
        _ => unreachable!("score helper builds a scenario command"),
    };
    let contract_query = contract
        .as_ref()
        .and_then(ValidatedProductCapabilityContract::pc_score_query_snapshot_for_test)
        .expect("typed score proof owns the query Arc");
    let PcScoreQuerySnapshot::Scenario(contract_query) = &contract_query else {
        panic!("scenario proof query");
    };
    assert!(Arc::ptr_eq(&command_query, contract_query));

    let validation_report = command.validate();
    assert!(!validation_report.is_empty());
    let additional = crate::app_context::checked_pc_score_direct_context_retained_bytes(
        &validation_report,
        &output_policy,
        contract.as_ref(),
    )
    .expect("direct context projection")
    .expect("score context has an external projection");
    assert!(additional > 0);

    let authority = PcScoreCompiledAuthority::compile_scenario(
        Arc::clone(&command_query),
        PcScoreIngressOrigin::CanonicalPcScore,
    )
    .expect("score authority");
    let PcScoreQuerySnapshot::Scenario(authority_query) = authority.query_snapshot_for_test()
    else {
        panic!("scenario authority query");
    };
    assert!(Arc::ptr_eq(&command_query, authority_query));
    assert_eq!(
        authority
            .checked_external_retained_upper_bound_bytes(additional)
            .expect("base plus direct context fits"),
        crate::pc_result_projection::PC_SCORE_EXTERNAL_RETAINED_UPPER_BOUND_BYTES
    );
}

#[test]
fn direct_wasm_pc_score_finalizes_one_basic_approximation_wrapper() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcScoreIngressOrigin::CanonicalPcScore;
    let expected_query = PcScoreQuerySnapshot::Scenario(Arc::new(
        score_command(origin, PieceKind::I).query().clone(),
    ));
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(score_request(origin, PieceKind::I));
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.pc_score_execution_evidence().is_none());

    let wrapper = response
        .product_capability_result()
        .expect("typed pc score wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcScore);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcScoreSummaryV2
    );
    assert_eq!(wrapper.validation_count(), 1);
    assert!(wrapper.pc_score_portfolio_v2().is_none());
    assert!(response.public_page_source_owner().is_none());
    let score = wrapper.pc_score_summary_v2().expect("score summary v2");
    assert_eq!(score.query(), &expected_query);
    assert_eq!(score.origin(), origin);
    assert_eq!(
        score.score_profile_selection(),
        ScoreProfileSelection::Tetrio
    );
    assert_eq!(score.accuracy_level(), "basic-approximation");
    assert_eq!(
        score.accuracy_reason(),
        "profile-specific basic score/attack tables with configurable spin detection"
    );
    assert!(!score.profile_specific_exact());
    assert!(score.completeness().complete());
    let [field] = score.solution_field_averages() else {
        panic!("one normalized solution must retain exactly one field-average row")
    };
    assert!(field.average_score() > 0.0);
    assert_eq!(field.covered_pattern_count(), 1);
    assert_eq!(field.pattern_count(), 1);
    assert!(field.score_complete());
    assert_eq!(score.overall_score_bits(), field.average_score_bits());
    let payload = wrapper
        .public_result_payload()
        .expect("ordinary pc.score field-summary payload");
    let ProductResultPayloadContent::PcScoreFieldSummary(summary) = payload.content() else {
        panic!("ordinary pc.score must not publish a candidate winner family")
    };
    assert_eq!(summary.fields().len(), 1);
    assert_eq!(summary.overall_score(), score.overall_score());
    assert_eq!(
        summary.overall_score_basis(),
        "all-materialized-patterns-failed-pc-zero"
    );
    let core = response
        .render_model()
        .and_then(|model| model.core_result())
        .expect("public pc score core result");
    assert!(core.exact_scoring_execution_batches().is_empty());
    assert!(core.pc_score_problem_evidence().is_none());
    assert!(core.postprocess_score_cells().is_empty());
    assert_eq!(
        core.field("score_accuracy_level"),
        Some("basic-approximation")
    );
    assert_eq!(core.bool_field("score_profile_specific_exact"), Some(false));
}

#[test]
fn direct_wasm_pc_score_finder_returns_the_complete_score_only_witness_family() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(score_finder_request(PieceKind::I));
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let wrapper = response
        .product_capability_result()
        .expect("typed pc score-finder wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcScoreFinder);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcFixedScoreWitnessV2
    );
    assert!(response.public_page_source_owner().is_none());
    let report = wrapper
        .pc_score_summary_v2()
        .expect("score-finder reuses the closed score evidence owner");
    assert_eq!(report.contract_id(), "pc-fixed-score-witness.v2");
    assert_eq!(
        report.origin(),
        PcScoreIngressOrigin::CanonicalPcScoreFinder
    );
    assert_eq!(report.materialized_pattern_count(), 1);
    assert_eq!(report.total_pattern_count(), 1);
    assert_eq!(
        report.score_profile_selection(),
        ScoreProfileSelection::JstrisUltra
    );
    assert_eq!(
        report.spin_profile_selection(),
        SpinProfileSelection::TSpins
    );
    assert_eq!(report.initial_b2b(), 1);
    let canonical_winner = report
        .canonical_winner()
        .expect("score-finder core-owned canonical witness");
    assert_eq!(
        canonical_winner.candidate_id(),
        report
            .pattern_winners()
            .iter()
            .map(|winner| winner.candidate_id())
            .min()
            .expect("non-empty score-finder winner family")
    );
    assert!(report
        .pattern_winners()
        .windows(2)
        .all(|pair| pair[0].candidate_id() < pair[1].candidate_id()));
    assert!(report.pattern_winners().iter().all(|winner| {
        winner.pattern_id() == 0
            && Some(winner.score()) == report.best_score()
            && winner.informational_attack_basis() == "canonical-equal-score-trace"
    }));

    let payload = wrapper
        .public_result_payload()
        .expect("score-finder public family payload");
    assert_eq!(payload.contract(), "pc.score-finder");
    assert_eq!(payload.result_kind(), "pc-fixed-score-witness.v2");
    let ProductResultPayloadContent::ScorePatternWinnerFamily(family) = payload.content() else {
        panic!("score-finder must publish a normal score witness family")
    };
    assert_eq!(family.equality(), "score-only-attack-informational");
    assert_eq!(family.canonical_selection(), report.canonical_selection());
    assert_eq!(
        family.canonical_winner().candidate_id(),
        canonical_winner.candidate_id().to_string()
    );
    assert_eq!(
        family.canonical_winner().normalized_solution_key(),
        canonical_winner.normalized_solution_key().to_string()
    );
    assert_eq!(
        family.informational_attack_basis(),
        "canonical-equal-score-trace"
    );
}

#[test]
fn direct_wasm_pc_score_minimals_finalizes_distinct_original_id_portfolio() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let command = multi_candidate_score_minimals_command();
    assert!(command.score_minimals_requested());
    assert_eq!(
        command.result_projection(),
        PcResultProjection::pc_score_minimals()
    );
    assert!(matches!(
        AppRequest::new(AppCommand::Scenario(command.clone()))
            .with_product_capability_contract(ProductCapabilityContract::PcScore),
        Err(ProductCapabilityContractError::ProjectionMismatch {
            contract: ProductCapabilityContract::PcScore,
            actual: PcResultProjection::ScorePortfolioV2(_),
        })
    ));

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(multi_candidate_score_minimals_request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.pc_score_execution_evidence().is_none());
    assert!(response.pc_score_portfolio_execution_evidence().is_none());

    let wrapper = response
        .product_capability_result()
        .expect("typed pc score-minimals wrapper");
    assert_eq!(
        wrapper.contract(),
        ProductCapabilityContract::PcScoreMinimals
    );
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcScorePortfolioV2
    );
    assert!(wrapper.pc_score_summary_v2().is_none());
    let report = wrapper
        .pc_score_portfolio_v2()
        .expect("score-minimals portfolio v2");
    assert_eq!(
        report.origin(),
        crate::PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals
    );
    assert!(report.completeness().complete());
    assert!(
        report.eligible_candidates().len() > 1,
        "fixture must exercise the full score candidate dictionary"
    );
    assert!(report.selected_score_candidate_ids().len() <= report.eligible_candidates().len());
    for candidate in report.eligible_candidates() {
        assert_eq!(
            report
                .portfolio_alternatives()
                .public_candidate_id(candidate.portfolio_candidate_id()),
            Some(candidate.score_candidate_id())
        );
    }

    let payload = wrapper
        .public_result_payload()
        .expect("finite canonical score-minimals payload");
    assert_eq!(payload.contract(), "pc.score-minimals");
    assert_eq!(payload.result_kind(), "pc-score-portfolio.v2");
    let ProductResultPayloadContent::CoveragePortfolio(payload) = payload.content() else {
        panic!("score-minimals payload must use the coverage portfolio DTO")
    };
    assert_eq!(payload.canonical_selection(), None);
    assert_eq!(payload.canonical_witness(), None);
    assert_eq!(
        report.canonical_selection(),
        crate::PC_SCORE_CANONICAL_SELECTION
    );
    assert!(payload.page_handle_available());
    assert!(payload.members().iter().all(|member| {
        let parsed = member.candidate_id().parse::<u64>().ok();
        parsed.is_some_and(|candidate_id| {
            report
                .eligible_candidates()
                .iter()
                .any(|candidate| candidate.score_candidate_id() == candidate_id)
        })
    }));

    let crate::ProductPageSourceOwner::CoveragePortfolio(owner) = response
        .public_page_source_owner()
        .expect("live score-minimals page owner")
    else {
        panic!("score-minimals page source must be the shared coverage portfolio")
    };
    assert_eq!(
        owner.set_identity_sha256(),
        report.portfolio_alternatives().set_identity_sha256()
    );
    let member_page = owner
        .member_page(owner.canonical_page(), 1)
        .expect("canonical mapped member page");
    assert!(member_page.members().iter().all(|member| report
        .eligible_candidates()
        .iter()
        .any(|candidate| candidate.score_candidate_id() == member.candidate_id())));
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn native_score_minimals_workers_preserve_the_typed_portfolio_and_report_tail_scope() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let base = pc_score_execution_policy().with_worker_hardware_limit(2);
    let serial = context.run(parallel_score_minimals_request(
        base.clone().with_workers(1),
    ));
    let fixed = context.run(parallel_score_minimals_request(
        base.clone()
            .with_workers(2)
            .with_use_all_logical_processors(true),
    ));
    let auto = context.run(parallel_score_minimals_request(
        base.with_worker_policy(WorkerPolicy::Auto)
            .with_automatic_worker_limit(2)
            .with_use_all_logical_processors(true),
    ));
    for response in [&serial, &fixed, &auto] {
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    }

    fn portfolio(response: &AppResponse) -> &crate::PcScorePortfolioV2Result {
        response
            .product_capability_result()
            .and_then(|result| result.pc_score_portfolio_v2())
            .expect("typed score-minimals portfolio")
    }
    fn assert_same_semantic_portfolio(
        left: &crate::PcScorePortfolioV2Result,
        right: &crate::PcScorePortfolioV2Result,
    ) {
        assert_eq!(left.contract_id(), right.contract_id());
        assert_eq!(left.origin(), right.origin());
        assert_eq!(left.problem_preset(), right.problem_preset());
        assert_eq!(left.problem_id(), right.problem_id());
        assert_eq!(left.score_profile_id(), right.score_profile_id());
        assert_eq!(
            left.materialized_pattern_count(),
            right.materialized_pattern_count()
        );
        assert_eq!(left.pattern_best_scores(), right.pattern_best_scores());
        assert_eq!(left.pattern_winners(), right.pattern_winners());
        assert_eq!(left.eligible_candidates(), right.eligible_candidates());
        assert_eq!(
            left.eligible_candidate_map_sha256(),
            right.eligible_candidate_map_sha256()
        );
        assert_eq!(
            left.score_eligibility_sha256(),
            right.score_eligibility_sha256()
        );
        assert_eq!(
            left.selected_score_candidate_ids(),
            right.selected_score_candidate_ids()
        );
        assert_eq!(
            left.selected_solution_keys(),
            right.selected_solution_keys()
        );
        assert_eq!(
            left.canonical_score_candidate_id(),
            right.canonical_score_candidate_id()
        );
        assert_eq!(
            left.canonical_solution_key(),
            right.canonical_solution_key()
        );
        assert_eq!(
            left.portfolio_alternatives(),
            right.portfolio_alternatives()
        );
        assert_eq!(left.completeness(), right.completeness());
    }
    assert_same_semantic_portfolio(portfolio(&fixed), portfolio(&serial));
    assert_same_semantic_portfolio(portfolio(&auto), portfolio(&serial));
    assert_eq!(
        fixed
            .product_capability_result()
            .and_then(|result| result.public_result_payload()),
        serial
            .product_capability_result()
            .and_then(|result| result.public_result_payload())
    );
    assert_eq!(
        auto.product_capability_result()
            .and_then(|result| result.public_result_payload()),
        serial
            .product_capability_result()
            .and_then(|result| result.public_result_payload())
    );

    for response in [&fixed, &auto] {
        let core = response
            .render_model()
            .and_then(crate::AppRenderModel::core_result)
            .expect("score-minimals Core telemetry");
        if core
            .usize_field("workers_used")
            .is_some_and(|workers| workers > 1)
        {
            assert_eq!(core.bool_field("cpu_parallel_execution"), Some(true));
            assert_eq!(
                core.field("cpu_parallel_decision_reason"),
                Some("parallel-immutable-family-queue")
            );
        }
        // The exact-family/BuildUp source is parallel. Candidate x pattern
        // scoring and the exact optimal-portfolio finalizer are intentionally
        // still one coordinator-owned deterministic tail.
        assert_eq!(
            core.field("score_execution_distribution"),
            Some("coordinator")
        );
    }
}

#[test]
fn opening_wasm_pc_score_validates_the_two_line_target_frame() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcScoreIngressOrigin::CanonicalPcScore;
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(opening_score_request(origin));
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");

    let score = response
        .product_capability_result()
        .and_then(|result| result.pc_score_summary_v2())
        .expect("typed opening pc score summary");
    assert_eq!(score.origin(), origin);
    assert!(matches!(score.query(), PcScoreQuerySnapshot::Opening(_)));
    assert_eq!(score.accuracy_level(), "basic-approximation");
    assert!(!score.profile_specific_exact());
    assert!(score.completeness().complete());
}

#[test]
fn pc_score_final_binding_shares_the_maximum_fixed_sequence_and_matches_fieldwise() {
    const MAX_QUEUE_LEN: usize = 16;

    fn borrowed_score_binding<'a>(
        validated: &'a ValidatedProductCapabilityContract,
    ) -> (PcScoreQueryBinding<'a>, PcScoreIngressOrigin) {
        validated
            .pc_score_binding()
            .expect("validated pc.score proof carries a borrowed score binding")
    }

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; MAX_QUEUE_LEN])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    let command = AppCommand::Scenario(
        ScenarioAppCommand::new(query.clone()).with_result_projection(
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore),
        ),
    );
    let command_query = match &command {
        AppCommand::Scenario(command) => command.query(),
        _ => unreachable!("score binding test uses a scenario command"),
    };
    let envelope = command.query_envelope();
    let validated = ProductCapabilityContract::PcScore
        .validate_request(&command, &envelope)
        .expect("maximum fixed-sequence pc.score request is structurally valid");

    let (binding, origin) = borrowed_score_binding(&validated);
    assert_eq!(origin, PcScoreIngressOrigin::CanonicalPcScore);
    let PcScoreQueryBinding::Scenario(borrowed) = &binding else {
        panic!("scenario pc.score proof must retain a scenario query")
    };
    assert!(std::ptr::eq(command_query, *borrowed));
    assert_eq!(
        borrowed
            .remaining_queue()
            .as_fixed_sequence()
            .expect("fixed queue remains fixed")
            .len(),
        MAX_QUEUE_LEN
    );

    let exact_snapshot = PcScoreQuerySnapshot::Scenario(Arc::new(query));
    assert!(binding.matches_snapshot(&exact_snapshot));

    let mut different_pieces = vec![PieceKind::I; MAX_QUEUE_LEN];
    different_pieces[MAX_QUEUE_LEN - 1] = PieceKind::O;
    let different_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(different_pieces)),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    assert!(!binding.matches_snapshot(&PcScoreQuerySnapshot::Scenario(Arc::new(different_query,))));
    assert!(
        !binding.matches_snapshot(&PcScoreQuerySnapshot::Opening(Arc::new(
            OpeningPcSearchQuery::new(PcTarget::two_lines())
                .with_execution_policy(pc_score_execution_policy())
                .with_objective(ObjectivePolicy::all().with_score_summary()),
        ),))
    );
}

#[test]
fn pc_score_contract_rejects_a_fixed_sequence_above_the_product_bound() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 17])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore),
    );
    assert_eq!(
        command
            .validate_result_projection()
            .expect_err("17 source pieces exceed the closed score product bound"),
        "pc score accepts at most 16 fixed source pieces"
    );
}

#[test]
fn pc_score_contract_measures_fixed_queue_capacity_instead_of_only_its_length() {
    let piece_size = core::mem::size_of::<PieceKind>();
    assert!(piece_size > 0);
    let retained_piece_capacity = (1024 * 1024 / piece_size) + 1;
    let mut pieces = Vec::with_capacity(retained_piece_capacity);
    pieces.push(PieceKind::I);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore),
    );

    assert_eq!(
        command
            .validate_result_projection()
            .expect_err("one retained element must not hide an oversized queue allocation"),
        "pc score fixed queue retains more than the product memory envelope"
    );
}

#[test]
fn pc_score_scenario_geometry_is_derived_after_initial_line_clear() {
    let full_bottom_row = 0x3ff_u64;
    let four_cells_above = 0x0f_u64 << 10;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, full_bottom_row | four_cells_above),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 4])),
        PieceWindow::new(4),
    )
    .with_exact_pieces(Some(4))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(pc_score_execution_policy())
    .with_objective(ObjectivePolicy::all().with_score_summary());
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore),
    );

    command
        .validate_result_projection()
        .expect("the normalized board has exactly sixteen empty cells");
}

#[test]
fn pc_score_opening_supply_window_stays_between_required_and_automatic_sources() {
    let base = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I;
            7
        ])))
        .with_execution_policy(pc_score_execution_policy())
        .with_objective(ObjectivePolicy::all().with_score_summary());

    for source_pieces in [4, 7] {
        let command = PcAppCommand::new(
            base.clone()
                .with_supply_window_size(SupplyWindowSize::new(source_pieces)),
        )
        .with_result_projection(PcResultProjection::ScoreSummaryV2(
            PcScoreIngressOrigin::CanonicalPcScore,
        ));
        assert_eq!(
            command
                .validate_result_projection()
                .expect_err("source window outside 5..=6 must fail closed"),
            "pc score source window must stay within the required automatic search window"
        );
    }
}

#[test]
fn pc_score_contract_rejects_stale_queries_profiles_and_memory_without_generic_inheritance() {
    let origin = PcScoreIngressOrigin::CanonicalPcScore;
    let attached = score_request(origin, PieceKind::I);
    let stale = attached.with_command_for_product_capability_test(AppCommand::Scenario(
        score_command(origin, PieceKind::O),
    ));
    assert_request_validation_contract_rejected(
        &stale,
        "pc.score product capability proof is stale for the current command",
    );

    let compatibility = AppRequest::new(AppCommand::Scenario(score_command(
        PcScoreIngressOrigin::CompatibilityScore,
        PieceKind::I,
    )))
    .with_product_capability_contract(ProductCapabilityContract::PcScore)
    .expect_err("compatibility score must reject the canonical Tetrio default");
    assert!(matches!(
        compatibility,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("fixed jstris-ultra profile")
    ));

    let base = score_command(origin, PieceKind::I).query().clone();
    let capped_policy = base
        .execution_policy()
        .clone()
        .with_max_memory_mib(Some(64));
    let capped = base.with_execution_policy(capped_policy);
    let typed = ScenarioAppCommand::new(capped.clone())
        .with_result_projection(PcResultProjection::ScoreSummaryV2(origin));
    let rejection = AppRequest::new(AppCommand::Scenario(typed))
        .with_product_capability_contract(ProductCapabilityContract::PcScore)
        .expect_err("typed score must reject an unaccounted transient memory cap");
    assert!(matches!(
        rejection,
        ProductCapabilityContractError::RequestContractRejected(reason)
            if reason.contains("fixed-cap CPU execution policy")
    ));

    let generic = AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(capped)));
    assert_eq!(generic.product_capability_contract(), None);
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn pc_score_native_worker_policies_reserve_their_effective_compute_width() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let base = pc_score_execution_policy();
    let policies = [
        base.clone()
            .with_workers(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true)
            .with_cpu_warmup(true),
        base.with_worker_policy(WorkerPolicy::Auto)
            .with_automatic_worker_limit(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    ];

    for policy in policies {
        let expected_workers = policy.workers();
        let query = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
            .query()
            .clone()
            .with_execution_policy(policy);
        let command = ScenarioAppCommand::new(query.clone()).with_result_projection(
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore),
        );
        command
            .validate_result_projection()
            .expect("native score accepts bounded CPU worker policy controls");
        let authority = PcScoreCompiledAuthority::compile_scenario(
            query,
            PcScoreIngressOrigin::CanonicalPcScore,
        )
        .expect("native score reserves its effective compute width before compilation");
        assert_eq!(
            authority
                .terminal_resource_authority()
                .compute_capacity_units(),
            u32::try_from(expected_workers).expect("effective host workers fit the authority")
        );
        drop(authority);
    }

    let invalid = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone()
        .with_execution_policy(pc_score_execution_policy().with_workers(0));
    assert_eq!(
        ScenarioAppCommand::new(invalid)
            .with_result_projection(PcResultProjection::ScoreSummaryV2(
                PcScoreIngressOrigin::CanonicalPcScore,
            ))
            .validate_result_projection()
            .expect_err("a zero-width fixed policy remains invalid"),
        "pc score requires its fixed-cap CPU execution policy"
    );
}

#[test]
fn generic_score_keeps_its_legacy_surface_but_never_leaks_problem_evidence() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = score_command(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I)
        .query()
        .clone();
    let request = AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(query)));
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.product_capability_result().is_none());
    assert!(response.pc_score_execution_evidence().is_none());
    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("generic score core result");
    assert!(
        !core.exact_scoring_execution_batches().is_empty(),
        "generic score retains its established replay surface"
    );
    assert!(core.pc_score_problem_evidence().is_none());
}

#[test]
fn pc_score_finalizer_rejects_incomplete_resource_evidence_and_consumes_all_transients() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let request = score_request(PcScoreIngressOrigin::CanonicalPcScore, PieceKind::I);
    let command_kind = request.command_kind();
    let (command, output_policy, resource_budget, _, _, contract) = request
        .into_execution_parts()
        .expect("valid pc score execution parts");
    let control = ExecutionControl::default();
    let validation_report = command.validate();
    let pc_score_external_retained_context_bytes =
        crate::app_context::checked_pc_score_direct_context_retained_bytes(
            &validation_report,
            &output_policy,
            contract.as_ref(),
        )
        .expect("direct score external owner projection is available");
    let execution_context = AppExecutionContext {
        services: context.services(),
        language: context.language(),
        file_policy: context.file_policy(),
        output_policy: &output_policy,
        resource_budget: &resource_budget,
        execution_control: &control,
        pc_score_external_retained_context_bytes,
    };
    let raw = command.run(&execution_context);
    assert!(raw.pc_score_execution_evidence().is_some());
    assert!(raw
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .is_some_and(|result| {
            result.exact_scoring_execution_batches().is_empty()
                && result.pc_score_problem_evidence().is_none()
                && result.postprocess_score_cells().is_empty()
                && result.solution_set_audit_report().is_none()
        }));

    let mut resources = raw.resource_report().clone();
    resources.probability_complete = false;
    let rejected = context.finalize_response_with_product_capability(
        raw.with_resource_report(resources),
        command_kind,
        &output_policy,
        contract,
    );
    assert_eq!(rejected.status(), AppStatus::ExecutionFailed);
    assert!(rejected.product_capability_result().is_none());
    assert!(rejected.pc_score_execution_evidence().is_none());
    assert!(rejected.render_model().is_none());
    assert!(rejected
        .error()
        .expect("resource-incomplete score rejection")
        .message()
        .contains("resource probability result is incomplete"));
}

#[test]
fn pc_score_zero_solution_matrix_is_complete_and_not_reclassified_as_missing() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcScoreIngressOrigin::CanonicalPcScore;
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(score_request(origin, PieceKind::O));
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let score = response
        .product_capability_result()
        .and_then(|result| result.pc_score_summary_v2())
        .expect("complete zero-solution score summary");
    assert_eq!(score.matrix_cell_count(), 0);
    assert_eq!(score.pattern_optimal_count(), 0);
    assert_eq!(score.failed_pc_pattern_count(), 1);
    assert_eq!(score.best_score(), None);
    assert_eq!(score.best_attack(), None);
    assert_eq!(score.covered_probability_bits(), 0.0_f64.to_bits());
    assert_eq!(score.overall_score_bits(), 0.0_f64.to_bits());
    assert_eq!(score.covered_pattern_conditional_average_score(), None);
    assert!(score.solution_field_averages().is_empty());
    let payload = response
        .product_capability_result()
        .and_then(|result| result.public_result_payload())
        .expect("failed input field-summary payload");
    let ProductResultPayloadContent::PcScoreFieldSummary(summary) = payload.content() else {
        panic!("failed ordinary pc.score still uses the field-summary payload")
    };
    assert_eq!(summary.scored_pattern_count(), "0");
    assert_eq!(summary.failed_pc_pattern_count(), "1");
    assert_eq!(summary.overall_score(), "0");
    assert_eq!(
        summary.score_covered_pattern_conditional_average_score(),
        None
    );
    assert_eq!(summary.solution_field_count(), "0");
    assert!(summary.fields().is_empty());
    assert!(score.completeness().matrix_complete());
    assert!(score.completeness().summary_complete());
}

#[test]
fn cooperative_pc_score_uses_the_same_closed_evidence_and_honors_cancellation() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );

    for (origin, expected_profile) in [
        (
            PcScoreIngressOrigin::CanonicalPcScore,
            ScoreProfileSelection::Tetrio,
        ),
        (
            PcScoreIngressOrigin::CompatibilityScore,
            ScoreProfileSelection::JstrisUltra,
        ),
    ] {
        let request = match origin {
            PcScoreIngressOrigin::CanonicalPcScore => score_request(origin, PieceKind::I),
            PcScoreIngressOrigin::CompatibilityScore => {
                let command = score_command(origin, PieceKind::I);
                let score = command
                    .query()
                    .objective()
                    .score()
                    .with_profile(expected_profile);
                let command = ScenarioAppCommand::new(
                    command
                        .query()
                        .clone()
                        .with_objective(ObjectivePolicy::all().with_score_policy(score)),
                )
                .with_result_projection(PcResultProjection::ScoreSummaryV2(origin));
                AppRequest::new(AppCommand::Scenario(command))
                    .with_product_capability_contract(ProductCapabilityContract::PcScore)
                    .expect("valid compatibility score request")
            }
            PcScoreIngressOrigin::CanonicalPcScoreFinder => {
                unreachable!("this regression iterates only pc.score origins")
            }
        };
        let mut execution = context.start_cooperative_execution(request.clone());
        let control = ExecutionControl::default();
        let mut response = None;
        for _ in 0..256 {
            match execution.advance(4_096, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(completed) => {
                    response = Some(completed);
                    break;
                }
                CooperativeAppAdvance::CompletedGoverned(_) => {
                    panic!("non-Build cooperative score returned a governed Build response")
                }
                CooperativeAppAdvance::FailedFinite(error) => {
                    panic!("non-Build cooperative score failed as finite: {error:?}")
                }
                CooperativeAppAdvance::Cancelled => {
                    panic!("uncancelled cooperative pc score was cancelled")
                }
            }
        }
        let response = response.expect("tiny cooperative pc score must complete within the bound");
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
        let score = response
            .product_capability_result()
            .and_then(|result| result.pc_score_summary_v2())
            .expect("cooperative score summary");
        assert_eq!(score.origin(), origin);
        assert_eq!(score.score_profile_selection(), expected_profile);
        assert!(score.completeness().complete());
        assert!(response.pc_score_execution_evidence().is_none());
        assert!(response
            .render_model()
            .and_then(crate::AppRenderModel::core_result)
            .is_some_and(|result| result.pc_score_problem_evidence().is_none()));

        let released_query = match request.command() {
            AppCommand::Scenario(command) => command.query_arc(),
            _ => unreachable!("score request is a scenario command"),
        };
        let reacquired = PcScoreCompiledAuthority::compile_scenario(released_query, origin)
            .expect("completed response retains evidence but no execution authority");
        drop(reacquired);

        let mut cancelled = context.start_cooperative_execution(request);
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        assert_eq!(
            cancelled.advance(1, &ExecutionControl::new(cancellation)),
            CooperativeAppAdvance::Cancelled
        );
    }
}

#[test]
fn pc_score_native_fails_closed_while_distributed_binds_typed_authority() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcScoreIngressOrigin::CanonicalPcScore;
    let request = score_request(origin, PieceKind::I);
    let native = AppContext::default().run(request.clone());
    assert_eq!(native.status(), AppStatus::Unsupported);
    assert!(native.product_capability_result().is_none());
    assert!(native.pc_score_execution_evidence().is_none());
    assert!(native.render_model().is_none());
    assert!(native
        .error()
        .expect("typed native score rejection")
        .message()
        .contains("pc_score_native_executed_problem_evidence_unavailable"));

    let DistributedSearchPreparation::Search(distributed) =
        AppContext::default().prepare_distributed_search(request)
    else {
        panic!("typed distributed score must bind its compiled authority");
    };
    assert!(distributed.is_pc_score());
    assert!(distributed
        .pc_score_terminal_resource_authority()
        .expect("typed score distributed authority projection")
        .is_some());
}

#[test]
fn distributed_typed_minimals_request_binds_result_identity_before_search() {
    let context = AppContext::default();
    let DistributedSearchPreparation::Search(prepared) =
        context.prepare_distributed_search(minimals_request())
    else {
        panic!("typed distributed minimals request must bind before search preparation");
    };
    assert!(!prepared.is_pc_score());
    assert!(prepared.problem().exact_pieces().is_some());
}

#[test]
fn distributed_pc_chance_binds_product_identity_before_solver_creation() {
    let context = AppContext::default();
    let DistributedSearchPreparation::Search(prepared) = context.prepare_distributed_search(
        probability_request(PcChanceIngressOrigin::CanonicalPcChance),
    ) else {
        panic!("typed pc chance must bind before distributed search preparation");
    };
    assert!(!prepared.is_pc_score());
    assert!(prepared.problem().exact_pieces().is_some());
}

fn assert_typed_request_rejected(response: &AppResponse, expected: &str) {
    assert_eq!(response.status(), AppStatus::ValidationFailed);
    assert_eq!(response.command(), Some(AppCommandKind::Pc));
    assert!(response.product_capability_result().is_none());
    assert!(response.render_model().is_none());
    let error = response.error().expect("typed request rejection");
    assert_eq!(error.code(), AppErrorCode::InvalidInput);
    assert!(
        error.message().contains(expected),
        "{:?} did not contain {expected:?}",
        error.message()
    );
}

fn assert_pc_probability_success(
    response: &AppResponse,
    origin: PcChanceIngressOrigin,
    expected_query: &PcChanceQuerySnapshot,
) {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.pc_chance_execution_evidence().is_none());
    let wrapper = response
        .product_capability_result()
        .expect("pc chance product wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcChance);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcProbabilityV2
    );
    assert_eq!(wrapper.validation_count(), 1);
    let probability = wrapper
        .pc_probability_v2()
        .expect("pc-probability.v2 report");
    assert_eq!(probability.contract_id(), "pc-probability.v2");
    assert_eq!(probability.origin(), origin);
    assert_eq!(probability.query(), expected_query);
    assert!(probability.completeness().complete());
    assert_eq!(
        probability.weighted_probability(),
        match f64::from_bits(probability.weighted_probability_bits()) {
            0.0 => "0".to_owned(),
            1.0 => "1".to_owned(),
            value => value.to_string(),
        }
    );
    let core = response
        .render_model()
        .and_then(crate::AppRenderModel::core_result)
        .expect("pc chance public Core result");
    assert!(core.pc_chance_coverage_evidence().is_none());
}

fn assert_contract_rejected(response: AppResponse, expected: &str) {
    let response = finalize_direct(chance_request(), response);
    assert_eq!(response.status(), AppStatus::ExecutionFailed);
    assert!(response.product_capability_result().is_none());
    let message = response.error().expect("fail-closed error").message();
    assert!(
        message.contains(expected),
        "{message:?} did not contain {expected:?}"
    );
}

#[test]
fn pc_failed_queue_requires_its_closed_contract_while_legacy_failed_queue_stays_generic() {
    let typed = AppRequest::new(AppCommand::Percent(failed_queue_command(
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        PieceKind::I,
        2,
    )));
    assert_request_validation_contract_rejected(
        &typed,
        "pc.failed-queue result projection requires its matching product capability contract",
    );

    let generic = AppRequest::new(AppCommand::Percent(
        PercentAppCommand::failed_queue(failed_queue_query(PieceKind::I))
            .with_failed_pattern_limit(2),
    ));
    assert_eq!(generic.product_capability_contract(), None);
    assert!(!AppContext::default()
        .validate_request(&generic)
        .has_errors());
    let rejection = generic
        .with_product_capability_contract(ProductCapabilityContract::PcFailedQueue)
        .expect_err("legacy failed-queue cannot inherit the typed product claim");
    assert!(matches!(
        rejection,
        ProductCapabilityContractError::RequestContractRejected(
            "pc.failed-queue requires a typed failed-queue command"
        )
    ));
}

#[test]
fn pc_failed_queue_proof_binds_query_origin_and_requested_limit() {
    let attached = failed_queue_request(
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        PieceKind::I,
        2,
    );
    assert!(!AppContext::default()
        .validate_request(&attached)
        .has_errors());

    for stale_command in [
        failed_queue_command(
            PcFailedQueueIngressOrigin::CompatibilityFailedQueueUnderscore,
            PieceKind::I,
            2,
        ),
        failed_queue_command(
            PcFailedQueueIngressOrigin::CanonicalFailedQueue,
            PieceKind::O,
            2,
        ),
        failed_queue_command(
            PcFailedQueueIngressOrigin::CanonicalFailedQueue,
            PieceKind::I,
            3,
        ),
    ] {
        let stale = attached
            .clone()
            .with_command_for_product_capability_test(AppCommand::Percent(stale_command));
        assert_request_validation_contract_rejected(
            &stale,
            "pc.failed-queue product capability proof is stale for the current command",
        );
    }
}

#[test]
fn pc_failed_queue_finalizer_rejects_missing_evidence_and_wrong_render_family() {
    let request = failed_queue_request(
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        PieceKind::I,
        1,
    );
    let missing = finalize_direct(
        request.clone(),
        AppResponse::success(crate::AppRenderModel::Percent(CoreExecutionResult::new(
            Vec::new(),
            Vec::new(),
        ))),
    );
    assert_eq!(missing.status(), AppStatus::ExecutionFailed);
    assert!(missing.product_capability_result().is_none());
    assert!(missing.pc_failed_queue_execution_evidence().is_none());
    assert!(missing
        .error()
        .expect("missing evidence rejection")
        .message()
        .contains("pc failed-queue execution evidence is missing"));

    let wrong_family = finalize_direct(
        request,
        AppResponse::success(crate::AppRenderModel::Pc(CoreExecutionResult::new(
            Vec::new(),
            Vec::new(),
        )))
        .with_result_kind_for_test("percent"),
    );
    assert_eq!(wrong_family.status(), AppStatus::ExecutionFailed);
    assert!(wrong_family.product_capability_result().is_none());
    assert!(wrong_family
        .error()
        .expect("wrong-family rejection")
        .message()
        .contains("response render kind mismatch"));
}

#[test]
fn pc_failed_queue_wasm_and_distributed_paths_fail_closed_without_fallback() {
    let request = failed_queue_request(
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        PieceKind::I,
        1,
    );
    let wasm = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request.clone());
    assert_eq!(wasm.status(), AppStatus::Unsupported, "{wasm:#?}");
    assert_eq!(wasm.command(), Some(AppCommandKind::Percent));
    assert!(wasm.product_capability_result().is_none());
    assert!(wasm.pc_failed_queue_execution_evidence().is_none());
    assert!(wasm
        .error()
        .expect("WASM typed failed-queue rejection")
        .message()
        .contains("pc_failed_queue_wasm_typed_execution_unavailable"));

    let DistributedSearchPreparation::Ready(distributed) =
        AppContext::default().prepare_distributed_search(request)
    else {
        panic!("typed failed-queue must stop before distributed search preparation");
    };
    assert_eq!(distributed.status(), AppStatus::ValidationFailed);
    assert_eq!(distributed.command(), Some(AppCommandKind::Percent));
    assert!(distributed.product_capability_result().is_none());
    assert!(distributed.pc_failed_queue_execution_evidence().is_none());
    assert!(distributed
        .error()
        .expect("distributed typed failed-queue rejection")
        .message()
        .contains("distributed product capability result binding is unavailable"));
}

#[test]
#[cfg_attr(
    not(feature = "native-c-core"),
    ignore = "requires the native clearra_core static library"
)]
fn pc_failed_queue_native_success_returns_one_v2_wrapper_and_strips_private_evidence() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let origin = PcFailedQueueIngressOrigin::CanonicalFailedQueue;
    let expected_query = crate::PcFailedQueueQuerySnapshot::Scenario(
        failed_queue_command(origin, PieceKind::I, 2)
            .query()
            .expect("scenario failed-queue query")
            .clone(),
    );
    let response = AppContext::default().run(failed_queue_request(origin, PieceKind::I, 2));

    assert_eq!(response.status(), AppStatus::Success, "{response:#?}");
    assert!(response.pc_failed_queue_execution_evidence().is_none());
    let wrapper = response
        .product_capability_result()
        .expect("pc failed-queue v2 wrapper");
    assert_eq!(wrapper.contract(), ProductCapabilityContract::PcFailedQueue);
    assert_eq!(
        wrapper.result_kind(),
        ProductCapabilityResultKind::PcFailedQueueV2
    );
    assert_eq!(wrapper.validation_count(), 1);
    let report = wrapper
        .pc_failed_queue_v2()
        .expect("pc-failed-queue.v2 report");
    assert_eq!(report.contract_id(), "pc-failed-queue.v2");
    assert_eq!(report.origin(), origin);
    assert_eq!(report.query(), &expected_query);
    assert_eq!(report.failed_pattern_limit(), 2);
}

#[test]
#[cfg_attr(
    not(feature = "native-c-core"),
    ignore = "requires the native clearra_core static library"
)]
fn pc_failed_queue_rejects_evidence_swapped_between_compiled_problem_owners() {
    use crate::pc_failed_queue_result::PcFailedQueueCompiledAuthority;

    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let first_query = failed_queue_query(PieceKind::I);
    let first = PcFailedQueueCompiledAuthority::scenario(
        &first_query,
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        1,
    )
    .expect("first failed-queue authority");
    let second_query = failed_queue_query(PieceKind::O);
    let second = PcFailedQueueCompiledAuthority::scenario(
        &second_query,
        PcFailedQueueIngressOrigin::CompatibilityFailedQueueUnderscore,
        1,
    )
    .expect("second failed-queue authority");
    let (result, evidence) = PercentService::execute_failed_queue(first.problem_arc(), 1)
        .expect("first native failed-queue execution")
        .into_parts();

    let rejection = second
        .validate_and_decorate(result, evidence)
        .expect_err("evidence from a different compiled owner must be rejected");
    assert!(
        rejection
            .reason()
            .contains("does not belong to the executed problem owner"),
        "{rejection}"
    );
}

#[test]
#[cfg_attr(
    not(feature = "native-c-core"),
    ignore = "requires the native clearra_core static library"
)]
fn pc_failed_queue_rejects_tampered_core_result_before_v2_decoration() {
    use crate::pc_failed_queue_result::PcFailedQueueCompiledAuthority;

    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let query = failed_queue_query(PieceKind::I);
    let authority = PcFailedQueueCompiledAuthority::scenario(
        &query,
        PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        1,
    )
    .expect("failed-queue authority");
    let (result, evidence) = PercentService::execute_failed_queue(authority.problem_arc(), 1)
        .expect("native failed-queue execution")
        .into_parts();
    let tampered = result.with_replaced_fields(vec![(
        "covered_pattern_count".to_owned(),
        usize::MAX.to_string(),
    )]);

    let rejection = authority
        .validate_and_decorate(tampered, evidence)
        .expect_err("tampered Core result must be rejected");
    assert!(
        rejection
            .reason()
            .contains("field is missing, duplicated, or mismatched"),
        "{rejection}"
    );
}

#[test]
fn pc_failed_queue_memory_admission_mapping_preserves_the_core_resource_report() {
    let mut report = CoreResourceReport::complete();
    report.observe_cpu_bytes(4096);
    report.observe_candidate_rows(7);
    let response =
        AppPcFailedQueueExecutionError::EvidenceMemoryAdmission(Box::new(report)).into_response();

    assert_eq!(response.status(), AppStatus::ExecutionFailed);
    assert_eq!(response.resource_report().peak_cpu_bytes, 4096);
    assert_eq!(response.resource_report().peak_candidate_rows, 7);
    assert!(response.resource_report().solver_executed());
    assert!(response.resource_report().probability_complete());
    assert!(response.product_capability_result().is_none());
    assert!(response.pc_failed_queue_execution_evidence().is_none());
}
