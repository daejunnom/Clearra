use clearra_core_domain::board::board_size::BoardSize;
use clearra_profiles::search::search_defaults::SearchDefaults;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTemplateBridge {
    id: String,
    label: Option<String>,
    board_size: BoardSize,
    slot_count: usize,
}

impl BuildTemplateBridge {
    pub fn new(id: impl Into<String>, board_size: BoardSize, slot_count: usize) -> Self {
        Self {
            id: id.into(),
            label: None,
            board_size,
            slot_count,
        }
    }
}
impl BuildTemplateBridge {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BuildTemplateBridge {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}
impl BuildTemplateBridge {
    pub fn board_size(&self) -> BoardSize {
        self.board_size
    }
}
impl BuildTemplateBridge {
    pub fn slot_count(&self) -> usize {
        self.slot_count
    }
}
impl BuildTemplateBridge {
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildProblemLimits {
    max_assignments: usize,
    max_patterns: usize,
}

impl BuildProblemLimits {
    pub fn new(max_assignments: usize, max_patterns: usize) -> Self {
        Self {
            max_assignments,
            max_patterns,
        }
    }
}
impl BuildProblemLimits {
    pub fn max_assignments(self) -> usize {
        self.max_assignments
    }
}
impl BuildProblemLimits {
    pub fn max_patterns(self) -> usize {
        self.max_patterns
    }
}

impl Default for BuildProblemLimits {
    fn default() -> Self {
        SearchDefaults::MVP1.into()
    }
}

impl From<SearchDefaults> for BuildProblemLimits {
    fn from(defaults: SearchDefaults) -> Self {
        Self {
            max_assignments: defaults.build_max_assignments(),
            max_patterns: defaults.build_max_patterns(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildQuery {
    template: BuildTemplateBridge,
    pattern_count: usize,
    selected_pattern_id: Option<usize>,
    limits: BuildProblemLimits,
}

impl BuildQuery {
    pub fn coverage_bridge(
        template: BuildTemplateBridge,
        pattern_count: usize,
        limits: BuildProblemLimits,
    ) -> Self {
        Self {
            template,
            pattern_count,
            selected_pattern_id: None,
            limits,
        }
    }
}
impl BuildQuery {
    pub fn template(&self) -> &BuildTemplateBridge {
        &self.template
    }
}
impl BuildQuery {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl BuildQuery {
    pub fn selected_pattern_id(&self) -> Option<usize> {
        self.selected_pattern_id
    }
}
impl BuildQuery {
    pub fn with_selected_pattern_id(mut self, pattern_id: usize) -> Self {
        self.selected_pattern_id = Some(pattern_id);
        self
    }
}
impl BuildQuery {
    pub fn limits(&self) -> BuildProblemLimits {
        self.limits
    }
}

#[cfg(test)]
#[path = "build_query_tests.rs"]
mod tests;
