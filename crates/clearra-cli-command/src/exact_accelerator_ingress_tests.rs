//! SRP rationale: accelerator selections cannot depend on option ordering or
//! the PC/Build/Setup command family. These tests lower requests, not searches.

use crate::{CliCommandErrorCode, CliCommandParser};

const COMMANDS: &[&str] = &[
    "clearra pc --lines 4 --backend cpu",
    "clearra pc --lines 2 --board-mask 0 --height 2 --pieces 5 --queue IIOOO --no-hold",
    "clearra pc minimals --lines 2 --queue IIOOO --no-hold",
    "clearra pc score --lines 2 --queue IIOOO --no-hold",
    "clearra pc score-minimals --lines 2 --queue IIOOO --no-hold",
    "clearra pc path --lines 2 --queue IIOOO --no-hold",
    "clearra setup --remaining IOTSZJL",
    "clearra setup pc --remaining IOTSZJL",
    "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror",
];

#[test]
fn exact_accelerator_ingress_preserves_every_independent_selection() {
    for command in COMMANDS {
        let default = CliCommandParser::parse(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"))
            .to_app_request()
            .expect("default App request");
        assert_eq!(
            default.command().exact_accelerator_policy(),
            Some((true, true))
        );
        for legal in [false, true] {
            for conditioned in [false, true] {
                let source = format!(
                    "{command} {} {}",
                    if legal {
                        "--legal-board"
                    } else {
                        "--no-legal-board"
                    },
                    if conditioned {
                        "--conditioned-reachability"
                    } else {
                        "--no-conditioned-reachability"
                    },
                );
                let parsed = CliCommandParser::parse(&source).expect(&source);
                let request = parsed.to_app_request().expect(&source);
                assert_eq!(
                    request.command().exact_accelerator_policy(),
                    Some((legal, conditioned)),
                    "{source}"
                );
                let tokens: Vec<_> = source.split_whitespace().map(str::to_owned).collect();
                assert_eq!(
                    CliCommandParser::parse_tokens(&tokens)
                        .expect(&source)
                        .to_app_request()
                        .expect(&source),
                    request,
                    "text and argv disagreed: {source}",
                );
            }
        }
    }
}

#[test]
fn exact_accelerator_ingress_rejects_duplicates_and_both_conflict_orders() {
    for command in COMMANDS {
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
            let source = format!("{command} {} {}", flags[0], flags[1]);
            assert_eq!(
                CliCommandParser::parse(&source).expect_err(&source).code(),
                CliCommandErrorCode::InvalidValue,
                "{source}",
            );
        }
    }
}

#[test]
fn exact_accelerator_ingress_keeps_tiling_inactive_option_rejection() {
    for command in [
        "clearra pc --lines 2 --queue IIOOO --tiling-only",
        "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --tiling-only",
    ] {
        for flag in ["--legal-board", "--conditioned-reachability"] {
            let source = format!("{command} {flag}");
            assert_eq!(CliCommandParser::parse(&source).expect_err(&source).code(), CliCommandErrorCode::InvalidValue);
        }
        let source = format!("{command} --no-legal-board --no-conditioned-reachability");
        assert!(CliCommandParser::parse(&source).is_ok(), "{source}");
    }
}
