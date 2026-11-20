//! Product-level parity for the smallest native/Rust next-PC handoff.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppResponse,
    AppServices, AppStatus, ContinueAppCommand, PcAppCommand,
};
use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcContinuationToken, PcContinuationTokenCodec, PcCountPolicy,
    PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
};
use clearra_supply::queue::queue_parser::parse_fixed_sequence;

fn rust_context() -> AppContext {
    AppContext::new(AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()))
}

fn request(queue: &str, hold: PcHoldPolicy) -> AppRequest {
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
    AppRequest::new(AppCommand::Pc(PcAppCommand::new(query)))
}

fn pc_result(response: &AppResponse) -> &CoreExecutionResult {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    response.render_model().unwrap().core_result().unwrap()
}

#[test]
fn fixed_no_hold_opening_continuation_matches_native_pc2_and_solves_the_suffix() {
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
        let native = AppContext::default().run(request("IIOOOIIOOO", PcHoldPolicy::Disabled));
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
        let native = AppContext::default().run(continuation_request());
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

    let hold = rust.run(request("IIOOOIIOOO", PcHoldPolicy::EnabledEmpty));
    assert_eq!(pc_result(&hold).field("continuation_token"), None);
}
