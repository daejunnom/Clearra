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

#[test]
fn algorithm_policy_documents_forbidden_mitm_backend_names() {
    let text = read_workspace_file("docs/algorithm-policy.md");

    for marker in [
        "Meet-in-the-middle PC search is not part of Clearra",
        "MeetInTheMiddlePacking",
        "mitm_pc_backend",
        "half_join_pc",
        "front_half_packing",
        "back_half_packing",
        "complement_join_pc",
        "mitm_static_tiling_in_search_path",
        "SmallComponentExactCover",
        "AreaFeasibilityChecker",
        "ComponentExactCoverVerifier",
        "BuildOrders(P) intersection HoldReachableOrders(Q) is empty",
        "architecture_validation_rejects_mitm_pc_backend",
    ] {
        assert_contains(&text, marker);
    }
}

#[test]
fn pruning_policy_documents_exact_and_forbidden_pruning_reasons() {
    let text = read_workspace_file("docs/pruning-policy.md");

    for marker in [
        "collision",
        "bounds overflow",
        "target mask overflow",
        "area overflow",
        "piece count overflow",
        "row capacity overflow",
        "exact hash confirm dedupe",
        "coverage universe identity mismatch reject",
        "BuildUp full-key memo dedupe",
        "HoldAutomaton impossible",
        "Reachability impossible",
        "MCTS low score",
        "rare piece heuristic",
        "bad shape heuristic",
        "probably impossible",
        "no immediate placement",
        "target-frame floating",
        "spin classifier unknown",
        "score below threshold",
        "first witness missing",
        "representative order failed",
        "Bloom filter false positive",
        "resource cap reached",
    ] {
        assert_contains(&text, marker);
    }
}

#[test]
fn forbidden_algorithm_validator_pins_required_diagnostics() {
    let text = read_workspace_file("scripts/architecture/validate_forbidden_algorithms.ps1");

    for marker in [
        "architecture_validation_rejects_mitm_pc_backend",
        "architecture_validation_rejects_heuristic_prune_reason",
        "architecture_validation_rejects_representative_order_only_coverage",
        "architecture_validation_rejects_first_witness_coverage",
    ] {
        assert_contains(&text, marker);
    }
}

#[test]
fn production_source_does_not_use_forbidden_mitm_names() {
    let forbidden = [
        "MeetInTheMiddlePacking",
        "mitm_pc_backend",
        "half_join_pc",
        "front_half_packing",
        "back_half_packing",
        "complement_join_pc",
        "mitm_static_tiling_in_search_path",
    ];

    assert_no_markers_in_product_sources(&forbidden);
}

fn assert_no_markers_in_product_sources(markers: &[&str]) {
    for path in product_source_files() {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for marker in markers {
            assert!(
                !text.contains(marker),
                "{} contains forbidden marker {marker:?}",
                path.display()
            );
        }
    }
}

fn product_source_files() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut files = Vec::new();

    collect_files(&root.join("crates"), &mut files, &["rs"]);
    collect_files(&root.join("core-c").join("src"), &mut files, &["c", "h"]);
    collect_files(&root.join("core-c").join("include"), &mut files, &["h"]);
    collect_files(
        &root.join("core-c").join("kernels"),
        &mut files,
        &["c", "h", "cl", "cu"],
    );

    files
        .into_iter()
        .filter(|path| {
            let normalized = path.to_string_lossy().replace('\\', "/");
            normalized.contains("/src/")
                || normalized.contains("/core-c/src/")
                || normalized.contains("/core-c/include/")
                || normalized.contains("/core-c/kernels/")
        })
        .filter(|path| {
            let normalized = path.to_string_lossy().replace('\\', "/");
            !normalized.contains("/tests/")
                && !normalized.contains("/fixtures/")
                && !normalized.ends_with("_tests.rs")
                && !normalized.contains("/target/")
                && !normalized.contains("/node_modules/")
                && !normalized.contains("/dist/")
                && !normalized.contains("/dist-server/")
                && !normalized.contains("/build/")
                && !normalized.contains("/coverage/")
                && !normalized.contains("/models/")
                && !normalized.contains("/checkpoints/")
                && !normalized.contains("/.cache/")
        })
        .collect()
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>, extensions: &[&str]) {
    if !dir.exists() {
        return;
    }

    for entry in fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()))
    {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files, extensions);
            continue;
        }

        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            files.push(path);
        }
    }
}

fn assert_contains(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected forbidden algorithm contract text to contain {needle:?}"
    );
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
