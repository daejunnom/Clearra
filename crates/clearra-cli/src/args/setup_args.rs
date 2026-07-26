use clearra_setup_search::query::{SetupCandidatePriority, SetupLengthPreference, SetupSearchMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupArgs {
    remaining: String,
    queue_based_pieces: Option<String>,
    next_cycle_remaining_pieces: Option<String>,
    allow_post_cycle_borrow: bool,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    max_setup_pieces: u8,
    search_mode: SetupSearchMode,
    rule: Option<String>,
    initial_hold: Option<String>,
    path_detail_setup_id: Option<String>,
    path_detail_condition_id: Option<String>,
}

impl SetupArgs {
    pub fn new(remaining: impl Into<String>, allow_post_cycle_borrow: bool) -> Self {
        Self {
            remaining: remaining.into(),
            queue_based_pieces: None,
            next_cycle_remaining_pieces: None,
            allow_post_cycle_borrow,
            candidate_priority: SetupCandidatePriority::default(),
            length_preference: SetupLengthPreference::default(),
            max_setup_pieces: 9,
            search_mode: SetupSearchMode::default(),
            rule: None,
            initial_hold: None,
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

    pub fn max_setup_pieces(&self) -> u8 {
        self.max_setup_pieces
    }

    pub fn search_mode(&self) -> SetupSearchMode {
        self.search_mode
    }

    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }

    pub fn queue_based_pieces(&self) -> Option<&str> {
        self.queue_based_pieces.as_deref()
    }

    pub fn next_cycle_remaining_pieces(&self) -> Option<&str> {
        self.next_cycle_remaining_pieces.as_deref()
    }

    pub fn initial_hold(&self) -> Option<&str> {
        self.initial_hold.as_deref()
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

    pub fn with_max_setup_pieces(mut self, max_setup_pieces: u8) -> Self {
        self.max_setup_pieces = max_setup_pieces;
        self
    }

    pub fn with_search_mode(mut self, mode: SetupSearchMode) -> Self {
        self.search_mode = mode;
        self
    }

    pub fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    pub fn with_queue_based_pieces(mut self, pieces: impl Into<String>) -> Self {
        self.queue_based_pieces = Some(pieces.into());
        self.search_mode = SetupSearchMode::QueueBased;
        self
    }

    pub fn with_next_cycle_remaining_pieces(mut self, pieces: impl Into<String>) -> Self {
        self.next_cycle_remaining_pieces = Some(pieces.into());
        self
    }

    pub fn with_initial_hold(mut self, initial_hold: impl Into<String>) -> Self {
        self.initial_hold = Some(initial_hold.into());
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
