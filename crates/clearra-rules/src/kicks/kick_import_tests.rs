use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use super::*;

#[test]
fn imported_kick_profile_json_roundtrips_180_transition_and_offset_order() {
    let json = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [
            {
                "piece": "T",
                "from": "0",
                "to": "2",
                "offsets": [
                    { "dx": 0, "dy": 0 },
                    { "dx": 1, "dy": 0 },
                    { "dx": -1, "dy": 0 }
                ]
            }
        ]
    }"#;

    let profile = KickImport::from_json(json).expect("valid json profile");
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Two);
    let sequence = profile.sequence_for(transition).expect("T 0->2");
    let exported = KickImport::to_json(&profile).expect("export json");
    let reparsed = KickImport::from_json(&exported).expect("roundtrip json profile");

    assert!(profile.supports_180());
    assert_eq!(sequence.offsets()[1], KickOffset::new(1, 0));
    assert_eq!(reparsed, profile);
}

#[test]
fn imported_kick_profile_rejects_unknown_piece_and_rotation() {
    let unknown_piece = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [{ "piece": "X", "from": "0", "to": "R", "offsets": [{ "dx": 0, "dy": 0 }] }]
    }"#;
    let unknown_rotation = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [{ "piece": "T", "from": "N", "to": "R", "offsets": [{ "dx": 0, "dy": 0 }] }]
    }"#;

    assert_eq!(
        KickImport::from_json(unknown_piece)
            .expect_err("unknown piece")
            .code(),
        "unknown_piece"
    );
    assert_eq!(
        KickImport::from_json(unknown_rotation)
            .expect_err("unknown rotation")
            .code(),
        "unknown_rotation"
    );
}

#[test]
fn imported_kick_profile_rejects_unknown_fields() {
    let unknown_root = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [],
        "unexpected": true
    }"#;
    let unknown_entry = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [
            {
                "piece": "T",
                "from": "0",
                "to": "R",
                "offsets": [{ "dx": 0, "dy": 0 }],
                "unknown_entry_field": true
            }
        ]
    }"#;
    let unknown_offset = r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [
            {
                "piece": "T",
                "from": "0",
                "to": "R",
                "offsets": [{ "dx": 0, "dy": 0, "dz": 0 }]
            }
        ]
    }"#;

    for json in [unknown_root, unknown_entry, unknown_offset] {
        assert_eq!(
            KickImport::from_json(json)
                .expect_err("unknown field")
                .code(),
            "invalid_json"
        );
    }
}
