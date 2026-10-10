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

fn selected_i_drawing(input: &[&str]) -> String {
    let height = input[1].parse::<usize>().unwrap();
    let mask = input[2];
    let mut words = [0_u64; 4];
    for (index, word) in words.iter_mut().enumerate() {
        let end = mask.len() - index * 16;
        *word = u64::from_str_radix(&mask[end - 16..end], 16).unwrap();
    }
    let cells = (0..height * 10)
        .map(|index| {
            if words[index / 64] & (1_u64 << (index % 64)) != 0 {
                clearra_app::Ctk3Color::Gray
            } else {
                clearra_app::Ctk3Color::Piece(clearra_app::Ctk3Piece::I)
            }
        })
        .collect();
    clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
        10,
        vec![clearra_app::Ctk3Page::new(height, cells)],
    ))
    .unwrap()
}

#[test]
fn canonical_full_height_pinned_drawings_resolve_only_against_the_complete_minimum_source() {
    let context = context();
    for input in inputs() {
        let base = format!(
            "clearra pc minimals --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --no-tablebase --no-build-dependency-dag --backend cpu --workers 1",
            input[1], input[2], input[1], input[3], input[4],
        );
        let ordinary = context.run(
            CliCommandParser::parse(&base)
                .unwrap()
                .to_app_request()
                .unwrap(),
        );
        let expected = assert_minimum(&ordinary, &input);
        let source = ordinary
            .product_capability_result()
            .unwrap()
            .pc_minimum_cover_v2()
            .unwrap();
        let pinned_command = format!(
            "{} --required-format ctk3 --required-document {} --expected-source-set-hash {}",
            base.replacen("pc minimals", "pc pinned-minimals", 1),
            selected_i_drawing(&input),
            ordinary
                .render_model()
                .unwrap()
                .core_result()
                .unwrap()
                .field("actual_normalized_solution_set_hash")
                .unwrap(),
        );
        let request = CliCommandParser::parse(&pinned_command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let pinned = context.run(request.clone());
        assert_eq!(assert_minimum(&pinned, &input), expected);
        let report = pinned
            .product_capability_result()
            .unwrap()
            .pc_minimum_cover_v2()
            .unwrap();
        assert_eq!(
            report.source_solution_count(),
            source.source_solution_count()
        );
        assert_eq!(
            report.required_pattern_count(),
            source.required_pattern_count()
        );
        assert_eq!(report.portfolio_alternatives().pinned_candidate_ids(), &[1]);
        let actual_source_hash = ordinary
            .render_model()
            .unwrap()
            .core_result()
            .unwrap()
            .field("actual_normalized_solution_set_hash")
            .unwrap();
        let stale_hash = if actual_source_hash == "cts1:0000000000000000" {
            "cts1:ffffffffffffffff"
        } else {
            "cts1:0000000000000000"
        };
        let stale_command = pinned_command.replace(actual_source_hash, stale_hash);
        let stale = context.run(
            CliCommandParser::parse(&stale_command)
                .unwrap()
                .to_app_request()
                .unwrap(),
        );
        assert_ne!(stale.status(), AppStatus::Success);
        assert!(
            format!("{stale:?}")
                .contains("pc pinned minimals source set changed; select drawings again"),
            "{stale:?}"
        );
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
                other => panic!("unexpected full-height pinned finalizer: {other:?}"),
            }
        }
        assert!(completed, "{} pinned minimum must finish", input[0]);
    }
}

#[test]
fn full_height_pinned_drawing_rejects_touching_same_kind_placement_ambiguity() {
    // Four I pieces can partition the same 4x4 colors either horizontally or
    // vertically. The fifth I keeps the target at eight rows. Colors alone
    // cannot select one of those distinct exact source identities.
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
    let mask = format!(
        "{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    );
    let input = ["ambiguous-eight", "8", mask.as_str(), "5", "IIIII"];
    let command = format!(
        "clearra pc pinned-minimals --lines 8 --height 8 --board-mask 0x{mask} --pieces 5 --queue IIIII --no-hold --rule srs-plus --backend cpu --workers 1 --no-tablebase --no-build-dependency-dag --required-format ctk3 --required-document {}",
        selected_i_drawing(&input),
    );
    let response = context().run(
        CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap(),
    );
    assert_ne!(response.status(), AppStatus::Success);
    assert!(
        format!("{response:?}").contains("pc pinned drawing matches multiple normalized solutions"),
        "{response:?}"
    );
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
fn canonical_full_height_gui_and_discord_replay_commands_share_one_lazy_source() {
    let context = context();
    for input in inputs() {
        let command = format!(
            "clearra pc path --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --no-tablebase --no-build-dependency-dag --backend auto --allow-backend-fallback --workers 1 --gpu-warmup",
            input[1], input[2], input[1], input[3], input[4],
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let direct = context.run(request.clone());
        assert_eq!(
            direct.status(),
            AppStatus::Success,
            "{}: {direct:?}",
            input[0]
        );
        let expected = direct
            .product_capability_result()
            .unwrap()
            .pc_path_family_v2()
            .unwrap();
        assert!(expected.completeness().complete());
        assert_eq!(expected.page_source().unwrap().geometry_count(), 1);
        let payload = direct
            .product_capability_result()
            .unwrap()
            .public_result_payload()
            .unwrap();
        let ProductResultPayloadContent::PcPathFamily(family) = payload.content() else {
            panic!("keep the existing replay family, not a second full-height presenter");
        };
        assert!(family.complete());
        let witness = family.canonical_witness().unwrap();
        assert_eq!(witness.steps().len().to_string(), input[3]);
        assert_eq!(
            witness.steps()[0].board_before_mask(),
            format!("0x{}", input[2])
        );
        assert_eq!(
            witness
                .steps()
                .last()
                .unwrap()
                .board_after_line_clear_mask(),
            format!("0x{}", "0".repeat(64))
        );
        assert!(witness.normalized_trace_key().starts_with("trk2:"));
        assert!(direct.public_page_source_owner().is_some());
        let mut execution = context.start_cooperative_execution(request);
        let mut completed = None;
        for _ in 0..4096 {
            match execution.advance(256, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(response) => {
                    completed = Some(response);
                    break;
                }
                other => panic!("unexpected full-height replay finalizer: {other:?}"),
            }
        }
        let cooperative = completed.expect("bounded few-piece GUI replay");
        assert_eq!(cooperative.status(), AppStatus::Success, "{cooperative:?}");
        let actual = cooperative
            .product_capability_result()
            .unwrap()
            .pc_path_family_v2()
            .unwrap();
        assert_eq!(actual.witness_count(), expected.witness_count());
        assert_eq!(actual.witnesses(), expected.witnesses());
        assert_eq!(
            actual.page_source().unwrap().identity_sha256(),
            expected.page_source().unwrap().identity_sha256()
        );
    }
}

#[test]
fn canonical_full_height_scores_keep_full_fields_and_share_the_gui_finalizer() {
    let context = context();
    for input in inputs() {
        for product in ["score", "score-minimals", "score-finder"] {
            let score_options = if product == "score-finder" {
                "--initial-b2b 0"
            } else {
                "--score-profile guideline --spin-profile t-spins --initial-b2b 0"
            };
            let command = format!(
                "clearra pc {product} --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --workers 1 {score_options}",
                input[1], input[2], input[1], input[3], input[4],
            );
            let request = CliCommandParser::parse(&command)
                .unwrap()
                .to_app_request()
                .unwrap();
            let mut execution = context.start_cooperative_execution(request.clone());
            let mut cooperative = None;
            for _ in 0..4096 {
                match execution.advance(256, &ExecutionControl::default()) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        cooperative = Some(response);
                        break;
                    }
                    other => panic!("unexpected full-height score finalizer: {other:?}"),
                }
            }
            let cooperative = cooperative.expect("bounded few-piece score request");
            let direct = context.run(request);
            for response in [&direct, &cooperative] {
                assert_eq!(
                    response.status(),
                    AppStatus::Success,
                    "{} {product}: {response:?}",
                    input[0]
                );
                let result = response.product_capability_result().unwrap();
                let keys: Vec<String> = if product == "score-minimals" {
                    let report = result.pc_score_portfolio_v2().unwrap();
                    assert!(report.completeness().complete());
                    assert_eq!(report.selected_score_candidate_ids(), &[1]);
                    report.selected_solution_keys().to_vec()
                } else {
                    let report = result.pc_score_summary_v2().unwrap();
                    assert!(report.completeness().complete());
                    assert_eq!(report.solution_field_count(), 1);
                    assert!(report.best_score().is_some());
                    assert_eq!(report.pattern_optimal_count(), 1);
                    assert_eq!(report.canonical_winner().unwrap().candidate_id(), 1);
                    report
                        .solution_field_averages()
                        .iter()
                        .map(|field| {
                            assert!(field.field_identity().standard_board64_identity().is_none());
                            field.normalized_field_key().to_string()
                        })
                        .collect()
                };
                assert_eq!(keys.len(), 1);
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
            }
            let direct_core = direct.render_model().unwrap().core_result().unwrap();
            let gui_core = cooperative.render_model().unwrap().core_result().unwrap();
            for field in [
                "score_best_score",
                "score_field_average_score",
                "score_covered_probability",
                "score_initial_b2b",
            ] {
                assert_eq!(
                    direct_core.field(field),
                    gui_core.field(field),
                    "{} {product} {field}",
                    input[0]
                );
            }
        }
    }
}

#[test]
fn canonical_full_height_failed_queue_uses_the_same_direct_and_gui_completion() {
    let context = context();
    for input in inputs() {
        let command = format!(
            "clearra pc failed-queue --lines {} --board-mask 0x{} --height {} --pieces {} --queue {} --no-hold --rule srs-plus --backend cpu --workers 1 --failed-count 1",
            input[1], input[2], input[1], input[3], input[4],
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let direct = context.run(request.clone());
        assert_eq!(
            direct.status(),
            AppStatus::Success,
            "{}: {direct:?}",
            input[0]
        );
        let expected = direct
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        assert_eq!(expected.success_pattern_count(), 1);
        assert_eq!(expected.failed_pattern_count(), 0);
        assert!(expected.examples().is_empty());
        let mut execution = context.start_cooperative_execution(request);
        let mut response = None;
        for _ in 0..4096 {
            match execution.advance(1, &ExecutionControl::default()) {
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                CooperativeAppAdvance::Completed(completed) => {
                    response = Some(completed);
                    break;
                }
                other => panic!("unexpected GUI failed-queue outcome: {other:?}"),
            }
        }
        let response = response.expect("bounded full-height failed queue");
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "{}: {response:?}",
            input[0]
        );
        let actual = response
            .product_capability_result()
            .unwrap()
            .pc_failed_queue_v2()
            .unwrap();
        assert_eq!(actual.problem_id(), expected.problem_id());
        assert_eq!(actual.pattern_universe_id(), expected.pattern_universe_id());
        assert_eq!(
            actual.failed_probability_bits(),
            expected.failed_probability_bits()
        );
        assert_eq!(
            actual.success_pattern_count(),
            expected.success_pattern_count()
        );
        assert_eq!(
            actual.failed_pattern_count(),
            expected.failed_pattern_count()
        );
        assert_eq!(actual.examples(), expected.examples());
    }
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
