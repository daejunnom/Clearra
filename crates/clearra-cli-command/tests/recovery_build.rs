use clearra_cli_command::CliCommandParser;
use clearra_app::{AppCommand,AppContext,AppStatus};
use clearra_host_contract::ProductResultPayloadContent;
const BASE:&str="clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply I --second-supply O";
#[test]
fn recovery_build_two_supplies_lower_into_the_new_typed_query() {
    let request=CliCommandParser::parse(BASE).unwrap().to_app_request().unwrap();
    let AppCommand::RecoveryBuild(ref command)=request.command() else {panic!("new typed route")};
    assert_eq!(command.query().first_supply,"I");assert_eq!(command.query().second_supply,"O");
    let response=AppContext::default().run(request);
    assert_eq!(response.status(),AppStatus::Success);
    let host=response.to_host_response();
    let ProductResultPayloadContent::RecoveryBuild(payload)=host.product_result_payload().unwrap().content() else {panic!("new output")};
    assert_eq!(payload.pattern_count,"1");assert_eq!(payload.normal_count,"1");
    assert!(!payload.all_paths_enumerated);assert_eq!(payload.examples.len(),1);
}
#[test]
fn recovery_build_patterns_remain_separate_canonical_expressions() {
    let command=BASE.replace("--first-supply I --second-supply O","--first-supply P7 --second-supply P7");
    let request=CliCommandParser::parse(&command).unwrap().to_app_request().unwrap();
    let AppCommand::RecoveryBuild(command)=request.command() else {panic!("new typed route")};
    assert_eq!(command.query().first_supply,"P7");assert_eq!(command.query().second_supply,"P7");
}
#[test]
fn recovery_build_exchange_and_limits_cannot_be_silently_ignored() {
    for suffix in [" --stage-one-count 7"," --queue-pattern P7"," --max-states 10"," --max-pattern-evaluations 1"," --role-mask 1:0xf"," --max-early auto --max-early 2"," --allow-piece-exchange --no-piece-exchange"] {
        assert!(CliCommandParser::parse(&format!("{BASE}{suffix}")).is_err(),"{suffix}");
    }
    let command=BASE.replace("--first-supply I --second-supply O","--first-supply O --second-supply I");
    for (flag,expected) in [("--no-piece-exchange","0"),("--allow-piece-exchange","1")] {
        let request=CliCommandParser::parse(&format!("{command} --no-hold {flag}")).unwrap().to_app_request().unwrap();
        let response=AppContext::default().run(request);assert_eq!(response.status(),AppStatus::Success);
        let host=response.to_host_response();
        let ProductResultPayloadContent::RecoveryBuild(payload)=host.product_result_payload().unwrap().content() else {panic!("new output")};
        assert_eq!(payload.recovery_count,expected);
    }
}
