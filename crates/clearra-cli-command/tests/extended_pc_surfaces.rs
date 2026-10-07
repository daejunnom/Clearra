//! Canonical GUI/Discord lowering, actual App execution and full-height output.
//! These few-piece fields are not a sixty-piece performance qualification.
use clearra_app::{
    AppContext, AppCoreExecutorService, AppResponse, AppServices, AppStatus, CooperativeAppAdvance,
};
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::{
    execution_cancellation::ExecutionControl, solution::ExtendedTilingSolutionKey,
};

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

fn assert_document(response: &AppResponse, input: &[&str]) -> Vec<String> {
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "{}: {response:?}",
        input[0]
    );
    let artifact = response
        .complete_solution_set_artifact()
        .unwrap_or_else(|| panic!("{}: missing complete full family: {response:?}", input[0]));
    assert_eq!(artifact.solution_count(), 1);
    let keys: Vec<String> = artifact
        .entries()
        .iter()
        .map(|entry| entry.key().to_owned())
        .collect();
    let identity = ExtendedTilingSolutionKey::parse_canonical(&keys[0]).unwrap();
    assert_eq!(identity.height().to_string(), input[1]);
    assert_eq!(identity.placement_count().to_string(), input[3]);
    let words = identity.initial_board().words();
    assert_eq!(
        format!(
            "{:016x}{:016x}{:016x}{:016x}",
            words[3], words[2], words[1], words[0]
        ),
        input[2]
    );
    let payload = response
        .bounded_solution_set_artifact_payload(1024 * 1024)
        .expect("the whole family must be copyable, not only a rendered slice");
    assert_eq!(payload.solution_count(), 1);
    let ctk3 = payload
        .formats()
        .iter()
        .find(|format| format.format() == "ctk3")
        .unwrap();
    assert!(ctk3.available());
    assert!(ctk3.document().unwrap().starts_with("ctk3_"));
    let fumen = payload
        .formats()
        .iter()
        .find(|format| format.format() == "fumen")
        .unwrap();
    if input[1] == "24" {
        assert!(!fumen.available());
        assert_eq!(fumen.unavailable_reason(), Some("fumen-height-unsupported"));
        assert!(
            fumen.document().is_none(),
            "never publish a truncated 23-row field"
        );
    } else {
        assert!(fumen.available());
    }
    keys
}

#[test]
fn canonical_extended_gui_commands_complete_with_the_same_direct_and_cooperative_documents() {
    let context = context();
    for input in inputs() {
        for product in ["pc", "pc tiling"] {
            // Exact options emitted by SolverWorkspace, including the automatic
            // backend choice but explicitly selected, unchanged worker count.
            let options = if product == "pc" {
                " --rule srs-plus --count unique --no-tablebase --no-build-dependency-dag --queue-knowledge oracle --no-legal-board --no-conditioned-reachability"
            } else {
                ""
            };
            let command = format!(
                "clearra {product} --lines {} --board-mask 0x{} --height {} --pieces {} --no-hold --queue {}{options} --backend auto --allow-backend-fallback --workers 1 --gpu-warmup",
                input[1], input[2], input[1], input[3], input[4]
            );
            let request = CliCommandParser::parse(&command)
                .unwrap()
                .to_app_request()
                .unwrap();
            let direct = context.run(request.clone());
            let expected = assert_document(&direct, &input);
            let mut execution = context.start_cooperative_execution(request);
            let mut completed = false;
            for _ in 0..4096 {
                match execution.advance(256, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        assert_eq!(assert_document(&response, &input), expected);
                        completed = true;
                        break;
                    }
                    other => panic!("unexpected bounded GUI outcome: {other:?}"),
                }
            }
            assert!(completed, "{} must finish", input[0]);
        }
    }
}

#[test]
fn canonical_extended_discord_tiling_uses_the_cli_app_product_and_the_whole_ctk3_family() {
    let context = context();
    for input in inputs() {
        let command = format!(
            "clearra pc tiling --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --backend cpu --workers 1",
            input[1], input[2], input[1], input[3], input[4]
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        assert_document(&context.run(request), &input);
    }
}
