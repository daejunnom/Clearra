use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AreaScopeDescriptor {
    TargetRows { rows: u16 },
    InterpretedTargetCells { cells: BTreeSet<u32> },
    WholeBoardTarget,
}

impl AreaScopeDescriptor {
    pub fn target_rows(rows: u16) -> Result<Self, AreaScopeError> {
        if rows == 0 {
            return Err(AreaScopeError::EmptyTargetRows);
        }
        Ok(Self::TargetRows { rows })
    }
}
impl AreaScopeDescriptor {
    pub fn interpreted_target_cells(
        cells: impl IntoIterator<Item = u32>,
    ) -> Result<Self, AreaScopeError> {
        let cells = cells.into_iter().collect::<BTreeSet<_>>();
        if cells.is_empty() {
            return Err(AreaScopeError::EmptyInterpretedTargetCells);
        }
        Ok(Self::InterpretedTargetCells { cells })
    }
}
impl AreaScopeDescriptor {
    pub fn whole_board_when_truly_target_region() -> Self {
        Self::WholeBoardTarget
    }
}
impl AreaScopeDescriptor {
    pub fn scope_kind(&self) -> &'static str {
        match self {
            Self::TargetRows { .. } => "target-rows",
            Self::InterpretedTargetCells { .. } => "interpreted-target-cells",
            Self::WholeBoardTarget => "whole-board-target",
        }
    }
}
impl AreaScopeDescriptor {
    pub fn is_explicit_target_scope(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaScopeError {
    EmptyTargetRows,
    EmptyInterpretedTargetCells,
}

pub fn scenario_area_pruner_requires_explicit_area_scope() -> bool {
    AreaScopeDescriptor::target_rows(4)
        .expect("target rows")
        .is_explicit_target_scope()
        && AreaScopeDescriptor::interpreted_target_cells([0, 1, 2])
            .expect("cells")
            .is_explicit_target_scope()
        && AreaScopeDescriptor::whole_board_when_truly_target_region().scope_kind()
            == "whole-board-target"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_area_pruner_requires_explicit_area_scope_marker() {
        assert!(scenario_area_pruner_requires_explicit_area_scope());
        assert_eq!(
            AreaScopeDescriptor::target_rows(0),
            Err(AreaScopeError::EmptyTargetRows)
        );
        assert_eq!(
            AreaScopeDescriptor::interpreted_target_cells([]),
            Err(AreaScopeError::EmptyInterpretedTargetCells)
        );
    }
}
