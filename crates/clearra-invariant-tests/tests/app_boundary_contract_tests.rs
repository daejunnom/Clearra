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
fn app_boundary_docs_pin_typed_product_contract() {
    let text = read_workspace_file("docs/app-boundary.md");

    for marker in [
        "CLI / GUI / WASM Command Runtime / Desktop host -> AppRequest -> clearra-app -> validation -> executor -> AppResponse",
        "AppCommandKind",
        "QueryEnvelope",
        "BackendPolicy",
        "OutputPolicy",
        "DiagnosticsPolicy",
        "LocalePolicy",
        "ResourceBudget",
        "cli_pc_builds_app_request",
        "gui_form_builds_app_request",
        "wasm_command_builds_app_request",
        "app_validation_runs_before_executor",
        "app_error_does_not_execute_solver",
        "output_consumes_app_response_only",
    ] {
        assert_contains(&text, marker);
    }
    assert!(
        !text.contains("- `Verify`") && !text.contains("- `VerifyKicks`"),
        "hidden diagnostic commands must not be described in public app-boundary docs"
    );
}

#[test]
fn host_contract_exposes_stable_command_set() {
    let text = read_workspace_directory("crates/clearra-host-contract/src");

    for marker in [
        "pub enum AppCommandKind",
        "Pc",
        "Path",
        "Percent",
        "Setup",
        "Cover",
        "Continue",
        "Rules",
        "Scoring",
        "Convert",
        "InspectUnsupported",
        "Verify",
        "VerifyKicks",
        "pub enum QueryEnvelope",
        "pub struct BackendPolicy",
        "pub struct ResourceBudget",
        "pub struct BackendReport",
        "pub struct CapabilityReport",
    ] {
        assert_contains(&text, marker);
    }
}

#[test]
fn app_boundary_validator_pins_required_completion_markers() {
    let text = read_workspace_file("scripts/architecture/validate_app_boundary_contract.ps1");

    for marker in [
        "cli_pc_builds_app_request",
        "gui_form_builds_app_request",
        "wasm_command_builds_app_request",
        "app_validation_runs_before_executor",
        "app_error_does_not_execute_solver",
        "output_consumes_app_response_only",
    ] {
        assert_contains(&text, marker);
    }
}

#[test]
fn cli_verify_command_uses_app_request_boundary() {
    let text = read_workspace_file("crates/clearra-cli/src/commands/verify_command.rs");

    assert_contains(&text, "AppRequest::new");
    assert_contains(&text, "AppResponseRenderer::render");
    assert!(!text.contains("PcCommand::run"));
    assert!(!text.contains("SetupCommand::run"));
    assert!(!text.contains("CoverCommand::run"));
    assert!(!text.contains("KickContractReport::verify_builtin_contracts"));
}

fn assert_contains(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected app boundary contract text to contain {needle:?}"
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
