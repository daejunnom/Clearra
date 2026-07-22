use clearra_geometry::area::AreaMultisetFeasibility;

use super::*;

#[test]
fn area_decomposition_is_necessary_condition_not_solver() {
    let feasibility = AreaMultisetFeasibility::new([4, 3]).expect("areas");

    let decision = AreaMultisetExactCoverBridge::check_before_exact_cover(7, &feasibility);

    assert_eq!(
        decision,
        AreaMultisetExactCoverDecision::NecessaryConditionPassed
    );
    assert!(!decision.is_solution_found());
}

#[test]
fn area_infeasible_rejects_before_expensive_search() {
    let feasibility = AreaMultisetFeasibility::new([4, 3]).expect("areas");

    let decision = AreaMultisetExactCoverBridge::check_before_exact_cover(5, &feasibility);

    assert_eq!(
        decision,
        AreaMultisetExactCoverDecision::RejectBeforeExactCover { component_area: 5 }
    );
}
