use super::drop_score_policy::DropScorePolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DropScorePolicyDescriptor {
    id: DropScorePolicy,
    display_name: &'static str,
    requires_trace_completeness: bool,
}

impl DropScorePolicyDescriptor {
    pub const fn new(
        id: DropScorePolicy,
        display_name: &'static str,
        requires_trace_completeness: bool,
    ) -> Self {
        Self {
            id,
            display_name,
            requires_trace_completeness,
        }
    }
}
impl DropScorePolicyDescriptor {
    pub const fn id(self) -> DropScorePolicy {
        self.id
    }
}
impl DropScorePolicyDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}
impl DropScorePolicyDescriptor {
    pub const fn requires_trace_completeness(self) -> bool {
        self.requires_trace_completeness
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DropScorePolicyRegistry;

impl DropScorePolicyRegistry {
    pub fn builtins() -> Vec<DropScorePolicyDescriptor> {
        vec![
            DropScorePolicyDescriptor::new(DropScorePolicy::Disabled, "Disabled", false),
            DropScorePolicyDescriptor::new(
                DropScorePolicy::HardDrop2SoftDrop1,
                "Hard drop 2 / soft drop 1",
                true,
            ),
        ]
    }
}
