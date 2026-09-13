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
        .args(["percent", "--queue", "IOT", "--min-len", "5"])
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
    let commands: [(&[&str], &str, &str); 5] = [
        (
            &[
                "--verbose",
                "pc",
                "--lines",
                "2",
                "--queue",
                "IJLOO",
                "--fixed",
                "--no-hold",
            ],
            "kind: pc",
            "route: search-problem-core-executor",
        ),
        (
            &[
                "--verbose",
                "pc-scenario",
                "--fixture",
                "tests/fixtures/pc/example.json",
            ],
            "kind: pc-scenario",
            "route: search-problem-core-executor",
        ),
        (
            &[
                "--verbose",
                "path",
                "--lines",
                "2",
                "--queue",
                "IIOOO",
                "--fixed",
                "--no-hold",
            ],
            "kind: path",
            "status: path-rendered",
        ),
        (
            &["--verbose", "percent", "--queue", "IOT", "--min-len", "5"],
            "kind: percent",
            "route: search-problem-core-executor",
        ),
        (
            &["--verbose", "cover", "--template", "basic"],
            "kind: build_coverage",
            "route: search-problem-core-executor",
        ),
    ];

    for (args, kind_marker, route_marker) in commands {
        let output = clearra()
            .args(args)
            .output()
            .expect("clearra-cli process runs");
        assert!(
            output.status.success(),
            "command failed: {:?}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "stderr for {args:?} was not empty"
        );
        let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
        assert!(
            stdout.contains(kind_marker),
            "missing {kind_marker}: {stdout}"
        );
        assert!(
            stdout.contains(route_marker),
            "missing {route_marker}: {stdout}"
        );
    }
}

#[test]
fn process_e2e_setup_uses_redesigned_ranked_family_and_preserves_legacy_alias() {
    // Cycle-seven QB fixture: [IOT]![SZJL]![^SZJL]! has 864 queue words
    // and one ten-piece multiset. TI / OS generated 20,160 eleven-piece
    // words plus hold-slack alternatives: --max-setup-pieces bounds only the
    // displayed prefix, not the complete 4L geometry behind its ranking.
    // Keep real completed family/alias checks without that unrelated search.
    const REMAINING: &str = "IOT";
    const OBSERVED: &str = "SZJL";
    let pieces = |text: &str| {
        text.chars()
            .map(|piece| {
                clearra_core_domain::piece::piece_kind::PieceKind::from_ascii(piece)
                    .expect("fixture tetromino")
            })
            .collect()
    };
    let fixture = clearra_problem::SetupSearchQuery::default()
        .with_remaining_pieces(pieces(REMAINING))
        .with_queue_based_pieces(pieces(OBSERVED))
        .with_max_setup_pieces(1);
    let conditions = clearra_problem::compile_setup_search_conditions(&fixture)
        .expect("the small fixture is a valid complete Setup request");
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].pattern_expression(), "[IOT]![SZJL]![^SZJL]!");
    assert_eq!(
        conditions[0]
            .problem()
            .piece_source()
            .materialized_universe()
            .expect("compiled Setup universe")
            .pattern_count(),
        864
    );
    for command in ["setup-finder", "setup"] {
        let output = clearra()
            .args([
                "--format",
                "json",
                command,
                "--remaining",
                REMAINING,
                "--mode",
                "qb",
                "--qb",
                OBSERVED,
                "--max-setup-pieces",
                "1",
                "--workers",
                "1",
            ])
            .output()
            .expect("setup process");
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        for marker in [
            "\"capability_id\":\"setup.joint\"",
            "\"result_contract\":\"setup-joint-ranking.v2\"",
            "\"payload_kind\":\"setup-ranked-family\"",
        ] {
            assert!(
                stdout.contains(marker),
                "{command}: missing {marker}: {stdout}"
            );
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
