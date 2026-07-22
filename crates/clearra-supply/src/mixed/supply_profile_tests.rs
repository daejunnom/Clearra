use clearra_core_domain::ids::piece_id::PieceDefinitionId;

use super::{super::CustomBagEntry, *};

#[test]
fn standard_7_bag_path_unchanged() {
    let profile = SupplyProfile::standard_7_bag_path_unchanged();

    assert_eq!(profile.kind().as_str(), "standard-7-bag");
    assert_eq!(profile.runtime_guard_reason(), None);
    assert_eq!(profile.provenance().bag_profile_id(), "standard-7-bag");
}

#[test]
fn custom_bag_profile_stays_guarded() {
    let custom = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            1,
            1,
        )],
    )
    .expect("custom bag");
    let provenance = SupplyProvenance::new(
        custom.bag_profile_id(),
        custom.piece_set_id(),
        None,
        super::super::BagBoundaryEvidence::NotEvaluated,
        false,
        false,
    )
    .expect("provenance");

    let profile = SupplyProfile::custom_bag_profile(&custom, provenance);

    assert_eq!(profile.kind().as_str(), "unsupported-extension");
    assert_eq!(
        profile
            .kind()
            .extension_id()
            .map(clearra_core_domain::ids::ExtensionId::as_str),
        Some("tri-bag")
    );
    assert_eq!(
        profile.runtime_guard_reason(),
        Some("custom_bag_runtime_not_connected")
    );
}
