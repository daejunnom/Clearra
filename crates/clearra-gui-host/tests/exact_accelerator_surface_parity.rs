//! SRP rationale: production Desktop argv and Web text must compile through
//! the CLI authority without changing products or either accelerator switch.
//! No executor, qualified asset, network, benchmark or legacy GUI DTO is used.

use clearra_app::AppContext;
use clearra_cli_command::CliCommandParser;
use clearra_gui_host::DesktopTauriCommandBridge;
use clearra_i18n::LanguageId;
use clearra_pc_graph::request::WorkerPolicy;
use serde_json::json;

const COMMANDS: &[&str] = &[
    "clearra pc --lines 4 --patterns P7P4 --objective all --count all",
    "clearra pc minimals --lines 2 --queue IIOOO --no-hold",
    "clearra pc score --lines 2 --queue IIOOO --no-hold",
    "clearra pc score-minimals --lines 2 --queue IIOOO --no-hold",
    "clearra pc path --lines 2 --queue IIOOO --no-hold",
    "clearra setup pc --remaining IOTSZJL",
    "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode all-solutions",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode complete-replay-paths",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode field-average-score",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode fixed-queue-maximum-score",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode highest-score-minimum-set",
    "clearra build-probability --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --no-mirror --result-mode failed-queues",
];

fn desktop_json(arguments: &[String]) -> String {
    json!({
        "app_request_model": "clearra-cli/CommandRequest",
        "command": "cli",
        "language": "ko",
        "arguments": arguments,
    })
    .to_string()
}

#[test]
fn v081_exact_accelerator_products_have_one_desktop_web_cli_request() {
    let desktop = DesktopTauriCommandBridge::new(AppContext::default());
    let allowed_workers =
        WorkerPolicy::default_worker_limit_for_hardware(WorkerPolicy::hardware_worker_limit());
    for base in COMMANDS {
        for profile in ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"] {
            for workers in [1, 2, 11] {
                for legal in [false, true] {
                    for conditioned in [false, true] {
                        let source = format!(
                            "{base} --rule {profile} --workers {workers} {} {}",
                            if legal {
                                "--legal-board"
                            } else {
                                "--no-legal-board"
                            },
                            if conditioned {
                                "--conditioned-reachability"
                            } else {
                                "--no-conditioned-reachability"
                            }
                        );
                        let args: Vec<String> =
                            source.split_whitespace().map(str::to_owned).collect();
                        // Keep native hardware admission authoritative. A small CI
                        // runner must reject the same request on every surface,
                        // not silently clamp it or simulate Desktop capacity.
                        if workers > allowed_workers {
                            assert!(CliCommandParser::parse_tokens(&args).is_err(), "{source}");
                            assert!(CliCommandParser::parse(&source).is_err(), "{source}");
                            assert!(
                                desktop.parse_app_request(&desktop_json(&args)).is_err(),
                                "{source}"
                            );
                            continue;
                        }
                        let direct = CliCommandParser::parse_tokens(&args)
                            .expect(&source)
                            .to_app_request()
                            .expect(&source)
                            .with_language(LanguageId::Ko);
                        let web = CliCommandParser::parse(&source)
                            .expect(&source)
                            .to_app_request()
                            .expect(&source)
                            .with_language(LanguageId::Ko);
                        let host = desktop
                            .parse_app_request(&desktop_json(&args))
                            .expect(&source);
                        assert_eq!(
                            direct.command().exact_accelerator_policy(),
                            Some((legal, conditioned)),
                            "{source}"
                        );
                        assert_eq!(host, direct, "Desktop argv drifted: {source}");
                        assert_eq!(web, direct, "Web text drifted: {source}");
                    }
                }
            }
        }
    }
}

#[test]
fn v081_exact_accelerator_desktop_rejects_ambiguous_flags_before_execution() {
    let desktop = DesktopTauriCommandBridge::new(AppContext::default());
    for base in COMMANDS {
        for suffix in [
            "--legal-board --no-legal-board",
            "--no-legal-board --legal-board",
            "--conditioned-reachability --no-conditioned-reachability",
            "--no-conditioned-reachability --conditioned-reachability",
            "--legal-board --legal-board",
            "--conditioned-reachability --conditioned-reachability",
        ] {
            let args: Vec<String> = format!("{base} {suffix}")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            assert!(
                desktop.parse_app_request(&desktop_json(&args)).is_err(),
                "{base} {suffix}"
            );
        }
    }
}
