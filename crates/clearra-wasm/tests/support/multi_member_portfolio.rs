//! SRP rationale: create a real public pinned-PC portfolio exceeding the UI's
//! 100-member render page, without fabricating candidates or changing page size.

use clearra_app::{
    decode_ctk3_exact, encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece,
};
use clearra_core_domain::{piece::piece_kind::PieceKind, solution::NormalizedTilingSolutionKey};
use clearra_host_contract::{AppStatus, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES};
use clearra_wasm::{
    serialize_coverage_portfolio_page, serialize_distributed_final_events,
    CoveragePortfolioPageStore, ProductPageSourceOwner, WasmCommandRuntime, WasmHostCapabilities,
};
use serde_json::{json, Value};

const QUERY: &str = "--lines 4 --board-mask 0x3c0f03c0f --height 4 --pieces 6 \
    --patterns P7 --hold empty --rule srs-plus --backend cpu --workers 1 --no-tablebase";
const EXPECTED_MEMBERS: usize = 246;

fn selected_document(keys: &[String]) -> String {
    let pages = keys
        .iter()
        .map(|key| {
            let identity = NormalizedTilingSolutionKey::parse_canonical(key)
                .unwrap()
                .standard_board64_identity()
                .unwrap();
            let mut cells = vec![Ctk3Color::Empty; 40];
            for (cell, color) in cells.iter_mut().enumerate() {
                if identity.initial_board_mask() & (1_u64 << cell) != 0 {
                    *color = Ctk3Color::Gray;
                }
            }
            for index in 0..identity.placement_count() {
                let placement = identity.placement(index).unwrap();
                let piece = match placement.piece() {
                    PieceKind::I => Ctk3Piece::I,
                    PieceKind::O => Ctk3Piece::O,
                    PieceKind::T => Ctk3Piece::T,
                    PieceKind::S => Ctk3Piece::S,
                    PieceKind::Z => Ctk3Piece::Z,
                    PieceKind::J => Ctk3Piece::J,
                    PieceKind::L => Ctk3Piece::L,
                };
                assert_eq!(placement.cells_mask() >> 40, 0);
                for (cell, color) in cells.iter_mut().enumerate() {
                    if placement.cells_mask() & (1_u64 << cell) != 0 {
                        assert_eq!(*color, Ctk3Color::Empty);
                        *color = Ctk3Color::Piece(piece);
                    }
                }
            }
            Ctk3Page::new(4, cells)
        })
        .collect();
    encode_ctk3_compact(&Ctk3Document::new(10, pages)).unwrap()
}

pub(super) fn capture_multi_member_cases() -> Vec<Value> {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    eprintln!("real_multi_member_source=started");
    let source = runtime
        .run_command_text(&format!(
            "clearra pc {QUERY} --objective unique --count unique \
            --no-legal-board --no-conditioned-reachability"
        ))
        .unwrap();
    assert_eq!(source.app_response().status(), AppStatus::Success);
    let report = source.search_report().unwrap();
    assert!(report.solution_keys_complete && report.count_complete);
    let keys = report.normalized_solution_keys.clone();
    assert_eq!(keys.len(), EXPECTED_MEMBERS);
    eprintln!("real_multi_member_source=complete members={EXPECTED_MEMBERS}");
    let document = selected_document(&keys);
    assert_eq!(
        decode_ctk3_exact(&document).unwrap().pages.len(),
        keys.len()
    );
    let source_hash = &report.normalized_solution_set_hash;
    [(false, false), (true, false), (false, true), (true, true)]
        .into_iter()
        .map(|(legal, conditioned)| {
            eprintln!("real_multi_member_portfolio=started legal={legal} conditioned={conditioned}");
            let command = format!(
                "clearra pc pinned-minimals {QUERY} --required-format ctk3 \
                --required-document {document} --expected-source-set-hash {source_hash} {} {}",
                if legal { "--legal-board" } else { "--no-legal-board" },
                if conditioned { "--conditioned-reachability" } else { "--no-conditioned-reachability" }
            );
            let execution = runtime.run_command_text(&command).unwrap();
            assert_eq!(execution.app_response().status(), AppStatus::Success);
            let ProductPageSourceOwner::CoveragePortfolio(set) =
                execution.product_page_source_owner().unwrap()
            else {
                panic!("public pinned-PC product must retain its exact portfolio source");
            };
            assert_eq!(set.candidates().len(), EXPECTED_MEMBERS);
            assert_eq!(set.optimal_cardinality(), EXPECTED_MEMBERS);
            let store = CoveragePortfolioPageStore::new(set.clone()).unwrap();
            let mut member_pages = Vec::new();
            let mut actual_keys = Vec::new();
            for page_number in 1..=3 {
                let members = store.member_page(1, page_number).unwrap();
                assert_eq!(members.total_member_pages(), 3);
                assert_eq!(members.members().len(), if page_number == 3 { 46 } else { 100 });
                actual_keys.extend(members.members().iter().map(|member| member.normalized_key().to_owned()));
                member_pages.push(serde_json::from_str::<Value>(
                    &serialize_coverage_portfolio_page(&store, 1, page_number).unwrap(),
                ).unwrap());
            }
            assert_eq!(actual_keys, keys);
            let final_wire: Value = serde_json::from_str(
                &serialize_distributed_final_events(74, &execution).unwrap(),
            ).unwrap();
            let final_response = final_wire.as_array().unwrap().iter()
                .find(|event| event["event"] == "final_response").unwrap()["response"].clone();
            let kind = execution.app_response().product_result_payload().unwrap().result_kind();
            let artifact = store.bounded_solution_set_artifact_payload(
                1, kind, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
            ).expect("whole real 246-member portfolio must be exportable");
            assert_eq!(artifact.solution_count() as usize, EXPECTED_MEMBERS);
            let ctk = artifact.formats().iter().find(|format| format.format() == "ctk3").unwrap();
            assert!(ctk.available());
            assert_eq!(decode_ctk3_exact(ctk.document().unwrap()).unwrap().pages.len(), EXPECTED_MEMBERS);
            eprintln!("real_multi_member_portfolio=complete legal={legal} conditioned={conditioned}");
            json!({ "legal": legal, "conditioned": conditioned, "candidate_keys": keys,
                "final_response": final_response, "member_pages": member_pages, "artifact": artifact })
        })
        .collect()
}
