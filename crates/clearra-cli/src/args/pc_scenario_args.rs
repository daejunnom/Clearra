#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PcScenarioArgs {
    pub(super) fixture: Option<String>,
    pub(super) field: Option<String>,
    pub(super) queue: Option<String>,
    pub(super) hold: Option<char>,
    pub(super) rule: Option<String>,
    pub(super) kick_profile_json: Option<String>,
    pub(super) requires_180: bool,
    pub(super) board_width: Option<u16>,
    pub(super) visible_height: Option<u16>,
    pub(super) max_pieces: Option<usize>,
    pub(super) exact_pieces: Option<usize>,
    pub(super) min_remaining_queue: Option<usize>,
    pub(super) allow_hold: Option<bool>,
    pub(super) count_policy: Option<String>,
    pub(super) retained_trace_limit: Option<usize>,
    pub(super) backend: Option<String>,
    pub(super) workers: Option<usize>,
    pub(super) use_all_logical_processors: Option<bool>,
    pub(super) cpu_warmup: Option<bool>,
    pub(super) gpu_warmup: Option<bool>,
    pub(super) deterministic: Option<bool>,
    pub(super) max_frontier_states: Option<usize>,
    pub(super) max_candidates: Option<usize>,
    pub(super) max_patterns: Option<usize>,
    pub(super) max_memory_mib: Option<usize>,
    pub(super) gpu_device: Option<String>,
    pub(super) allow_backend_fallback: Option<bool>,
    pub(super) verify_expected: bool,
    pub(super) solution_probabilities: bool,
}

impl PcScenarioArgs {
    pub fn new(fixture: Option<String>) -> Self {
        Self {
            fixture,
            field: None,
            queue: None,
            hold: None,
            rule: None,
            kick_profile_json: None,
            requires_180: false,
            board_width: None,
            visible_height: None,
            max_pieces: None,
            exact_pieces: None,
            min_remaining_queue: None,
            allow_hold: None,
            count_policy: None,
            retained_trace_limit: None,
            backend: None,
            workers: None,
            use_all_logical_processors: None,
            cpu_warmup: None,
            gpu_warmup: None,
            deterministic: None,
            max_frontier_states: None,
            max_candidates: None,
            max_patterns: None,
            max_memory_mib: None,
            gpu_device: None,
            allow_backend_fallback: None,
            verify_expected: false,
            solution_probabilities: false,
        }
    }
}
impl PcScenarioArgs {
    pub fn fixture(&self) -> Option<&str> {
        self.fixture.as_deref()
    }
}
