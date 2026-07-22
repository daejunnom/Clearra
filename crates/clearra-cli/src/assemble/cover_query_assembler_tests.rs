use super::*;

#[test]
fn assembles_canonical_build_coverage_query() {
    let query =
        CoverQueryAssembler::assemble(&CoverArgs::new(Some("tspin".to_owned()))).expect("query");

    assert_eq!(query.template().id(), "tspin");
    assert_eq!(query.template().slots().len(), 1);
    assert_eq!(query.domains().len(), 1);
    assert_eq!(query.domains()[0].pieces(), &[PieceKind::I]);
    assert_eq!(query.limits().max_patterns(), 4096);
}

#[test]
fn assembles_native_json_template_into_domains_and_constraints() {
    let json = r#"{
        "schema_version": 2,
        "id": "native-cover",
        "label": "Native cover",
        "board": { "width": 10, "height": 4 },
        "symmetry": "none",
        "canonicalization": "canonical-by-geometry",
        "slots": [{
            "id": 7,
            "label": "slot",
            "cells": [{ "x": 0, "y": 0 }],
            "allowed_pieces": ["I", "O"],
            "required_piece": "I",
            "hold_constraint": "any",
            "order_constraint": { "kind": "any" },
            "symmetry": "none",
            "canonicalization": "none"
        }]
    }"#;

    let query = CoverQueryAssembler::assemble(
        &CoverArgs::new(None).with_template_json(Some(json.to_owned())),
    )
    .expect("native json query");

    assert_eq!(query.template().id(), "native-cover");
    assert_eq!(query.domains()[0].pieces(), &[PieceKind::I, PieceKind::O]);
    assert_eq!(query.constraints()[0].required_piece(), Some(PieceKind::I));
}

#[test]
fn rejects_ambiguous_template_sources() {
    let error = CoverQueryAssembler::assemble(
        &CoverArgs::new(Some("template.json".to_owned())).with_template_json(Some("{}".to_owned())),
    )
    .expect_err("conflicting sources");

    assert_eq!(error, CoverQueryAssemblyError::ConflictingTemplateSources);
}

#[test]
fn rejects_sensitive_template_file_paths_before_reading() {
    let error = CoverQueryAssembler::assemble(
        &CoverArgs::new(None).with_template_file(Some("service-account.json".to_owned())),
    )
    .expect_err("sensitive path");

    assert!(error.to_string().contains("sensitive-looking file path"));
}

#[test]
fn template_file_errors_redact_absolute_paths_by_default() {
    let path = std::env::temp_dir().join(format!(
        "clearra-cover-template-redacted-{}.txt",
        unique_suffix()
    ));
    let error = CoverQueryAssembler::assemble(
        &CoverArgs::new(None).with_template_file(Some(path.display().to_string())),
    )
    .expect_err("invalid extension");
    let message = error.to_string();

    assert!(message.contains(".../clearra-cover-template-redacted-"));
    assert!(!message.contains(&std::env::temp_dir().display().to_string()));
}

#[test]
fn template_file_errors_show_full_path_when_verbose_paths_are_enabled() {
    let path = std::env::temp_dir().join(format!(
        "clearra-cover-template-verbose-{}.txt",
        unique_suffix()
    ));

    let error = crate::input::file_input_guard::with_verbose_paths(true, || {
        CoverQueryAssembler::assemble(
            &CoverArgs::new(None).with_template_file(Some(path.display().to_string())),
        )
    })
    .expect_err("invalid extension");

    assert!(error.to_string().contains(&path.display().to_string()));
}

#[test]
fn assembles_template_file_after_file_guard() {
    let path = temp_template_path("clearra-cover-template");
    std::fs::write(
        &path,
        r#"{
            "schema_version": 2,
            "id": "file-cover",
            "board": { "width": 10, "height": 4 },
            "slots": [{
                "id": 3,
                "cells": [{ "x": 0, "y": 0 }],
                "allowed_pieces": ["I"]
            }]
        }"#,
    )
    .expect("template file");

    let query = CoverQueryAssembler::assemble(
        &CoverArgs::new(None).with_template_file(Some(path.display().to_string())),
    )
    .expect("template-file query");

    assert_eq!(query.template().id(), "file-cover");
}

fn temp_template_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}.json", unique_suffix()))
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}
