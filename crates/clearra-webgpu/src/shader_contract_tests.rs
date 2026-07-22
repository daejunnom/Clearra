use super::*;

#[test]
fn webgpu_user_shader_rejected() {
    let contract = WebGpuShaderContract::embedded_reviewed();
    let error = contract
        .reject_user_provided_wgsl(Some("@compute @workgroup_size(1) fn main() {}"))
        .expect_err("user WGSL must be rejected");

    assert_eq!(error.code(), "E_WEBGPU_USER_PROVIDED_WGSL_REJECTED");
    assert!(error.message().contains("user_provided_wgsl_rejected"));
    assert!(contract.no_user_provided_wgsl());
    assert!(contract.no_runtime_shader_injection());
}

#[test]
fn shader_hash_reported() {
    let contract = WebGpuShaderContract::embedded_reviewed();

    assert_eq!(contract.shader_version(), "pattern-bitset-union-webgpu-v1");
    assert!(contract.shader_hash().starts_with("wgsl-fnv64:"));
    assert!(contract.shader_hash_reported());
    assert!(contract.shader_byte_len() > 128);
}
