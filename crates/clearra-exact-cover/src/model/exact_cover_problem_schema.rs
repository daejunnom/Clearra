mod area_constraint_column {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct AreaConstraintColumn {
        label: String,
        component_area: usize,
    }

    impl AreaConstraintColumn {
        pub fn new(label: impl Into<String>, component_area: usize) -> Self {
            Self {
                label: label.into(),
                component_area,
            }
        }
    }
    impl AreaConstraintColumn {
        pub fn label(&self) -> &str {
            &self.label
        }
    }
    impl AreaConstraintColumn {
        pub fn component_area(&self) -> usize {
            self.component_area
        }
    }
}
mod candidate_row {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExactCoverCandidateRow {
        row_id: usize,
        required_columns: Vec<usize>,
        optional_columns: Vec<usize>,
    }

    impl ExactCoverCandidateRow {
        pub fn new(
            row_id: usize,
            required_columns: Vec<usize>,
            optional_columns: Vec<usize>,
        ) -> Self {
            Self {
                row_id,
                required_columns,
                optional_columns,
            }
        }
    }
    impl ExactCoverCandidateRow {
        pub fn row_id(&self) -> usize {
            self.row_id
        }
    }
    impl ExactCoverCandidateRow {
        pub fn required_columns(&self) -> &[usize] {
            &self.required_columns
        }
    }
    impl ExactCoverCandidateRow {
        pub fn optional_columns(&self) -> &[usize] {
            &self.optional_columns
        }
    }
}
mod column {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExactCoverColumn {
        id: usize,
        label: String,
        kind: ExactCoverColumnKind,
    }

    impl ExactCoverColumn {
        pub fn required(id: usize, label: impl Into<String>) -> Self {
            Self {
                id,
                label: label.into(),
                kind: ExactCoverColumnKind::Required,
            }
        }
    }
    impl ExactCoverColumn {
        pub fn optional(id: usize, label: impl Into<String>) -> Self {
            Self {
                id,
                label: label.into(),
                kind: ExactCoverColumnKind::Optional,
            }
        }
    }
    impl ExactCoverColumn {
        pub fn id(&self) -> usize {
            self.id
        }
    }
    impl ExactCoverColumn {
        pub fn label(&self) -> &str {
            &self.label
        }
    }
    impl ExactCoverColumn {
        pub fn kind(&self) -> ExactCoverColumnKind {
            self.kind
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ExactCoverColumnKind {
        Required,
        Optional,
    }
}
mod error {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum ExactCoverProblemSchemaError {
        EmptyCellUniverse,
        EmptyRequiredColumns,
        EmptyCandidateRows,
        InvalidPieceUsageConstraint { piece_id: String },
        ZeroAreaConstraint { label: String },
        CandidateWithoutRequiredColumns { row_id: usize },
        RequiredColumnOutOfRange { row_id: usize, column: usize },
        OptionalColumnOutOfRange { row_id: usize, column: usize },
    }
}
mod generic_schema_fixture {
    use super::{
        AreaConstraintColumn, ExactCoverCandidateRow, ExactCoverColumn, ExactCoverProblemSchema,
        PieceUsageConstraint, SlotConstraintColumn,
    };

    pub fn generic_exact_cover_candidate_schema_validates() -> bool {
        let schema = ExactCoverProblemSchema::new(
            vec![0, 1],
            vec![PieceUsageConstraint::new("std:I", 0, 1)],
            vec![SlotConstraintColumn::new("slot-a", true)],
            vec![AreaConstraintColumn::new("area-a", 2)],
            vec![ExactCoverColumn::required(0, "cell:0")],
            vec![ExactCoverColumn::optional(0, "piece:std:I")],
            vec![ExactCoverCandidateRow::new(1, vec![0], vec![0])],
        )
        .expect("schema");

        schema.to_problem().required_column_count() == 1
            && schema.to_problem().optional_column_count() == 1
    }
}
mod piece_usage_constraint {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PieceUsageConstraint {
        piece_id: String,
        min_usage: usize,
        max_usage: usize,
    }

    impl PieceUsageConstraint {
        pub fn new(piece_id: impl Into<String>, min_usage: usize, max_usage: usize) -> Self {
            Self {
                piece_id: piece_id.into(),
                min_usage,
                max_usage,
            }
        }
    }
    impl PieceUsageConstraint {
        pub fn piece_id(&self) -> &str {
            &self.piece_id
        }
    }
    impl PieceUsageConstraint {
        pub fn min_usage(&self) -> usize {
            self.min_usage
        }
    }
    impl PieceUsageConstraint {
        pub fn max_usage(&self) -> usize {
            self.max_usage
        }
    }
}
mod schema {
    use crate::model::{ExactCoverCandidate, ExactCoverProblem};

    use super::{
        schema_validator::validate_schema, AreaConstraintColumn, ExactCoverCandidateRow,
        ExactCoverColumn, ExactCoverProblemSchemaError, PieceUsageConstraint, SlotConstraintColumn,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ExactCoverProblemSchema {
        cell_universe: Vec<u32>,
        piece_usage_constraints: Vec<PieceUsageConstraint>,
        slot_constraints: Vec<SlotConstraintColumn>,
        area_constraints: Vec<AreaConstraintColumn>,
        required_columns: Vec<ExactCoverColumn>,
        optional_columns: Vec<ExactCoverColumn>,
        candidate_rows: Vec<ExactCoverCandidateRow>,
    }

    impl ExactCoverProblemSchema {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            cell_universe: Vec<u32>,
            piece_usage_constraints: Vec<PieceUsageConstraint>,
            slot_constraints: Vec<SlotConstraintColumn>,
            area_constraints: Vec<AreaConstraintColumn>,
            required_columns: Vec<ExactCoverColumn>,
            optional_columns: Vec<ExactCoverColumn>,
            candidate_rows: Vec<ExactCoverCandidateRow>,
        ) -> Result<Self, ExactCoverProblemSchemaError> {
            validate_schema(
                &cell_universe,
                &piece_usage_constraints,
                &area_constraints,
                &required_columns,
                &optional_columns,
                &candidate_rows,
            )?;
            Ok(Self {
                cell_universe,
                piece_usage_constraints,
                slot_constraints,
                area_constraints,
                required_columns,
                optional_columns,
                candidate_rows,
            })
        }
    }
    impl ExactCoverProblemSchema {
        pub fn to_problem(&self) -> ExactCoverProblem {
            let required_count = self.required_columns.len();
            let candidates = self
                .candidate_rows
                .iter()
                .map(|row| {
                    let mut columns = row.required_columns().to_vec();
                    columns.extend(
                        row.optional_columns()
                            .iter()
                            .map(|column| required_count + column),
                    );
                    ExactCoverCandidate::new(row.row_id(), columns)
                })
                .collect();
            ExactCoverProblem::with_optional_columns(
                self.required_columns.len(),
                self.optional_columns.len(),
                candidates,
            )
        }
    }
    impl ExactCoverProblemSchema {
        pub fn cell_universe(&self) -> &[u32] {
            &self.cell_universe
        }
    }
    impl ExactCoverProblemSchema {
        pub fn piece_usage_constraints(&self) -> &[PieceUsageConstraint] {
            &self.piece_usage_constraints
        }
    }
    impl ExactCoverProblemSchema {
        pub fn slot_constraints(&self) -> &[SlotConstraintColumn] {
            &self.slot_constraints
        }
    }
    impl ExactCoverProblemSchema {
        pub fn area_constraints(&self) -> &[AreaConstraintColumn] {
            &self.area_constraints
        }
    }
    impl ExactCoverProblemSchema {
        pub fn required_columns(&self) -> &[ExactCoverColumn] {
            &self.required_columns
        }
    }
    impl ExactCoverProblemSchema {
        pub fn optional_columns(&self) -> &[ExactCoverColumn] {
            &self.optional_columns
        }
    }
    impl ExactCoverProblemSchema {
        pub fn candidate_rows(&self) -> &[ExactCoverCandidateRow] {
            &self.candidate_rows
        }
    }
}
mod schema_validator {
    use super::{
        AreaConstraintColumn, ExactCoverCandidateRow, ExactCoverColumn,
        ExactCoverProblemSchemaError, PieceUsageConstraint,
    };

    pub(super) fn validate_schema(
        cell_universe: &[u32],
        piece_usage_constraints: &[PieceUsageConstraint],
        area_constraints: &[AreaConstraintColumn],
        required_columns: &[ExactCoverColumn],
        optional_columns: &[ExactCoverColumn],
        candidate_rows: &[ExactCoverCandidateRow],
    ) -> Result<(), ExactCoverProblemSchemaError> {
        if cell_universe.is_empty() {
            return Err(ExactCoverProblemSchemaError::EmptyCellUniverse);
        }
        if required_columns.is_empty() {
            return Err(ExactCoverProblemSchemaError::EmptyRequiredColumns);
        }
        if candidate_rows.is_empty() {
            return Err(ExactCoverProblemSchemaError::EmptyCandidateRows);
        }
        for constraint in piece_usage_constraints {
            if constraint.piece_id().is_empty() || constraint.min_usage() > constraint.max_usage() {
                return Err(ExactCoverProblemSchemaError::InvalidPieceUsageConstraint {
                    piece_id: constraint.piece_id().to_owned(),
                });
            }
        }
        for constraint in area_constraints {
            if constraint.component_area() == 0 {
                return Err(ExactCoverProblemSchemaError::ZeroAreaConstraint {
                    label: constraint.label().to_owned(),
                });
            }
        }
        for row in candidate_rows {
            validate_candidate_row(row, required_columns.len(), optional_columns.len())?;
        }
        Ok(())
    }

    fn validate_candidate_row(
        row: &ExactCoverCandidateRow,
        required_column_count: usize,
        optional_column_count: usize,
    ) -> Result<(), ExactCoverProblemSchemaError> {
        if row.required_columns().is_empty() {
            return Err(
                ExactCoverProblemSchemaError::CandidateWithoutRequiredColumns {
                    row_id: row.row_id(),
                },
            );
        }
        for column in row.required_columns() {
            if *column >= required_column_count {
                return Err(ExactCoverProblemSchemaError::RequiredColumnOutOfRange {
                    row_id: row.row_id(),
                    column: *column,
                });
            }
        }
        for column in row.optional_columns() {
            if *column >= optional_column_count {
                return Err(ExactCoverProblemSchemaError::OptionalColumnOutOfRange {
                    row_id: row.row_id(),
                    column: *column,
                });
            }
        }
        Ok(())
    }
}
mod slot_constraint_column {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SlotConstraintColumn {
        slot_id: String,
        required: bool,
    }

    impl SlotConstraintColumn {
        pub fn new(slot_id: impl Into<String>, required: bool) -> Self {
            Self {
                slot_id: slot_id.into(),
                required,
            }
        }
    }
    impl SlotConstraintColumn {
        pub fn slot_id(&self) -> &str {
            &self.slot_id
        }
    }
    impl SlotConstraintColumn {
        pub fn required(&self) -> bool {
            self.required
        }
    }
}

pub use area_constraint_column::AreaConstraintColumn;
pub use candidate_row::ExactCoverCandidateRow;
pub use column::{ExactCoverColumn, ExactCoverColumnKind};
pub use error::ExactCoverProblemSchemaError;
pub use generic_schema_fixture::generic_exact_cover_candidate_schema_validates;
pub use piece_usage_constraint::PieceUsageConstraint;
pub use schema::ExactCoverProblemSchema;
pub use slot_constraint_column::SlotConstraintColumn;

#[cfg(test)]
#[path = "exact_cover_problem_schema_tests.rs"]
mod tests;
