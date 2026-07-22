use clearra_rules::kicks::{KickProfileRegistry, KickTableProfileId};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use super::KickTableEditorSchema;

#[test]
fn kick_table_editor_schema_exposes_registry_preview_and_import_export() {
    let schema = KickTableEditorSchema::mvp2();

    assert!(schema.editable());
    assert!(schema.import_export().import_json_enabled());
    assert_eq!(
        schema.import_export().adapter(),
        "clearra-rules::KickImport"
    );
    assert_eq!(
        schema.previews().len(),
        KickProfileRegistry::builtin_profiles().len()
    );
    assert!(schema
        .previews()
        .iter()
        .any(|preview| preview.profile_id() == KickTableProfileId::Srs90.as_str()));
    let srs_plus = schema
        .previews()
        .iter()
        .find(|preview| preview.profile_id() == KickTableProfileId::SrsPlus.as_str())
        .expect("srs-plus preview");
    assert_eq!(srs_plus.source_kind(), "built-in-exact");
    assert!(srs_plus.source_description().contains("TETR.IO SRS+"));
    assert_eq!(srs_plus.transition_count(), 80);
    assert!(srs_plus.first_success_order_preserved());
    assert!(srs_plus.provenance().contains("symmetric I"));
    assert!(srs_plus.verified());
    assert!(srs_plus.supports_180());
    assert!(srs_plus.supports_exact_180());
    assert!(srs_plus.c_compact_descriptor_ready());
    assert_eq!(srs_plus.unsupported_backend_reason(), "none");
}

#[test]
fn unsupported_kick_profiles_carry_diagnostic_disabled_reason() {
    let schema = KickTableEditorSchema::mvp2();
    let srs_x = schema
        .previews()
        .iter()
        .find(|preview| preview.profile_id() == KickTableProfileId::SrsX.as_str())
        .expect("srs-x preview");

    assert!(!srs_x.search_backend_supported());
    assert!(!srs_x.c_compact_descriptor_ready());
    assert_eq!(srs_x.transition_count(), 84);
    assert!(srs_x.first_success_order_preserved());
    assert_eq!(
        srs_x.unsupported_backend_reason(),
        "srs_x_profile_requires_imported_kick_table"
    );
    assert_eq!(
        srs_x.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
}
