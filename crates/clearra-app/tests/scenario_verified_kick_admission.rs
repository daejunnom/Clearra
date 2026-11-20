//! Product admission for a CLI pc-scenario supplying a verified kick table.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppErrorCode, AppRequest, AppResponse,
    AppServices, AppStatus, CooperativeAppAdvance, ScenarioAppCommand, ScenarioAppRenderContract,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_rules::kicks::{
    KickImport, KickOffsetSequence, KickTableEntry, KickTableProfile, SrsKicks,
    VerifiedKickTableProfile,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn scenario_request(profile: &KickTableProfile) -> AppRequest {
    // Use the same JSON/verification boundary as the legacy CLI assembler.
    let json = KickImport::to_json(profile).unwrap();
    let profile = VerifiedKickTableProfile::try_new(KickImport::from_json(&json).unwrap()).unwrap();
    // The 1L target has six filled cells, leaving an exact horizontal I.
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_verified_kick_table_profile(profile)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_workers(1)
            .with_allow_backend_fallback(false),
    );
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query)
            .with_render_contract(ScenarioAppRenderContract::new(false, Vec::new())),
    ))
}

fn cooperatively(context: &AppContext, request: AppRequest) -> AppResponse {
    let mut execution = context.start_cooperative_execution(request);
    let control = ExecutionControl::default();
    for _ in 0..4096 {
        match execution.advance(64, &control) {
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Completed(response) => return response,
            other => panic!("unexpected cooperative outcome: {other:?}"),
        }
    }
    panic!("bounded verified-kick scenario did not finish")
}

fn expect_single_i(response: &AppResponse) -> &CoreExecutionResult {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let result = response.render_model().unwrap().core_result().unwrap();
    assert_eq!(result.normalized_solution_keys().len(), 1);
    assert_eq!(
        result.normalized_solution_keys()[0],
        "ctk1|initial=00000000000003f0|placements=I:000000000000000f"
    );
    assert_eq!(result.bool_field("count_complete"), Some(true));
    result
}

#[test]
fn imported_srs_plus_requires_the_same_ordered_transition_table_in_rust() {
    let builtin = SrsKicks::srs_plus_profile();
    // A JSON importer can reorder transition records without changing lookup.
    let mut entries = builtin.entries().to_vec();
    entries.reverse();
    let canonical = KickTableProfile::new(builtin.id(), builtin.source_rule(), entries.clone());

    // Keep ID, source, transition keys and candidate membership identical.
    // Changing a T kick order does not affect the independent I-only oracle,
    // but cannot be accepted as a supported profile for other boards.
    let entry = entries
        .iter_mut()
        .find(|entry| entry.transition().piece() == PieceKind::T && entry.sequence().len() > 1)
        .unwrap();
    let mut offsets = entry.sequence().offsets().to_vec();
    offsets.reverse();
    assert_ne!(offsets.as_slice(), entry.sequence().offsets());
    *entry = KickTableEntry::new(entry.transition(), KickOffsetSequence::new(offsets));
    let reordered = KickTableProfile::new(builtin.id(), builtin.source_rule(), entries);

    // Process-wide execution leases remain sequential across both drivers.
    let rust = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let accepted_direct = rust.run(scenario_request(&canonical));
    let accepted_cooperative = cooperatively(&rust, scenario_request(&canonical));
    let direct = expect_single_i(&accepted_direct);
    let cooperative = expect_single_i(&accepted_cooperative);
    let exact_digest = direct.field("normalized_solution_set_hash").unwrap();
    assert_eq!(
        cooperative.field("normalized_solution_set_hash"),
        Some(exact_digest)
    );

    #[cfg(feature = "native-c-core")]
    {
        let native = AppContext::default();
        for profile in [&canonical, &reordered] {
            let response = native.run(scenario_request(profile));
            assert_eq!(
                expect_single_i(&response).field("normalized_solution_set_hash"),
                Some(exact_digest)
            );
        }
    }

    for rejected in [
        rust.run(scenario_request(&reordered)),
        cooperatively(&rust, scenario_request(&reordered)),
    ] {
        assert_eq!(
            rejected.status(),
            AppStatus::ExecutionFailed,
            "{rejected:?}"
        );
        assert_eq!(
            rejected.error().unwrap().code(),
            AppErrorCode::ExecutionFailed
        );
        assert!(rejected.render_model().is_none());
    }
}
