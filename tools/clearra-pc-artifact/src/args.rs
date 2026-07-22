use std::path::PathBuf;

const MEMORY_BOUNDED_INDEX_LIMIT: usize = u32::MAX as usize - 1;

use clearra_pc_graph::request::{GpuDeviceSelection, RequestedSearchBackend};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactRule {
    SrsPlus,
    Srs,
}

impl ArtifactRule {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "srs-plus" | "srs+" => Some(Self::SrsPlus),
            "srs" => Some(Self::Srs),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SrsPlus => "srs-plus",
            Self::Srs => "srs",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactScenario {
    Pco6p,
    TsarCannon,
    Empty4l,
}

impl ArtifactScenario {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pco-6p" | "pco6p" | "pco" => Some(Self::Pco6p),
            "tsar-cannon" | "tsar" => Some(Self::TsarCannon),
            "empty-4l" | "empty4l" | "4l" => Some(Self::Empty4l),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pco6p => "pco-6p",
            Self::TsarCannon => "tsar-cannon",
            Self::Empty4l => "empty-4l",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactCountMode {
    All,
    Unique,
}

impl ArtifactCountMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "all" | "count-all" => Some(Self::All),
            "unique" | "count-unique" => Some(Self::Unique),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "count-all",
            Self::Unique => "count-unique",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactArgs {
    pub list_gpu_devices: bool,
    pub probe_gpu_warmup: bool,
    pub prewarm_gpu: bool,
    pub profile_stages: bool,
    pub scenario: ArtifactScenario,
    pub scenarios: Vec<ArtifactScenario>,
    pub rule: ArtifactRule,
    pub count_mode: Option<ArtifactCountMode>,
    pub backend: RequestedSearchBackend,
    pub gpu_device: GpuDeviceSelection,
    pub workers: usize,
    pub use_all_logical_processors: bool,
    pub cpu_warmup: bool,
    pub max_patterns: Option<usize>,
    pub max_candidates: usize,
    pub max_frontier_states: usize,
    pub allow_fallback: bool,
    pub emit_fumen: bool,
    pub output_directory: PathBuf,
    pub sfinder_html: Option<PathBuf>,
}

impl ArtifactArgs {
    pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut scenarios = Vec::new();
        let mut list_gpu_devices = false;
        let mut probe_gpu_warmup = false;
        let mut prewarm_gpu = false;
        let mut profile_stages = false;
        let mut rule = ArtifactRule::SrsPlus;
        let mut count_mode = None;
        let mut backend = RequestedSearchBackend::Auto;
        let mut gpu_device = GpuDeviceSelection::Auto;
        let logical_processors = std::thread::available_parallelism().map_or(1, usize::from);
        let mut workers = logical_processors.saturating_sub(1).max(1);
        let mut workers_explicit = false;
        let mut use_all_logical_processors = false;
        let mut cpu_warmup = false;
        let mut max_patterns = None;
        let mut max_candidates = MEMORY_BOUNDED_INDEX_LIMIT;
        let mut max_frontier_states = MEMORY_BOUNDED_INDEX_LIMIT;
        let mut allow_fallback = true;
        let mut emit_fumen = true;
        let mut output_directory = None;
        let mut sfinder_html = None;
        let mut arguments = arguments.into_iter();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--list-gpu-devices" => list_gpu_devices = true,
                "--probe-gpu-warmup" => probe_gpu_warmup = true,
                "--prewarm-gpu" => prewarm_gpu = true,
                "--profile-stages" => profile_stages = true,
                "--scenario" => {
                    let value = required_value(&mut arguments, "--scenario")?;
                    scenarios.push(
                        ArtifactScenario::parse(&value)
                            .ok_or_else(|| format!("unsupported scenario: {value}"))?,
                    );
                }
                "--count" => {
                    let value = required_value(&mut arguments, "--count")?;
                    count_mode = Some(
                        ArtifactCountMode::parse(&value)
                            .ok_or_else(|| format!("unsupported count mode: {value}"))?,
                    );
                }
                "--rule" => {
                    let value = required_value(&mut arguments, "--rule")?;
                    rule = ArtifactRule::parse(&value)
                        .ok_or_else(|| format!("unsupported rule profile: {value}"))?;
                }
                "--backend" => {
                    let value = required_value(&mut arguments, "--backend")?;
                    backend = RequestedSearchBackend::parse(&value)
                        .ok_or_else(|| format!("unsupported backend: {value}"))?;
                }
                "--gpu-device" => {
                    let value = required_value(&mut arguments, "--gpu-device")?;
                    gpu_device = GpuDeviceSelection::parse(&value)
                        .ok_or_else(|| format!("unsupported GPU device selection: {value}"))?;
                }
                "--workers" => {
                    workers = parse_positive_usize(
                        &required_value(&mut arguments, "--workers")?,
                        "--workers",
                    )?;
                    workers_explicit = true;
                }
                "--use-all-cpu-threads" => use_all_logical_processors = true,
                "--cpu-warmup" => cpu_warmup = true,
                "--max-patterns" => {
                    max_patterns = Some(parse_positive_usize(
                        &required_value(&mut arguments, "--max-patterns")?,
                        "--max-patterns",
                    )?);
                }
                "--max-candidates" => {
                    max_candidates = parse_positive_usize(
                        &required_value(&mut arguments, "--max-candidates")?,
                        "--max-candidates",
                    )?;
                }
                "--max-frontier-states" => {
                    max_frontier_states = parse_positive_usize(
                        &required_value(&mut arguments, "--max-frontier-states")?,
                        "--max-frontier-states",
                    )?;
                }
                "--no-fallback" => allow_fallback = false,
                "--skip-fumen" => emit_fumen = false,
                "--output-dir" => {
                    output_directory = Some(PathBuf::from(required_value(
                        &mut arguments,
                        "--output-dir",
                    )?));
                }
                "--sfinder-html" => {
                    sfinder_html = Some(PathBuf::from(required_value(
                        &mut arguments,
                        "--sfinder-html",
                    )?));
                }
                "--help" | "-h" => return Err(usage().to_owned()),
                unknown => return Err(format!("unknown argument: {unknown}\n{}", usage())),
            }
        }

        if list_gpu_devices && probe_gpu_warmup {
            return Err(
                "--list-gpu-devices and --probe-gpu-warmup are mutually exclusive".to_owned(),
            );
        }
        if scenarios.is_empty() {
            scenarios.push(ArtifactScenario::Pco6p);
        }
        if scenarios.len() > 1 && sfinder_html.is_some() {
            return Err("--sfinder-html is available only for a single scenario".to_owned());
        }
        let scenario = scenarios[0];
        if use_all_logical_processors && !workers_explicit {
            workers = logical_processors;
        }
        if workers > logical_processors {
            return Err(format!(
                "--workers {workers} exceeds the hard logical processor limit {logical_processors}"
            ));
        }
        let default_worker_limit = logical_processors.saturating_sub(1).max(1);
        if workers > default_worker_limit && !use_all_logical_processors {
            return Err(format!(
                "--workers {workers} uses the reserved logical processor; pass --use-all-cpu-threads to opt in"
            ));
        }
        let diagnostic_only = list_gpu_devices || probe_gpu_warmup;
        let output_directory = match (diagnostic_only, output_directory) {
            (true, None) => PathBuf::new(),
            (_, Some(path)) => path,
            (false, None) => {
                return Err(format!("--output-dir is required\n{}", usage()));
            }
        };
        Ok(Self {
            list_gpu_devices,
            probe_gpu_warmup,
            prewarm_gpu,
            profile_stages,
            scenario,
            scenarios,
            rule,
            count_mode,
            backend,
            gpu_device,
            workers,
            use_all_logical_processors,
            cpu_warmup,
            max_patterns,
            max_candidates,
            max_frontier_states,
            allow_fallback,
            emit_fumen,
            output_directory,
            sfinder_html,
        })
    }
}

fn required_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_positive_usize(value: &str, option: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{option} requires a positive integer: {value}"))?;
    if parsed == 0 {
        return Err(format!("{option} must be at least 1"));
    }
    Ok(parsed)
}

fn usage() -> &'static str {
    "usage: clearra-pc-artifact --list-gpu-devices\n       clearra-pc-artifact --probe-gpu-warmup [--gpu-device auto|N]\n       clearra-pc-artifact --output-dir PATH [--scenario pco-6p|tsar-cannon|empty-4l]... [--rule srs-plus|srs] [--count all|unique] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--prewarm-gpu] [--profile-stages] [--workers N] [--use-all-cpu-threads] [--cpu-warmup] [--max-patterns N] [--max-candidates N] [--max-frontier-states N] [--sfinder-html PATH] [--skip-fumen] [--no-fallback]"
}
