use clearra_app::ResourceBudget;
use clearra_pc_graph::request::WorkerPolicy;

use crate::args::setup_args::SetupArgs;

pub(crate) fn setup_resource_budget(args: &SetupArgs) -> ResourceBudget {
    let hardware_limit = WorkerPolicy::hardware_worker_limit();
    let automatic_workers = WorkerPolicy::Auto
        .effective_for_hardware_limit(args.use_all_logical_processors(), hardware_limit);
    let workers =
        args.workers()
            .filter(|workers| *workers > 0)
            .map_or(automatic_workers, |workers| {
                WorkerPolicy::clamp_requested_for_hardware(
                    workers,
                    args.use_all_logical_processors(),
                    hardware_limit,
                )
            });
    let workers = args
        .automatic_worker_limit()
        .map_or(workers, |limit| workers.min(limit.max(1)));

    ResourceBudget::new(u16::try_from(workers).unwrap_or(u16::MAX), None, None)
}
