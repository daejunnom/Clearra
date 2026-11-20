use std::{path::PathBuf, process::Command};

use serde_json::Value;

fn clearra() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_clearra"));
    command.current_dir(workspace_root());
    command
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn run_json(args: &[&str]) -> Value {
    let output = clearra()
        .args(["--format", "json"])
        .args(args)
        .output()
        .expect("launch the product CLI");
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{args:?}: {}, stdout={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).expect("product CLI JSON")
}

fn assert_rust_cpu(summary: &Value) {
    assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
    for key in ["native_c_core_linked", "native_c_core_executed"] {
        if let Some(flag) = summary.get(key) {
            assert_eq!(flag, &Value::Bool(false), "{key}: {summary}");
        }
    }
}

#[path = "process_e2e/process_e2e_opening.rs"]
mod process_e2e_opening;
#[path = "process_e2e/process_e2e_path_percent.rs"]
mod process_e2e_path_percent;
#[path = "process_e2e/process_e2e_routing.rs"]
mod process_e2e_routing;
#[path = "process_e2e/process_e2e_scenario.rs"]
mod process_e2e_scenario;
#[cfg(not(feature = "webgpu-search"))]
#[path = "process_e2e/process_e2e_setup_backend.rs"]
mod process_e2e_setup_backend;
#[path = "process_e2e/process_e2e_verify_continue.rs"]
mod process_e2e_verify_continue;
