use super::*;

#[test]
fn capability_report_entry_keeps_disabled_reason_visible() {
    let entry = CapabilityReportEntrySchema::new(
        "CustomBagProfile",
        CapabilityState::Unsupported,
        Some("custom_bag_runtime_not_connected".to_owned()),
    );

    assert_eq!(entry.state_label(), "Unsupported");
    assert!(!entry.runtime_execution_allowed());
    assert!(!entry.exact_claim_allowed());
    assert_eq!(
        entry.disabled_reason(),
        Some("custom_bag_runtime_not_connected")
    );
    assert!(!entry.missing_disabled_reason());
}

#[test]
fn unsupported_capability_requires_visible_reason() {
    let entry =
        CapabilityReportEntrySchema::new("WideBoardRuntime", CapabilityState::Unsupported, None);

    assert!(entry.missing_disabled_reason());
}
