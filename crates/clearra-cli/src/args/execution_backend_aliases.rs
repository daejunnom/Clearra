use super::CliParseError;

pub(crate) fn resolve_cpu_execution_aliases(
    backend: Option<String>,
    workers: Option<usize>,
    cpu_threads: Option<usize>,
    no_gpu: bool,
    gpu_device: Option<&str>,
) -> Result<(Option<String>, Option<usize>), CliParseError> {
    if let (Some(workers), Some(cpu_threads)) = (workers, cpu_threads) {
        if workers != cpu_threads {
            return Err(CliParseError::InvalidValue {
                option: "--cpu-threads",
                value: format!("{cpu_threads} conflicts with --workers {workers}"),
            });
        }
    }

    let cpu_only = no_gpu || cpu_threads.is_some();
    if cpu_only {
        if let Some(requested) = backend.as_deref() {
            if !matches!(requested, "auto" | "cpu") {
                return Err(CliParseError::InvalidValue {
                    option: "--backend",
                    value: format!("{requested} conflicts with CPU-only execution"),
                });
            }
        }
        if let Some(device) = gpu_device {
            return Err(CliParseError::InvalidValue {
                option: "--gpu-device",
                value: format!("{device} conflicts with CPU-only execution"),
            });
        }
    }

    Ok((
        cpu_only.then(|| "cpu".to_owned()).or(backend),
        cpu_threads.or(workers),
    ))
}
