use clearra_pc_graph::request::PcExecutionPolicy;

use crate::dropdown::DropdownOption;

use super::{
    backend_preset_schema::{backend_options, backend_presets, BackendPresetSchema},
    execution_limits_schema::{gpu_device_options, worker_options},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionOptionsSchema {
    backend_options: Vec<DropdownOption>,
    backend_presets: Vec<BackendPresetSchema>,
    worker_options: Vec<DropdownOption>,
    deterministic_default: bool,
    max_frontier_states_default: usize,
    max_memory_mib_default: Option<u64>,
    allow_backend_fallback_default: bool,
    gpu_device_options: Vec<DropdownOption>,
}

impl ExecutionOptionsSchema {
    pub fn mvp2() -> Self {
        let defaults = PcExecutionPolicy::mvp_default();
        let backend_presets = backend_presets();
        Self {
            backend_options: backend_options(&backend_presets),
            backend_presets,
            worker_options: worker_options(),
            deterministic_default: defaults.deterministic(),
            max_frontier_states_default: defaults.max_frontier_states(),
            max_memory_mib_default: defaults.max_memory_mib(),
            allow_backend_fallback_default: defaults.allow_backend_fallback(),
            gpu_device_options: gpu_device_options(),
        }
    }

    pub fn with_gpu_device_inventory<I, S>(mut self, devices: I) -> Self
    where
        I: IntoIterator<Item = (u8, S)>,
        S: Into<String>,
    {
        self.gpu_device_options = gpu_device_options();
        self.gpu_device_options
            .extend(devices.into_iter().map(|(index, display_name)| {
                DropdownOption::new(index.to_string(), display_name.into())
            }));
        self
    }
}
impl ExecutionOptionsSchema {
    pub fn backend_options(&self) -> &[DropdownOption] {
        &self.backend_options
    }
}
impl ExecutionOptionsSchema {
    pub fn backend_presets(&self) -> &[BackendPresetSchema] {
        &self.backend_presets
    }
}
impl ExecutionOptionsSchema {
    pub fn worker_options(&self) -> &[DropdownOption] {
        &self.worker_options
    }
}
impl ExecutionOptionsSchema {
    pub fn deterministic_default(&self) -> bool {
        self.deterministic_default
    }
}
impl ExecutionOptionsSchema {
    pub fn max_frontier_states_default(&self) -> usize {
        self.max_frontier_states_default
    }
}
impl ExecutionOptionsSchema {
    pub fn max_memory_mib_default(&self) -> Option<u64> {
        self.max_memory_mib_default
    }
}
impl ExecutionOptionsSchema {
    pub fn allow_backend_fallback_default(&self) -> bool {
        self.allow_backend_fallback_default
    }
}
impl ExecutionOptionsSchema {
    pub fn gpu_device_options(&self) -> &[DropdownOption] {
        &self.gpu_device_options
    }
}

impl Default for ExecutionOptionsSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[cfg(test)]
#[path = "execution_options_schema_tests.rs"]
mod tests;
