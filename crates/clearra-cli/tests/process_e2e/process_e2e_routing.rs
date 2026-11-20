use super::*;

#[test]
fn process_e2e_mvp2_cli_commands_are_routed() {
    let rules = clearra()
        .args(["rules", "inspect", "--profile", "srs"])
        .output()
        .expect("clearra-cli process runs");
    let scoring = clearra()
        .args(["scoring", "inspect", "--profile", "tetrio"])
        .output()
        .expect("clearra-cli process runs");
    let percent = clearra()
        .args(["percent", "--queue", "IOT", "--fixed", "--min-len", "3"])
        .output()
        .expect("clearra-cli process runs");
    let path = clearra()
        .args([
            "path",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
        ])
        .output()
        .expect("clearra-cli process runs");

    for output in [&rules, &scoring, &percent, &path] {
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
    }
    assert!(String::from_utf8(rules.stdout)
        .expect("rules stdout utf8")
        .contains("effective_kick_model: srs-90"));
    let scoring_contract = String::from_utf8(scoring.stdout).expect("scoring stdout utf8");
    assert!(scoring_contract.contains("attack_model: tetrio"));
    assert!(scoring_contract.contains("accuracy_level: basic-approximation"));
    assert!(scoring_contract.contains("profile_specific_exact: false"));
    assert!(String::from_utf8(percent.stdout)
        .expect("percent stdout utf8")
        .contains("kind: percent"));
    assert!(String::from_utf8(path.stdout)
        .expect("path stdout utf8")
        .contains("kind: path"));
}

#[test]
fn process_e2e_m18_cli_commands_use_search_problem_executor_route() {
    // Opening, scenario, Path and Percent are covered by their command-specific
    // process tests. Keep the remaining legacy route under executable coverage
    // with its real compiled template and complete Rust build union.
    let cover = run_json(&["cover", "--template", "basic"]);
    assert_eq!(cover["kind"], "build_coverage");
    let summary = &cover["summary"];
    assert_eq!(summary["coverage_scope"], "requested-materialized-ordinals");
    assert_eq!(summary["count_complete"], true);
    assert_eq!(summary["probability_complete"], true);
}

const SETUP_REMAINING: &str = "IOT";
const SETUP_QB: &str = "[SZJL]!";

#[test]
fn cycle_seven_setup_condition_keeps_its_canonical_universe_and_qb_denominator() {
    // Cycle-seven QB fixture: the canonical [IOT]!P7 universe is intersected
    // with the explicit unordered QB expression [SZJL]!, leaving 864
    // conditioned queue words and one ten-piece multiset. Plain `SZJL` now
    // means that exact order; spelling the group preserves this routing
    // fixture's historical unordered domain without reviving ambiguity.
    let pieces = |text: &str| {
        text.chars()
            .map(|piece| {
                clearra_core_domain::piece::piece_kind::PieceKind::from_ascii(piece)
                    .expect("fixture tetromino")
            })
            .collect()
    };
    let qb = clearra_supply::queue::queue_pattern_expression::QueuePatternExpression::parse(
        SETUP_QB, 24,
    )
    .expect("fixture QB expression");
    let fixture = clearra_problem::SetupSearchQuery::default()
        .with_remaining_pieces(pieces(SETUP_REMAINING))
        .with_queue_based_pattern_expression(qb)
        .with_max_setup_pieces(1);
    let conditions = clearra_problem::compile_setup_search_conditions(&fixture)
        .expect("the small fixture is a valid complete Setup request");
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].pattern_expression(), "[IOT]!P7");
    assert_eq!(conditions[0].probability_pattern_count(), 864);
    assert_eq!(
        conditions[0]
            .problem()
            .piece_source()
            .materialized_universe()
            .expect("compiled Setup universe")
            .pattern_count(),
        30_240
    );
}

#[test]
fn process_e2e_setup_alias_uses_the_same_inventory_validation() {
    let mut reference = None;
    for command in ["setup-finder", "setup"] {
        let output = clearra()
            .args([
                "--format",
                "json",
                command,
                "--remaining",
                "OOII",
                "--mode",
                "qb",
                "--qb",
                "JLS",
            ])
            .output()
            .expect("setup process");
        assert_eq!(output.status.code(), Some(2), "{command}: {output:?}");
        assert!(output.stderr.is_empty());
        let json: Value = serde_json::from_slice(&output.stdout).expect("diagnostic JSON");
        assert_eq!(json["kind"], "diagnostic");
        let diagnostics = json["summary"]["diagnostics"]
            .as_array()
            .expect("Setup diagnostics");
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == "E_SETUP_QUERY_INVALID"
                && diagnostic["evidence"]["reason"] == "remaining_piece_duplicate_inventory_invalid"
        }));
        if let Some(reference) = &reference {
            assert_eq!(diagnostics, reference, "{command}");
        } else {
            reference = Some(diagnostics.clone());
        }
    }
    let obsolete = clearra()
        .args(["setup", "--remaining", "TI", "--fixed"])
        .output()
        .unwrap();
    assert_eq!(obsolete.status.code(), Some(2));
    assert!(obsolete.stdout.is_empty());
    assert!(String::from_utf8_lossy(&obsolete.stderr).contains("E_CLI_UNKNOWN_OPTION"));
}

#[test]
#[ignore = "full empty-4L exact Setup acceptance; run outside the fast process smoke"]
fn process_e2e_setup_uses_redesigned_ranked_family_and_preserves_legacy_alias() {
    let run_setup = |command| {
        run_json(&[
            command,
            "--remaining",
            SETUP_REMAINING,
            "--mode",
            "qb",
            "--qb",
            SETUP_QB,
            "--max-setup-pieces",
            "1",
            "--workers",
            "1",
        ])
    };
    let canonical = run_setup("setup-finder");
    let legacy = run_setup("setup");
    assert_eq!(canonical["kind"], "setup-joint-ranking.v2");
    assert_eq!(canonical["kind"], legacy["kind"]);
    let summary = &canonical["summary"];
    assert_eq!(summary, &legacy["summary"]);
    assert_eq!(summary["capability_id"], "setup.joint");
    assert_eq!(summary["result_contract"], "setup-joint-ranking.v2");
    assert_eq!(summary["payload_kind"], "setup-ranked-family");
    assert_eq!(summary["ordering"], "joint-probability-descending");
    assert_eq!(summary["resolved_length_preference"], "longer");
    for identity in [
        "query_identity_sha256",
        "supply_identity_sha256",
        "universe_identity_sha256",
    ] {
        let digest = summary[identity].as_str().expect("Setup SHA-256 identity");
        assert_eq!(digest.len(), 64, "{identity}");
        assert!(
            digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{identity}"
        );
    }
    let candidates = summary["candidates"].as_array().expect("ranked candidates");
    let candidate_count = summary["candidate_count"]
        .as_str()
        .expect("Setup decimal candidate count")
        .parse::<usize>()
        .expect("valid Setup candidate count");
    assert!(
        !candidates.is_empty(),
        "cycle seven fixture has 4L finishes"
    );
    assert_eq!(candidate_count, candidates.len());
    for candidate in candidates {
        assert_eq!(candidate["condition_id"], "hold-empty");
        assert!(candidate["candidate_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()));
        assert!(candidate["setup_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()));
    }
}
