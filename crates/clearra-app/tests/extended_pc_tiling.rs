//! Small real product requests, never an empty 24L/sixty-piece enumeration.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppResponse, AppServices,
    AppStatus, CooperativeAppAdvance, PcAppCommand, PcResultProjection, PcTilingIngressOrigin,
    ProductCapabilityContract, ScenarioAppCommand,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    pc::pc_target::PcTarget, piece::piece_kind::PieceKind, solution::ExtendedTilingSolutionKey,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    RequestedSearchBackend,
};
use clearra_rules::profile::{
    builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
    rule_profile::RuleProfile,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

#[test]
fn full_height_failed_queue_direct_and_cooperative_products_share_the_same_source() {
    use clearra_app::{PcFailedQueueIngressOrigin, PercentAppCommand};
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7, 8, 12, 24] {
        let (source, _, _) = forced_request(height, srs_plus());
        let AppCommand::Scenario(source) = source.command() else {
            panic!("scenario fixture");
        };
        let request = AppRequest::new(AppCommand::Percent(
            PercentAppCommand::pc_failed_queue(
                source
                    .query()
                    .clone()
                    .with_objective(ObjectivePolicy::all())
                    .with_count_policy(PcCountPolicy::CountAll),
                PcFailedQueueIngressOrigin::CanonicalFailedQueue,
            )
            .with_failed_pattern_limit(1),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcFailedQueue)
        .unwrap();
        let direct = context.run(request.clone());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let direct_report = direct
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        assert_eq!(direct_report.success_pattern_count(), 1);
        assert_eq!(direct_report.failed_pattern_count(), 0);
        let mut execution = context.start_cooperative_execution(request);
        let control = ExecutionControl::default();
        let mut response = None;
        for _ in 0..4096 {
            match execution.advance(1, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(completed) => {
                    response = Some(completed);
                    break;
                }
                other => panic!("unexpected cooperative failed queue: {other:?}"),
            }
        }
        let response = response.expect("bounded cooperative request");
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
        let report = response
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        assert_eq!(report.problem_id(), direct_report.problem_id());
        assert_eq!(
            report.success_pattern_count(),
            direct_report.success_pattern_count()
        );
        assert_eq!(
            report.failed_probability_bits(),
            direct_report.failed_probability_bits()
        );
        assert_eq!(
            report.pattern_universe_id(),
            direct_report.pattern_universe_id()
        );
        assert_eq!(report.examples(), direct_report.examples());
    }
}

#[test]
fn full_height_distributed_failed_queue_requires_the_original_problem_owner() {
    use clearra_app::{
        DistributedSearchPreparation, PcFailedQueueIngressOrigin, PercentAppCommand,
    };
    use clearra_core_executor::{WasmPcFailedQueueAdvance, WasmPcFailedQueueSession};

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7, 8, 12, 24] {
        let (source, _, _) = forced_request(height, srs_plus());
        let AppCommand::Scenario(source) = source.command() else {
            panic!("scenario fixture");
        };
        let request = AppRequest::new(AppCommand::Percent(
            PercentAppCommand::pc_failed_queue(
                source
                    .query()
                    .clone()
                    .with_objective(ObjectivePolicy::all())
                    .with_count_policy(PcCountPolicy::CountAll),
                PcFailedQueueIngressOrigin::CanonicalFailedQueue,
            )
            .with_failed_pattern_limit(1),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcFailedQueue)
        .unwrap();
        let prepare = || match context.prepare_distributed_search(request.clone()) {
            DistributedSearchPreparation::Search(prepared) => prepared,
            DistributedSearchPreparation::Ready(response) => panic!("{response:?}"),
        };
        let original = prepare();
        let foreign = prepare();
        let cancelled = prepare();
        let mut producer = WasmPcFailedQueueSession::new(original.problem_arc()).unwrap();
        let control = ExecutionControl::default();
        let mut completed = None;
        for _ in 0..4096 {
            match producer.advance(1, &control).unwrap() {
                WasmPcFailedQueueAdvance::Pending => {}
                WasmPcFailedQueueAdvance::Completed(result, evidence) => {
                    completed = Some((result, evidence));
                    break;
                }
                WasmPcFailedQueueAdvance::Cancelled => panic!("unexpected cancellation"),
            }
        }
        let (result, evidence) = completed.expect("bounded request-owned failed-queue evidence");
        // Completion transfers the immutable proof, not the producer's live
        // execution lease. Match the WASM coordinator's drop-before-App seam;
        // retaining this producer would deny the subsequent direct request.
        drop(producer);
        let finite = match context.prepare_distributed_search(request.clone().with_resource_budget(
            clearra_host_contract::ResourceBudget::new(1, None, Some(256)),
        )) {
            DistributedSearchPreparation::Search(prepared) => prepared,
            DistributedSearchPreparation::Ready(response) => panic!("{response:?}"),
        };
        let finite = finite.complete_failed_queue(result.clone(), evidence.clone(), &control);
        assert_eq!(finite.status(), AppStatus::Unsupported);
        assert!(finite.product_capability_result().is_none());
        assert!(finite.result().is_none());
        assert!(finite.public_result_payload().is_none());
        assert!(finite
            .error()
            .unwrap()
            .message()
            .contains("distributed_pc_failed_queue_finite_completion_unavailable"));
        let mismatched = foreign.complete_failed_queue(result.clone(), evidence.clone(), &control);
        // A foreign authority is refused at this App completion ingress, using
        // the established RuntimeUnavailable mapping rather than a fabricated
        // failed-queue product or a completed complement proof.
        assert_eq!(mismatched.status(), AppStatus::Unsupported);
        assert!(mismatched.product_capability_result().is_none());
        assert!(mismatched.result().is_none());
        assert!(mismatched.public_result_payload().is_none());
        assert!(mismatched
            .error()
            .unwrap()
            .message()
            .contains("distributed_pc_failed_queue_authority_mismatch"));

        let stop = ExecutionControl::default();
        stop.cancellation.handle().cancel();
        let stopped = cancelled.complete_failed_queue(result.clone(), evidence.clone(), &stop);
        assert_ne!(stopped.status(), AppStatus::Success);
        assert!(stopped.product_capability_result().is_none());
        assert!(stopped.result().is_none());
        assert!(stopped.public_result_payload().is_none());

        let accepted = original.complete_failed_queue(result, evidence, &control);
        assert_eq!(accepted.status(), AppStatus::Success, "{accepted:?}");
        let actual = accepted
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        let direct = context.run(request);
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let expected = direct
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        assert_eq!(
            actual.success_pattern_count(),
            expected.success_pattern_count()
        );
        assert_eq!(
            actual.failed_pattern_count(),
            expected.failed_pattern_count()
        );
        assert_eq!(actual.pattern_universe_id(), expected.pattern_universe_id());
        assert_eq!(actual.examples(), expected.examples());
    }
}

#[test]
fn full_height_tiling_opening_keeps_a_distinct_typed_ingress_without_enumeration() {
    for lines in (8..=24).step_by(2) {
        let query = OpeningPcSearchQuery::new(PcTarget::new(lines).unwrap())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I;
                usize::from(lines) * 10 / 4
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::tiling())
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_execution_policy(
                PcExecutionPolicy::default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(1),
            );
        let command = PcAppCommand::new(query).with_result_projection(
            PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling),
        );
        assert_eq!(command.validate_result_projection(), Ok(()));
        assert!(AppRequest::new(AppCommand::Pc(command))
            .with_product_capability_contract(ProductCapabilityContract::PcTiling)
            .is_ok());
    }
}

#[test]
fn tiling_option_combinations_preserve_long_windows_and_occupied_hold_without_enumeration() {
    use clearra_pc_graph::request::SupplyWindowSize;
    use clearra_problem::ProblemCompiler;

    // These are input/compiler checks, not an empty 24L enumeration. The
    // fixed source has one pattern at every size; the standard-bag source is
    // checked at the product boundary without materializing its population.
    for (height, initial, pieces) in [
        (6, [3 | (3 << 10), 0, 0, 0], 14),
        (6, [0; 4], 15),
        (8, [0; 4], 20),
        (24, [0; 4], 60),
    ] {
        for (hold_enabled, held) in [(false, None), (true, None), (true, Some(PieceKind::O))] {
            let required_draws = pieces - usize::from(held.is_some());
            for draws in required_draws..=required_draws + usize::from(hold_enabled) {
                for standard_bag in [false, true] {
                    let queue = if standard_bag {
                        PcQueueInput::standard_7_bag()
                    } else {
                        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; draws]))
                    };
                    let query = PcScenarioQuery::new(
                        PcScenarioBoard::standard_10_from_words(height, initial).unwrap(),
                        queue,
                        PieceWindow::new(pieces),
                    )
                    .with_exact_pieces(Some(pieces))
                    .with_allow_hold(hold_enabled)
                    .with_hold_piece(held)
                    .with_supply_window_size(SupplyWindowSize::new(draws))
                    .with_objective(ObjectivePolicy::tiling())
                    .with_count_policy(PcCountPolicy::CountUnique);
                    let command = ScenarioAppCommand::new(query.clone()).with_result_projection(
                        PcResultProjection::TilingFamilyV1(
                            PcTilingIngressOrigin::CanonicalPcTiling,
                        ),
                    );
                    assert_eq!(command.validate_result_projection(), Ok(()));
                    assert!(AppRequest::new(AppCommand::Scenario(command))
                        .with_product_capability_contract(ProductCapabilityContract::PcTiling)
                        .is_ok());
                    if !standard_bag {
                        let compiled = ProblemCompiler::compile_scenario_pc_tiling(&query).unwrap();
                        assert_eq!(compiled.piece_window().max_pieces(), pieces);
                        assert_eq!(compiled.exact_pieces(), Some(pieces));
                        assert_eq!(
                            compiled.core_query().supply_window_size(),
                            query.supply_window_size()
                        );
                        assert_eq!(compiled.core_query().hold_state(), query.hold_state());
                        assert_eq!(
                            compiled
                                .piece_source()
                                .materialized_universe()
                                .unwrap()
                                .pattern_count(),
                            1
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn tiling_option_combinations_retain_the_shared_compiler_supply_bounds() {
    use clearra_pc_graph::request::SupplyWindowSize;
    use clearra_problem::{ProblemCompileError, ProblemCompiler};

    let base = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 15])),
        PieceWindow::new(15),
    )
    .with_exact_pieces(Some(15))
    .with_allow_hold(false)
    .with_objective(ObjectivePolicy::tiling())
    .with_count_policy(PcCountPolicy::CountUnique);
    for (draws, expected) in [
        (
            14,
            ProblemCompileError::SupplyWindowTooShort {
                source_pieces: 14,
                required_source_pieces: 15,
            },
        ),
        (
            16,
            ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                source_pieces: 16,
                queue_pieces: 15,
            },
        ),
    ] {
        let query = base
            .clone()
            .with_supply_window_size(SupplyWindowSize::new(draws));
        assert_eq!(
            ProblemCompiler::compile_scenario_pc_tiling(&query),
            Err(expected)
        );
    }
}

#[test]
fn empty_extended_app_opening_reports_finite_memory_failure_not_unsupported_or_success() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for lines in [8, 24] {
        for tiling in [false, true] {
            let query = OpeningPcSearchQuery::new(PcTarget::new(lines).unwrap())
                .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                    PieceKind::I;
                    usize::from(lines) * 10 / 4
                ])))
                .with_hold_policy(PcHoldPolicy::Disabled)
                .with_count_policy(PcCountPolicy::CountUnique)
                .with_objective(if tiling {
                    ObjectivePolicy::tiling()
                } else {
                    ObjectivePolicy::all()
                })
                .with_execution_policy(
                    PcExecutionPolicy::default()
                        .with_requested_backend(RequestedSearchBackend::Cpu)
                        .with_workers(1)
                        .with_max_memory_mib(Some(1)),
                );
            let mut command = PcAppCommand::new(query);
            if tiling {
                command = command.with_result_projection(PcResultProjection::TilingFamilyV1(
                    PcTilingIngressOrigin::CanonicalPcTiling,
                ));
            }
            let mut request = AppRequest::new(AppCommand::Pc(command));
            if tiling {
                request = request
                    .with_product_capability_contract(ProductCapabilityContract::PcTiling)
                    .unwrap();
            }
            let response = context.run(request);
            assert_eq!(
                response.status(),
                AppStatus::ExecutionFailed,
                "{response:?}"
            );
            assert!(response
                .resource_report()
                .execution_availability()
                .reason()
                .is_some());
            assert!(response.render_model().is_none());
            assert!(response.product_capability_result().is_none());
        }
    }
}

fn forced_request(height: u8, rule: RuleProfile) -> (AppRequest, Board256Mask, usize) {
    let starts = if height == 7 {
        vec![0, 3]
    } else {
        (0..u16::from(height)).step_by(4).collect()
    };
    let count = starts.len();
    let mut holes = Board256Mask::EMPTY;
    for (column, start) in starts.into_iter().enumerate() {
        for row in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
        }
    }
    let initial = Board256Mask::all_cells(u16::from(height) * 10)
        .unwrap()
        .without(holes);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; count])),
        PieceWindow::new(count),
    )
    .with_rule(rule)
    .with_exact_pieces(Some(count))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::tiling())
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    );
    let command = ScenarioAppCommand::new(query).with_result_projection(
        PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling),
    );
    (
        AppRequest::new(AppCommand::Scenario(command))
            .with_product_capability_contract(ProductCapabilityContract::PcTiling)
            .unwrap(),
        initial,
        count,
    )
}

fn assert_family(
    response: &AppResponse,
    height: u8,
    initial: Board256Mask,
    pieces: usize,
) -> Vec<String> {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let result = response
        .product_capability_result()
        .unwrap()
        .pc_tiling_family_v1()
        .unwrap();
    assert!(result.completeness().family_complete());
    assert_eq!(result.normalized_solution_count(), 1);
    let keys = result.page_keys(0, 100).unwrap();
    assert_eq!(keys, result.initial_page_keys());
    assert!(result.page_keys(1, usize::MAX).unwrap().is_empty());
    let identity = ExtendedTilingSolutionKey::parse_canonical(&keys[0]).unwrap();
    assert_eq!(identity.height(), height);
    assert_eq!(identity.initial_board(), initial);
    assert_eq!(identity.placement_count(), pieces);
    let full = identity
        .placements()
        .fold(initial, |board, placement| board.union(placement.cells()));
    assert_eq!(
        full,
        Board256Mask::all_cells(u16::from(height) * 10).unwrap()
    );
    keys
}

#[test]
fn full_height_tiling_uses_the_real_app_product_in_all_profiles() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            let (request, initial, pieces) = forced_request(height, rule);
            let response = context.run(request);
            assert_family(&response, height, initial, pieces);
            let core = response.render_model().unwrap().core_result().unwrap();
            assert!(core.pc_tiling_family_publication_contract_is_valid());
            assert_eq!(core.bool_field("buildup_executed"), Some(false));
            assert_eq!(core.bool_field("probability_calculated"), Some(false));
            assert!(core.tiling_solution_page_store().unwrap().is_extended());
            assert!(core
                .tiling_solution_page_store()
                .unwrap()
                .page_identities(0, 1)
                .is_err());
        }
    }
}

#[test]
fn full_height_fixed_queue_scenario_replay_reuses_the_same_rust_family() {
    use clearra_app::{AppRenderModel, ContinueAppCommand};
    use clearra_pc_graph::request::{PcContinuationToken, PcContinuationTokenCodec};
    use clearra_supply::QueueObservationPolicy;

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for (height, observation) in [
        (7, QueueObservationPolicy::VisibleSeven),
        (24, QueueObservationPolicy::FullQueueOracle),
    ] {
        let (base, initial, _) = ordinary_request(height, false);
        let AppCommand::Scenario(scenario) = base.command() else {
            panic!("scenario fixture");
        };
        let query = scenario
            .query()
            .clone()
            .with_queue_observation_policy(observation);
        let request = AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(query)));
        let direct = context.run(request.clone());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let core = direct.render_model().unwrap().core_result().unwrap();
        assert_eq!(core.field("actual_backend"), Some("wasm-cpu-pc-extended"));
        assert_eq!(core.field("unique_solution_count"), Some("1"));
        assert_eq!(core.bool_field("count_complete"), Some(true));
        assert_eq!(core.bool_field("resource_truncated"), Some(false));
        assert_eq!(core.field("scenario_replay_token_version"), Some("sr3"));
        let token = core.field("scenario_replay_token").unwrap();

        let mut execution = context.start_cooperative_execution(request.clone());
        let control = ExecutionControl::default();
        let cooperative = (0..4096)
            .find_map(|_| match execution.advance(1, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                CooperativeAppAdvance::Completed(response) => Some(response),
                other => panic!("unexpected cooperative Scenario replay result: {other:?}"),
            })
            .expect("bounded full-height Scenario completed");
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
        let cooperative = cooperative.render_model().unwrap().core_result().unwrap();
        for key in [
            "scenario_replay_token_version",
            "scenario_replay_token",
            "replay_hint",
            "continuation_token",
            "normalized_solution_set_hash",
            "queue_knowledge",
            "observation_policy_evaluated",
            "covered_pattern_count",
            "coverage_pattern_count",
            "count_complete",
            "resource_truncated",
        ] {
            assert_eq!(cooperative.field(key), core.field(key), "{height}L: {key}");
        }
        let PcContinuationToken::ScenarioReplay(decoded) =
            PcContinuationTokenCodec::parse(token).unwrap()
        else {
            panic!("expected input replay, not a continuation");
        };
        assert_eq!(decoded.initial_board().occupied_words(), initial.words());
        assert_eq!(decoded.queue_observation_policy(), observation);

        let replay = context.run(AppRequest::new(AppCommand::Continue(
            ContinueAppCommand::new(Some(token.to_owned())),
        )));
        assert_eq!(replay.status(), AppStatus::Success, "{replay:?}");
        let Some(AppRenderModel::Continue(message)) = replay.render_model() else {
            panic!("expected scenario replay result");
        };
        let field = |key: &str| {
            message
                .fields()
                .iter()
                .find(|field| field.key() == key)
                .unwrap()
                .value()
                .as_text()
        };
        assert_eq!(field("continuation_kind"), "scenario-replay");
        if observation.requires_observation_policy() {
            assert_eq!(core.bool_field("observation_policy_evaluated"), Some(true));
            assert_eq!(field("queue_knowledge"), observation.keyword());
            assert_eq!(field("observation_policy_evaluated"), "true");
        }
        for key in [
            "actual_backend",
            "unique_solution_count",
            "normalized_solution_set_hash",
            "coverage_pattern_count",
            "covered_pattern_count",
            "count_complete",
        ] {
            assert_eq!(field(key), core.field(key).unwrap(), "{height}L: {key}");
        }
        if height == 7 {
            assert_cancelled_at_postprocess_publication(&context, request);
        }
    }
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn full_height_ordinary_scenario_uses_bounded_native_worker_waves() {
    use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;

    if CpuCapacity::current().hard_limit() < 2 {
        return;
    }
    let context = AppContext::rust_cpu_product();
    for height in [7, 24] {
        let (base, _, _) = ordinary_request(height, false);
        let AppCommand::Scenario(scenario) = base.command() else {
            panic!("scenario fixture");
        };
        let source = scenario.query();
        let query = source.clone().with_execution_policy(
            source
                .execution_policy()
                .clone()
                .with_workers(2)
                .with_use_all_logical_processors(true),
        );
        let request = AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(query)));
        let direct = context.run(request.clone());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        let expected = direct.render_model().unwrap().core_result().unwrap();

        let mut session = context.start_cooperative_execution(request);
        let control = ExecutionControl::default();
        assert!(matches!(
            session.advance(1, &control),
            CooperativeAppAdvance::Pending
        ));
        let response = (0..4096)
            .find_map(|_| match session.advance(64, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                CooperativeAppAdvance::Completed(response) => Some(response),
                other => panic!("unexpected cooperative result: {other:?}"),
            })
            .expect("bounded fixed queue must settle");
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
        let actual = response.render_model().unwrap().core_result().unwrap();
        assert_eq!(
            actual.normalized_solution_keys(),
            expected.normalized_solution_keys()
        );
        assert_eq!(actual.path_steps(), expected.path_steps());
        assert_eq!(
            actual.coverage_pattern_words(),
            expected.coverage_pattern_words()
        );
        for field in [
            "workers_used",
            "build_variant_count",
            "scenario_replay_token",
        ] {
            assert_eq!(
                actual.field(field),
                expected.field(field),
                "{height}L {field}"
            );
        }
    }
}

fn assert_cancelled_at_postprocess_publication(context: &AppContext, request: AppRequest) {
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress, ProgressSink,
    };
    use std::sync::Arc;

    struct CancelAtPublication(ExecutionCancellationHandle);
    impl ProgressSink for CancelAtPublication {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "postprocess" && progress.completed == 1 {
                self.0.cancel();
            }
        }
    }

    let mut execution = context.start_cooperative_execution(request);
    let cancellation = ExecutionCancellationToken::new();
    let handle = cancellation.handle();
    let control = ExecutionControl::new(cancellation)
        .with_progress_sink(Arc::new(CancelAtPublication(handle.clone())));
    let cancelled = (0..4096).any(|_| match execution.advance(1, &control) {
        CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => false,
        CooperativeAppAdvance::Cancelled => true,
        other => panic!("cancelled replay must not publish a result or token: {other:?}"),
    });
    assert!(cancelled && handle.is_cancelled());
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn full_height_sr3_local_continue_is_cooperative_on_one_worker() {
    use clearra_app::{AppRenderModel, ContinueAppCommand};
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
    use clearra_pc_graph::request::PcContinuationTokenCodec;
    use clearra_supply::QueueObservationPolicy;
    use std::{env, process::Command};

    // Runtime CPU capacity reads an existing outer process setting. Set it
    // only in this isolated test subprocess, never in the parallel test runner.
    const TEST: &str = "full_height_sr3_local_continue_is_cooperative_on_one_worker";
    const CHILD: &str = "CLEARRA_TEST_SR3_COOPERATIVE_CHILD";
    if env::var_os(CHILD).is_none() {
        let output = Command::new(env::current_exe().expect("current test executable"))
            .args(["--exact", TEST])
            .env(CHILD, "1")
            .env("CLEARRA_EXPECTED_VCPUS", "1")
            .output()
            .expect("run isolated one-worker sr3 regression");
        assert!(
            output.status.success(),
            "isolated sr3 regression failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_eq!(PcExecutionPolicy::mvp_default().workers(), 1);

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for (height, observation) in [
        (7, QueueObservationPolicy::VisibleSeven),
        (24, QueueObservationPolicy::FullQueueOracle),
    ] {
        let (fixture, _, _) = ordinary_request(height, false);
        let AppCommand::Scenario(scenario) = fixture.command() else {
            panic!("scenario fixture");
        };
        let query = scenario
            .query()
            .clone()
            .with_queue_observation_policy(observation);
        let token = PcContinuationTokenCodec::encode_scenario_replay(&query).unwrap();
        assert!(token.starts_with("sr3:"));
        let request = AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(token))));
        let direct = context.run(request.clone());
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");

        let control = ExecutionControl::default();
        let mut execution = context.start_cooperative_execution(request.clone());
        assert!(matches!(
            execution.advance(1, &control),
            CooperativeAppAdvance::Pending
        ));
        let response = (0..4096)
            .find_map(|_| match execution.advance(1, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                CooperativeAppAdvance::Completed(response) => Some(response),
                other => panic!("unexpected cooperative sr3 response: {other:?}"),
            })
            .expect("bounded sr3 replay completed");
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
        let (Some(AppRenderModel::Continue(expected)), Some(AppRenderModel::Continue(actual))) =
            (direct.render_model(), response.render_model())
        else {
            panic!("both local drivers must use Continue's existing result");
        };
        let value = |fields: &[clearra_output::model::RenderField], key: &str| {
            fields
                .iter()
                .find(|field| field.key() == key)
                .map(|field| field.value().as_text())
        };
        for key in [
            "status",
            "continuation_kind",
            "input_continuation_token",
            "interactive_prompt",
            "actual_backend",
            "normalized_solution_set_hash",
            "coverage_pattern_count",
            "covered_pattern_count",
            "count_complete",
        ] {
            let expected = value(expected.fields(), key)
                .unwrap_or_else(|| panic!("{height}L direct Continue missing {key}"));
            let actual = value(actual.fields(), key)
                .unwrap_or_else(|| panic!("{height}L cooperative Continue missing {key}"));
            assert_eq!(expected, actual, "{height}L: {key}");
        }
        // The extended reducer emits policy evidence only when it actually
        // evaluates an observation policy. The oracle has no such owner.
        for fields in [expected.fields(), actual.fields()] {
            if observation.requires_observation_policy() {
                assert_eq!(
                    value(fields, "queue_knowledge").as_deref(),
                    Some(observation.keyword()),
                    "{height}L must preserve the requested queue observation"
                );
                assert_eq!(
                    value(fields, "observation_policy_evaluated").as_deref(),
                    Some("true"),
                    "{height}L must retain evidence of evaluation"
                );
            } else {
                assert_eq!(value(fields, "queue_knowledge"), None, "{height}L oracle");
                assert_eq!(
                    value(fields, "observation_policy_evaluated"),
                    None,
                    "{height}L oracle must not claim policy evaluation"
                );
            }
        }

        let mut early = context.start_cooperative_execution(request.clone());
        let cancellation = ExecutionCancellationToken::new();
        let early_control = ExecutionControl::new(cancellation.clone());
        assert!(matches!(
            early.advance(1, &early_control),
            CooperativeAppAdvance::Pending
        ));
        cancellation.handle().cancel();
        assert!(matches!(
            early.advance(1, &early_control),
            CooperativeAppAdvance::Cancelled
        ));

        assert_cancelled_at_postprocess_publication(&context, request);
    }

    let malformed = AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(
        "sr3:invalid".to_owned(),
    ))));
    let direct = context.run(malformed.clone());
    let mut execution = context.start_cooperative_execution(malformed.clone());
    let CooperativeAppAdvance::Completed(response) =
        execution.advance(1, &ExecutionControl::default())
    else {
        panic!("invalid sr3 must retain the original immediate diagnostic");
    };
    assert_eq!(direct.status(), response.status());
    assert_eq!(
        direct.error().unwrap().code(),
        response.error().unwrap().code()
    );

    let mut cancelled = context.start_cooperative_execution(malformed);
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert!(matches!(
        cancelled.advance(1, &ExecutionControl::new(token)),
        CooperativeAppAdvance::Cancelled
    ));
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn full_height_sr3_uses_native_worker_waves_without_publishing_on_a_zero_tick() {
    use clearra_app::{AppRenderModel, ContinueAppCommand};
    use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;
    use clearra_pc_graph::request::PcContinuationTokenCodec;
    use clearra_supply::QueueObservationPolicy;
    use std::{env, process::Command};

    const TEST: &str = "full_height_sr3_uses_native_worker_waves_without_publishing_on_a_zero_tick";
    const CHILD: &str = "CLEARRA_TEST_SR3_NATIVE_WAVES_CHILD";
    if env::var_os(CHILD).is_none() {
        if CpuCapacity::current().hard_limit() < 3 {
            eprintln!("three logical processors are required to check two native Continue workers");
            return;
        }
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .env("CLEARRA_EXPECTED_VCPUS", "3")
            .output()
            .expect("isolated native Continue regression");
        assert!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line == "clearra_native_continue=complete"),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_eq!(PcExecutionPolicy::mvp_default().workers(), 2);

    // The split fixture has multiple candidate ordinals. Neither a
    // no-Geometry handoff nor a single degenerate verification is a worker
    // wave proof.
    let mut holes = Board256Mask::EMPTY;
    for row in 0..7_u16 {
        let start = if row < 3 { 0 } else { 6 };
        for col in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + col).unwrap());
        }
    }
    let initial = Board256Mask::all_cells(70).unwrap().without(holes);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(7, initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 7])),
        PieceWindow::new(7),
    )
    .with_exact_pieces(Some(7))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll);
    let make_request = |query: &PcScenarioQuery| {
        let token = PcContinuationTokenCodec::encode_scenario_replay(query).unwrap();
        assert!(token.starts_with("sr3:"));
        AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(token))))
    };
    let request = make_request(&query);
    let context = AppContext::rust_cpu_product();
    let direct = context.run(request.clone());
    assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
    let mut execution = context.start_cooperative_execution(request.clone());
    let control = ExecutionControl::default();
    assert!(matches!(
        execution.advance(0, &control),
        CooperativeAppAdvance::Pending
    ));
    assert!(matches!(
        execution.advance(1, &control),
        CooperativeAppAdvance::Pending
    ));
    let actual = (0..16384)
        .find_map(|_| match execution.advance(4, &control) {
            CooperativeAppAdvance::Pending => None,
            CooperativeAppAdvance::Progress => {
                // The terminal result is still carrying its original lease.
                assert!(matches!(
                    execution.advance(0, &control),
                    CooperativeAppAdvance::Pending
                ));
                match execution.advance(1, &control) {
                    CooperativeAppAdvance::Completed(response) => Some(response),
                    other => panic!("native publication requires one work unit: {other:?}"),
                }
            }
            other => panic!("expected terminal worker-wave handoff: {other:?}"),
        })
        .expect("native Continue completed");
    assert_eq!(actual.status(), AppStatus::Success, "{actual:?}");
    let (Some(AppRenderModel::Continue(expected)), Some(AppRenderModel::Continue(actual))) =
        (direct.render_model(), actual.render_model())
    else {
        panic!("both paths must use the existing Continue message");
    };
    let field = |fields: &[clearra_output::model::RenderField], key: &str| {
        fields
            .iter()
            .find(|field| field.key() == key)
            .unwrap()
            .value()
            .as_text()
    };
    for key in [
        "input_continuation_token",
        "count_complete",
        "coverage_pattern_count",
        "covered_pattern_count",
        "build_variant_count",
        "normalized_solution_set_hash",
        "workers_used",
        "cpu_parallel_execution",
    ] {
        assert_eq!(
            field(actual.fields(), key),
            field(expected.fields(), key),
            "{key}"
        );
    }
    assert_eq!(field(actual.fields(), "workers_used"), "2");
    assert_eq!(field(actual.fields(), "cpu_parallel_execution"), "true");
    let host = context.run_preferring_native_cooperative_continue(request.clone(), &control);
    assert_eq!(host.status(), AppStatus::Success, "{host:?}");
    let Some(AppRenderModel::Continue(host)) = host.render_model() else {
        panic!("native CLI driver must use the existing Continue message");
    };
    assert_eq!(
        field(host.fields(), "normalized_solution_set_hash"),
        field(expected.fields(), "normalized_solution_set_hash")
    );
    assert_eq!(field(host.fields(), "workers_used"), "2");
    assert_cancelled_at_postprocess_publication(&context, request);

    // Different evidence level: no bounded substitution on the two-worker
    // host, so the existing synchronous result is still used.
    let observed =
        make_request(&query.with_queue_observation_policy(QueueObservationPolicy::VisibleSeven));
    let host_fallback =
        context.run_preferring_native_cooperative_continue(observed.clone(), &control);
    let mut fallback = context.start_cooperative_execution(observed);
    let CooperativeAppAdvance::Completed(response) = fallback.advance(1, &control) else {
        panic!("observation policy must retain its existing immediate route");
    };
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let (Some(AppRenderModel::Continue(host_fallback)), Some(AppRenderModel::Continue(response))) =
        (host_fallback.render_model(), response.render_model())
    else {
        panic!("both fallback entrypoints must retain the Continue response");
    };
    for key in [
        "input_continuation_token",
        "normalized_solution_set_hash",
        "queue_knowledge",
    ] {
        assert_eq!(
            field(host_fallback.fields(), key),
            field(response.fields(), key),
            "{key}"
        );
    }
    println!("clearra_native_continue=complete");
}

#[test]
fn full_height_scenario_continuation_restarts_at_an_accepted_path_checkpoint() {
    use clearra_app::{AppRenderModel, ContinueAppCommand, PcPathIngressOrigin};
    use clearra_pc_graph::request::{PcContinuationToken, PcContinuationTokenCodec};

    #[cfg(not(target_family = "wasm"))]
    {
        use std::{env, process::Command};

        const CHILD: &str = "CLEARRA_TEST_SC3_COOPERATIVE_CHILD";
        if env::var_os(CHILD).is_none() {
            let output = Command::new(env::current_exe().expect("current test executable"))
                .args([
                    "--exact",
                    "full_height_scenario_continuation_restarts_at_an_accepted_path_checkpoint",
                ])
                .env(CHILD, "1")
                .env("CLEARRA_EXPECTED_VCPUS", "1")
                .output()
                .expect("run isolated one-worker sc3 regression");
            assert!(
                output.status.success(),
                "isolated sc3 regression failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert_eq!(PcExecutionPolicy::mvp_default().workers(), 1);
    }

    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let (base, initial, pieces) = forced_request(24, srs_plus());
    let AppCommand::Scenario(source) = base.command() else {
        panic!("scenario fixture");
    };
    // The geometry has room only for vertical Is. Store the first O to
    // force the first successful lock to consume the I from the hold slot.
    let mut queue = vec![PieceKind::I; pieces];
    queue[0] = PieceKind::O;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(24, initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(queue.clone())),
        PieceWindow::new(pieces),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(pieces))
    .with_hold_piece(Some(PieceKind::I))
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(source.query().execution_policy().clone());
    let request = AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(PcResultProjection::PathFamilyV2(
            PcPathIngressOrigin::CanonicalPcPath,
        )),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcPath)
    .unwrap();
    let path = context.run(request);
    assert_eq!(path.status(), AppStatus::Success, "{path:?}");
    let report = path
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    assert!(report.completeness().complete());
    let witness = report.canonical_witness().expect("exact reference path");
    let checkpoint = &witness.steps()[0];
    assert_eq!(checkpoint.board_before_mask().words(), initial.words());
    assert_eq!(checkpoint.cleared_lines(), 4);
    assert_eq!(checkpoint.active_piece(), PieceKind::I);
    assert_eq!(checkpoint.input_hold_piece(), Some(PieceKind::I));
    assert_eq!(checkpoint.output_hold_piece(), Some(PieceKind::O));
    assert_eq!(checkpoint.output_cursor(), 1);

    let after = checkpoint.board_after_line_clear_mask().words();
    assert_ne!(after[3], 0, "checkpoint still needs the fourth word");
    let remaining_pieces = witness.steps().len() - 1;
    let suffix = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(20, after).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(
            queue[checkpoint.output_cursor()..].to_vec(),
        )),
        PieceWindow::new(remaining_pieces),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(remaining_pieces))
    .with_hold_piece(checkpoint.output_hold_piece())
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(source.query().execution_policy().clone());
    let token = PcContinuationTokenCodec::encode_scenario_continuation(&suffix).unwrap();
    assert!(token.starts_with("sc3:"));
    let PcContinuationToken::Scenario(decoded) = PcContinuationTokenCodec::parse(&token).unwrap()
    else {
        panic!("checkpoint must stay distinct from an input replay");
    };
    assert_eq!(decoded.initial_board().occupied_words(), after);
    assert_eq!(decoded.hold_state().piece(), Some(PieceKind::O));
    assert_eq!(decoded.remaining_queue(), suffix.remaining_queue());

    let direct = context.run(AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(suffix),
    )));
    assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
    let expected = direct.render_model().unwrap().core_result().unwrap();
    assert_eq!(expected.normalized_solution_keys().len(), 1);
    assert_eq!(expected.field("coverage_probability"), Some("1"));
    let identity =
        ExtendedTilingSolutionKey::parse_canonical(&expected.normalized_solution_keys()[0])
            .unwrap();
    assert_eq!(identity.initial_board().words(), after);
    assert_eq!(identity.height(), 20);
    let request = AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(token))));
    let continued = context.run(request.clone());
    let cooperative = {
        let control = ExecutionControl::default();
        let mut execution = context.start_cooperative_execution(request.clone());
        assert!(matches!(
            execution.advance(1, &control),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress
        ));
        (0..4096)
            .find_map(|_| match execution.advance(1, &control) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                CooperativeAppAdvance::Completed(response) => Some(response),
                other => panic!("unexpected cooperative checkpoint result: {other:?}"),
            })
            .expect("bounded cooperative checkpoint completed")
    };
    assert_eq!(continued.status(), AppStatus::Success, "{continued:?}");
    assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
    for response in [&continued, &cooperative] {
        let Some(AppRenderModel::Continue(message)) = response.render_model() else {
            panic!("expected checkpoint result");
        };
        let field = |key: &str| {
            message
                .fields()
                .iter()
                .find(|field| field.key() == key)
                .unwrap()
                .value()
                .as_text()
        };
        assert_eq!(field("continuation_kind"), "scenario");
        assert_eq!(field("actual_backend"), "wasm-cpu-pc-extended");
        for key in [
            "normalized_solution_set_hash",
            "unique_solution_count",
            "coverage_pattern_count",
            "covered_pattern_count",
            "coverage_probability",
            "build_variant_count",
            "count_complete",
        ] {
            assert_eq!(field(key), expected.field(key).unwrap(), "{key}");
        }
    }
    assert_cancelled_at_postprocess_publication(&context, request.clone());
    #[cfg(feature = "native-c-core")]
    {
        let native = AppContext::native_c_reference().run(request);
        assert_ne!(native.status(), AppStatus::Success, "{native:?}");
        assert!(native.render_model().is_none());
    }
}

#[test]
fn full_height_replay_direct_cooperative_and_lazy_pages_preserve_all_words() {
    use clearra_app::{PcPathIngressOrigin, PcReplayPageStore};
    use std::collections::BTreeSet;
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7, 8, 12, 24] {
            let (base, initial, pieces) = forced_request(height, rule);
            let AppCommand::Scenario(source) = base.command() else {
                panic!("scenario fixture");
            };
            let command = ScenarioAppCommand::new(
                source
                    .query()
                    .clone()
                    .with_objective(ObjectivePolicy::all())
                    .with_count_policy(PcCountPolicy::CountAll),
            )
            .with_result_projection(PcResultProjection::PathFamilyV2(
                PcPathIngressOrigin::CanonicalPcPath,
            ));
            let request = AppRequest::new(AppCommand::Scenario(command))
                .with_product_capability_contract(ProductCapabilityContract::PcPath)
                .unwrap();
            let direct = context.run(request.clone());
            assert_eq!(direct.status(), AppStatus::Success, "{height} {direct:?}");
            let report = direct
                .product_capability_result()
                .unwrap()
                .pc_path_family_v2()
                .unwrap();
            assert!(report.completeness().complete());
            let witness = report.canonical_witness().expect("first canonical member");
            assert_eq!(witness.steps().len(), pieces);
            assert_eq!(
                witness.steps()[0].board_before_mask().words(),
                initial.words()
            );
            assert!(witness
                .steps()
                .last()
                .unwrap()
                .board_after_line_clear_mask()
                .is_empty());
            assert!(witness.normalized_trace_key().starts_with("trk2:"));
            let core = direct.render_model().unwrap().core_result().unwrap();
            assert!(
                core.pc_full_height_replay_evidence().is_none(),
                "private source must be consumed"
            );
            assert!(core.exact_scoring_execution_batches().is_empty());
            let page_source = report.page_source().unwrap();
            assert_eq!(page_source.geometry_count(), 1);
            let mut store = PcReplayPageStore::new(page_source.clone());
            let control = ExecutionControl::default();
            let first = store.page(1, 1, &control).unwrap();
            assert_eq!(first.witnesses.len(), report.witnesses().len());
            assert_eq!(
                first.witnesses[0].normalized_trace_key(),
                witness.normalized_trace_key()
            );
            assert_eq!(first.witnesses[0].steps()[0].board_before_mask().len(), 66);
            let pages = first.metadata.member_page_count.parse::<usize>().unwrap();
            let mut keys = BTreeSet::new();
            for page in 1..=pages {
                for member in store.page(1, page, &control).unwrap().witnesses {
                    assert!(
                        keys.insert(member.normalized_trace_key().to_owned()),
                        "no duplicate replay identity"
                    );
                }
            }
            assert_eq!(keys.len() as u128, report.witness_count());
            assert_eq!(store.page(1, 1, &control).unwrap(), first);
            let mut execution = context.start_cooperative_execution(request);
            let mut completed = None;
            for _ in 0..4096 {
                match execution.advance(128, &control) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        completed = Some(response);
                        break;
                    }
                    other => panic!("unexpected replay step: {other:?}"),
                }
            }
            let response = completed.expect("bounded replay request completes");
            assert_eq!(response.status(), AppStatus::Success, "{response:?}");
            let other = response
                .product_capability_result()
                .unwrap()
                .pc_path_family_v2()
                .unwrap();
            assert_eq!(other.witness_count(), report.witness_count());
            assert_eq!(other.witnesses(), report.witnesses());
            assert_eq!(
                other.page_source().unwrap().identity_sha256(),
                page_source.identity_sha256()
            );
        }
    }
}

#[test]
fn full_height_cooperative_tiling_and_direct_product_have_the_same_family() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7, 8, 12, 24] {
        let (request, initial, pieces) = forced_request(height, srs_plus());
        let direct = context.run(request.clone());
        let expected = assert_family(&direct, height, initial, pieces);
        let mut execution = context.start_cooperative_execution(request);
        let mut completed = false;
        for _ in 0..4096 {
            match execution.advance(256, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(response) => {
                    assert_eq!(assert_family(&response, height, initial, pieces), expected);
                    completed = true;
                    break;
                }
                other => panic!("unexpected full-height PC outcome: {other:?}"),
            }
        }
        assert!(completed, "bounded forced PC must complete");
    }
}

#[test]
#[cfg(not(feature = "parallel"))]
fn extended_direct_tiling_never_reduces_an_explicit_multiworker_request() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for workers in [2, 11] {
        // The hosted runner may expose only two logical processors. Inject a
        // sufficient declared limit to reach the terminal's fixed-worker
        // guard, not the independent request-admission hardware guard. This
        // request is rejected before any worker is started.
        let request = forced_worker_request(workers, workers + 1, false);
        let response = context.run(request);
        assert_eq!(response.status(), AppStatus::Unsupported, "{response:?}");
        assert!(response
            .error()
            .expect("the terminal guard must return a typed runtime error")
            .message()
            .contains("shared_terminal_memory_authority_requires_single_worker"));
        assert!(response.product_capability_result().is_none());
        assert!(response.render_model().is_none());
        assert!(!response.resource_report().solver_executed());
    }
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn extended_native_parallel_tiling_keeps_the_real_complete_family_and_terminal_authority() {
    use clearra_core_domain::board::standard_pc_board::Board256Mask;
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7_u8, 8, 12, 24] {
        let mut holes = Board256Mask::EMPTY;
        for row in 0..4 {
            for col in 0..4 {
                holes = holes.union(Board256Mask::singleton(row * 10 + col).unwrap());
            }
        }
        // Keep every initial row incomplete: importing completed rows would
        // clear them before Geometry and change the scenario's piece count.
        // Each remaining four-row block has its own forced-I column, so no
        // column develops a large family of inverse row selections.
        let tail_start = if height == 7 { 3 } else { 4 };
        for row in tail_start..u16::from(height) {
            let column = 5 + (row - tail_start) / 4;
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
        let pieces = holes.count_ones() as usize / 4;
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(holes);
        let mut baseline = None;
        for workers in [1, 2] {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words())
                    .unwrap(),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
                PieceWindow::new(pieces),
            )
            .with_rule(srs_plus())
            .with_exact_pieces(Some(pieces))
            .with_allow_hold(false)
            .with_min_remaining_queue(0)
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::tiling())
            .with_retained_trace_limit(1)
            .with_execution_policy(
                PcExecutionPolicy::default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(workers)
                    .with_worker_hardware_limit(3),
            );
            let request = AppRequest::new(AppCommand::Scenario(
                ScenarioAppCommand::new(query).with_result_projection(
                    PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling),
                ),
            ))
            .with_product_capability_contract(ProductCapabilityContract::PcTiling)
            .unwrap();
            let response = context.run(request.clone());
            assert_eq!(response.status(), AppStatus::Success, "{response:?}");
            let core = response.render_model().unwrap().core_result().unwrap();
            assert_eq!(core.usize_field("workers_requested"), Some(workers));
            assert_eq!(core.usize_field("workers_used"), Some(workers));
            assert_eq!(core.bool_field("cpu_parallel_execution"), Some(workers > 1));
            assert!(core.pc_tiling_family_publication_contract_is_valid());
            assert_eq!(core.bool_field("buildup_executed"), Some(false));
            let family = response
                .product_capability_result()
                .unwrap()
                .pc_tiling_family_v1()
                .unwrap();
            assert_eq!(
                family.normalized_solution_count(),
                2,
                "height={height}, workers={workers}"
            );
            let snapshot = (
                family.normalized_solution_set_hash().to_owned(),
                family.page_keys(0, 100).unwrap(),
            );
            if let Some(expected) = baseline.as_ref() {
                assert_eq!(&snapshot, expected);
            } else {
                baseline = Some(snapshot.clone());
            }
            let mut cooperative = context.start_cooperative_execution(request);
            let mut complete = false;
            for _ in 0..4096 {
                match cooperative.advance(256, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
                        let core = response.render_model().unwrap().core_result().unwrap();
                        assert_eq!(core.usize_field("workers_used"), Some(workers));
                        assert!(core.pc_tiling_family_publication_contract_is_valid());
                        let family = response
                            .product_capability_result()
                            .unwrap()
                            .pc_tiling_family_v1()
                            .unwrap();
                        assert_eq!(family.normalized_solution_count(), 2);
                        assert_eq!(
                            (
                                family.normalized_solution_set_hash().to_owned(),
                                family.page_keys(0, 100).unwrap(),
                            ),
                            snapshot
                        );
                        complete = true;
                        break;
                    }
                    other => panic!("unexpected extended parallel PC outcome: {other:?}"),
                }
            }
            assert!(complete, "bounded parallel Tiling must complete");
        }
    }
}

fn forced_worker_request(workers: usize, hardware_limit: usize, all_cpu: bool) -> AppRequest {
    let (request, _, _) = forced_request(24, srs_plus());
    let AppCommand::Scenario(command) = request.command() else {
        unreachable!()
    };
    let query = command.query().clone().with_execution_policy(
        command
            .query()
            .execution_policy()
            .clone()
            .with_workers(workers)
            .with_worker_hardware_limit(hardware_limit)
            .with_use_all_logical_processors(all_cpu),
    );
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(command.result_projection()),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcTiling)
    .unwrap()
}

#[test]
fn extended_tiling_hardware_admission_is_distinct_from_terminal_admission() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for (workers, all_cpu, reason) in [
        (2, false, "execution_workers_require_all_cpu_opt_in"),
        (11, true, "execution_workers_exceed_hardware"),
    ] {
        let response = context.run(forced_worker_request(workers, 2, all_cpu));
        assert_eq!(
            response.status(),
            AppStatus::ValidationFailed,
            "{response:?}"
        );
        assert!(response
            .diagnostics()
            .validation()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic
                .evidence()
                .iter()
                .any(|evidence| evidence.key() == "reason" && evidence.value() == reason)));
        assert!(response.error().is_none());
        assert!(response.product_capability_result().is_none());
        assert!(response.render_model().is_none());
        assert!(!response.resource_report().solver_executed());
    }
}

#[test]
fn extended_tiling_keeps_the_fixed_trace_contract_at_admission() {
    let (request, _, _) = forced_request(7, srs_plus());
    let AppCommand::Scenario(command) = request.command() else {
        unreachable!()
    };
    for limit in [0, 2] {
        let query = command.query().clone().with_retained_trace_limit(limit);
        let rejected = AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(query).with_result_projection(command.result_projection()),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcTiling);
        assert!(rejected
            .unwrap_err()
            .to_string()
            .contains("fixed unused retained-trace limit"));
    }
}

fn ordinary_request(height: u8, probabilities: bool) -> (AppRequest, Board256Mask, usize) {
    let (tiling, initial, pieces) = forced_request(height, srs_plus());
    let AppCommand::Scenario(command) = tiling.command() else {
        unreachable!()
    };
    let query = command
        .query()
        .clone()
        .with_objective(ObjectivePolicy::all())
        .with_count_policy(PcCountPolicy::CountAll)
        .with_solution_probability_policy(if probabilities {
            PcSolutionProbabilityPolicy::Include
        } else {
            PcSolutionProbabilityPolicy::Omit
        });
    (
        AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(query))),
        initial,
        pieces,
    )
}

#[test]
fn full_height_ordinary_pc_reaches_app_presentation_after_actual_buildup() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7, 8, 12, 24] {
        let (request, initial, pieces) = ordinary_request(height, false);
        let response = context.run(request);
        assert_eq!(response.status(), AppStatus::Success, "{response:?}");
        let result = response.render_model().unwrap().core_result().unwrap();
        assert_eq!(result.field("actual_backend"), Some("wasm-cpu-pc-extended"));
        assert_eq!(result.bool_field("buildup_executed"), Some(true));
        assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
        assert_eq!(result.normalized_solution_keys().len(), 1);
        let identity =
            ExtendedTilingSolutionKey::parse_canonical(&result.normalized_solution_keys()[0])
                .unwrap();
        assert_eq!(identity.initial_board(), initial);
        assert_eq!(identity.placement_count(), pieces);
        assert_eq!(
            result
                .path_steps()
                .iter()
                .map(|step| u16::from(step.cleared_lines()))
                .sum::<u16>(),
            u16::from(height)
        );
        assert!(result.pc_tiling_memory_admission_evidence().is_none());
    }
}

#[test]
fn full_height_ordinary_pc_optional_probabilities_keep_the_same_complete_family() {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in [7, 8, 12, 24] {
        let (without, _, _) = ordinary_request(height, false);
        let (with, _, _) = ordinary_request(height, true);
        let baseline = context.run(without);
        let included = context.run(with);
        assert_eq!(included.status(), AppStatus::Success, "{included:?}");
        let baseline = baseline.render_model().unwrap().core_result().unwrap();
        let result = included.render_model().unwrap().core_result().unwrap();
        assert_eq!(
            result.normalized_solution_keys(),
            baseline.normalized_solution_keys()
        );
        assert_eq!(result.solution_probabilities().len(), 1);
        assert_eq!(
            result.solution_probabilities()[0].solution_key(),
            &result.normalized_solution_keys()[0]
        );
        assert_eq!(result.solution_probabilities()[0].probability(), "1");
        assert!(result.solution_probabilities()[0].probability_complete());
        assert!(result.exact_scoring_execution_batches().is_empty());
    }
}
