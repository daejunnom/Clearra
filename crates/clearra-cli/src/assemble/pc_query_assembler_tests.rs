use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_rules::profile::rule_profile::RuleProfileId;

use super::*;

#[test]
fn assembles_supported_mvp_pc_query() {
    let query = PcQueryAssembler::assemble(&PcArgs::new(4)).expect("query");

    assert_eq!(query.target().lines(), 4);
    assert_eq!(query.queue().mode(), "observed");
    assert!(query.hold_policy().is_enabled());
    assert_eq!(query.objective().kind(), ObjectiveKind::All);
}

#[test]
fn preserves_visible_seven_policy_in_pc_query() {
    let args = PcArgs::new(4).with_queue_observation_policy(
        clearra_supply::queue::queue_observation_policy::QueueObservationPolicy::VisibleSeven,
    );
    let query = PcQueryAssembler::assemble(&args).expect("query");

    assert_eq!(
        query.queue_observation_policy(),
        clearra_supply::queue::queue_observation_policy::QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn rejects_unsupported_even_target() {
    assert_eq!(
        PcQueryAssembler::assemble(&PcArgs::new(8)),
        Err(PcQueryAssemblyError::UnsupportedMvpTarget { lines: 8 })
    );
}

#[test]
fn assembles_queue_hold_and_objective_into_pc_query() {
    let args = PcArgs::new(2)
        .with_queue("I,O,T", true)
        .with_hold_enabled(false)
        .with_objective("unique");
    let query = PcQueryAssembler::assemble(&args).expect("query");

    assert_eq!(query.queue().mode(), "fixed");
    assert_eq!(query.queue().len(), 3);
    assert!(!query.hold_policy().is_enabled());
    assert_eq!(query.objective().kind(), ObjectiveKind::Unique);
}

#[test]
fn assembles_rule_and_verified_kick_profile_override_into_pc_query() {
    let import_json =
        clearra_rules::kicks::KickImport::to_json(&clearra_rules::kicks::NoKick::profile())
            .expect("no-kick json");
    let args = PcArgs::new(2)
        .with_rule(Some("no-kick".to_owned()))
        .with_kick_profile_json(Some(import_json));
    let query = PcQueryAssembler::assemble(&args).expect("query");

    assert_eq!(query.rule().id(), RuleProfileId::NoKick);
    assert!(query.verified_kick_profile().is_some());
}

#[test]
fn assembles_execution_policy_into_pc_query() {
    let args = PcArgs::new(2)
        .with_backend(Some("cpu".to_owned()))
        .with_workers(Some(4))
        .with_max_frontier_states(Some(256))
        .with_allow_backend_fallback(Some(false));

    let query = PcQueryAssembler::assemble(&args).expect("query");

    assert_eq!(
        query.execution_policy().requested_backend(),
        clearra_pc_graph::request::RequestedSearchBackend::Cpu
    );
    assert_eq!(query.execution_policy().workers(), 4);
    assert_eq!(query.execution_policy().max_frontier_states(), 256);
    assert!(!query.execution_policy().allow_backend_fallback());
}

#[test]
fn rejects_unverified_kick_profile_override_before_search_query_runs() {
    let incomplete = r#"{"id":"imported","source_rule":"custom","entries":[]}"#;
    let args = PcArgs::new(2).with_kick_profile_json(Some(incomplete.to_owned()));

    assert!(matches!(
        PcQueryAssembler::assemble(&args),
        Err(PcQueryAssemblyError::UnverifiedKickProfile { .. })
    ));
}

#[test]
fn rejects_unknown_queue_piece() {
    let args = PcArgs::new(2).with_queue("IX", false);

    assert_eq!(
        PcQueryAssembler::assemble(&args),
        Err(PcQueryAssemblyError::UnknownPiece {
            index: 1,
            value: 'X'
        })
    );
}

#[test]
fn rejects_unknown_objective() {
    let args = PcArgs::new(2).with_objective("speed");

    assert_eq!(
        PcQueryAssembler::assemble(&args),
        Err(PcQueryAssemblyError::UnsupportedObjective {
            value: "speed".to_owned()
        })
    );
}

#[test]
fn rejects_unknown_execution_backend() {
    let args = PcArgs::new(2).with_backend(Some("quantum".to_owned()));

    assert!(matches!(
        PcQueryAssembler::assemble(&args),
        Err(PcQueryAssemblyError::InvalidExecutionPolicy { message })
            if message.contains("quantum")
    ));
}
