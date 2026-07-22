use clearra_geometry::area::{AreaMultisetFeasibility, AreaScopeDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileAreaPruneInput {
    scope: AreaScopeDescriptor,
    component_areas: Vec<usize>,
    feasibility: AreaMultisetFeasibility,
}

impl CompileAreaPruneInput {
    pub fn new(
        scope: AreaScopeDescriptor,
        component_areas: impl IntoIterator<Item = usize>,
        feasibility: AreaMultisetFeasibility,
    ) -> Result<Self, AreaPrunerError> {
        let component_areas = component_areas.into_iter().collect::<Vec<_>>();
        if component_areas.is_empty() {
            return Err(AreaPrunerError::NoComponents);
        }
        if component_areas.contains(&0) {
            return Err(AreaPrunerError::ZeroAreaComponent);
        }

        Ok(Self {
            scope,
            component_areas,
            feasibility,
        })
    }
}
impl CompileAreaPruneInput {
    pub fn scope(&self) -> &AreaScopeDescriptor {
        &self.scope
    }
}
impl CompileAreaPruneInput {
    pub fn component_areas(&self) -> &[usize] {
        &self.component_areas
    }
}
impl CompileAreaPruneInput {
    pub fn feasibility(&self) -> &AreaMultisetFeasibility {
        &self.feasibility
    }
}

pub struct CompileAreaPruner;

impl CompileAreaPruner {
    pub fn evaluate(input: &CompileAreaPruneInput) -> AreaPrunerDecision {
        let failing_component = input
            .component_areas()
            .iter()
            .copied()
            .find(|area| !input.feasibility().can_fill_exactly(*area));

        match failing_component {
            Some(component_area) => AreaPrunerDecision::RejectAreaInfeasible {
                scope_kind: input.scope().scope_kind(),
                component_area,
            },
            None => AreaPrunerDecision::SearchMayContinue {
                scope_kind: input.scope().scope_kind(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AreaPrunerDecision {
    RejectAreaInfeasible {
        scope_kind: &'static str,
        component_area: usize,
    },
    SearchMayContinue {
        scope_kind: &'static str,
    },
}

impl AreaPrunerDecision {
    pub fn is_solution_found(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaPrunerError {
    NoComponents,
    ZeroAreaComponent,
}

pub fn scenario_area_pruner_requires_explicit_area_scope() -> bool {
    CompileAreaPruneInput::new(
        AreaScopeDescriptor::target_rows(4).expect("target rows"),
        [4],
        AreaMultisetFeasibility::standard_tetrominoes(1).expect("areas"),
    )
    .map(|input| input.scope().is_explicit_target_scope())
    .unwrap_or(false)
}

#[cfg(test)]
#[path = "area_pruner_tests.rs"]
mod tests;
