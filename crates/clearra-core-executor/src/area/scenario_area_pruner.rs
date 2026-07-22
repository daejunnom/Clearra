use crate::board::board_state_backend::BoardStateBackend;

use super::{
    area_component::AreaComponent,
    area_decomposition::{AreaDecomposer, AreaDecomposition, AreaScope},
    area_tileability::{AreaTileabilityReport, AreaTileabilityRules},
    RegionKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaPruneDecision {
    decomposition: AreaDecomposition,
    component_reports: Vec<AreaTileabilityReport>,
    failing_component_index: Option<usize>,
}

impl AreaPruneDecision {
    pub fn new(
        decomposition: AreaDecomposition,
        component_reports: Vec<AreaTileabilityReport>,
        failing_component_index: Option<usize>,
    ) -> Self {
        Self {
            decomposition,
            component_reports,
            failing_component_index,
        }
    }
}
impl AreaPruneDecision {
    pub fn should_prune(&self) -> bool {
        self.failing_component_index.is_some()
    }
}
impl AreaPruneDecision {
    pub fn decomposition(&self) -> &AreaDecomposition {
        &self.decomposition
    }
}
impl AreaPruneDecision {
    pub fn component_reports(&self) -> &[AreaTileabilityReport] {
        &self.component_reports
    }
}
impl AreaPruneDecision {
    pub fn failing_component(&self) -> Option<&AreaComponent> {
        self.failing_component_index
            .and_then(|index| self.decomposition.components().get(index))
    }
}
impl AreaPruneDecision {
    pub fn failing_report(&self) -> Option<&AreaTileabilityReport> {
        self.failing_component_index
            .and_then(|index| self.component_reports.get(index))
    }
}

pub struct ScenarioAreaPruner;

impl ScenarioAreaPruner {
    pub fn check_empty_components<B: BoardStateBackend>(
        board: &B,
        scope: AreaScope,
        rules: &AreaTileabilityRules,
    ) -> AreaPruneDecision {
        let decomposition = AreaDecomposer::decompose_in_scope(board, RegionKind::Empty, scope);
        let component_reports = decomposition
            .components()
            .iter()
            .map(|component| AreaTileabilityReport::check_component(component, rules))
            .collect::<Vec<_>>();
        let failing_component_index = component_reports
            .iter()
            .position(|report| !report.tileable());

        AreaPruneDecision::new(decomposition, component_reports, failing_component_index)
    }
}
impl ScenarioAreaPruner {
    pub fn check_empty_components_below_rows<B: BoardStateBackend>(
        board: &B,
        rows: u16,
        rules: &AreaTileabilityRules,
    ) -> AreaPruneDecision {
        Self::check_empty_components(board, AreaScope::rows_below(rows), rules)
    }
}
