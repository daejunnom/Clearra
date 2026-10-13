//! Full-height observation stays on the canonical CLI/App/Core route.
//! Bounded few-piece fixtures exercise execution, not performance acceptance.
use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppResponse, AppServices, AppStatus,
    CooperativeAppAdvance,
};
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_supply::QueueObservationPolicy;

const INPUTS: &str =
    include_str!("../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv");

fn inputs() -> impl Iterator<Item = Vec<&'static str>> {
    INPUTS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

fn context() -> AppContext {
    AppContext::new(AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()))
}

fn command(input: &[&str], product: &str, options: &str) -> String {
    format!(
        "clearra pc {product} --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --backend cpu --workers 1 --no-tablebase --no-build-dependency-dag {options}",
        input[1], input[2], input[1], input[3], input[4],
    )
}

fn assert_observation(response: &AppResponse) -> Vec<String> {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let core = response.render_model().unwrap().core_result().unwrap();
    assert_eq!(core.field("queue_knowledge"), Some("visible-7"));
    assert_eq!(
        core.field("coverage_semantics"),
        Some("visible-seven-policy")
    );
    assert_eq!(core.bool_field("observation_policy_evaluated"), Some(true));
    assert!(
        core.usize_field("observation_policy_action_checks")
            .unwrap()
            > 0
    );
    assert_eq!(core.usize_field("coverage_pattern_count"), Some(1));
    assert_eq!(core.field("coverage_probability"), Some("1"));
    assert_eq!(core.bool_field("probability_complete"), Some(true));
    response
        .complete_solution_set_artifact()
        .expect("retain the exact complete ordinary family")
        .entries()
        .iter()
        .map(|entry| entry.key().to_owned())
        .collect()
}

#[test]
fn ordinary_full_height_visible_seven_keeps_direct_and_cooperative_policy_evidence() {
    let context = context();
    for input in inputs() {
        for count in ["all", "unique"] {
            let options = format!("--count {count} --queue-knowledge visible-7");
            let source = command(&input, "", &options);
            let request = CliCommandParser::parse(&source)
                .unwrap()
                .to_app_request()
                .unwrap();
            let AppCommand::Scenario(scenario) = request.command() else {
                panic!("the same full-height scenario ingress must own the observation request");
            };
            assert_eq!(
                scenario.query().queue_observation_policy(),
                QueueObservationPolicy::VisibleSeven
            );
            let reference = context.run(
                CliCommandParser::parse(&source.replace("visible-7", "oracle"))
                    .unwrap()
                    .to_app_request()
                    .unwrap(),
            );
            let expected: Vec<String> = reference
                .complete_solution_set_artifact()
                .unwrap()
                .entries()
                .iter()
                .map(|entry| entry.key().to_owned())
                .collect();
            assert_eq!(assert_observation(&context.run(request.clone())), expected);
            let mut execution = context.start_cooperative_execution(request);
            let mut completed = false;
            for _ in 0..4096 {
                match execution.advance(256, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        assert_eq!(assert_observation(&response), expected);
                        completed = true;
                        break;
                    }
                    other => panic!("unexpected observation completion: {other:?}"),
                }
            }
            assert!(completed, "bounded {} observation must finish", input[0]);
        }
    }
}

#[test]
fn ordinary_visible_seven_per_solution_probability_is_explicitly_unavailable_not_oracle() {
    let context = context();
    for input in inputs() {
        let source = command(
            &input,
            "",
            "--count unique --queue-knowledge visible-7 --solution-probabilities",
        );
        let request = CliCommandParser::parse(&source)
            .unwrap()
            .to_app_request()
            .unwrap();
        let response = context.run(request);
        assert_observation(&response);
        let core = response.render_model().unwrap().core_result().unwrap();
        assert!(core.solution_probabilities().is_empty());
        assert_eq!(
            core.bool_field("solution_probabilities_requested"),
            Some(true)
        );
        assert_eq!(
            core.bool_field("solution_probability_complete"),
            Some(false)
        );
        assert_eq!(
            core.field("solution_probability_basis"),
            Some("unsupported-under-visible-seven-policy")
        );
        assert_eq!(
            core.field("solution_probability_incomplete_reason"),
            Some("per-solution-policy-language-not-materialized")
        );
    }
}

#[test]
fn full_height_observation_does_not_silently_rebind_oracle_only_typed_products() {
    let context = context();
    for input in inputs() {
        for product in [
            "minimals",
            "chance",
            "failed-queue",
            "path",
            "score",
            "score-minimals",
        ] {
            let source = command(&input, product, "--queue-knowledge visible-7");
            // A product may reject the unsupported policy at its own ingress,
            // or later at the shared producer contract, but may not drop it.
            let Ok(parsed) = CliCommandParser::parse(&source) else {
                continue;
            };
            let Ok(request) = parsed.to_app_request() else {
                continue;
            };
            let query = match request.command() {
                AppCommand::Scenario(scenario) => scenario.query(),
                AppCommand::Percent(percent) if product == "failed-queue" => percent
                    .query()
                    .expect("the failed-queue command must retain its scenario query"),
                _ => panic!("full-height input must not be converted to compact oracle input"),
            };
            assert_eq!(
                query.queue_observation_policy(),
                QueueObservationPolicy::VisibleSeven
            );
            assert_eq!(
                query.initial_board().visible_height(),
                input[1].parse::<u16>().unwrap()
            );
            assert_eq!(
                query.remaining_queue().as_fixed_sequence().unwrap().len(),
                input[3].parse::<usize>().unwrap()
            );
            assert_ne!(
                context.run(request).status(),
                AppStatus::Success,
                "{source}"
            );
        }
    }
}
