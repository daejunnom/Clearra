//! Product-level parity through the public App boundary, including real
//! cooperative-source and native completion ownership.
#![cfg(all(feature = "parallel", not(target_family = "wasm")))]
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppResponse, AppServices,
    AppStatus, CooperativeAppAdvance, PcPathIngressOrigin, PcResultProjection,
    ProductCapabilityContract, ScenarioAppCommand, PC_SCORE_MAX_PATTERNS,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;
use std::sync::{Arc, Mutex};

static REQUESTS: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct ComputeEvents(Mutex<Vec<(&'static str, std::thread::ThreadId)>>);

impl clearra_core_domain::execution_cancellation::ProgressSink for ComputeEvents {
    fn report(&self, event: clearra_core_domain::execution_cancellation::ExecutionProgress) {
        self.0
            .lock()
            .unwrap()
            .push((event.stage, std::thread::current().id()));
    }
}

fn request(workers: usize) -> AppRequest {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_use_all_logical_processors(true)
        .with_allow_backend_fallback(false)
        .with_max_patterns(PC_SCORE_MAX_PATTERNS);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0xfc3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 2])),
        PieceWindow::new(2),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(2))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all())
    .with_retained_trace_limit(1)
    .with_execution_policy(policy);
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(PcResultProjection::PathFamilyV2(
            PcPathIngressOrigin::CanonicalPcPath,
        )),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcPath)
    .unwrap()
}

fn cooperative(context: &AppContext) -> AppResponse {
    let mut cursor = context.start_cooperative_execution(request(1));
    for _ in 0..16384 {
        match cursor.advance(64, &ExecutionControl::default()) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => (),
            CooperativeAppAdvance::Completed(response) => return response,
            other => panic!("unexpected cooperative outcome: {other:?}"),
        }
    }
    panic!("small two-piece fixture did not finish");
}

#[test]
fn parallel_pc_path_preserves_the_cooperative_source_hash_counts_and_first_page() {
    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let expected = cooperative(&context);
    assert_eq!(expected.status(), AppStatus::Success, "{expected:?}");
    let expected = expected
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    assert!(expected.completeness().complete());
    assert!(
        expected.witness_count() >= 2,
        "both orders of the two O pieces are witnesses"
    );
    for workers in [1, 2, 4, 8] {
        let caller = std::thread::current().id();
        let events = Arc::new(ComputeEvents::default());
        let actual = context.run_with_execution_control(
            request(workers),
            &ExecutionControl::default()
                .with_parallelism(workers)
                .with_progress_sink(events.clone()),
        );
        assert_eq!(
            actual.status(),
            AppStatus::Success,
            "N={workers}: {actual:?}"
        );
        let observed = events.0.lock().unwrap();
        assert!(observed.iter().any(|(stage, _)| *stage == "validation"));
        assert!(
            observed.iter().all(|(_, thread)| *thread != caller),
            "N={workers}: the product caller must not perform validation, search or completion"
        );
        let actual = actual
            .product_capability_result()
            .unwrap()
            .pc_path_family_v2()
            .unwrap();
        assert!(actual.completeness().complete());
        assert_eq!(actual.witnesses(), expected.witnesses());
        assert_eq!(actual.witness_count(), expected.witness_count());
        let source = actual.page_source().unwrap();
        let expected_source = expected.page_source().unwrap();
        assert_eq!(source.identity_sha256(), expected_source.identity_sha256());
        assert_eq!(source.geometry_count(), expected_source.geometry_count());
        assert_eq!(
            source.page_metadata(1, 1).unwrap(),
            expected_source.page_metadata(1, 1).unwrap()
        );
    }
}

#[test]
fn setup_score_keeps_original_pattern_reduction_and_caller_isolation() {
    use clearra_app::{FieldDocumentFormat, SetupScoreAppCommand, SetupScoreDocumentV1};
    use clearra_ctk3::{encode_ctk3, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
    use clearra_objectives::policy::score_objective_policy::ScoreProfileSelection;
    use clearra_supply::queue::{queue_parser, queue_pattern_expression::QueuePatternExpression};

    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    register_native_product_host();
    let pages = [0, 6].map(|start| {
        let mut cells = vec![Ctk3Color::Empty; 20];
        cells[start..start + 4].fill(Ctk3Color::Piece(Ctk3Piece::I));
        Ctk3Page::new(2, cells)
    });
    let encoded = encode_ctk3(&Ctk3Document::new(10, pages.to_vec())).unwrap();
    let document = SetupScoreDocumentV1::decode(FieldDocumentFormat::Ctk3, &encoded).unwrap();
    let mut baseline = None;
    for workers in [1, 2, 4, 8] {
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_use_all_logical_processors(true)
            .with_workers(workers)
            .with_max_patterns(usize::MAX);
        let command = SetupScoreAppCommand::new(
            document.clone(),
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("P7", usize::MAX).unwrap(),
            ),
            None,
            PcQueueInput::fixed_sequence(queue_parser::parse_fixed_sequence("OTSJ").unwrap()),
            None,
            2,
            false,
            ScoreProfileSelection::Tetrio,
            0,
            srs_plus(),
            policy,
        )
        .unwrap();
        let caller = std::thread::current().id();
        let events = Arc::new(ComputeEvents::default());
        let response = AppContext::rust_cpu_product().run_with_execution_control(
            AppRequest::new(AppCommand::SetupScore(command)),
            &ExecutionControl::default()
                .with_parallelism(workers)
                .with_progress_sink(events.clone()),
        );
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "N={workers}: {response:?}"
        );
        let payload = response.public_result_payload().unwrap();
        if let Some(expected) = &baseline {
            assert_eq!(payload, expected);
        } else {
            baseline = Some(payload.clone());
        }
        let observed = events.0.lock().unwrap();
        assert!(observed
            .iter()
            .any(|(stage, _)| *stage == "setup-score-reduction"));
        assert!(observed.iter().all(|(_, thread)| *thread != caller));
    }
}

#[test]
fn long_boundary_recovery_keeps_public_queue_and_role_indices() {
    use clearra_app::BoundaryRecoveryAppCommand;
    use clearra_core_domain::board::standard_pc_board::Board256Mask;
    use clearra_forward_search::{BoundaryRecoveryQuery, EarlyPlacementPolicy};
    use clearra_host_contract::ProductResultPayloadContent;
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;

    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let count = 260;
    let query = BoundaryRecoveryQuery {
        initial_board: Board256Mask::EMPTY,
        stage_one_target: Board256Mask::EMPTY,
        final_board: Board256Mask::EMPTY,
        height: 4,
        queue: vec![PieceKind::I; count],
        stage_one_queue_len: 10,
        required_placements: Some(count),
        placement_role_masks: (0..count)
            .map(|role| {
                let column = role % 10;
                Board256Mask::from_words([
                    (0..4).fold(0_u64, |word, row| word | (1_u64 << (row * 10 + column))),
                    0,
                    0,
                    0,
                ])
            })
            .collect(),
        placement_role_pieces: vec![PieceKind::I; count],
        max_early_placements: Some(0),
        early_placement: EarlyPlacementPolicy::AnyStageTwoRole,
        hold_enabled: false,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
        preserve_b2b_by_stage: [false; 2],
        preserve_b2b_bag_mask: 0,
        initial_b2b: true,
        max_states: None,
    };
    for workers in [1, 8] {
        let response = AppContext::rust_cpu_product().run_with_execution_control(
            AppRequest::new(AppCommand::BoundaryRecovery(
                BoundaryRecoveryAppCommand::new(query.clone()),
            )),
            &ExecutionControl::default().with_parallelism(workers),
        );
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "N={workers}: {response:?}"
        );
        let ProductResultPayloadContent::BoundaryRecovery(payload) =
            response.public_result_payload().unwrap().content()
        else {
            panic!("missing boundary recovery product");
        };
        assert_eq!(payload.status, "normal");
        assert_eq!(payload.steps.len(), count);
        for (index, step) in payload.steps.iter().enumerate() {
            assert_eq!(usize::from(step.source_queue_index), index);
        }
        let roles: std::collections::BTreeSet<_> = payload
            .steps
            .iter()
            .map(|step| step.placement_role_index)
            .collect();
        assert_eq!(roles.len(), count);
        assert_eq!(roles.last().copied(), Some(259));
    }
}

#[test]
fn native_minimum_completion_keeps_the_canonical_source_and_probability_at_every_width() {
    use clearra_app::PcMinimalsIngressOrigin;
    use clearra_pc_graph::request::PcSolutionProbabilityPolicy;

    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let minimum_request = |workers| {
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
        .with_rule(srs_plus())
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(workers)
                .with_use_all_logical_processors(true),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(5))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_objective(ObjectivePolicy::minimum_cover());
        AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(query).with_result_projection(
                PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals),
            ),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcMinimals)
        .unwrap()
    };
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let mut cursor = context.start_cooperative_execution(minimum_request(1));
    let expected = (0..65536)
        .find_map(
            |_| match cursor.advance(256, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                CooperativeAppAdvance::Completed(response) => Some(response),
                other => panic!("unexpected cooperative minimum outcome: {other:?}"),
            },
        )
        .expect("small complete source finishes");
    assert_eq!(expected.status(), AppStatus::Success, "{expected:?}");
    let expected = expected
        .product_capability_result()
        .unwrap()
        .pc_minimum_cover_v2()
        .unwrap();
    assert!(expected.source_solution_count() > 1);
    assert!(!expected.selected_solution_probabilities().is_empty());
    for workers in [1, 2, 4, 8] {
        let caller = std::thread::current().id();
        let events = Arc::new(ComputeEvents::default());
        let response = context.run_with_execution_control(
            minimum_request(workers),
            &ExecutionControl::default()
                .with_parallelism(workers)
                .with_progress_sink(events.clone()),
        );
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "N={workers}: {response:?}"
        );
        let actual = response
            .product_capability_result()
            .unwrap()
            .pc_minimum_cover_v2()
            .unwrap();
        assert_eq!(
            actual.source_solution_count(),
            expected.source_solution_count()
        );
        assert_eq!(
            actual.selected_solution_count(),
            expected.selected_solution_count()
        );
        assert_eq!(
            actual.required_pattern_count(),
            expected.required_pattern_count()
        );
        assert_eq!(
            actual.normalized_solution_set_hash(),
            expected.normalized_solution_set_hash()
        );
        assert_eq!(
            actual.selected_solution_keys(),
            expected.selected_solution_keys()
        );
        assert_eq!(
            actual.selected_solution_probabilities(),
            expected.selected_solution_probabilities()
        );
        assert_eq!(
            actual.portfolio_alternatives().canonical_page(),
            expected.portfolio_alternatives().canonical_page()
        );
        assert_eq!(
            actual.portfolio_alternatives().set_identity_sha256(),
            expected.portfolio_alternatives().set_identity_sha256()
        );
        let observed = events.0.lock().unwrap();
        assert!(observed.iter().any(|(stage, _)| *stage == "validation"));
        assert!(
            observed.iter().all(|(_, thread)| *thread != caller),
            "N={workers}: caller performs no product computation"
        );
    }
}

#[test]
fn native_save_completion_preserves_bag_provenance_and_terminal_groups_at_every_width() {
    use clearra_app::PcSaveIngressOrigin;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for hold in [false, true] {
        let mut baseline = None;
        for workers in [1, 2, 4, 8] {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, 0xff3fc),
                PcQueueInput::pattern_expression(
                    QueuePatternExpression::parse("P7", usize::MAX).unwrap(),
                ),
                PieceWindow::new(1),
            )
            .with_rule(srs_plus())
            .with_exact_pieces(Some(1))
            .with_allow_hold(hold)
            .with_count_policy(PcCountPolicy::CountAll)
            .with_objective(ObjectivePolicy::all())
            .with_retained_trace_limit(1)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_allow_backend_fallback(false)
                    .with_workers(workers)
                    .with_use_all_logical_processors(true),
            );
            let request = AppRequest::new(AppCommand::Scenario(
                ScenarioAppCommand::new(query).with_result_projection(
                    PcResultProjection::SaveGroupsV2(PcSaveIngressOrigin::CanonicalPcSaves),
                ),
            ))
            .with_product_capability_contract(ProductCapabilityContract::PcSaves)
            .unwrap();
            let caller = std::thread::current().id();
            let events = Arc::new(ComputeEvents::default());
            let response = AppContext::rust_cpu_product().run_with_execution_control(
                request,
                &ExecutionControl::default()
                    .with_parallelism(workers)
                    .with_progress_sink(events.clone()),
            );
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "hold={hold}, N={workers}: {response:?}"
            );
            let payload = response.public_result_payload().unwrap();
            if let Some(expected) = &baseline {
                assert_eq!(payload, expected);
            } else {
                baseline = Some(payload.clone());
            }
            let product = response
                .product_capability_result()
                .unwrap()
                .pc_save_groups_v2()
                .unwrap();
            assert!(!product.groups().is_empty());
            assert_eq!(product.materialized_pattern_count(), 5040);
            assert!(events
                .0
                .lock()
                .unwrap()
                .iter()
                .all(|(_, thread)| *thread != caller));
        }
    }
}

fn register_native_product_host() {
    use clearra_app::{
        register_native_build_probability_host, NativeBuildProbabilityHostRegistration,
        SystemNativeBuildProbabilityAdmissionProvider,
    };
    use std::{
        path::PathBuf,
        sync::Once,
        time::{SystemTime, UNIX_EPOCH},
    };
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let uuid = format!(
            "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
            std::process::id(),
            (nonce >> 48) & 0xffff,
            (nonce >> 36) & 0xfff,
            (nonce >> 24) & 0xfff,
            nonce & 0xffffffffffff
        );
        register_native_build_probability_host(
            NativeBuildProbabilityHostRegistration::new(
                SystemNativeBuildProbabilityAdmissionProvider,
                repository.join("_local/artifacts/parallelism-20261125/app-journals"),
                uuid,
            )
            .unwrap(),
        )
        .expect("the same source-bound native provider used by CLI startup");
    });
}
