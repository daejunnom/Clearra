//! SRP rationale: native compatibility grammar keeps one explicit selection
//! per accelerator without changing its other options or product routing.
use super::{CliParseError, CliParser};

#[test]
fn exact_accelerator_flags_reject_native_compatibility_duplicates_and_conflicts() {
    for base in [
        "clearra pc --lines 4",
        "clearra setup-finder --remaining IOTSZJL",
        "clearra setup --remaining IOTSZJL",
    ] {
        for flags in [
            ["--legal-board", "--legal-board"],
            ["--no-legal-board", "--no-legal-board"],
            ["--legal-board", "--no-legal-board"],
            ["--no-legal-board", "--legal-board"],
            ["--conditioned-reachability", "--conditioned-reachability"],
            [
                "--no-conditioned-reachability",
                "--no-conditioned-reachability",
            ],
            [
                "--conditioned-reachability",
                "--no-conditioned-reachability",
            ],
            [
                "--no-conditioned-reachability",
                "--conditioned-reachability",
            ],
        ] {
            let source = format!("{base} {} {}", flags[0], flags[1]);
            let args: Vec<String> = source.split_whitespace().map(str::to_owned).collect();
            assert!(
                matches!(
                    CliParser::parse(&args),
                    Err(CliParseError::InvalidValue { .. })
                ),
                "{source}"
            );
        }
    }
}
