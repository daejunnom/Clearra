use super::pc_scenario_args::PcScenarioArgs;

impl PcScenarioArgs {
    pub fn with_queue(mut self, queue: Option<String>) -> Self {
        self.queue = queue;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_hold(mut self, hold: Option<char>) -> Self {
        self.hold = hold;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_max_pieces(mut self, max_pieces: Option<usize>) -> Self {
        self.max_pieces = max_pieces;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_exact_pieces(mut self, exact_pieces: Option<usize>) -> Self {
        self.exact_pieces = exact_pieces;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_min_remaining_queue(mut self, min_remaining_queue: Option<usize>) -> Self {
        self.min_remaining_queue = min_remaining_queue;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_allow_hold(mut self, allow_hold: Option<bool>) -> Self {
        self.allow_hold = allow_hold;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_count_policy(mut self, count_policy: Option<String>) -> Self {
        self.count_policy = count_policy;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_retained_trace_limit(mut self, retained_trace_limit: Option<usize>) -> Self {
        self.retained_trace_limit = retained_trace_limit;
        self
    }
}
impl PcScenarioArgs {
    pub fn queue(&self) -> Option<&str> {
        self.queue.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn hold(&self) -> Option<char> {
        self.hold
    }
}
impl PcScenarioArgs {
    pub fn max_pieces(&self) -> Option<usize> {
        self.max_pieces
    }
}
impl PcScenarioArgs {
    pub fn exact_pieces(&self) -> Option<usize> {
        self.exact_pieces
    }
}
impl PcScenarioArgs {
    pub fn min_remaining_queue(&self) -> Option<usize> {
        self.min_remaining_queue
    }
}
impl PcScenarioArgs {
    pub fn allow_hold(&self) -> Option<bool> {
        self.allow_hold
    }
}
impl PcScenarioArgs {
    pub fn count_policy(&self) -> Option<&str> {
        self.count_policy.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn retained_trace_limit(&self) -> Option<usize> {
        self.retained_trace_limit
    }
}
