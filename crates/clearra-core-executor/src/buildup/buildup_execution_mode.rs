#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BuildUpExecutionMode {
    #[default]
    VerifyFirst,
    EnumerateVariants,
    CountVariants,
}

impl BuildUpExecutionMode {
    pub const fn coverage_producing() -> Self {
        Self::EnumerateVariants
    }
}
impl BuildUpExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VerifyFirst => "verify-first",
            Self::EnumerateVariants => "enumerate-variants",
            Self::CountVariants => "count-variants",
        }
    }
}
impl BuildUpExecutionMode {
    pub const fn can_source_coverage(self) -> bool {
        matches!(self, Self::EnumerateVariants)
    }
}
impl BuildUpExecutionMode {
    pub const fn can_source_min_cover(self) -> bool {
        matches!(self, Self::EnumerateVariants)
    }
}

#[cfg(test)]
#[path = "buildup_execution_mode_tests.rs"]
mod tests;
