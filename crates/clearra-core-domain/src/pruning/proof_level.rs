#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProofLevel {
    LocalOnly,
    ClearStateConditional,
    AllReachableClearStates,
    GlobalSafe,
}

impl ProofLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "LocalOnly",
            Self::ClearStateConditional => "ClearStateConditional",
            Self::AllReachableClearStates => "AllReachableClearStates",
            Self::GlobalSafe => "GlobalSafe",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_level_is_reporting_metadata_only() {
        assert_eq!(ProofLevel::LocalOnly.as_str(), "LocalOnly");
        assert_eq!(
            ProofLevel::ClearStateConditional.as_str(),
            "ClearStateConditional"
        );
        assert_eq!(
            ProofLevel::AllReachableClearStates.as_str(),
            "AllReachableClearStates"
        );
        assert_eq!(ProofLevel::GlobalSafe.as_str(), "GlobalSafe");
    }
}
