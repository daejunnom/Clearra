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
    let commands: [(&[&str], &str, &str); 6] = [
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
            &["--verbose", "setup", "--queue", "IOTSZJL", "--fixed"],
            "kind: setup",
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
