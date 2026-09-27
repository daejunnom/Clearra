use clearra_app::{AppCommand, AppContext, AppStatus};
use clearra_cli_command::CliCommandParser;
use clearra_host_contract::ProductResultPayloadContent;
const BASE: &str = "clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply I --second-supply O";
#[test]
fn recovery_build_two_supplies_lower_into_the_new_typed_query() {
    let request = CliCommandParser::parse(BASE)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::RecoveryBuild(ref command) = request.command() else {
        panic!("new typed route")
    };
    assert_eq!(command.query().first_supply, "I");
    assert_eq!(command.query().second_supply, "O");
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);
    let host = response.to_host_response();
    let ProductResultPayloadContent::RecoveryBuild(payload) =
        host.product_result_payload().unwrap().content()
    else {
        panic!("new output")
    };
    assert_eq!(payload.pattern_count, "1");
    assert_eq!(payload.normal_count, "1");
    assert!(!payload.all_paths_enumerated);
    assert_eq!(payload.examples.len(), 1);
}
#[test]
fn recovery_build_patterns_remain_separate_canonical_expressions() {
    let command = BASE.replace(
        "--first-supply I --second-supply O",
        "--first-supply P7 --second-supply P7",
    );
    let request = CliCommandParser::parse(&command)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::RecoveryBuild(command) = request.command() else {
        panic!("new typed route")
    };
    assert_eq!(command.query().first_supply, "P7");
    assert_eq!(command.query().second_supply, "P7");
}
#[test]
fn recovery_build_exchange_and_limits_cannot_be_silently_ignored() {
    for suffix in [
        " --stage-one-count 7",
        " --queue-pattern P7",
        " --max-states 10",
        " --max-pattern-evaluations 1",
        " --role-mask 1:0xf",
        " --max-early auto --max-early 2",
        " --allow-piece-exchange --no-piece-exchange",
    ] {
        assert!(
            CliCommandParser::parse(&format!("{BASE}{suffix}")).is_err(),
            "{suffix}"
        );
    }
    let command = BASE.replace(
        "--first-supply I --second-supply O",
        "--first-supply O --second-supply I",
    );
    for (flag, expected) in [
        ("--no-piece-exchange", "0"),
        ("--allow-piece-exchange", "1"),
    ] {
        let request = CliCommandParser::parse(&format!("{command} --no-hold {flag}"))
            .unwrap()
            .to_app_request()
            .unwrap();
        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::Success);
        let host = response.to_host_response();
        let ProductResultPayloadContent::RecoveryBuild(payload) =
            host.product_result_payload().unwrap().content()
        else {
            panic!("new output")
        };
        assert_eq!(payload.recovery_count, expected);
    }
}

#[test]
fn recovery_build_cursor_consumes_every_value_and_switch_once() {
    // Move every value-bearing option through the beginning and end of the
    // command. Interleave switches to cover both token-consumption contracts.
    let options = [
        ("--start-mask", "0"),
        ("--middle-mask", "0xf"),
        ("--result-mask", "0xc030"),
        ("--height", "8"),
        ("--first-supply", "O"),
        ("--second-supply", "I"),
        ("--max-early", "1"),
        ("--initial-b2b", "1"),
        ("--rule", "srs-plus"),
        ("--spin-profile", "all-spin-plus"),
    ];
    for offset in 0..options.len() {
        let mut parts = vec!["clearra", "recovery", "build", "--no-hold"];
        for index in 0..options.len() {
            let (option, value) = options[(index + offset) % options.len()];
            parts.extend([option, value]);
            if index == 3 {
                parts.push("--allow-piece-exchange");
            }
            if index == 6 {
                parts.push("--preserve-b2b");
            }
        }
        let command = parts.join(" ");
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let AppCommand::RecoveryBuild(recovery) = request.command() else {
            panic!("typed new route")
        };
        assert_eq!(recovery.query().first_supply, "O");
        assert_eq!(recovery.query().second_supply, "I");
        assert_eq!(
            recovery.query().early_limit,
            clearra_forward_search::CrossStageEarlyLimit::AtMost(1)
        );
        assert!(!recovery.query().hold_enabled);
        assert!(recovery.query().allow_piece_exchange);
        assert!(recovery.query().preserve_b2b);
        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::Success);
        let host = response.to_host_response();
        let ProductResultPayloadContent::RecoveryBuild(payload) =
            host.product_result_payload().unwrap().content()
        else {
            panic!("typed output")
        };
        assert_eq!(payload.recovery_count, "1");
    }
    for (option, _) in options {
        assert!(CliCommandParser::parse(&format!("{BASE} {option}")).is_err());
    }
    assert!(CliCommandParser::parse(&format!("{BASE} --hold --no-hold")).is_err());
}

#[test]
fn recovery_build_worker_budget_is_explicit_and_parallel_payload_matches_serial() {
    let command = BASE.replace(
        "--first-supply I --second-supply O",
        "--first-supply [IO] --second-supply [IO]",
    );
    let mut expected = None;
    for workers in [1, 2, 4] {
        let request = CliCommandParser::parse(&format!(
            "{command} --workers {workers} --allow-piece-exchange --no-hold"
        ))
        .unwrap()
        .to_app_request()
        .unwrap();
        assert_eq!(request.resource_budget().workers(), workers);
        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::Success);
        let public = response.product_result_payload().unwrap().clone();
        if let Some(expected) = &expected {
            assert_eq!(&public, expected);
        } else {
            expected = Some(public);
        }
    }
    for invalid in ["0", "-1", "65536", "NaN", "1.5"] {
        assert!(CliCommandParser::parse(&format!("{BASE} --workers {invalid}")).is_err());
    }
    assert!(CliCommandParser::parse(&format!("{BASE} --workers 2 --workers 3")).is_err());
}
