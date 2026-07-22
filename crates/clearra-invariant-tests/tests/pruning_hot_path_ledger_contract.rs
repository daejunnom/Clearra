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

#[test]
fn product_cpu_pruning_context_comes_from_the_problem_descriptor() {
    let catalog = read_workspace_responsibility("core-c/src/packing/geometry_catalog.c");
    let context = read_workspace_responsibility("core-c/src/packing/packing_prune_context.c");

    assert_contains(&catalog, "clearra_packing_prune_context_from_problem");
    assert_contains(
        &catalog,
        "clearra_placement_candidates_visit_with_pruning_ledger",
    );
    assert!(!catalog.contains("prune_context.batch_id = UINT64_C(1)"));
    assert!(!catalog.contains("prune_context.rule_profile_id = CLR_RULE_SRS"));
    assert_contains(&context, "clearra_cache_identity_from_packing_problem");
    assert_contains(&context, "problem->rule.piece_set_profile_id");
    assert_contains(&context, "problem->rule.rule_profile_id");
    assert_contains(&context, "problem->rule.kick_profile_id");
}

#[test]
fn ledger_aware_drop_api_rejects_missing_context_or_ledger() {
    let pruner = read_workspace_responsibility("core-c/src/packing/packing_pruner.c");

    assert_contains(
        &pruner,
        "!clearra_packing_prune_context_is_valid(context) || ledger == 0",
    );
    assert!(!pruner.contains("if (ledger == 0) {\n        return CLEARRA_PACKING_OK;"));
}

fn assert_contains(text: &str, marker: &str) {
    assert!(
        text.contains(marker),
        "expected pruning hot-path contract to contain {marker:?}"
    );
}
