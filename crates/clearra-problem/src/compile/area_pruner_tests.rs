use super::*;

#[test]
fn scenario_area_pruner_requires_explicit_area_scope_marker() {
    assert!(scenario_area_pruner_requires_explicit_area_scope());
}

#[test]
fn area_decomposition_is_necessary_condition_not_solver() {
    let input = CompileAreaPruneInput::new(
        AreaScopeDescriptor::target_rows(4).expect("rows"),
        [7],
        AreaMultisetFeasibility::new([4, 3]).expect("areas"),
    )
    .expect("input");

    let decision = CompileAreaPruner::evaluate(&input);

    assert_eq!(
        decision,
        AreaPrunerDecision::SearchMayContinue {
            scope_kind: "target-rows"
        }
    );
    assert!(!decision.is_solution_found());
}

#[test]
fn area_infeasible_rejects_before_compile_search() {
    let input = CompileAreaPruneInput::new(
        AreaScopeDescriptor::interpreted_target_cells([0, 1, 2, 3, 4]).expect("cells"),
        [5],
        AreaMultisetFeasibility::new([4, 3]).expect("areas"),
    )
    .expect("input");

    assert_eq!(
        CompileAreaPruner::evaluate(&input),
        AreaPrunerDecision::RejectAreaInfeasible {
            scope_kind: "interpreted-target-cells",
            component_area: 5
        }
    );
}
