mod host_clock;
mod search_stage_profiler;

pub use host_clock::CooperativeWorkQuantum;
pub(crate) use host_clock::{host_elapsed_ns, host_now, HostInstant};
pub use search_stage_profiler::PcReplayProfileSpan;
#[cfg(feature = "parallel")]
pub(crate) use search_stage_profiler::WorkerSearchProfile;
pub(crate) use search_stage_profiler::{
    profile_sample_scale, ExecutorSearchStage, SearchStageSpan,
};
#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
pub use search_stage_profiler::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
