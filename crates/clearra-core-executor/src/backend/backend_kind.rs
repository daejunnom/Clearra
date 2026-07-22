use super::SelectedSearchBackend;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendKind {
    #[default]
    None,
    Cpu,
    Gpu,
    Hybrid,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Hybrid => "hybrid",
        }
    }
}
impl BackendKind {
    pub fn from_selected_backend(backend: SelectedSearchBackend) -> Self {
        match backend {
            SelectedSearchBackend::None => Self::None,
            SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover => Self::Cpu,
            SelectedSearchBackend::Gpu => Self::Gpu,
            SelectedSearchBackend::Hybrid => Self::Hybrid,
        }
    }
}

impl From<SelectedSearchBackend> for BackendKind {
    fn from(value: SelectedSearchBackend) -> Self {
        Self::from_selected_backend(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_classifies_selected_backend_compute_family() {
        assert_eq!(
            BackendKind::from_selected_backend(SelectedSearchBackend::CpuGeometryExactCover),
            BackendKind::Cpu
        );
    }
}
