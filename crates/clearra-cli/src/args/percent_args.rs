#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PercentArgs {
    queue: String,
    mode: PercentQueueMode,
    minimum_len: Option<usize>,
    max_patterns: usize,
}

impl PercentArgs {
    pub fn new(queue: impl Into<String>) -> Self {
        Self {
            queue: queue.into(),
            mode: PercentQueueMode::Observed,
            minimum_len: None,
            max_patterns: 0,
        }
    }
}
impl PercentArgs {
    pub fn with_mode(mut self, mode: PercentQueueMode) -> Self {
        self.mode = mode;
        self
    }
}
impl PercentArgs {
    pub fn with_minimum_len(mut self, minimum_len: Option<usize>) -> Self {
        self.minimum_len = minimum_len;
        self
    }
}
impl PercentArgs {
    pub fn with_max_patterns(mut self, max_patterns: usize) -> Self {
        self.max_patterns = max_patterns;
        self
    }
}
impl PercentArgs {
    pub fn queue(&self) -> &str {
        &self.queue
    }
}
impl PercentArgs {
    pub fn mode(&self) -> PercentQueueMode {
        self.mode
    }
}
impl PercentArgs {
    pub fn minimum_len(&self) -> Option<usize> {
        self.minimum_len
    }
}
impl PercentArgs {
    pub fn max_patterns(&self) -> usize {
        self.max_patterns
    }
}

impl Default for PercentArgs {
    fn default() -> Self {
        Self::new("")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PercentQueueMode {
    #[default]
    Observed,
    BagAligned,
    Fixed,
}

impl PercentQueueMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::BagAligned => "bag-aligned",
            Self::Fixed => "fixed",
        }
    }
}
