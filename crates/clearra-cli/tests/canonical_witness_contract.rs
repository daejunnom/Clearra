#![cfg(feature = "wasm-cpu-runtime")]

use clearra_cli::{exit::ExitCode, run_with_args};
use serde_json::Value;

#[test]
fn canonical_pc_cli_json_supplies_every_single_witness_presenter_authority() {
    let path = run_json(&[
        "clearra",
        "--format",
        "json",
        "pc",
        "path",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--queue",
        "I",
    ]);
    assert_first_numeric_witness(
        &path["summary"],
        "canonical_witness",
        "witnesses",
        "candidate_id",
    );

    let minimals = run_json(&[
        "clearra",
        "--format",
        "json",
        "pc",
        "minimals",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--queue",
        "I",
    ]);
    assert_first_numeric_witness(
        &minimals["summary"],
        "canonical_witness",
        "members",
        "candidate_id",
    );

    let score_finder = run_json(&[
        "clearra",
        "--format",
        "json",
        "pc",
        "score-finder",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--queue",
        "I",
    ]);
    assert_first_numeric_witness(
        &score_finder["summary"],
        "score_pattern_canonical_winner",
        "score_pattern_winners",
        "candidate_id",
    );

    let best_save = run_json(&[
        "clearra",
        "--format",
        "json",
        "pc",
        "best-save",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--patterns",
        "I",
    ]);
    assert_eq!(
        best_save["summary"]["best_save_canonical_selection"],
        "smallest-canonical-candidate-id"
    );
    let winners = best_save["summary"]["best_save_winners"]
        .as_array()
        .expect("best-save winner family");
    let canonical = &best_save["summary"]["best_save_canonical_winner"];
    assert_eq!(Some(canonical), winners.first());
    let canonical_id = decimal_id(&canonical["group"]["canonical_candidate_id"]);
    assert!(winners
        .iter()
        .all(|winner| { decimal_id(&winner["group"]["canonical_candidate_id"]) >= canonical_id }));

    let saves = run_json(&[
        "clearra",
        "--format",
        "json",
        "pc",
        "saves",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--patterns",
        "I",
    ]);
    for group in saves["summary"]["save_groups"]
        .as_array()
        .expect("save groups")
    {
        let witnesses = group["witnesses"].as_array().expect("save witnesses");
        let canonical_id = decimal_id(&group["canonical_candidate_id"]);
        assert_eq!(canonical_id, decimal_id(&witnesses[0]["candidate_id"]));
        assert!(witnesses
            .iter()
            .all(|witness| decimal_id(&witness["candidate_id"]) >= canonical_id));
    }
}

fn run_json(arguments: &[&str]) -> Value {
    let output = run_with_args(arguments.iter().copied());
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    serde_json::from_str(output.stdout()).expect("typed CLI JSON")
}

#[test]
fn extended_pc_minimum_preserves_exact_empty_optimum_instead_of_runtime_unsupported() {
    let input = include_str!("../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv")
        .lines()
        .find(|line| line.starts_with("forced-7L\t"))
        .unwrap()
        .split('\t')
        .collect::<Vec<_>>();
    for profile in ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"] {
        for preserve_b2b in [false, true] {
            let command = format!(
                "clearra --format json pc minimals --lines 7 --height 7 --board-mask 0x{} --pieces {} --queue {} --no-hold --workers 1 --backend cpu --rule {profile} --no-tablebase --no-legal-board --no-conditioned-reachability {}",
                input[2], input[3], input[4], if preserve_b2b { "--preserve-b2b --spin-profile t-spins" } else { "" },
            );
            let arguments = command.split_whitespace().collect::<Vec<_>>();
            let value = run_json(&arguments);
            assert_eq!(value["kind"], "pc-minimum-cover.v2", "{profile}");
            let summary = &value["summary"];
            assert_eq!(
                summary["canonical_selection"],
                "smallest-canonical-candidate-id"
            );
            assert_eq!(summary["known_alternative_count"], "1");
            assert_eq!(summary["total_alternative_count"], "1");
            assert_eq!(summary["enumeration_complete"], true);
            assert_eq!(summary["alternative_index"], "1");
            assert_eq!(summary["member_page_number"], "1");
            assert_eq!(summary["total_member_pages"], "1");
            if preserve_b2b {
                // A mandatory 3-line clear means no preserving-B2B trace.
                // The exact cover of its *coverable* pattern subset is empty;
                // it is not a presenter-owned proof that the board is UNSAT.
                assert_eq!(summary["optimal_cardinality"], "0");
                assert_eq!(summary["members"], serde_json::json!([]));
                assert!(summary["canonical_witness"].is_null());
                assert!(summary.get("unsatisfiable").is_none());
            } else {
                assert_eq!(summary["optimal_cardinality"], "1");
                assert_eq!(summary["members"].as_array().unwrap().len(), 1);
                assert_eq!(summary["canonical_witness"], summary["members"][0]);
                let prefix = format!("ctk2|height=7|initial={}|placements=", input[2]);
                assert!(summary["members"][0]["normalized_solution_key"]
                    .as_str()
                    .unwrap()
                    .starts_with(&prefix));
            }
        }
    }
}

fn assert_first_numeric_witness(
    summary: &Value,
    canonical_key: &str,
    family_key: &str,
    candidate_key: &str,
) {
    assert_eq!(
        summary["canonical_selection"]
            .as_str()
            .or_else(|| { summary["score_pattern_canonical_selection"].as_str() }),
        Some("smallest-canonical-candidate-id")
    );
    let family = summary[family_key].as_array().expect("canonical family");
    let canonical = &summary[canonical_key];
    assert_eq!(Some(canonical), family.first());
    let canonical_id = decimal_id(&canonical[candidate_key]);
    assert!(family
        .iter()
        .all(|member| decimal_id(&member[candidate_key]) >= canonical_id));
}

fn decimal_id(value: &Value) -> u64 {
    value
        .as_str()
        .expect("decimal candidate ID string")
        .parse::<u64>()
        .expect("numeric candidate ID")
}
