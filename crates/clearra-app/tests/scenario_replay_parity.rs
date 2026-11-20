//! Canonical pc-scenario input replay across C and both Rust App drivers.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppResponse,
    AppServices, AppStatus, ContinueAppCommand, CooperativeAppAdvance, ScenarioAppCommand,
    ScenarioAppRenderContract,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    PcContinuationToken, PcContinuationTokenCodec, PcCountPolicy, PcExecutionPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn scenario_query() -> PcScenarioQuery {
    // The full bottom row clears before search. In the 2L target only O
    // can cover the two remaining bottom holes; the two horizontal I pieces
    // then uniquely tile x=2..9 in the original upper row.
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, (0x3fc << 10) | 0x3ff),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
        ])),
        PieceWindow::new(3),
    )
    .with_exact_pieces(Some(3))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    )
}

fn legacy_scenario(query: PcScenarioQuery) -> AppRequest {
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query)
            .with_render_contract(ScenarioAppRenderContract::new(false, Vec::new())),
    ))
}

fn core(response: &AppResponse) -> &CoreExecutionResult {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    response.render_model().unwrap().core_result().unwrap()
}

fn complete_cooperatively(context: &AppContext, request: AppRequest) -> AppResponse {
    let mut execution = context.start_cooperative_execution(request);
    let control = ExecutionControl::default();
    for _ in 0..4096 {
        match execution.advance(64, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(response) => return response,
            other => panic!("unexpected cooperative outcome: {other:?}"),
        }
    }
    panic!("bounded compact scenario did not finish")
}

#[test]
fn compact_fixed_scenario_input_replay_matches_native_and_cooperative() {
    // All real searches run here in one test; Native and Rust admissions
    // share a process-wide lease and must not overlap inside this fixture.
    let rust = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let request = legacy_scenario(scenario_query());
    let direct = rust.run(request.clone());
    let cooperative = complete_cooperatively(&rust, request);
    let direct = core(&direct);
    let cooperative = core(&cooperative);
    let expected_key = concat!(
        "ctk1|initial=00000000000003fc|placements=",
        "I:000000000000f000,I:00000000000f0000,O:0000000000000c03",
    );
    assert_eq!(direct.normalized_solution_keys().len(), 1);
    assert_eq!(direct.normalized_solution_keys()[0], expected_key);
    assert_eq!(direct.bool_field("count_complete"), Some(true));
    assert_eq!(direct.field("scenario_replay_token_version"), Some("sr2"));
    let token = direct.field("scenario_replay_token").unwrap();
    let PcContinuationToken::ScenarioReplay(decoded) =
        PcContinuationTokenCodec::parse(token).unwrap()
    else {
        panic!("must replay the initial scenario, not promise a subsequent PC");
    };
    assert_eq!(
        decoded.initial_board(),
        &PcScenarioBoard::standard_10(2, 0x3fc)
    );
    assert_eq!(
        decoded
            .remaining_queue()
            .as_fixed_sequence()
            .unwrap()
            .pieces(),
        &[PieceKind::O, PieceKind::I, PieceKind::I]
    );
    assert_eq!(decoded.exact_pieces(), Some(3));
    assert_eq!(decoded.count_policy(), PcCountPolicy::CountAll);
    assert!(!decoded.allow_hold());
    let fields = [
        "scenario_replay_token_version",
        "scenario_replay_token",
        "replay_hint",
        "normalized_solution_set_hash",
        "covered_pattern_count",
        "count_complete",
    ];
    for field in fields {
        assert_eq!(direct.field(field), cooperative.field(field), "{field}");
    }
    #[cfg(feature = "native-c-core")]
    {
        let native = AppContext::default().run(legacy_scenario(scenario_query()));
        let native = core(&native);
        for field in fields {
            assert_eq!(direct.field(field), native.field(field), "{field}");
        }
    }

    let replayed = rust.run(AppRequest::new(AppCommand::Continue(
        ContinueAppCommand::new(Some(token.to_owned())),
    )));
    assert_eq!(replayed.status(), AppStatus::Success, "{replayed:?}");
    let Some(AppRenderModel::Continue(message)) = replayed.render_model() else {
        panic!("scenario replay must use the existing Continue App result");
    };
    let field = |name| {
        message
            .fields()
            .iter()
            .find(|field| field.key() == name)
            .map(|field| field.value().as_text())
    };
    assert_eq!(
        field("continuation_kind").as_deref(),
        Some("scenario-replay")
    );
    assert_eq!(field("solution_found").as_deref(), Some("true"));
    assert_eq!(
        field("normalized_solution_set_hash").as_deref(),
        direct.field("normalized_solution_set_hash")
    );

    // sr2 cannot describe a caller-supplied supply window. Keep the actual
    // solve available and omit a token that would silently change its input.
    let override_request =
        legacy_scenario(scenario_query().with_supply_window_size(SupplyWindowSize::new(3)));
    let override_direct = rust.run(override_request.clone());
    let override_cooperative = complete_cooperatively(&rust, override_request);
    assert_eq!(core(&override_direct).field("scenario_replay_token"), None);
    assert_eq!(
        core(&override_cooperative).field("scenario_replay_token"),
        None
    );

    // The bare Scenario path is not the legacy pc-scenario publication surface.
    let bare = rust.run(AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(scenario_query()),
    )));
    assert_eq!(core(&bare).field("scenario_replay_token"), None);
    assert_eq!(core(&bare).field("expected_checked"), None);
}
