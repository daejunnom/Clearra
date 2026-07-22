use clearra_pc_graph::request::PcCompletionGoal;

use super::spin_target_goal::SpinTargetRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTemplateGoal {
    template_id: String,
}

impl BuildTemplateGoal {
    pub fn new(template_id: impl Into<String>) -> Self {
        Self {
            template_id: template_id.into(),
        }
    }
}
impl BuildTemplateGoal {
    pub fn template_id(&self) -> &str {
        &self.template_id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompositeGoal {
    goals: Vec<SearchGoal>,
}

impl CompositeGoal {
    pub fn new(goals: Vec<SearchGoal>) -> Self {
        Self { goals }
    }
}
impl CompositeGoal {
    pub fn clear_then_spin(spin_target: SpinTargetRequest) -> Self {
        Self::new(vec![
            SearchGoal::ClearToEmpty,
            SearchGoal::SpinTarget(spin_target),
        ])
    }
}
impl CompositeGoal {
    pub fn goals(&self) -> &[SearchGoal] {
        &self.goals
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchGoal {
    ClearToEmpty,
    BuildTemplate(BuildTemplateGoal),
    SpinTarget(SpinTargetRequest),
    Composite(CompositeGoal),
}

impl SearchGoal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClearToEmpty => "clear-to-empty",
            Self::BuildTemplate(_) => "build-template",
            Self::SpinTarget(_) => "spin-target",
            Self::Composite(_) => "composite",
        }
    }
}
impl SearchGoal {
    pub fn completion_goal(&self) -> PcCompletionGoal {
        PcCompletionGoal::ClearToEmpty
    }
}
impl SearchGoal {
    pub fn spin_target(&self) -> Option<&SpinTargetRequest> {
        match self {
            Self::SpinTarget(target) => Some(target),
            Self::Composite(composite) => {
                composite.goals().iter().find_map(SearchGoal::spin_target)
            }
            Self::ClearToEmpty | Self::BuildTemplate(_) => None,
        }
    }
}
impl SearchGoal {
    pub fn is_spin_target(&self) -> bool {
        matches!(self, Self::SpinTarget(_))
    }
}
