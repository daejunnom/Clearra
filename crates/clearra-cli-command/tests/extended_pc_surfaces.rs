//! Canonical GUI/Discord lowering, actual App execution and full-height output.
//! These few-piece fields are not a sixty-piece performance qualification.
use clearra_app::{
    AppContext, AppCoreExecutorService, AppResponse, AppServices, AppStatus, CooperativeAppAdvance,
};
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    solution::ExtendedTilingSolutionKey,
};
use clearra_host_contract::ProductResultPayloadContent;

const INPUTS: &str =
    include_str!("../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv");

fn inputs() -> impl Iterator<Item = Vec<&'static str>> {
    INPUTS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

#[test]
fn extended_minimum_publishes_first_canonical_set_and_keeps_equal_minima_lazy() {
    let mut holes = Board256Mask::EMPTY;
    for row in 0..4 {
        for column in 0..4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
    }
    for row in 4..8 {
        holes = holes.union(Board256Mask::singleton(row * 10 + 5).unwrap());
    }
    let words = Board256Mask::all_cells(80).unwrap().without(holes).words();
    let command = format!(
        "clearra pc minimals --lines 8 --height 8 --pieces 5 --board-mask 0x{:016x}{:016x}{:016x}{:016x} --queue IIIII --no-hold --rule srs-plus --no-tablebase --no-build-dependency-dag --backend cpu --workers 1",
        words[3], words[2], words[1], words[0],
    );
    let request = CliCommandParser::parse(&command)
        .unwrap()
        .to_app_request()
        .unwrap();
    let response = context().run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let minimum = response
        .product_capability_result()
        .unwrap()
        .pc_minimum_cover_v2()
        .unwrap();
    assert!(minimum.completeness().complete());
    assert!(minimum.source_solution_count() >= 2);
    assert_eq!(minimum.selected_solution_count(), 1);
    assert_eq!(minimum.canonical_candidate().unwrap().0, 1);
    let set = minimum.portfolio_alternatives();
    assert_eq!(set.known_alternative_count_decimal(), "1");
    assert!(
        !set.enumeration_complete(),
        "do not eagerly enumerate all equal minimum sets"
    );
    let mut store = set.open_store().unwrap();
    let next = store.next_page(65536, &mut || false).unwrap();
    let page = next
        .page()
        .expect("the existing lazy cursor must expose the second exact minimum");
    assert_eq!(page.optimal_cardinality(), 1);
    assert_ne!(
        page.portfolio().candidate_ids(),
        set.canonical_page().portfolio().candidate_ids()
    );
}

fn context() -> AppContext {
    AppContext::new(AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()))
}

#[test]
fn canonical_full_height_chance_preserves_four_word_authority_and_exact_probability() {
    let context = context();
    for input in inputs() {
        let command = format!(
            "clearra pc chance --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --no-tablebase --no-build-dependency-dag --backend cpu --workers 1",
            input[1], input[2], input[1], input[3], input[4]
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let assert_report = |response: &AppResponse| {
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "{}: {response:?}",
                input[0]
            );
            let report = response
                .product_capability_result()
                .unwrap()
                .pc_probability_v2()
                .unwrap();
            assert!(report.completeness().complete());
            assert_eq!(report.coverage_row_count(), 1);
            assert_eq!(report.covered_pattern_count(), 1);
            assert_eq!(report.weighted_probability(), "1");
            let words = report.compiled_board_occupied_words();
            assert_eq!(
                format!(
                    "{:016x}{:016x}{:016x}{:016x}",
                    words[3], words[2], words[1], words[0]
                ),
                input[2]
            );
            assert!(
                report.compiled_board_occupied_mask().is_none(),
                "do not silently truncate a tall input"
            );
            assert_eq!(report.compiled_board_visible_height().to_string(), input[1]);
            assert!(report.compiled_search_height() >= report.compiled_board_visible_height());
        };
        assert_report(&context.run(request.clone()));
        let mut execution = context.start_cooperative_execution(request);
        let mut completed = false;
        for _ in 0..4096 {
            match execution.advance(256, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(response) => {
                    assert_report(&response);
                    completed = true;
                    break;
                }
                other => panic!("unexpected bounded GUI chance outcome: {other:?}"),
            }
        }
        assert!(completed, "{} must complete", input[0]);
    }
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

fn assert_minimum(response: &AppResponse, input: &[&str]) -> Vec<String> {
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "{}: {response:?}",
        input[0]
    );
    let minimum = response
        .product_capability_result()
        .unwrap()
        .pc_minimum_cover_v2()
        .unwrap();
    assert!(minimum.completeness().complete());
    assert_eq!(minimum.selected_solution_count(), 1);
    assert_eq!(minimum.source_solution_count(), 1);
    assert_eq!(minimum.required_pattern_count(), 1);
    assert_eq!(minimum.canonical_candidate().unwrap().0, 1);
    let identity =
        ExtendedTilingSolutionKey::parse_canonical(&minimum.selected_solution_keys()[0]).unwrap();
    assert_eq!(identity.height().to_string(), input[1]);
    assert_eq!(identity.placement_count().to_string(), input[3]);
    let payload = response
        .product_capability_result()
        .unwrap()
        .public_result_payload()
        .expect("the existing portfolio page/copy contract must be published");
    let ProductResultPayloadContent::CoveragePortfolio(page) = payload.content() else {
        panic!("minimum output must remain the existing portfolio, not a second full family");
    };
    assert_eq!(page.members().len(), 1);
    assert_eq!(
        page.members()[0].normalized_solution_key(),
        minimum.selected_solution_keys()[0]
    );
    assert_eq!(
        page.canonical_witness().unwrap().normalized_solution_key(),
        minimum.canonical_candidate().unwrap().1
    );
    assert!(
        response.public_page_source_owner().is_some(),
        "lazy page/copy must retain its exact source owner"
    );
    minimum.selected_solution_keys().to_vec()
}

#[test]
fn canonical_extended_gui_and_discord_minimum_commands_use_the_exact_common_reducer() {
    let context = context();
    for input in inputs() {
        let command = format!(
            "clearra pc minimals --lines {} --board-mask 0x{} --height {} --pieces {} --no-hold --queue {} --rule srs-plus --no-tablebase --no-build-dependency-dag --backend cpu --workers 1",
            input[1], input[2], input[1], input[3], input[4],
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let direct = context.run(request.clone());
        let expected = assert_minimum(&direct, &input);
        let mut execution = context.start_cooperative_execution(request);
        let mut completed = false;
        for _ in 0..4096 {
            match execution.advance(256, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(response) => {
                    assert_eq!(assert_minimum(&response, &input), expected);
                    completed = true;
                    break;
                }
                other => panic!("unexpected bounded minimum outcome: {other:?}"),
            }
        }
        assert!(completed, "{} minimum must finish", input[0]);
    }
}
