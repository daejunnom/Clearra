use crate::dropdown::DropdownOption;

pub(crate) fn worker_options() -> Vec<DropdownOption> {
    ["1", "2", "4", "8", "auto"]
        .into_iter()
        .map(|workers| DropdownOption::new(workers, workers))
        .collect()
}

pub(crate) fn gpu_device_options() -> Vec<DropdownOption> {
    vec![DropdownOption::new("auto", "Auto")]
}
