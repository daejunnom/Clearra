//! Full-population evidence is opt-in and bounded by its CI process owner.
//! The command comes from the real UI adapter and the unchanged original JSON
//! fixture. This test neither rewrites its target nor samples either P7 source.
use clearra_app::{AppCommand, AppContext, AppStatus};
use clearra_cli_command::CliCommandParser;
use clearra_forward_search::CrossStageEarlyLimit;
use clearra_host_contract::ProductResultPayloadContent;
use std::time::Instant;

#[test]
#[ignore = "full original P7/P7; run explicitly under the finite recovery fixture CI owner"]
fn recovery_full_original_fixture_completes_through_public_app() {
    let command = std::env::var("CLEARRA_RECOVERY_FULL_COMMAND")
        .expect("the original fixture must be lowered by the current UI adapter");
    let logical = std::thread::available_parallelism()
        .expect("record actual host capacity")
        .get();
    let workers = logical.saturating_sub(1).max(1);
    let request = CliCommandParser::parse_with_worker_limit(
        &format!("{command} --workers {workers}"),
        logical,
    )
    .unwrap()
    .to_app_request()
    .unwrap();
    assert_eq!(usize::from(request.resource_budget().workers()), workers);
    let AppCommand::RecoveryBuild(recovery) = request.command() else {
        panic!("the public Recovery Build route is required");
    };
    let query = recovery.query();
    assert_eq!(query.fields.height, 10);
    assert_eq!(query.fields.initial.words(), [0xc0383f3fc7, 0, 0, 0]);
    assert_eq!(query.fields.middle.words(), [0x3ff3fc7c0c038, 0, 0, 0]);
    assert_eq!(query.fields.result.words(), [0x30483f07f3f8f, 0, 0, 0]);
    assert_eq!(query.first_supply, "P7");
    assert_eq!(query.second_supply, "P7");
    assert_eq!(query.early_limit, CrossStageEarlyLimit::Auto);
    assert!(query.hold_enabled);
    assert!(query.allow_piece_exchange);
    assert!(query.preserve_b2b);
    assert!(query.initial_b2b);
    assert_eq!(query.rule_profile.as_str(), "srs-plus");
    assert_eq!(query.spin_profile.as_str(), "all-spin-plus");
    eprintln!(
        "recovery_full_start logical_processors={logical} worker_slots={workers} command={command}"
    );
    let started = Instant::now();
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);
    let host = response.to_host_response();
    let ProductResultPayloadContent::RecoveryBuild(payload) =
        host.product_result_payload().unwrap().content()
    else {
        panic!("exact public recovery payload is required");
    };
    assert!(payload.complete);
    assert!(!payload.all_paths_enumerated);
    assert_eq!(payload.pattern_count, "25401600");
    assert_eq!(payload.evaluated_pattern_count, payload.pattern_count);
    let counts = [
        payload.normal_count.parse::<u128>().unwrap(),
        payload.recovery_count.parse::<u128>().unwrap(),
        payload.no_path_count.parse::<u128>().unwrap(),
    ];
    assert_eq!(counts.iter().sum::<u128>(), 25_401_600);
    assert!(
        counts[0] + counts[1] > 0,
        "the original drawing has known positive paths"
    );
    let probabilities = [
        payload.normal_probability.parse::<f64>().unwrap(),
        payload.recovery_probability.parse::<f64>().unwrap(),
        payload.no_path_probability.parse::<f64>().unwrap(),
    ];
    for (count, probability) in counts.iter().zip(probabilities) {
        assert!(probability.is_finite() && (0.0..=1.0).contains(&probability));
        assert!((probability - *count as f64 / 25_401_600.0).abs() < 1e-9);
    }
    assert!(!payload.examples.is_empty());
    for example in &payload.examples {
        assert_eq!(example.steps.len(), 14);
        assert!(example.steps.iter().all(|step| step.b2b_active));
    }
    eprintln!(
        "recovery_full_result {{\"complete\":true,\"logical_processors\":{logical},\"worker_slots\":{workers},\"evaluated\":25401600,\"normal\":{},\"recovery\":{},\"no_path\":{},\"states\":{},\"elapsed_ms\":{}}}",
        counts[0], counts[1], counts[2], payload.state_count, started.elapsed().as_millis()
    );
}
