pub const CLEARRA_CORE_ABI_VERSION: i32 = 23;
pub const CLEARRA_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreAbiVersion {
    value: i32,
}

impl CoreAbiVersion {
    pub const fn current() -> Self {
        Self {
            value: CLEARRA_CORE_ABI_VERSION,
        }
    }
}
impl CoreAbiVersion {
    pub const fn from_runtime(value: i32) -> Self {
        Self { value }
    }
}
impl CoreAbiVersion {
    pub const fn value(self) -> i32 {
        self.value
    }
}
impl CoreAbiVersion {
    pub const fn is_compatible_with(self, runtime_version: i32) -> bool {
        self.value == runtime_version
    }
}

#[cfg(test)]
#[path = "version_tests.rs"]
mod tests;
