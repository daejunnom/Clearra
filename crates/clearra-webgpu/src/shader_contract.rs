use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuShaderContract {
    shader_version: &'static str,
    shader_source: &'static str,
    user_provided_wgsl_allowed: bool,
    runtime_shader_injection_allowed: bool,
}

impl WebGpuShaderContract {
    pub fn embedded_reviewed() -> Self {
        Self {
            shader_version: "pattern-bitset-union-webgpu-v1",
            shader_source: include_str!("embedded_pattern_bitset_union.wgsl"),
            user_provided_wgsl_allowed: false,
            runtime_shader_injection_allowed: false,
        }
    }

    pub fn embedded_geometry_exact_cover() -> Self {
        Self {
            shader_version: "geometry-exact-cover-webgpu-v2",
            shader_source: include_str!("embedded_geometry_exact_cover.wgsl"),
            user_provided_wgsl_allowed: false,
            runtime_shader_injection_allowed: false,
        }
    }
}
impl WebGpuShaderContract {
    pub fn reject_user_provided_wgsl(
        &self,
        maybe_wgsl: Option<&str>,
    ) -> Result<(), WebGpuShaderContractError> {
        if maybe_wgsl.is_some() {
            return Err(WebGpuShaderContractError::new(
                "E_WEBGPU_USER_PROVIDED_WGSL_REJECTED",
                "user_provided_wgsl_rejected: WebGPU accepts embedded reviewed shaders only",
            ));
        }
        Ok(())
    }
}
impl WebGpuShaderContract {
    pub fn shader_version(&self) -> &'static str {
        self.shader_version
    }
}
impl WebGpuShaderContract {
    pub fn shader_hash(&self) -> String {
        format!("wgsl-fnv64:{:016x}", fnv1a64(self.shader_source.as_bytes()))
    }
}
impl WebGpuShaderContract {
    pub fn shader_hash_reported(&self) -> bool {
        !self.shader_hash().is_empty()
    }

    pub fn shader_byte_len(&self) -> usize {
        self.shader_source.len()
    }

    pub(crate) fn shader_source(&self) -> &'static str {
        self.shader_source
    }
}
impl WebGpuShaderContract {
    pub fn no_user_provided_wgsl(&self) -> bool {
        !self.user_provided_wgsl_allowed
    }
}
impl WebGpuShaderContract {
    pub fn no_runtime_shader_injection(&self) -> bool {
        !self.runtime_shader_injection_allowed
    }
}

impl Default for WebGpuShaderContract {
    fn default() -> Self {
        Self::embedded_reviewed()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuShaderContractError {
    code: &'static str,
    message: String,
}

impl WebGpuShaderContractError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl WebGpuShaderContractError {
    pub fn code(&self) -> &'static str {
        self.code
    }
}
impl WebGpuShaderContractError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for WebGpuShaderContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WebGpuShaderContractError {}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "shader_contract_tests.rs"]
mod tests;
