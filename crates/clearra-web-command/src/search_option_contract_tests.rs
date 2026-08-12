use std::collections::{BTreeMap, BTreeSet};

use crate::WebCommandParser;

const CONTRACT: &str = include_str!("../../../tests/fixtures/contracts/search_option_contract.tsv");

#[derive(Clone, Copy, Debug)]
struct ContractRow<'a> {
    family: &'a str,
    option: &'a str,
    valid: &'a str,
    invalid: &'a str,
    web_default: &'a str,
    native_default: &'a str,
    discord_surface: &'a str,
}

fn rows() -> Vec<ContractRow<'static>> {
    CONTRACT
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                columns.len(),
                9,
                "search option contract rows must have nine columns: {line}"
            );
            assert!(!columns[3].is_empty(), "valid representatives: {line}");
            ContractRow {
                family: columns[0],
                option: columns[1],
                valid: columns[3],
                invalid: columns[4],
                web_default: columns[5],
                native_default: columns[6],
                discord_surface: columns[7],
            }
        })
        .collect()
}

#[test]
fn shared_contract_covers_the_five_search_families_without_duplicate_options() {
    let rows = rows();
    let families = rows.iter().map(|row| row.family).collect::<BTreeSet<_>>();
    assert_eq!(
        families,
        BTreeSet::from(["build", "damage", "pc", "setup", "spin-finder"])
    );

    let mut options = BTreeSet::new();
    for row in rows {
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
    assert_eq!(pc_lines.web_default, "4");
    assert_eq!(pc_lines.native_default, "2");
    assert_eq!(find("pc", "score-mode").web_default, "off");
    assert_eq!(find("pc", "score-mode").native_default, "all");

    assert_eq!(
        find("build", "aggregation").discord_surface,
        "sfinder-baked"
    );
    assert_ne!(
        find("build", "aggregation").discord_surface,
        "packed:aggregation",
        "Discord must not expose a general tiling/aggregation selector"
    );
    for option in ["backend", "fallback", "workers"] {
        assert_eq!(find("pc", option).discord_surface, "host");
    }
    assert_eq!(find("setup", "mode").discord_surface, "packed:mode");
    assert_eq!(
        find("damage", "preserve-b2b").discord_surface,
        "packed:preserve-b2b"
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
        let parsed = WebCommandParser::parse(input).expect("canonical web command");
        let request = parsed.to_app_request().expect("normalized AppRequest");
        assert_eq!(request.command_kind().as_str(), expected_kind, "{input}");
    }
}
