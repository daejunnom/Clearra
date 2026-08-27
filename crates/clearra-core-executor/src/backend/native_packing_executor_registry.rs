use super::SearchBackendExecutorResolver;

#[cfg(test)]
use super::{
    NativeCpuPackingExecutor, NativeGpuPackingExecutor, NativeParallelPackingExecutor,
    SearchBackendExecutor, SelectedSearchBackend,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativePackingExecutorRegistry {
    #[cfg(test)]
    cpu_geometry_exact_cover: NativeCpuPackingExecutor,
    #[cfg(test)]
    cpu_parallel_geometry_exact_cover: NativeParallelPackingExecutor,
    #[cfg(test)]
    gpu: NativeGpuPackingExecutor,
}

impl Default for NativePackingExecutorRegistry {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            cpu_geometry_exact_cover: NativeCpuPackingExecutor::new(
                SelectedSearchBackend::CpuGeometryExactCover,
            ),
            #[cfg(test)]
            cpu_parallel_geometry_exact_cover: NativeParallelPackingExecutor,
            #[cfg(test)]
            gpu: NativeGpuPackingExecutor,
        }
    }
}

impl SearchBackendExecutorResolver for NativePackingExecutorRegistry {
    #[cfg(test)]
    fn executor_for(&self, backend: SelectedSearchBackend) -> Option<&dyn SearchBackendExecutor> {
        match backend {
            SelectedSearchBackend::None | SelectedSearchBackend::Hybrid => None,
            SelectedSearchBackend::CpuGeometryExactCover => Some(&self.cpu_geometry_exact_cover),
            SelectedSearchBackend::CpuParallelGeometryExactCover => {
                Some(&self.cpu_parallel_geometry_exact_cover)
            }
            SelectedSearchBackend::Gpu => Some(&self.gpu),
        }
    }

    #[cfg(test)]
    fn cpu_fallback_executor(&self) -> &dyn SearchBackendExecutor {
        &self.cpu_geometry_exact_cover
    }

    fn supports_native_candidate_streaming(&self) -> bool {
        // The buildable streaming path owns native catalog allocation and its
        // shared resource lease.  When the native C core is not linked there is
        // no allocator to govern, so report the runtime as unavailable before
        // attempting resource admission or materializing the packing family.
        cfg!(feature = "native-c-core")
    }
}

#[cfg(test)]
mod tests {
    use super::{NativePackingExecutorRegistry, SearchBackendExecutorResolver};

    #[test]
    fn native_candidate_streaming_requires_the_linked_native_feature() {
        assert_eq!(
            NativePackingExecutorRegistry::default().supports_native_candidate_streaming(),
            cfg!(feature = "native-c-core")
        );
    }
}
