use super::*;

#[test]
fn capability_state_exposes_only_stable_product_states() {
    let states = [
        CapabilityState::Unsupported,
        CapabilityState::ConnectedApproximate,
        CapabilityState::ConnectedExact,
    ];

    assert_eq!(states.len(), 3);
    assert_eq!(
        CapabilityState::ConnectedApproximate.as_str(),
        "ConnectedApproximate"
    );
}

#[test]
fn capability_state_runtime_requires_connected_or_exact() {
    assert!(!CapabilityState::Unsupported.runtime_execution_allowed());
    assert!(CapabilityState::ConnectedApproximate.runtime_execution_allowed());
    assert!(CapabilityState::ConnectedExact.runtime_execution_allowed());
}

#[test]
fn capability_state_exact_claim_requires_exact_supported() {
    assert!(!CapabilityState::ConnectedApproximate.exact_claim_allowed());
    assert!(CapabilityState::ConnectedExact.exact_claim_allowed());
}

#[test]
fn capability_state_maps_validation_registries_to_ui_schema() {
    assert_eq!(
        CapabilityState::from_mvp2_state(Mvp2CapabilityState::ConnectedApproximate),
        CapabilityState::ConnectedApproximate
    );
    assert_eq!(
        CapabilityState::from_mvp3_state(Mvp3CapabilityState::Unsupported),
        CapabilityState::Unsupported
    );
}
