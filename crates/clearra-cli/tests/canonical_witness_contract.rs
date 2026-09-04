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
