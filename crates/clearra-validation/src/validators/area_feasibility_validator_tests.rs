use super::*;

#[test]
fn area_multiset_feasibility_uses_piece_area_multiset() {
    let report = AreaFeasibilityValidator::validate_components(
        &AreaScopeDescriptor::target_rows(4).expect("scope"),
        &[7],
        &AreaMultisetFeasibility::new([4, 3]).expect("areas"),
    );

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IAreaNecessaryConditionPassed));
    assert!(report
        .diagnostics()
        .iter()
        .flat_map(|diagnostic| diagnostic.evidence())
        .any(
            |evidence| evidence.key() == "area_feasible_is_solution_found"
                && evidence.value() == "false"
        ));
}

#[test]
fn area_infeasible_reports_error_before_search() {
    let report = AreaFeasibilityValidator::validate_components(
        &AreaScopeDescriptor::interpreted_target_cells([0, 1, 2, 3, 4]).expect("scope"),
        &[5],
        &AreaMultisetFeasibility::new([4, 3]).expect("areas"),
    );

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EAreaInfeasible));
}
