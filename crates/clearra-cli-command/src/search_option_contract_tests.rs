use std::collections::{BTreeMap, BTreeSet};

use crate::CliCommandParser;

const CONTRACT: &str = include_str!("../../../tests/fixtures/contracts/search_option_contract.tsv");

#[derive(Clone, Copy, Debug)]
struct ContractRow<'a> {
    family: &'a str,
    option: &'a str,
    kind: &'a str,
    valid: &'a str,
    invalid: &'a str,
    discord_default: &'a str,
    native_default: &'a str,
    disposition: &'a str,
    discord_path: &'a str,
    exposure: &'a str,
    lowering: &'a str,
    reason: &'a str,
    dependencies: &'a str,
}

fn rows() -> Vec<ContractRow<'static>> {
    CONTRACT
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                columns.len(),
                13,
                "search option contract rows must have thirteen columns: {line}"
            );
            assert!(!columns[3].is_empty(), "valid representatives: {line}");
            ContractRow {
                family: columns[0],
                option: columns[1],
                kind: columns[2],
                valid: columns[3],
                invalid: columns[4],
                discord_default: columns[5],
                native_default: columns[6],
                disposition: columns[7],
                discord_path: columns[8],
                exposure: columns[9],
                lowering: columns[10],
                reason: columns[11],
                dependencies: columns[12],
            }
        })
        .collect()
}

#[test]
fn shared_contract_covers_each_semantic_family_without_duplicate_options() {
    let rows = rows();
    let families = rows.iter().map(|row| row.family).collect::<BTreeSet<_>>();
    assert_eq!(
        families,
        BTreeSet::from([
            "build",
            "finesse-score",
            "forward-damage",
            "forward-spin",
            "pc",
            "sequence",
            "sequence-dependencies",
            "setup",
            "spin-structure",
        ])
    );

    let mut options = BTreeSet::new();
    for row in rows {
        assert!(!row.kind.is_empty(), "{}.{} kind", row.family, row.option);
        assert!(
            !row.discord_default.is_empty() && !row.native_default.is_empty(),
            "{}.{} defaults",
            row.family,
            row.option
        );
        assert!(
            !row.reason.is_empty(),
            "{}.{} reason",
            row.family,
            row.option
        );
        assert!(
            !row.dependencies.is_empty(),
            "{}.{} dependencies",
            row.family,
            row.option
        );
        match row.disposition {
            "named" | "preset" => {
                assert_ne!(row.discord_path, "-", "{}.{} path", row.family, row.option);
                assert_ne!(row.exposure, "-", "{}.{} exposure", row.family, row.option);
                assert_ne!(
                    row.lowering, "none",
                    "{}.{} lowering",
                    row.family, row.option
                );
            }
            "excluded" => {
                assert_eq!(row.discord_path, "-", "{}.{} path", row.family, row.option);
                assert_eq!(
                    row.lowering, "none",
                    "{}.{} lowering",
                    row.family, row.option
                );
            }
            value => panic!("unknown disposition {value}: {}.{}", row.family, row.option),
        }
        assert!(
            options.insert((row.family, row.option)),
            "duplicate contract option {}.{}",
            row.family,
            row.option
        );
    }
}

#[test]
fn shared_contract_generates_every_single_and_ordered_pair_case() {
    let mut by_family = BTreeMap::<&str, Vec<ContractRow<'_>>>::new();
    for row in rows() {
        by_family.entry(row.family).or_default().push(row);
    }

    for (family, rows) in by_family {
        let mut cases = BTreeSet::new();
        for row in &rows {
            assert!(cases.insert(format!("{family}:single:{}:omitted", row.option)));
            for value in row.valid.split('|') {
                assert!(cases.insert(format!("{family}:single:{}:{value}", row.option)));
            }
            if row.invalid != "-" {
                for value in row.invalid.split('|') {
                    assert!(cases.insert(format!("{family}:invalid:{}:{value}", row.option)));
                }
            }
        }
        for left in 0..rows.len() {
            for right in (left + 1)..rows.len() {
                for left_value in rows[left].valid.split('|') {
                    for right_value in rows[right].valid.split('|') {
                        assert!(cases.insert(format!(
                            "{family}:pair:{}={left_value}:{}={right_value}:forward",
                            rows[left].option, rows[right].option
                        )));
                        assert!(cases.insert(format!(
                            "{family}:pair:{}={right_value}:{}={left_value}:reverse",
                            rows[right].option, rows[left].option
                        )));
                    }
                }
            }
        }

        assert!(
            cases.iter().any(|case| case.contains(":pair:")),
            "{family} must have pair cases"
        );
    }
}

#[test]
fn shared_contract_preserves_surface_defaults_and_discord_boundaries() {
    let rows = rows();
    let find = |family: &str, option: &str| {
        rows.iter()
            .copied()
            .find(|row| row.family == family && row.option == option)
            .unwrap_or_else(|| panic!("missing {family}.{option}"))
    };

    let pc_lines = find("pc", "lines");
    assert_eq!(pc_lines.discord_default, "auto");
    assert_eq!(pc_lines.native_default, "2");
    assert_eq!(
        find("pc", "score-mode").discord_default,
        "score-only-summary"
    );
    assert_eq!(
        find("pc", "score-mode").native_default,
        "score-only-summary"
    );

    let aggregation = find("build", "aggregation");
    assert_eq!(aggregation.disposition, "named");
    assert_eq!(aggregation.discord_path, "/build cover");
    assert_eq!(aggregation.exposure, "aggregation");
    assert_eq!(aggregation.lowering, "--aggregate|--tiling-only");
    for option in ["backend", "fallback", "workers"] {
        let row = find("pc", option);
        assert_eq!(row.disposition, "excluded");
        assert_eq!(row.exposure, "host-policy");
        assert_eq!(row.lowering, "none");
    }
    assert_eq!(find("setup", "mode").exposure, "mode");
    assert_eq!(find("setup", "mode").lowering, "--mode");
    assert_eq!(
        find("forward-damage", "preserve-b2b").exposure,
        "preserve-b2b"
    );
    assert_eq!(
        find("forward-damage", "preserve-b2b").lowering,
        "--preserve-b2b"
    );
    assert_eq!(
        find("spin-structure", "hold").exposure,
        "semantic-exclusion"
    );
}

#[test]
fn canonical_family_inputs_compile_to_normalized_app_requests() {
    let cases = [
        (
            "pc",
            "clearra pc --lines 2 --queue IOTSZ --count unique --backend auto",
        ),
        (
            "setup",
            "clearra setup-finder --remaining IOTSZJL --mode oracle --queue-knowledge oracle",
        ),
        (
            "build-probability",
            "clearra build-probability --base-mask 0 --target-mask 0xf --height 1 --queue I --no-hold --aggregate buildability",
        ),
        (
            "damage",
            "clearra damage --board-mask 0 --height 8 --queue T --no-hold --spin-profile t-spins",
        ),
        (
            "spin-finder",
            "clearra spin-finder --board-mask 0 --height 8 --queue T --no-hold --spin-profile t-spins --lines any",
        ),
    ];

    for (expected_kind, input) in cases {
        let parsed = CliCommandParser::parse(input).expect("canonical CLI command");
        let request = parsed.to_app_request().expect("normalized AppRequest");
        assert_eq!(request.command_kind().as_str(), expected_kind, "{input}");
    }
}
