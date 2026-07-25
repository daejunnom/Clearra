use clearra_setup_search::query::SetupCandidatePriority;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupArgs {
    remaining: String,
    allow_post_cycle_borrow: bool,
    candidate_priority: SetupCandidatePriority,
}

impl SetupArgs {
    pub fn new(remaining: impl Into<String>, allow_post_cycle_borrow: bool) -> Self {
        Self {
            remaining: remaining.into(),
            allow_post_cycle_borrow,
            candidate_priority: SetupCandidatePriority::default(),
        }
    }
}
impl SetupArgs {
    pub fn remaining(&self) -> &str {
        &self.remaining
    }
}
impl SetupArgs {
    pub fn allow_post_cycle_borrow(&self) -> bool {
        self.allow_post_cycle_borrow
    }

    pub fn candidate_priority(&self) -> SetupCandidatePriority {
        self.candidate_priority
    }

    pub fn with_candidate_priority(mut self, priority: SetupCandidatePriority) -> Self {
        self.candidate_priority = priority;
        self
    }
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self::new("IOTSZJL", false)
    }
}
