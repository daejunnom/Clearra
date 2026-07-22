use super::*;

#[test]
fn generic_exact_cover_candidate_schema_validates_marker() {
    assert!(generic_exact_cover_candidate_schema_validates());
}

#[test]
fn exact_cover_problem_schema_tracks_required_and_optional_columns() {
    let schema = ExactCoverProblemSchema::new(
        vec![2, 5],
        vec![PieceUsageConstraint::new("custom:domino", 0, 2)],
        vec![SlotConstraintColumn::new("slot-a", true)],
        vec![AreaConstraintColumn::new("shape", 2)],
        vec![
            ExactCoverColumn::required(0, "cell:2"),
            ExactCoverColumn::required(1, "cell:5"),
        ],
        vec![ExactCoverColumn::optional(0, "piece:custom:domino")],
        vec![ExactCoverCandidateRow::new(10, vec![0, 1], vec![0])],
    )
    .expect("schema");
    let problem = schema.to_problem();

    assert_eq!(schema.cell_universe(), &[2, 5]);
    assert_eq!(
        schema.piece_usage_constraints()[0].piece_id(),
        "custom:domino"
    );
    assert_eq!(schema.slot_constraints()[0].slot_id(), "slot-a");
    assert_eq!(schema.area_constraints()[0].component_area(), 2);
    assert_eq!(problem.required_column_count(), 2);
    assert_eq!(problem.optional_column_count(), 1);
    assert_eq!(problem.candidates()[0].columns(), &[0, 1, 2]);
}

#[test]
fn exact_cover_problem_schema_rejects_invalid_candidate_rows() {
    assert_eq!(
        ExactCoverProblemSchema::new(
            vec![0],
            vec![],
            vec![],
            vec![],
            vec![ExactCoverColumn::required(0, "cell")],
            vec![],
            vec![ExactCoverCandidateRow::new(9, vec![], vec![])],
        ),
        Err(ExactCoverProblemSchemaError::CandidateWithoutRequiredColumns { row_id: 9 })
    );
}
