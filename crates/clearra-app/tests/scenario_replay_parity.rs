//! Canonical pc-scenario input replay across C and both Rust App drivers.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppResponse,
    AppServices, AppStatus, ContinueAppCommand, CooperativeAppAdvance, ScenarioAppCommand,
    ScenarioAppExpected, ScenarioAppRenderContract,
};
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    PcContinuationToken, PcContinuationTokenCodec, PcCountPolicy, PcExecutionPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs;
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
        let native = AppContext::native_c_reference().run(legacy_scenario(scenario_query()));
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

    assert_legacy_tall_visible_scenario(&rust);
    assert_legacy_variable_max_scenarios(&rust);
}

fn assert_legacy_tall_visible_scenario(rust: &AppContext) {
    let query = |occupied_mask, allow_hold| {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, occupied_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_rule(srs())
        .with_exact_pieces(Some(1))
        .with_allow_hold(allow_hold)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(1)
    };
    let raw = query((0x3f0 << 10) | 0x3ff, false).with_count_policy(PcCountPolicy::CountUnique);
    let raw_count_all = raw.clone().with_count_policy(PcCountPolicy::CountAll);
    // Matches the public scenario_simple_4l fixture: holding the only I
    // cannot substitute another current piece, so the legal lock is unchanged.
    let singleton_hold = query(0x3f0, true);
    for input in [&raw, &raw_count_all, &singleton_hold] {
        let explicit = ProblemCompiler::compile_scenario_pc(input).unwrap();
        let legacy = ProblemCompiler::compile_legacy_scenario_pc(input).unwrap();
        assert_eq!(legacy.core_query(), explicit.core_query());
        assert_eq!(legacy.initial_board(), explicit.initial_board());
        assert_eq!(legacy.visible_height(), 4);
        assert_eq!(legacy.compact_pc_geometry_height(), 1);
        assert_eq!(explicit.compact_pc_geometry_height(), 4);
        assert_ne!(legacy.problem_id(), explicit.problem_id());

        let request = || {
            let contract = ScenarioAppRenderContract::new(true, Vec::new())
                .with_expected(Some(ScenarioAppExpected::new(true, Some(true))));
            AppRequest::new(AppCommand::Scenario(
                ScenarioAppCommand::new(input.clone()).with_render_contract(contract),
            ))
        };
        let direct = rust.run(request());
        let cooperative = complete_cooperatively(rust, request());
        let direct = core(&direct);
        let cooperative = core(&cooperative);
        assert_eq!(direct.normalized_solution_keys().len(), 1);
        assert_eq!(
            direct.normalized_solution_keys()[0],
            "ctk1|initial=00000000000003f0|placements=I:000000000000000f"
        );
        assert_eq!(direct.field("visible_height"), Some("4"));
        assert_eq!(
            direct.field("initial_board_mask"),
            Some("0x00000000000003f0")
        );
        assert_eq!(direct.bool_field("expected_match"), Some(true));
        let token = direct.field("scenario_replay_token").unwrap();
        assert!(token.starts_with("sr2:w10:v4:"));
        let PcContinuationToken::ScenarioReplay(decoded) =
            PcContinuationTokenCodec::parse(token).unwrap()
        else {
            panic!("legacy scenario input replay");
        };
        assert_eq!(decoded.initial_board(), legacy.initial_board());
        assert_eq!(decoded.allow_hold(), input.allow_hold());
        let keys = [
            "scenario_replay_token",
            "visible_height",
            "initial_board_mask",
            "normalized_solution_set_hash",
            "solution_found",
            "count_complete",
        ];
        for key in keys {
            assert_eq!(direct.field(key), cooperative.field(key), "{key}");
        }
        assert_eq!(
            direct.normalized_solution_keys(),
            cooperative.normalized_solution_keys()
        );
        assert_eq!(direct.bool_field("probability_complete"), Some(true));
        assert_eq!(cooperative.bool_field("probability_complete"), Some(true));
        assert_eq!(direct.field("covered_pattern_count"), Some("1"));
        assert_eq!(cooperative.field("covered_pattern_count"), Some("1"));
        #[cfg(feature = "native-c-core")]
        {
            let native = AppContext::native_c_reference().run(request());
            let native = core(&native);
            assert_eq!(
                direct.normalized_solution_keys(),
                native.normalized_solution_keys()
            );
            for key in keys {
                assert_eq!(direct.field(key), native.field(key), "{key}");
            }
            if input.count_policy() == PcCountPolicy::CountUnique {
                // Native verifies existence per candidate without producing
                // pattern coverage in this mode. Its placeholder count is
                // not the cardinality of a completed coverage result.
                assert_eq!(native.bool_field("probability_complete"), Some(false));
            } else {
                assert_eq!(native.bool_field("probability_complete"), Some(true));
                assert_eq!(native.field("covered_pattern_count"), Some("1"));
            }
        }
        let replayed = rust.run(AppRequest::new(AppCommand::Continue(
            ContinueAppCommand::new(Some(token.to_owned())),
        )));
        assert_eq!(replayed.status(), AppStatus::Success, "{replayed:?}");
        let Some(AppRenderModel::Continue(message)) = replayed.render_model() else {
            panic!("scenario Continue result");
        };
        let hash = message
            .fields()
            .iter()
            .find(|field| field.key() == "normalized_solution_set_hash");
        assert_eq!(
            hash.map(|field| field.value().as_text()).as_deref(),
            direct.field("normalized_solution_set_hash")
        );
    }

    let explicit_request =
        AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(raw.clone())));
    let explicit = rust.run(explicit_request);
    assert_ne!(explicit.status(), AppStatus::Success);
    assert!(explicit.render_model().is_none());
}

fn assert_legacy_variable_max_scenarios(rust: &AppContext) {
    let query = |height, mask, pieces: &[PieceKind], max| {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces.to_vec())),
            PieceWindow::new(max),
        )
        .with_rule(srs())
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(1)
    };
    let raw = (0x3f0 << 10) | 0x3ff;
    let cap_queue = [
        PieceKind::I,
        PieceKind::I,
        PieceKind::O,
        PieceKind::O,
        PieceKind::O,
        PieceKind::I,
    ];
    for (input, active_rows, empty_family_depth, resolves) in [
        // Only the first I can fill the one-row hole. OI cannot use the I
        // suffix beyond its admitted one-piece family.
        (
            query(4, raw, &[PieceKind::I, PieceKind::O], 1),
            1,
            None,
            true,
        ),
        (
            query(4, raw, &[PieceKind::O, PieceKind::I], 1),
            1,
            None,
            false,
        ),
        // max=2 rounds the active frame to two rows. Fourteen missing
        // cells cannot be covered by any integral number of tetrominoes.
        (
            query(4, 0x3f0, &[PieceKind::I, PieceKind::I], 2),
            2,
            Some(2),
            false,
        ),
    ] {
        let legacy = ProblemCompiler::compile_legacy_scenario_pc(&input).unwrap();
        assert_eq!(legacy.exact_pieces(), None);
        assert_eq!(legacy.compact_pc_geometry_height(), active_rows);
        assert_eq!(
            legacy.compact_pc_proven_empty_family_depth(),
            empty_family_depth
        );
        let request = || legacy_scenario(input.clone());
        if empty_family_depth.is_some() {
            let cancellation = ExecutionCancellationToken::new();
            cancellation.handle().cancel();
            let control = ExecutionControl::new(cancellation);
            let mut cancelled = rust.start_cooperative_execution(request());
            assert_eq!(
                cancelled.advance(1, &control),
                CooperativeAppAdvance::Cancelled
            );
        }
        let direct = rust.run(request());
        let cooperative = complete_cooperatively(rust, request());
        let direct = core(&direct);
        let cooperative = core(&cooperative);
        assert_eq!(direct.bool_field("solution_found"), Some(resolves));
        assert_eq!(direct.bool_field("count_complete"), Some(true));
        assert_eq!(direct.bool_field("probability_complete"), Some(true));
        assert_eq!(direct.bool_field("resource_truncated"), Some(false));
        assert_eq!(
            direct.field("covered_pattern_count"),
            Some(if resolves { "1" } else { "0" })
        );
        assert_eq!(
            direct.normalized_solution_keys().len(),
            usize::from(resolves)
        );
        if resolves {
            assert_eq!(
                direct.normalized_solution_keys()[0],
                "ctk1|initial=00000000000003f0|placements=I:000000000000000f"
            );
        }
        let token = direct.field("scenario_replay_token").unwrap();
        let prefix = format!("sr2:w10:v{}:", input.initial_board().visible_height());
        assert!(token.starts_with(&prefix));
        let PcContinuationToken::ScenarioReplay(decoded) =
            PcContinuationTokenCodec::parse(token).unwrap()
        else {
            panic!("expected a legacy input replay");
        };
        assert!(token.split(':').any(|segment| segment == "xnone"));
        assert_eq!(decoded.exact_pieces(), None);
        assert_eq!(decoded.piece_window(), input.piece_window());
        assert_eq!(decoded.remaining_queue(), input.remaining_queue());
        assert_eq!(decoded.initial_board(), legacy.initial_board());
        let fields = [
            "scenario_replay_token",
            "normalized_solution_set_hash",
            "covered_pattern_count",
            "solution_found",
            "probability_complete",
            "count_complete",
        ];
        for key in fields {
            assert_eq!(direct.field(key), cooperative.field(key), "{key}");
        }
        #[cfg(feature = "native-c-core")]
        {
            let native = AppContext::native_c_reference().run(request());
            let native = core(&native);
            assert_eq!(
                direct.normalized_solution_keys(),
                native.normalized_solution_keys()
            );
            for key in fields {
                assert_eq!(direct.field(key), native.field(key), "{key}");
            }
        }

        let replay = rust.run(AppRequest::new(AppCommand::Continue(
            ContinueAppCommand::new(Some(token.to_owned())),
        )));
        assert_eq!(replay.status(), AppStatus::Success, "{replay:?}");
        let Some(AppRenderModel::Continue(message)) = replay.render_model() else {
            panic!("expected the ordinary Continue result");
        };
        for key in [
            "normalized_solution_set_hash",
            "solution_found",
            "probability_complete",
            "count_complete",
        ] {
            let replay_value = message.fields().iter().find(|field| field.key() == key);
            assert_eq!(
                replay_value.map(|field| field.value().as_text()).as_deref(),
                direct.field(key)
            );
        }
    }

    // Native's root projection needs a family vector of geometry depth 5.
    // Capping the frame at 2L with max=6 does not give that vector. The
    // legacy Rust route must neither claim proved-empty nor solve a shorter
    // prefix; this family is not qualified for a Native-to-Rust handoff.
    let capped = query(2, 0, &cap_queue, 6);
    let legacy = ProblemCompiler::compile_legacy_scenario_pc(&capped).unwrap();
    let ordinary = ProblemCompiler::compile_scenario_pc(&capped).unwrap();
    assert_eq!(legacy.core_query(), ordinary.core_query());
    assert_eq!(legacy.initial_board(), ordinary.initial_board());
    assert_ne!(legacy.problem_id(), ordinary.problem_id());
    assert!(!ordinary.compact_pc_has_unqualified_capped_family());
    assert!(legacy.compact_pc_has_unqualified_capped_family());
    assert_eq!(legacy.compact_pc_proven_empty_family_depth(), None);
    let request = legacy_scenario(capped);
    for response in [
        rust.run(request.clone()),
        complete_cooperatively(rust, request.clone()),
    ] {
        assert_eq!(
            response.status(),
            AppStatus::ExecutionFailed,
            "{response:?}"
        );
        assert!(response.render_model().is_none());
    }
    #[cfg(feature = "native-c-core")]
    {
        let native = AppContext::native_c_reference().run(request);
        assert_eq!(native.status(), AppStatus::ExecutionFailed);
        assert_eq!(
            native.error().map(clearra_app::AppError::code),
            Some(clearra_app::AppErrorCode::ExecutionFailed)
        );
        assert!(native.render_model().is_none());
    }

    // The same five-piece prefix solves when it is the admitted family.
    let prefix = query(2, 0, &cap_queue, 5);
    assert_eq!(
        core(&rust.run(legacy_scenario(prefix.clone()))).bool_field("solution_found"),
        Some(true)
    );
    #[cfg(feature = "native-c-core")]
    assert_eq!(
        core(&AppContext::native_c_reference().run(legacy_scenario(prefix)))
            .bool_field("solution_found"),
        Some(true)
    );

    // Other families preserve their original extent; none is turned
    // into a cropped target, shorter-prefix solution or artificial empty.
    let unchanged = [
        query(4, 0x3f0 << 20, &[PieceKind::I], 1), // occupied above area
        query(4, 0x3f0, &[PieceKind::I, PieceKind::O], 1).with_allow_hold(true),
        query(2, 0, &[PieceKind::I; 16], 16), // Native compact family limit
    ];
    for input in unchanged {
        let legacy = ProblemCompiler::compile_legacy_scenario_pc(&input).unwrap();
        let ordinary = ProblemCompiler::compile_scenario_pc(&input).unwrap();
        assert_eq!(legacy, ordinary);
    }
    let short = query(4, 0x3f0, &[PieceKind::I], 2);
    assert!(ProblemCompiler::compile_legacy_scenario_pc(&short).is_err());
}
