use super::*;

#[test]
fn custom_bag_schema_valid() {
    let profile = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            2,
            1,
        )],
    )
    .expect("custom bag");

    assert!(profile.custom_bag_schema_valid());
    assert_eq!(profile.bag_size(), 2);
    assert_eq!(profile.total_weight(), 1);
}

#[test]
fn custom_bag_runtime_not_connected_until_runtime_exists() {
    let profile = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            1,
            1,
        )],
    )
    .expect("custom bag");

    assert_eq!(
        profile.runtime_guard().reason(),
        "custom_bag_runtime_not_connected"
    );
}
