mod host_clock;
mod search_stage_profiler;

pub use host_clock::CooperativeWorkQuantum;
pub(crate) use host_clock::{host_elapsed_ns, host_now, HostInstant};
#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
pub use search_stage_profiler::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
pub(crate) use search_stage_profiler::{ExecutorSearchStage, SearchStageSpan};
