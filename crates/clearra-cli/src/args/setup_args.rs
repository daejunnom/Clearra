use clearra_setup_search::query::{SetupCandidatePriority, SetupLengthPreference, SetupSearchMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupArgs {
    remaining: String,
    queue_based_pieces: Option<String>,
    allow_post_cycle_borrow: bool,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    search_mode: SetupSearchMode,
    path_detail_setup_id: Option<String>,
    path_detail_condition_id: Option<String>,
}

impl SetupArgs {
    pub fn new(remaining: impl Into<String>, allow_post_cycle_borrow: bool) -> Self {
        Self {
            remaining: remaining.into(),
            queue_based_pieces: None,
            allow_post_cycle_borrow,
            candidate_priority: SetupCandidatePriority::default(),
            length_preference: SetupLengthPreference::default(),
            search_mode: SetupSearchMode::default(),
            path_detail_setup_id: None,
            path_detail_condition_id: None,
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

    pub fn length_preference(&self) -> SetupLengthPreference {
        self.length_preference
    }

    pub fn search_mode(&self) -> SetupSearchMode {
        self.search_mode
    }

    pub fn queue_based_pieces(&self) -> Option<&str> {
        self.queue_based_pieces.as_deref()
    }

    pub fn path_detail_setup_id(&self) -> Option<&str> {
        self.path_detail_setup_id.as_deref()
    }

    pub fn path_detail_condition_id(&self) -> Option<&str> {
        self.path_detail_condition_id.as_deref()
    }

    pub fn with_candidate_priority(mut self, priority: SetupCandidatePriority) -> Self {
        self.candidate_priority = priority;
        self
    }

    pub fn with_length_preference(mut self, preference: SetupLengthPreference) -> Self {
        self.length_preference = preference;
        self
    }

    pub fn with_search_mode(mut self, mode: SetupSearchMode) -> Self {
        self.search_mode = mode;
        self
    }

    pub fn with_queue_based_pieces(mut self, pieces: impl Into<String>) -> Self {
        self.queue_based_pieces = Some(pieces.into());
        self.search_mode = SetupSearchMode::QueueBased;
        self
    }

    pub fn with_path_detail(
        mut self,
        setup_id: impl Into<String>,
        condition_id: impl Into<String>,
    ) -> Self {
        self.path_detail_setup_id = Some(setup_id.into());
        self.path_detail_condition_id = Some(condition_id.into());
        self
    }
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self::new("IOTSZJL", false)
    }
}
