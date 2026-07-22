#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBackendKind {
    Contract,
    NativeSkeleton,
    NativeBound,
}

impl MemoryBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::NativeSkeleton => "native-skeleton",
            Self::NativeBound => "native-bound",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryBackendKind;

    #[test]
    fn memory_backend_kind_has_stable_labels() {
        assert_eq!(MemoryBackendKind::Contract.as_str(), "contract");
        assert_eq!(
            MemoryBackendKind::NativeSkeleton.as_str(),
            "native-skeleton"
        );
        assert_eq!(MemoryBackendKind::NativeBound.as_str(), "native-bound");
    }
}
