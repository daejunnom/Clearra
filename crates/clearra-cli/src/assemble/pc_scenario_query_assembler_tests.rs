use std::path::PathBuf;

use crate::{args::pc_scenario_args::PcScenarioArgs, assemble::PcScenarioQueryAssembler};

#[test]
fn assembles_inline_scenario_query_with_verified_kick_profile() {
    let import_json =
        clearra_rules::kicks::KickImport::to_json(&clearra_rules::kicks::NoKick::profile())
            .expect("no-kick json");
    let args = PcScenarioArgs::new(None)
        .with_field(Some("0x00000000000003f0".to_owned()))
        .with_queue(Some("I, O T".to_owned()))
        .with_rule(Some("no-kick".to_owned()))
        .with_kick_profile_json(Some(import_json))
        .with_max_pieces(Some(1))
        .with_exact_pieces(Some(1));

    let assembly = PcScenarioQueryAssembler::assemble(&args).expect("assembly");

    assert_eq!(assembly.query().remaining_queue().len(), 3);
    assert_eq!(assembly.query().piece_window().max_pieces(), 1);
    assert!(assembly.query().verified_kick_profile().is_some());
    assert!(assembly.fixture().is_none());
}

#[test]
fn inline_scenario_defaults_to_srs_plus_and_enabled_hold() {
    let args = PcScenarioArgs::new(None)
        .with_field(Some("0x00000000000003f0".to_owned()))
        .with_queue(Some("I".to_owned()))
        .with_max_pieces(Some(1));

    let assembly = PcScenarioQueryAssembler::assemble(&args).expect("assembly");

    assert_eq!(
        assembly.query().rule().id(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsPlus
    );
    assert!(assembly.query().allow_hold());
}

#[test]
fn assembles_fixture_scenario_query_and_source_fields() {
    let path = fixture_path("tests/fixtures/pc/example.json");
    let args = PcScenarioArgs::new(Some(path.display().to_string()));

    let assembly = PcScenarioQueryAssembler::assemble(&args).expect("assembly");
    let fields = assembly.input_fields();

    assert_eq!(assembly.query().remaining_queue().len(), 1);
    assert!(assembly.fixture().is_some());
    assert!(fields.contains(&("input_mode".to_owned(), "fixture".to_owned())));
    assert!(fields.contains(&("fixture_source_site".to_owned(), "harddrop".to_owned())));
}

#[test]
fn assembles_execution_policy_from_inline_scenario_args() {
    let args = PcScenarioArgs::new(None)
        .with_field(Some("0x00000000000003f0".to_owned()))
        .with_queue(Some("I".to_owned()))
        .with_max_pieces(Some(1))
        .with_backend(Some("hybrid".to_owned()))
        .with_workers(Some(2))
        .with_max_memory_mib(Some(256))
        .with_allow_backend_fallback(Some(false));

    let assembly = PcScenarioQueryAssembler::assemble(&args).expect("assembly");

    assert_eq!(
        assembly.query().execution_policy().requested_backend(),
        clearra_pc_graph::request::RequestedSearchBackend::Hybrid
    );
    assert_eq!(assembly.query().execution_policy().workers(), 2);
    assert_eq!(
        assembly.query().execution_policy().max_memory_mib(),
        Some(256)
    );
    assert!(!assembly.query().execution_policy().allow_backend_fallback());
}

#[test]
fn fixture_file_errors_redact_absolute_paths_by_default() {
    let path =
        std::env::temp_dir().join(format!("clearra-fixture-redaction-{}.txt", unique_suffix()));
    let args = PcScenarioArgs::new(Some(path.display().to_string()));

    let error = PcScenarioQueryAssembler::assemble(&args).expect_err("invalid extension");
    let message = error.message();

    assert!(message.contains(".../clearra-fixture-redaction-"));
    assert!(!message.contains(&std::env::temp_dir().display().to_string()));
}

#[test]
fn fixture_file_errors_show_full_path_when_verbose_paths_are_enabled() {
    let path =
        std::env::temp_dir().join(format!("clearra-fixture-verbose-{}.txt", unique_suffix()));
    let args = PcScenarioArgs::new(Some(path.display().to_string()));

    let error = crate::input::file_input_guard::with_verbose_paths(true, || {
        PcScenarioQueryAssembler::assemble(&args)
    })
    .expect_err("invalid extension");

    assert!(error.message().contains(&path.display().to_string()));
}

fn fixture_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .join(relative_path)
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}
