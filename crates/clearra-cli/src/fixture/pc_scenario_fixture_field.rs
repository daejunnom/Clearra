use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScenarioFixtureSource {
    site: String,
    page: String,
    section: String,
    human_verified: bool,
}

impl ScenarioFixtureSource {
    pub(super) fn source_fields(&self) -> Vec<(String, String)> {
        vec![
            ("fixture_source_site".to_owned(), self.site.clone()),
            ("fixture_source_page".to_owned(), self.page.clone()),
            ("fixture_source_section".to_owned(), self.section.clone()),
            (
                "fixture_source_human_verified".to_owned(),
                self.human_verified.to_string(),
            ),
        ]
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFixtureInput {
    board_width: u16,
    visible_height: u16,
    initial_board_mask: String,
    remaining_queue: String,
    #[serde(default = "default_queue_mode")]
    queue_mode: String,
    hold: Option<char>,
    rule: String,
    requires_180: bool,
    goal: String,
    max_pieces: usize,
    exact_pieces: Option<usize>,
    #[serde(default)]
    min_remaining_queue: Option<usize>,
    #[serde(default)]
    allow_hold: Option<bool>,
    #[serde(default)]
    count_policy: Option<String>,
    #[serde(default)]
    retained_trace_limit: Option<usize>,
    #[serde(default)]
    kick_profile_json: Option<serde_json::Value>,
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    workers: Option<usize>,
    #[serde(default)]
    deterministic: Option<bool>,
    #[serde(default)]
    max_frontier_states: Option<usize>,
    #[serde(default)]
    max_candidates: Option<usize>,
    #[serde(default)]
    max_patterns: Option<usize>,
    #[serde(default)]
    max_memory_mib: Option<usize>,
    #[serde(default)]
    gpu_device: Option<String>,
    #[serde(default)]
    allow_backend_fallback: Option<bool>,
}

impl ScenarioFixtureInput {
    pub fn board_width(&self) -> u16 {
        self.board_width
    }
}
impl ScenarioFixtureInput {
    pub fn visible_height(&self) -> u16 {
        self.visible_height
    }
}
impl ScenarioFixtureInput {
    pub fn initial_board_mask(&self) -> &str {
        &self.initial_board_mask
    }
}
impl ScenarioFixtureInput {
    pub fn remaining_queue(&self) -> &str {
        &self.remaining_queue
    }
}
impl ScenarioFixtureInput {
    pub fn queue_mode(&self) -> &str {
        &self.queue_mode
    }
}
impl ScenarioFixtureInput {
    pub fn hold(&self) -> Option<char> {
        self.hold
    }
}
impl ScenarioFixtureInput {
    pub fn rule(&self) -> &str {
        &self.rule
    }
}
impl ScenarioFixtureInput {
    pub fn requires_180(&self) -> bool {
        self.requires_180
    }
}
impl ScenarioFixtureInput {
    pub fn goal(&self) -> &str {
        &self.goal
    }
}
impl ScenarioFixtureInput {
    pub fn max_pieces(&self) -> usize {
        self.max_pieces
    }
}
impl ScenarioFixtureInput {
    pub fn exact_pieces(&self) -> Option<usize> {
        self.exact_pieces
    }
}
impl ScenarioFixtureInput {
    pub fn min_remaining_queue(&self) -> usize {
        self.min_remaining_queue.unwrap_or(0)
    }
}
impl ScenarioFixtureInput {
    pub fn allow_hold(&self) -> bool {
        self.allow_hold.unwrap_or(true)
    }
}
impl ScenarioFixtureInput {
    pub fn count_policy(&self) -> Option<&str> {
        self.count_policy.as_deref()
    }
}
impl ScenarioFixtureInput {
    pub fn retained_trace_limit(&self) -> Option<usize> {
        self.retained_trace_limit
    }
}
impl ScenarioFixtureInput {
    pub fn kick_profile_json_string(&self) -> Option<String> {
        self.kick_profile_json
            .as_ref()
            .map(|value| value.to_string())
    }
}
impl ScenarioFixtureInput {
    pub fn backend(&self) -> Option<&str> {
        self.backend.as_deref()
    }
}
impl ScenarioFixtureInput {
    pub fn workers(&self) -> Option<usize> {
        self.workers
    }
}
impl ScenarioFixtureInput {
    pub fn deterministic(&self) -> Option<bool> {
        self.deterministic
    }
}
impl ScenarioFixtureInput {
    pub fn max_frontier_states(&self) -> Option<usize> {
        self.max_frontier_states
    }
}
impl ScenarioFixtureInput {
    pub fn max_candidates(&self) -> Option<usize> {
        self.max_candidates
    }

    pub fn max_patterns(&self) -> Option<usize> {
        self.max_patterns
    }
}
impl ScenarioFixtureInput {
    pub fn max_memory_mib(&self) -> Option<usize> {
        self.max_memory_mib
    }
}

fn default_queue_mode() -> String {
    "fixed".to_owned()
}
impl ScenarioFixtureInput {
    pub fn gpu_device(&self) -> Option<&str> {
        self.gpu_device.as_deref()
    }
}
impl ScenarioFixtureInput {
    pub fn allow_backend_fallback(&self) -> Option<bool> {
        self.allow_backend_fallback
    }
}
