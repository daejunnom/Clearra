use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFixtureExpected {
    solution_exists: bool,
    expected_total_solution_count: Option<usize>,
    #[serde(default)]
    count_complete: Option<bool>,
    #[serde(default)]
    unsupported: Option<bool>,
    #[serde(default)]
    unsupported_reason: Option<String>,
    #[serde(default)]
    accepted_retained_trace_keys: Vec<String>,
    #[serde(default)]
    normalized_solution_oracle: Option<String>,
    #[serde(default)]
    expected_normalized_solution_set_hash: Option<String>,
    #[serde(default)]
    expected_normalized_solution_keys: Vec<String>,
    #[serde(default)]
    operation_replay_available: Option<bool>,
}

impl ScenarioFixtureExpected {
    pub fn solution_exists(&self) -> bool {
        self.solution_exists
    }
}
impl ScenarioFixtureExpected {
    pub fn expected_total_solution_count(&self) -> Option<usize> {
        self.expected_total_solution_count
    }
}
impl ScenarioFixtureExpected {
    pub fn count_complete(&self) -> Option<bool> {
        self.count_complete
    }
}
impl ScenarioFixtureExpected {
    pub fn unsupported(&self) -> bool {
        self.unsupported.unwrap_or(false)
    }
}
impl ScenarioFixtureExpected {
    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }
}
impl ScenarioFixtureExpected {
    pub fn accepted_retained_trace_keys(&self) -> &[String] {
        &self.accepted_retained_trace_keys
    }
}
impl ScenarioFixtureExpected {
    pub fn normalized_solution_oracle(&self) -> Option<&str> {
        self.normalized_solution_oracle.as_deref()
    }

    pub fn expected_normalized_solution_set_hash(&self) -> Option<&str> {
        self.expected_normalized_solution_set_hash.as_deref()
    }

    pub fn expected_normalized_solution_keys(&self) -> &[String] {
        &self.expected_normalized_solution_keys
    }

    pub fn operation_replay_available(&self) -> Option<bool> {
        self.operation_replay_available
    }
}
