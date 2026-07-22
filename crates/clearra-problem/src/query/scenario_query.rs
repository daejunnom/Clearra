use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_pc_graph::{
    classification::{ChainClass, ChainClassifier},
    dag::CheckpointSchedule,
    request::{PcCompletionGoal, PcScenarioBoard, PcScenarioQuery, PieceWindow},
};

use super::{build_query::BuildQuery, pc_query::PcQuery, setup_query::SetupSearchQuery};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioQuerySource {
    OpeningPreset,
    ScenarioPreset,
    SetupPreset,
    BuildPreset,
}

impl ScenarioQuerySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPreset => "opening-preset",
            Self::ScenarioPreset => "scenario-preset",
            Self::SetupPreset => "setup-preset",
            Self::BuildPreset => "build-preset",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioQuery {
    source: ScenarioQuerySource,
    core_query: PcScenarioQuery,
    labels: Vec<String>,
    checkpoint_schedule: Option<CheckpointSchedule>,
    chain_class: ChainClass,
    exact_target_policy: Option<PcTarget>,
    pc_query: Option<PcQuery>,
    setup_query: Option<SetupSearchQuery>,
    build_query: Option<BuildQuery>,
}

impl ScenarioQuery {
    pub fn opening_preset(
        core_query: PcScenarioQuery,
        target: PcTarget,
        labels: Vec<String>,
        pc_query: PcQuery,
    ) -> Self {
        Self {
            source: ScenarioQuerySource::OpeningPreset,
            core_query,
            labels,
            checkpoint_schedule: CheckpointSchedule::for_opening_target(target).ok(),
            chain_class: ChainClassifier::opening(target),
            exact_target_policy: Some(target),
            pc_query: Some(pc_query),
            setup_query: None,
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn scenario_preset(core_query: PcScenarioQuery) -> Self {
        Self {
            source: ScenarioQuerySource::ScenarioPreset,
            core_query,
            labels: Vec::new(),
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: None,
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn setup_preset(core_query: PcScenarioQuery, setup_query: SetupSearchQuery) -> Self {
        Self {
            source: ScenarioQuerySource::SetupPreset,
            core_query,
            labels: vec!["setup".to_owned()],
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: Some(setup_query),
            build_query: None,
        }
    }
}
impl ScenarioQuery {
    pub fn build_preset(core_query: PcScenarioQuery, build_query: BuildQuery) -> Self {
        let label = format!("build:{}", build_query.template().id());
        Self {
            source: ScenarioQuerySource::BuildPreset,
            core_query,
            labels: vec![label],
            checkpoint_schedule: None,
            chain_class: ChainClassifier::scenario(),
            exact_target_policy: None,
            pc_query: None,
            setup_query: None,
            build_query: Some(build_query),
        }
    }
}
impl ScenarioQuery {
    pub fn source(&self) -> ScenarioQuerySource {
        self.source
    }
}
impl ScenarioQuery {
    pub fn core_query(&self) -> &PcScenarioQuery {
        &self.core_query
    }
}
impl ScenarioQuery {
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}
impl ScenarioQuery {
    pub fn checkpoint_schedule(&self) -> Option<&CheckpointSchedule> {
        self.checkpoint_schedule.as_ref()
    }
}
impl ScenarioQuery {
    pub fn chain_class(&self) -> ChainClass {
        self.chain_class
    }
}
impl ScenarioQuery {
    pub fn exact_target_policy(&self) -> Option<PcTarget> {
        self.exact_target_policy
    }
}
impl ScenarioQuery {
    pub fn pc_query(&self) -> Option<&PcQuery> {
        self.pc_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn setup_query(&self) -> Option<&SetupSearchQuery> {
        self.setup_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn build_query(&self) -> Option<&BuildQuery> {
        self.build_query.as_ref()
    }
}
impl ScenarioQuery {
    pub fn initial_board(&self) -> &PcScenarioBoard {
        self.core_query.initial_board()
    }
}
impl ScenarioQuery {
    pub fn piece_window(&self) -> PieceWindow {
        self.core_query.piece_window()
    }
}
impl ScenarioQuery {
    pub fn exact_pieces(&self) -> Option<usize> {
        self.core_query.exact_pieces()
    }
}
impl ScenarioQuery {
    pub fn goal(&self) -> PcCompletionGoal {
        self.core_query.completion_goal()
    }
}
