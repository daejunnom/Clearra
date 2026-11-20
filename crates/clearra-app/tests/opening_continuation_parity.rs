//! Product-level parity for the smallest native/Rust next-PC handoff.
use std::sync::{Mutex, MutexGuard, PoisonError};

use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppResponse,
    AppServices, AppStatus, ContinueAppCommand, CooperativeAppAdvance, PcAppCommand,
    PcPathIngressOrigin, PcResultProjection, ProductCapabilityContract,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, pc::pc_target::PcTarget};
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcContinuationToken, PcContinuationTokenCodec, PcCountPolicy,
    PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
};
use clearra_supply::queue::queue_parser::parse_fixed_sequence;

fn lock_fixture_execution() -> MutexGuard<'static, ()> {
    // Each search in this test binary uses the real process-wide resource
    // lease; unbounded Native packing reserves the complete memory slice.
    // Serialize this small fixture family rather than retrying admission,
    // changing the requested resources, or serializing the entire test suite.
    static FIXTURE_EXECUTION: Mutex<()> = Mutex::new(());
    FIXTURE_EXECUTION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn rust_context() -> AppContext {
    AppContext::new(AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()))
}

fn request(queue: &str, hold: PcHoldPolicy) -> AppRequest {
    AppRequest::new(AppCommand::Pc(opening_command(queue, hold)))
}

fn opening_command(queue: &str, hold: PcHoldPolicy) -> PcAppCommand {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(
            parse_fixed_sequence(queue).unwrap(),
        ))
        .with_hold_policy(hold)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_execution_policy(
            PcExecutionPolicy::default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_allow_backend_fallback(false),
        );
    PcAppCommand::new(query)
}

fn pc_result(response: &AppResponse) -> &CoreExecutionResult {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    response.render_model().unwrap().core_result().unwrap()
}

#[test]
fn fixed_no_hold_opening_continuation_matches_native_pc2_and_solves_the_suffix() {
    let _execution = lock_fixture_execution();
    let rust = rust_context();
    let result = rust.run(request("IIOOOIIOOO", PcHoldPolicy::Disabled));
    let first = pc_result(&result);
    assert_eq!(first.bool_field("solution_found"), Some(true));
    assert_eq!(first.bool_field("continuation_token_available"), Some(true));
    assert_eq!(first.field("remaining_queue_len"), Some("5"));
    assert_eq!(first.field("remaining_queue_preview"), Some("IIOOO"));
    assert_eq!(first.field("next_pc_candidate"), Some("2L"));
    assert_eq!(first.field("continuation_token_version"), Some("pc2"));
    let token = first.field("continuation_token").unwrap();
    let PcContinuationToken::Opening(decoded) = PcContinuationTokenCodec::parse(token).unwrap()
    else {
        panic!("expected the existing pc2 Opening token");
    };
    assert_eq!(decoded.target(), PcTarget::two_lines());
    assert_eq!(decoded.hold_policy(), PcHoldPolicy::Disabled);
    assert_eq!(decoded.count_policy(), PcCountPolicy::CountAll);
    assert_eq!(
        decoded.queue().as_fixed_sequence().unwrap(),
        &parse_fixed_sequence("IIOOO").unwrap()
    );

    #[cfg(feature = "native-c-core")]
    {
        let native =
            AppContext::native_c_reference().run(request("IIOOOIIOOO", PcHoldPolicy::Disabled));
        let native = pc_result(&native);
        for key in [
            "remaining_queue_len",
            "remaining_queue_preview",
            "remaining_hold",
            "next_pc_candidate",
            "continuation_token_version",
            "continuation_token",
            "normalized_solution_set_hash",
            "covered_pattern_count",
            "count_complete",
        ] {
            assert_eq!(first.field(key), native.field(key), "{key}");
        }
    }

    let continuation_request = || {
        AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(
            token.to_owned(),
        ))))
    };
    let continued = rust.run(continuation_request());
    assert_eq!(continued.status(), AppStatus::Success, "{continued:?}");
    let Some(AppRenderModel::Continue(message)) = continued.render_model() else {
        panic!("expected the ordinary Continue result");
    };
    let field = |name| {
        message
            .fields()
            .iter()
            .find(|field| field.key() == name)
            .map(|field| field.value().as_text())
    };
    assert_eq!(field("status").as_deref(), Some("continued-searched"));
    assert_eq!(field("continuation_kind").as_deref(), Some("opening"));
    assert_eq!(field("solution_found").as_deref(), Some("true"));
    assert_eq!(field("continuation_token"), None);
    let direct_suffix = rust.run(request("IIOOO", PcHoldPolicy::Disabled));
    let direct_suffix = pc_result(&direct_suffix);
    let result_fields = [
        "normalized_solution_set_hash",
        "covered_pattern_count",
        "count_complete",
    ];
    for key in result_fields {
        assert_eq!(field(key).as_deref(), direct_suffix.field(key), "{key}");
    }

    #[cfg(feature = "native-c-core")]
    {
        let native = AppContext::native_c_reference().run(continuation_request());
        assert_eq!(native.status(), AppStatus::Success, "{native:?}");
        let Some(AppRenderModel::Continue(native)) = native.render_model() else {
            panic!("native continuation response");
        };
        for key in result_fields {
            let actual = native
                .fields()
                .iter()
                .find(|value| value.key() == key)
                .map(|value| value.value().as_text());
            assert_eq!(field(key), actual, "{key}");
        }
    }
}

#[test]
fn fixed_no_hold_opening_continuation_omits_unproven_handoffs() {
    let _execution = lock_fixture_execution();
    let rust = rust_context();
    let short = rust.run(request("IIOOOIIOO", PcHoldPolicy::Disabled));
    let short = pc_result(&short);
    assert_eq!(short.bool_field("solution_found"), Some(true));
    assert_eq!(short.field("continuation_token"), None);

    // Five Ts cannot fill a two-row rectangle: their row imbalances are
    // odd-in-total multiples of ±2. The extra five pieces alone prove nothing.
    let no_witness = rust.run(request("TTTTTIIOOO", PcHoldPolicy::Disabled));
    let no_witness = pc_result(&no_witness);
    assert_eq!(no_witness.bool_field("solution_found"), Some(false));
    assert_eq!(no_witness.field("continuation_token"), None);

    let mismatched_target = rust.run(request("IIOOOIIOOOIIOOO", PcHoldPolicy::Disabled));
    let mismatched_target = pc_result(&mismatched_target);
    assert_eq!(mismatched_target.bool_field("solution_found"), Some(true));
    assert_eq!(mismatched_target.field("continuation_token"), None);
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
    panic!("bounded cooperative Opening fixture did not finish")
}

#[test]
fn fixed_opening_continuation_preserves_four_and_six_line_targets() {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;

    let _execution = lock_fixture_execution();
    let rust = rust_context();
    for (target, placements, target_label) in [
        (PcTarget::four_lines(), 10, "4L"),
        (PcTarget::six_lines(), 15, "6L"),
    ] {
        // A grid of O placements admits exactly one colored tiling without
        // making this contract test an unbounded 6L search fixture.
        let queue = "O".repeat(placements * 2);
        let query = OpeningPcSearchQuery::new(target)
            .with_queue(PcQueueInput::fixed_sequence(
                parse_fixed_sequence(&queue).unwrap(),
            ))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::unique())
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_workers(1)
                    .with_allow_backend_fallback(false),
            );
        let request = AppRequest::new(AppCommand::Pc(PcAppCommand::new(query)));
        let direct = rust.run(request.clone());
        let cooperative = complete_cooperatively(&rust, request);
        let direct = pc_result(&direct);
        let cooperative = pc_result(&cooperative);
        assert_eq!(direct.bool_field("solution_found"), Some(true));
        assert_eq!(direct.path_steps().len(), placements);
        assert_eq!(direct.field("next_pc_candidate"), Some(target_label));
        let token = direct.field("continuation_token").expect("witnessed pc2");
        assert_eq!(cooperative.field("continuation_token"), Some(token));
        assert_eq!(
            cooperative.field("normalized_solution_set_hash"),
            direct.field("normalized_solution_set_hash")
        );
        let PcContinuationToken::Opening(decoded) = PcContinuationTokenCodec::parse(token).unwrap()
        else {
            panic!("must publish a same-target opening");
        };
        assert_eq!(decoded.target(), target);
        assert_eq!(decoded.count_policy(), PcCountPolicy::CountUnique);
        assert_eq!(decoded.hold_policy(), PcHoldPolicy::Disabled);
        assert_eq!(
            decoded.queue().as_fixed_sequence().unwrap().len(),
            placements
        );
    }
}

#[test]
fn fixed_opening_continuation_keeps_the_reached_hold_and_exact_suffix() {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    let _execution = lock_fixture_execution();
    let rust = rust_context();
    // A lone T has nonzero checkerboard imbalance, but the other available
    // I and O pieces do not. The first move must store T, consume the next I
    // and leave T held at the first empty-field checkpoint.
    let source = request("TIIOOOIIOOO", PcHoldPolicy::EnabledEmpty);
    let direct = rust.run(source.clone());
    let cooperative = complete_cooperatively(&rust, source);
    let direct = pc_result(&direct);
    let cooperative = pc_result(&cooperative);
    assert_eq!(direct.bool_field("solution_found"), Some(true));
    assert_eq!(direct.path_steps()[0].hold(), "store-current-use-next");
    assert_eq!(direct.field("remaining_hold"), Some("T"));
    let token = direct.field("continuation_token").expect("witnessed pc2");
    assert_eq!(cooperative.field("continuation_token"), Some(token));
    let PcContinuationToken::Opening(decoded) = PcContinuationTokenCodec::parse(token).unwrap()
    else {
        panic!("must publish opening continuation");
    };
    assert_eq!(
        decoded.hold_policy(),
        PcHoldPolicy::EnabledWithPiece(PieceKind::T)
    );
    assert_eq!(
        decoded.queue().as_fixed_sequence().unwrap(),
        &parse_fixed_sequence("IIOOO").unwrap()
    );
}

#[test]
fn fixed_no_hold_opening_continuation_matches_cooperative_pc2_only_for_ordinary_pc() {
    let _execution = lock_fixture_execution();
    let rust = rust_context();
    let request = request("IIOOOIIOOO", PcHoldPolicy::Disabled);
    let direct = rust.run(request.clone());
    let cooperative = complete_cooperatively(&rust, request);
    let direct = pc_result(&direct);
    let cooperative = pc_result(&cooperative);
    assert_eq!(
        cooperative.bool_field("continuation_token_available"),
        Some(true)
    );
    for field in [
        "continuation_token",
        "continuation_token_version",
        "remaining_queue_len",
        "remaining_queue_preview",
        "normalized_solution_set_hash",
        "covered_pattern_count",
        "count_complete",
    ] {
        assert_eq!(cooperative.field(field), direct.field(field), "{field}");
    }

    let typed_path = AppRequest::new(AppCommand::Pc(
        opening_command("IIOOOIIOOO", PcHoldPolicy::Disabled).with_result_projection(
            PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
        ),
    ))
    .with_product_capability_contract(ProductCapabilityContract::PcPath)
    .unwrap();
    let path = complete_cooperatively(&rust, typed_path);
    assert_eq!(pc_result(&path).field("continuation_token"), None);
    assert!(path
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap()
        .completeness()
        .complete());
}

#[test]
fn fixed_no_hold_opening_continuation_is_not_published_after_cooperative_cancellation() {
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress, ProgressSink,
    };
    use std::sync::Arc;

    struct CancelBeforePublication(ExecutionCancellationHandle);
    impl ProgressSink for CancelBeforePublication {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "postprocess" && progress.completed == 1 {
                self.0.cancel();
            }
        }
    }
    let _execution = lock_fixture_execution();
    let rust = rust_context();
    let mut execution =
        rust.start_cooperative_execution(request("IIOOOIIOOO", PcHoldPolicy::Disabled));
    let token = ExecutionCancellationToken::new();
    let handle = token.handle();
    let control = ExecutionControl::new(token)
        .with_progress_sink(Arc::new(CancelBeforePublication(handle.clone())));
    for _ in 0..4096 {
        match execution.advance(64, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => {
                assert!(handle.is_cancelled());
                return;
            }
            other => panic!("cancelled Opening must not publish a result: {other:?}"),
        }
    }
    panic!("cooperative Opening did not reach the cancellation boundary")
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn compact_continue_families_run_cooperatively_and_preserve_their_restart_state() {
    use std::{env, process::Command, sync::Arc};

    use clearra_core_domain::{
        execution_cancellation::{
            ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress,
            ProgressSink,
        },
        piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    // Tokens do not encode workers. Do not modify the parallel test runner's
    // global capacity to qualify these single-worker cooperative boundaries.
    const TEST: &str =
        "compact_continue_families_run_cooperatively_and_preserve_their_restart_state";
    const CHILD: &str = "CLEARRA_TEST_COMPACT_CONTINUE_CHILD";
    if env::var_os(CHILD).is_none() {
        let output = Command::new(env::current_exe().expect("current test executable"))
            .args(["--exact", TEST])
            .env(CHILD, "1")
            .env("CLEARRA_EXPECTED_VCPUS", "1")
            .output()
            .expect("run isolated one-worker compact Continue");
        assert!(
            output.status.success(),
            "compact Continue failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_eq!(PcExecutionPolicy::mvp_default().workers(), 1);

    struct CancelAtPublication(ExecutionCancellationHandle);
    impl ProgressSink for CancelAtPublication {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "postprocess" && progress.completed == 1 {
                self.0.cancel();
            }
        }
    }

    let scenario = |board, pieces: &[PieceKind], count| {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, board),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces.to_vec())),
            PieceWindow::new(count),
        )
        .with_exact_pieces(Some(count))
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(1)
    };
    // After O closes the holes in the first row, its two upper cells
    // become the 0x3 row of the accepted checkpoint. The checkpoint starts
    // with two I pieces, while replay must redo the O lock first.
    let replay_query = scenario(0x3fc, &[PieceKind::O, PieceKind::I, PieceKind::I], 3);
    let checkpoint_query = scenario(0x3, &[PieceKind::I, PieceKind::I], 2);
    let queue = parse_fixed_sequence("IIOOOIIOOO").unwrap();
    let opening = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_count_policy(PcCountPolicy::CountAll);
    let tokens = [
        (
            PcContinuationTokenCodec::encode_opening_continuation(&opening, None, queue.pieces()),
            "opening",
            "continued-searched",
        ),
        (
            PcContinuationTokenCodec::encode_scenario_replay(&replay_query).unwrap(),
            "scenario-replay",
            "scenario-replayed-searched",
        ),
        (
            PcContinuationTokenCodec::encode_scenario_continuation(&checkpoint_query).unwrap(),
            "scenario",
            "scenario-continued-searched",
        ),
    ];
    let rust = rust_context();
    let mut scenario_families = Vec::new();
    for (token, expected_kind, expected_status) in tokens {
        let request = AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(
            token.clone(),
        ))));
        let direct = rust.run(request.clone());
        let cooperative = {
            let mut execution = rust.start_cooperative_execution(request.clone());
            let control = ExecutionControl::default();
            assert!(
                matches!(
                    execution.advance(1, &control),
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress
                ),
                "{expected_kind} did not yield"
            );
            (0..4096)
                .find_map(|_| match execution.advance(64, &control) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => None,
                    CooperativeAppAdvance::Completed(response) => Some(response),
                    other => panic!("unexpected {expected_kind}: {other:?}"),
                })
                .expect("bounded cooperative Continue")
        };
        assert_eq!(direct.status(), AppStatus::Success, "{direct:?}");
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
        let (Some(AppRenderModel::Continue(direct)), Some(AppRenderModel::Continue(cooperative))) =
            (direct.render_model(), cooperative.render_model())
        else {
            panic!("{expected_kind} must preserve the Continue model");
        };
        let value = |message: &clearra_app::AppMessage, key: &str| {
            message
                .fields()
                .iter()
                .find(|field| field.key() == key)
                .map(|field| field.value().as_text())
        };
        for (key, expected) in [
            ("status", expected_status),
            ("continuation_kind", expected_kind),
            ("input_continuation_token", token.as_str()),
            ("solution_found", "true"),
            ("count_complete", "true"),
        ] {
            assert_eq!(value(direct, key).as_deref(), Some(expected), "{key}");
            assert_eq!(value(cooperative, key).as_deref(), Some(expected), "{key}");
        }
        for key in [
            "normalized_solution_set_hash",
            "covered_pattern_count",
            "build_variant_count",
        ] {
            assert!(
                value(direct, key).is_some(),
                "{expected_kind}: missing {key}"
            );
            assert_eq!(value(direct, key), value(cooperative, key), "{key}");
        }
        if expected_kind == "opening" {
            let next = value(cooperative, "continuation_token").expect("eligible pc2");
            assert_eq!(Some(next.clone()), value(direct, "continuation_token"));
            let PcContinuationToken::Opening(decoded) =
                PcContinuationTokenCodec::parse(&next).unwrap()
            else {
                panic!("opening continuation must preserve the accepted suffix");
            };
            assert_eq!(
                decoded.queue().as_fixed_sequence().unwrap(),
                &parse_fixed_sequence("IIOOO").unwrap()
            );
        } else {
            scenario_families.push(value(cooperative, "normalized_solution_set_hash"));
        }

        let new_control = || {
            let cancellation = ExecutionCancellationToken::new();
            let handle = cancellation.handle();
            ExecutionControl::new(cancellation)
                .with_progress_sink(Arc::new(CancelAtPublication(handle)))
        };
        let cancelled = rust.run_with_execution_control(request.clone(), &new_control());
        assert!(cancelled.render_model().is_none(), "{cancelled:?}");
        let mut execution = rust.start_cooperative_execution(request);
        let control = new_control();
        assert!((0..4096).any(|_| match execution.advance(64, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => false,
            CooperativeAppAdvance::Cancelled => true,
            other => panic!("cancelled {expected_kind} published: {other:?}"),
        }));
    }
    assert_ne!(scenario_families[0], scenario_families[1]);
}
