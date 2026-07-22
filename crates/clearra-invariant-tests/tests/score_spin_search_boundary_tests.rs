use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn read_workspace_directory(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let directory = root.join(path);
    let mut entries = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("failed to read directory {path}: {error}"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("failed to enumerate directory {path}: {error}"));
    entries.sort_by_key(|entry| entry.path());

    let mut text = String::new();
    for entry in entries {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let relative = entry_path
                .strip_prefix(&root)
                .expect("workspace child")
                .to_string_lossy()
                .replace('\\', "/");
            text.push_str(&read_workspace_directory(&relative));
        } else {
            text.push_str(
                &std::fs::read_to_string(&entry_path).unwrap_or_else(|error| {
                    panic!("failed to read {}: {error}", entry_path.display())
                }),
            );
            text.push('\n');
        }
    }
    text
}
pub fn read_workspace_responsibility(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let relative_path = std::path::Path::new(path);
    let mut text = std::fs::read_to_string(root.join(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
    let parent = relative_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let stem = relative_path
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("responsibility file stem");

    for suffix in ["functions", "types", "impls", "methods", "api"] {
        let companion = parent.join(format!("{stem}_{suffix}"));
        if root.join(&companion).is_dir() {
            text.push('\n');
            text.push_str(&read_workspace_directory(
                &companion.to_string_lossy().replace('\\', "/"),
            ));
        }
    }
    text
}
pub fn read_workspace_contract_surface(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let relative_path = std::path::Path::new(path);
    let mut text = read_workspace_responsibility(path);
    let file_name = relative_path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("contract file name");

    if file_name == "lib.rs" || file_name == "mod.rs" {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        text.push('\n');
        text.push_str(&read_workspace_directory(
            &parent.to_string_lossy().replace('\\', "/"),
        ));
    }

    if relative_path.extension().and_then(|value| value.to_str()) == Some("rs") {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let stem = relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("Rust contract stem");
        for suffix in ["tests", "contract_tests"] {
            let test_file = parent.join(format!("{stem}_{suffix}.rs"));
            if root.join(&test_file).is_file() {
                text.push('\n');
                text.push_str(&read_workspace_responsibility(
                    &test_file.to_string_lossy().replace('\\', "/"),
                ));
            }
        }
    }

    if relative_path.extension().and_then(|value| value.to_str()) == Some("ps1") {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let stem = relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("PowerShell contract stem");
        let mut entries = std::fs::read_dir(root.join(parent))
            .expect("PowerShell contract directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("PowerShell contract entries");
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let entry_name = entry.file_name().to_string_lossy().to_string();
            if !entry_name.starts_with(&format!("{stem}."))
                && !entry_name.starts_with(&format!("{stem}_"))
            {
                continue;
            }
            let companion = parent.join(&entry_name);
            text.push('\n');
            if entry.path().is_dir() {
                text.push_str(&read_workspace_directory(
                    &companion.to_string_lossy().replace('\\', "/"),
                ));
            } else {
                text.push_str(&read_workspace_responsibility(
                    &companion.to_string_lossy().replace('\\', "/"),
                ));
            }
        }
    }
    text
}

#[test]
fn score_event_spin_detector_are_postprocess_only() {
    let score_event =
        read_workspace_contract_surface("crates/clearra-scoring/src/event/score_event.rs");
    assert_contains_all(
        &score_event,
        &[
            "score_event_from_step_postprocess_only",
            "score_must_not_prune_packing_candidate",
            "SpinDetector::detect",
        ],
    );

    let spin_detector =
        read_workspace_contract_surface("crates/clearra-scoring/src/event/spin_detector.rs");
    assert_contains_all(
        &spin_detector,
        &[
            "spin_detector_postprocess_only",
            "accepted replay evidence",
            "unknown_spin_not_false_for_pc_pruning",
        ],
    );
}

#[test]
fn scoring_source_does_not_own_search_pruning_surface() {
    let scoring_source = read_dir_text("crates/clearra-scoring/src");

    assert_not_contains_any(
        &scoring_source,
        &[
            "CPackingCandidate",
            "PruneReason",
            "DropCandidate",
            "prune_candidate",
            "can_drop_candidate",
            "clearra_core_ffi",
            "clearra_core_executor",
        ],
    );
}

#[test]
fn spin_unknown_and_postprocess_probability_boundaries_are_pinned() {
    let spin =
        read_workspace_contract_surface("crates/clearra-spin/src/target/predicate_result.rs");
    assert_contains_all(&spin, &["Unknown", "is_false_for_pc_pruning"]);

    let boundary = read_workspace_file("docs/search-postprocess-boundary.md");
    assert_contains_all(
        &boundary,
        &[
            "Unknown or incomplete spin classification is not `false` for PC pruning",
            "postprocess_does_not_change_pc_probability",
        ],
    );
}

#[test]
fn fin_iso_neo_are_not_rule_kick_tables() {
    let rules_source = read_dir_text("crates/clearra-rules/src");

    assert_not_contains_any(
        &rules_source,
        &[
            "KickTableProfileId::FinSpecial",
            "KickTableProfileId::IsoSpecial",
            "KickTableProfileId::NeoSpecial",
            "FinSpecial",
            "IsoSpecial",
            "NeoSpecial",
        ],
    );

    let spin_special_cases =
        read_workspace_contract_surface("crates/clearra-spin/src/special/special_spin_case_id.rs");
    assert_contains_all(
        &spin_special_cases,
        &["SpecialSpinCaseId", "Fin", "Iso", "Neo"],
    );
}

#[test]
fn pco_external_pc_solution_set_is_colored_tiling_oracle_not_replay_oracle() {
    let pco_source =
        read_workspace_file("tests/fixtures/external-pc/pco_opener_full_63.source_solutions.json");
    assert_contains_all(
        &pco_source,
        &[
            "\"solution_set_contract\": \"fumen-colored-tiling-set\"",
            "\"operation_replay_available\": false",
            "\"worker_correctness_basis\": \"source-fumen-colored-tiling-set\"",
            "Color groups provide exact piece placement masks",
        ],
    );

    let pco_fixture =
        read_workspace_file("tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json");
    assert_contains_all(
        &pco_fixture,
        &[
            "\"oracle_kind\": \"source-fumen-colored-tiling-set\"",
            "\"documented_source_page_count\": 63",
            "\"expected_normalized_unique_solution_count\": 63",
            "\"worker_correctness_basis\": \"source-fumen-colored-tiling-set\"",
        ],
    );
}

fn read_dir_text(relative_dir: &str) -> String {
    let mut output = String::new();
    read_dir_text_inner(&workspace_root().join(relative_dir), &mut output);
    output
}

fn read_dir_text_inner(dir: &Path, output: &mut String) {
    let mut entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            panic!(
                "failed to read directory entry in {}: {error}",
                dir.display()
            )
        });
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            read_dir_text_inner(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push_str(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
            );
            output.push('\n');
        }
    }
}

fn assert_contains_all(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "expected text to contain {needle:?}");
    }
}

fn assert_not_contains_any(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            !text.contains(needle),
            "expected text not to contain forbidden marker {needle:?}"
        );
    }
}

fn read_workspace_file(path: &str) -> String {
    fs::read_to_string(workspace_root().join(path))
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
