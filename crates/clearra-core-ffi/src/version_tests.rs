use super::{CoreAbiVersion, CLEARRA_CORE_ABI_VERSION, CLEARRA_CORE_VERSION};

#[test]
fn abi_version_matches_current_contract() {
    let version = CoreAbiVersion::current();
    let mismatched = CoreAbiVersion::from_runtime(CLEARRA_CORE_ABI_VERSION + 1);

    assert_eq!(version.value(), CLEARRA_CORE_ABI_VERSION);
    assert!(version.is_compatible_with(CLEARRA_CORE_ABI_VERSION));
    assert!(!mismatched.is_compatible_with(CLEARRA_CORE_ABI_VERSION));
}

#[test]
fn package_version_is_exposed_for_core_handshake() {
    assert!(!CLEARRA_CORE_VERSION.is_empty());
}
