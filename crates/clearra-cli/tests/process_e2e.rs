#![cfg(feature = "native-c-core")]

use std::{path::PathBuf, process::Command};

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

fn native_core_enabled() -> bool {
    cfg!(feature = "native-c-core")
}

fn expected_retained_trace_key_prefix() -> &'static str {
    if native_core_enabled() {
        "\"retained_trace_keys\":[\"bvk2:"
    } else {
        "\"retained_trace_keys\":[\"trk1:"
    }
}

fn expected_percent_covered_pattern_count() -> usize {
    0
}

fn expected_percent_probability() -> &'static str {
    "0"
}

fn expected_path_solution_count() -> usize {
    if native_core_enabled() {
        1536
    } else {
        1
    }
}

fn expected_path_retained_trace_count() -> usize {
    if native_core_enabled() {
        64
    } else {
        1
    }
}

fn expected_scenario_solution_count() -> usize {
    if native_core_enabled() {
        2
    } else {
        1
    }
}

fn expected_setup_tiling_variant_count() -> &'static str {
    if native_core_enabled() {
        "tiling_variant_count: 0"
    } else {
        "tiling_variant_count: 1"
    }
}

fn expected_setup_build_variant_count() -> &'static str {
    if native_core_enabled() {
        "build_variant_count: 0"
    } else {
        "build_variant_count: 1"
    }
}

fn expected_setup_covered_pattern_count() -> &'static str {
    "covered_pattern_count: 0"
}

fn expected_setup_coverage_probability() -> &'static str {
    "coverage_probability: 0.0"
}

fn expected_scenario_coverage_probability_json() -> &'static str {
    if native_core_enabled() {
        "\"coverage_probability\":1.0"
    } else {
        "\"coverage_probability\":0.0"
    }
}

fn expected_cpu_confirmed_json() -> &'static str {
    if native_core_enabled() {
        "true"
    } else {
        "false"
    }
}

fn expected_candidate_backend() -> &'static str {
    "cpu-packing"
}

fn expected_buildup_backend() -> &'static str {
    "cpu-buildup"
}

fn expected_native_c_core_executed_json() -> &'static str {
    if native_core_enabled() {
        "true"
    } else {
        "false"
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
#[path = "process_e2e/process_e2e_setup_backend.rs"]
mod process_e2e_setup_backend;
#[path = "process_e2e/process_e2e_verify_continue.rs"]
mod process_e2e_verify_continue;
