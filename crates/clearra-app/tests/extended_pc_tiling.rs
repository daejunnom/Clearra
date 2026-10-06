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
