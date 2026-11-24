/// Stage CPU timings are measured by the native engine. Web hosts own wall
/// timings; an engine value of zero on wasm means unmeasured, never a search
/// deadline or a synthetic duration.
pub(crate) struct StageTimer {
    #[cfg(not(target_family = "wasm"))]
    start: std::time::Instant,
}

impl StageTimer {
    pub(crate) fn start() -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            start: std::time::Instant::now(),
        }
    }

    pub(crate) fn elapsed_ns(&self) -> u64 {
        #[cfg(not(target_family = "wasm"))]
        {
            u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX)
        }
        #[cfg(target_family = "wasm")]
        {
            0
        }
    }
}
