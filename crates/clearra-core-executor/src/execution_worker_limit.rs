use clearra_pc_graph::request::WorkerPolicy;

pub(crate) fn hardware_worker_limit() -> usize {
    WorkerPolicy::hardware_worker_limit()
}

#[cfg(test)]
pub(crate) fn clamp_requested_workers(requested: usize, use_all: bool) -> usize {
    WorkerPolicy::clamp_requested(requested, use_all)
}

#[cfg(test)]
mod tests {
    use super::{clamp_requested_workers, hardware_worker_limit};

    #[test]
    fn worker_count_reserves_one_logical_processor_by_default() {
        let expected_limit =
            clearra_core_domain::runtime_cpu_capacity::CpuCapacity::current().hard_limit();
        let expected_default = expected_limit.saturating_sub(1).max(1);
        assert!(clamp_requested_workers(usize::MAX, false) >= 1);
        assert_eq!(hardware_worker_limit(), expected_limit);
        assert_eq!(clamp_requested_workers(usize::MAX, false), expected_default);
        assert_eq!(clamp_requested_workers(usize::MAX, true), expected_limit);
    }
}
