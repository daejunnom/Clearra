use std::{fs, path::PathBuf};

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
fn dependency_boundary_contract_documents_product_pipeline() {
    let text = read_workspace_file("docs/dependency-boundary.md");

    assert_contains(&text, "CLI / GUI / WASM Command Runtime -> AppRequest");
    assert_contains(&text, "C Geometry Skeleton Exact Cover");
    assert_contains(&text, "C BuildUp BFS");
    assert_contains(&text, "architecture_validation_rejects_cli_to_core_ffi");
    assert_contains(&text, "architecture_validation_rejects_gui_to_cli");
    assert_contains(&text, "architecture_validation_rejects_render_to_solver");
    assert_contains(&text, "architecture_validation_rejects_fumen_to_solver");
    assert_contains(&text, "architecture_validation_rejects_coverage_to_scoring");
    assert_contains(&text, "architecture_validation_rejects_spin_to_scoring");
    assert_contains(
        &text,
        "architecture_validation_rejects_core_executor_runtime_scoring",
    );
    assert_contains(&text, "clearra-postprocess-gpu -> clearra-postprocess");
}

#[test]
fn core_executor_runtime_does_not_depend_on_scoring() {
    let manifest = read_workspace_file("crates/clearra-core-executor/Cargo.toml");
    let runtime_dependencies = manifest
        .split_once("[dependencies]")
        .expect("executor manifest has dependencies")
        .1
        .split("[dev-dependencies]")
        .next()
        .expect("executor runtime dependency section");

    assert!(!runtime_dependencies.contains("clearra-scoring"));
    let dev_dependencies = manifest
        .split_once("[dev-dependencies]")
        .expect("executor manifest has dev dependencies")
        .1
        .split("[lints]")
        .next()
        .expect("executor dev dependency section");
    assert_contains(dev_dependencies, "clearra-scoring");

    let spin_modules =
        read_workspace_file("crates/clearra-core-executor/src/spin/mod.rs").replace("\r\n", "\n");
    for module in [
        "spin_input_from_replay",
        "spin_target_coverage_bridge",
        "spin_target_execution_report",
        "spin_target_result_reducer",
        "spin_target_runner",
        "spin_target_runner_error",
        "spin_target_threshold",
    ] {
        assert_contains(&spin_modules, &format!("#[cfg(test)]\npub mod {module};"));
    }

    let app_manifest = read_workspace_file("crates/clearra-app/Cargo.toml");
    assert_contains(&app_manifest, "clearra-postprocess");
    let postprocessor = read_workspace_directory("crates/clearra-postprocess/src/pc_scoring");
    assert_contains(&postprocessor, "score_postprocess_owner");
}

#[test]
fn search_postprocess_boundary_keeps_adapters_out_of_search() {
    let text = read_workspace_file("docs/search-postprocess-boundary.md");

    assert_contains(&text, "Fumen-like data is an adapter format");
    assert_contains(&text, "clearra-render must not call search");
    assert_contains(
        &text,
        "Unknown or incomplete spin classification is not `false` for PC pruning",
    );
    assert_contains(&text, "Resource-cap truncation produces incomplete output");
    assert_contains(
        &text,
        "CandidateExecutionAggregate -> ReplayTrace -> ScoreMatrix",
    );
    assert_contains(&text, "PostProcessCoverageUnion -> WebGPU bitset union");
    assert_contains(&text, "PostGpuTrustState");
}

#[test]
fn product_boundary_validator_pins_required_diagnostics() {
    let text = read_workspace_file("scripts/architecture/validate_product_boundary.ps1");

    for marker in [
        "architecture_validation_rejects_cli_to_core_ffi",
        "architecture_validation_rejects_gui_to_cli",
        "architecture_validation_rejects_render_to_solver",
        "architecture_validation_rejects_fumen_to_solver",
        "architecture_validation_rejects_coverage_to_scoring",
        "architecture_validation_rejects_spin_to_scoring",
    ] {
        assert_contains(&text, marker);
    }
}

fn assert_contains(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected dependency boundary text to contain {needle:?}"
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
