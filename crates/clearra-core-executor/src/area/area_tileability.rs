use clearra_geometry::area::{AreaMultisetFeasibility, StandardTetrominoAreaRule};
use clearra_piece_registry::registry::mixed_piece_set::MixedPieceSet;

use super::area_component::AreaComponent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaTileabilityRules {
    piece_areas: Vec<usize>,
    rule_kind: AreaTileabilityRuleKind,
}

impl AreaTileabilityRules {
    pub fn new(piece_areas: impl IntoIterator<Item = usize>) -> Result<Self, AreaTileabilityError> {
        let piece_areas = piece_areas.into_iter().collect::<Vec<_>>();
        if piece_areas.is_empty() {
            return Err(AreaTileabilityError::EmptyPieceAreas);
        }
        if piece_areas.contains(&0) {
            return Err(AreaTileabilityError::ZeroPieceArea);
        }
        Ok(Self {
            piece_areas,
            rule_kind: AreaTileabilityRuleKind::ActivePieceAreaMultiset,
        })
    }
}
impl AreaTileabilityRules {
    pub fn standard_tetrominoes() -> Self {
        Self {
            piece_areas: vec![StandardTetrominoAreaRule::PIECE_AREA],
            rule_kind: AreaTileabilityRuleKind::StandardTetrominoArea4FastPath,
        }
    }
}
impl AreaTileabilityRules {
    pub fn from_mixed_piece_set(piece_set: &MixedPieceSet) -> Result<Self, AreaTileabilityError> {
        Self::new(piece_set.entries().iter().map(|entry| entry.area()))
    }
}
impl AreaTileabilityRules {
    pub fn piece_areas(&self) -> &[usize] {
        &self.piece_areas
    }
}
impl AreaTileabilityRules {
    pub fn can_compose_area(&self, area: usize) -> bool {
        match self.rule_kind {
            AreaTileabilityRuleKind::StandardTetrominoArea4FastPath => {
                StandardTetrominoAreaRule::can_fill_component_area(area)
            }
            AreaTileabilityRuleKind::ActivePieceAreaMultiset => {
                AreaMultisetFeasibility::new(self.piece_areas.clone())
                    .map(|feasibility| feasibility.can_fill_exactly(area))
                    .unwrap_or(false)
            }
        }
    }
}
impl AreaTileabilityRules {
    pub fn rule_kind(&self) -> AreaTileabilityRuleKind {
        self.rule_kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaTileabilityError {
    EmptyPieceAreas,
    ZeroPieceArea,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaTileabilityRuleKind {
    StandardTetrominoArea4FastPath,
    ActivePieceAreaMultiset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaTileabilityFailure {
    ComponentAreaCannotBeComposed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaTileabilityReport {
    component_area: usize,
    allowed_piece_areas: Vec<usize>,
    tileable: bool,
    failure: Option<AreaTileabilityFailure>,
}

impl AreaTileabilityReport {
    pub fn check_component(component: &AreaComponent, rules: &AreaTileabilityRules) -> Self {
        let component_area = component.area();
        let tileable = rules.can_compose_area(component_area);
        Self {
            component_area,
            allowed_piece_areas: rules.piece_areas().to_vec(),
            tileable,
            failure: (!tileable).then_some(AreaTileabilityFailure::ComponentAreaCannotBeComposed),
        }
    }
}
impl AreaTileabilityReport {
    pub fn component_area(&self) -> usize {
        self.component_area
    }
}
impl AreaTileabilityReport {
    pub fn allowed_piece_areas(&self) -> &[usize] {
        &self.allowed_piece_areas
    }
}
impl AreaTileabilityReport {
    pub fn tileable(&self) -> bool {
        self.tileable
    }
}
impl AreaTileabilityReport {
    pub fn failure(&self) -> Option<AreaTileabilityFailure> {
        self.failure
    }
}

#[cfg(test)]
#[path = "area_tileability_tests.rs"]
mod tests;
