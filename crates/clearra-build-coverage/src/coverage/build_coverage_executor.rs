mod assignment_solver {
    use clearra_exact_cover::solver::DlxSearchLimits;

    use crate::{
        assignment::{
            assignment_csp::{AssignmentCsp, AssignmentCspLimits},
            assignment_exact_cover::AssignmentExactCoverBridge,
            slot_assignment::SlotAssignment,
        },
        query::build_coverage_query::BuildCoverageQuery,
    };

    use super::{exact_cover_summary::ExactCoverSummary, BuildCoverageExecutionError};

    pub(super) fn solve_assignments_from_exact_report(
        query: &BuildCoverageQuery,
        exact: &ExactCoverSummary,
    ) -> Vec<SlotAssignment> {
        if !exact.assignments.is_empty() {
            return exact.assignments.clone();
        }
        AssignmentCsp::new(
            query.domains().to_vec(),
            query.constraints().to_vec(),
            AssignmentCspLimits::new(query.limits().max_assignments()),
        )
        .solve()
    }

    pub(super) fn solve_exact_cover(
        query: &BuildCoverageQuery,
    ) -> Result<ExactCoverSummary, BuildCoverageExecutionError> {
        let limits = query.limits();
        // An assignment budget is not a node budget. In particular, a
        // difficult unsatisfiable template must not be truncated merely
        // because it has few requested output assignments.
        let search_limits = if limits.max_assignments() == 0 {
            DlxSearchLimits::unlimited()
        } else {
            DlxSearchLimits::new(limits.max_assignments(), usize::MAX)
        };
        let report =
            AssignmentExactCoverBridge::new(query.domains().to_vec(), query.constraints().to_vec())
                .solve(search_limits)?;
        Ok(ExactCoverSummary {
            assignments: report.assignments().to_vec(),
            complete: report.complete(),
            searched_nodes: report.searched_nodes(),
        })
    }
}
mod coverage_identity {
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct BuildCoverageIdentity {
        pub(super) piece_source_id: u64,
        pub(super) pattern_universe_id: PatternUniverseId,
        pub(super) pattern_weight_model_id: PatternWeightModelId,
    }
}
mod coverage_identity_factory {
    use clearra_coverage::{
        row::coverage_row::CoverageRow,
        universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        },
    };

    use crate::query::build_coverage_query::BuildCoverageQuery;

    use super::{
        coverage_identity::BuildCoverageIdentity, stable_identity::stable_nonzero_identity,
    };

    pub(super) fn coverage_identity_for_query_or_rows(
        query: &BuildCoverageQuery,
        rows: &[CoverageRow],
    ) -> BuildCoverageIdentity {
        if let Some(row) = rows.first() {
            return BuildCoverageIdentity {
                piece_source_id: row.piece_source_id(),
                pattern_universe_id: row.pattern_universe_id(),
                pattern_weight_model_id: row.pattern_weight_model_id(),
            };
        }

        let material = format!(
            "build-template={};patterns={};slots={}",
            query.template().id(),
            query.pattern_count(),
            query.template().slots().len()
        );
        BuildCoverageIdentity {
            piece_source_id: stable_nonzero_identity(&format!(
                "clearra:build-coverage-piece-source:{material}"
            )),
            pattern_universe_id: PatternUniverseId::new(stable_nonzero_identity(&format!(
                "clearra:build-coverage-universe:{material}"
            ))),
            pattern_weight_model_id: PatternWeightModelId::new(stable_nonzero_identity(&format!(
                "clearra:build-coverage-weight-model:{material}"
            ))),
        }
    }
}
mod coverage_row_validator {
    use clearra_coverage::row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind};

    use super::{coverage_identity::BuildCoverageIdentity, BuildCoverageExecutionError};

    pub(super) fn validate_c_coverage_rows(
        pattern_count: usize,
        identity: BuildCoverageIdentity,
        rows: &[CoverageRow],
    ) -> Result<(), BuildCoverageExecutionError> {
        for row in rows {
            if row.row_kind() != &CoverageRowKind::Build {
                return Err(BuildCoverageExecutionError::CoverageRowKindMismatch {
                    actual: row.row_kind().clone(),
                });
            }
            if row.pattern_universe_id() != identity.pattern_universe_id {
                return Err(BuildCoverageExecutionError::CoverageUniverseMismatch {
                    expected: identity.pattern_universe_id,
                    actual: row.pattern_universe_id(),
                });
            }
            if row.pattern_weight_model_id() != identity.pattern_weight_model_id {
                return Err(BuildCoverageExecutionError::CoverageWeightModelMismatch {
                    expected: identity.pattern_weight_model_id,
                    actual: row.pattern_weight_model_id(),
                });
            }
            if row.piece_source_id() != identity.piece_source_id {
                return Err(BuildCoverageExecutionError::CoveragePieceSourceMismatch {
                    expected: identity.piece_source_id,
                    actual: row.piece_source_id(),
                });
            }
            if row.pattern_count() != pattern_count {
                return Err(BuildCoverageExecutionError::CoveragePatternCountMismatch {
                    expected: pattern_count,
                    actual: row.pattern_count(),
                });
            }
        }
        Ok(())
    }
}
mod error {
    use clearra_coverage::{
        pattern::weighted_pattern_set::WeightedPatternSetError,
        probability::union_probability::UnionProbabilityError,
        row::coverage_row_kind::CoverageRowKind,
        universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        },
    };

    use crate::{
        assignment::assignment_exact_cover::AssignmentExactCoverError,
        coverage::build_coverage_matrix::BuildCoverageMatrixError,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub enum BuildCoverageExecutionError {
        AssignmentExactCover(AssignmentExactCoverError),
        NoAssignments,
        NoCoverageRows,
        CoveragePatternCountMismatch {
            expected: usize,
            actual: usize,
        },
        CoverageRowKindMismatch {
            actual: CoverageRowKind,
        },
        CoverageUniverseMismatch {
            expected: PatternUniverseId,
            actual: PatternUniverseId,
        },
        CoverageWeightModelMismatch {
            expected: PatternWeightModelId,
            actual: PatternWeightModelId,
        },
        CoveragePieceSourceMismatch {
            expected: u64,
            actual: u64,
        },
        CoverageRowAssignmentCountMismatch {
            assignments: usize,
            c_coverage_rows: usize,
        },
        InvalidConcreteAssignmentTemplate,
        InvalidConcreteCoverageScope,
        InvalidConcreteCoverageLimits,
        ConcreteAssignmentSearchIncomplete,
        ConcreteCoverageStorageUnavailable,
        AmbiguousConcreteAssignment,
        IncompleteConcreteCoverageRow {
            row: usize,
        },
        ConcreteCoverageScopeMismatch {
            row: usize,
        },
        ForeignConcreteCoverageRow {
            row: usize,
        },
        DuplicateConcreteCoverageRow {
            assignment: usize,
        },
        MissingConcreteCoverageRow {
            assignment: usize,
        },
        Matrix(BuildCoverageMatrixError),
        Weights(WeightedPatternSetError),
        Probability(UnionProbabilityError),
    }

    impl From<AssignmentExactCoverError> for BuildCoverageExecutionError {
        fn from(error: AssignmentExactCoverError) -> Self {
            Self::AssignmentExactCover(error)
        }
    }

    impl From<BuildCoverageMatrixError> for BuildCoverageExecutionError {
        fn from(error: BuildCoverageMatrixError) -> Self {
            Self::Matrix(error)
        }
    }

    impl From<WeightedPatternSetError> for BuildCoverageExecutionError {
        fn from(error: WeightedPatternSetError) -> Self {
            Self::Weights(error)
        }
    }

    impl From<UnionProbabilityError> for BuildCoverageExecutionError {
        fn from(error: UnionProbabilityError) -> Self {
            Self::Probability(error)
        }
    }
}
mod exact_cover_summary {
    use crate::assignment::slot_assignment::SlotAssignment;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct ExactCoverSummary {
        pub(super) assignments: Vec<SlotAssignment>,
        pub(super) complete: bool,
        pub(super) searched_nodes: usize,
    }
}
mod execution {
    use crate::{
        assignment::slot_assignment::SlotAssignment,
        coverage::{
            build_coverage_matrix::BuildCoverageMatrix, build_coverage_result::BuildCoverageResult,
            build_union_coverage::BuildUnionCoverage,
        },
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct BuildCoverageExecution {
        pub(super) assignments: Vec<SlotAssignment>,
        pub(super) exact_cover_complete: bool,
        pub(super) exact_cover_searched_nodes: usize,
        pub(super) c_coverage_row_count: usize,
        pub(super) matrix: BuildCoverageMatrix,
        pub(super) union: BuildUnionCoverage,
        pub(super) result: BuildCoverageResult,
    }

    impl BuildCoverageExecution {
        pub fn assignments(&self) -> &[SlotAssignment] {
            &self.assignments
        }
    }
    impl BuildCoverageExecution {
        pub fn exact_cover_complete(&self) -> bool {
            self.exact_cover_complete
        }
    }
    impl BuildCoverageExecution {
        pub fn exact_cover_searched_nodes(&self) -> usize {
            self.exact_cover_searched_nodes
        }
    }
    impl BuildCoverageExecution {
        pub fn c_coverage_row_count(&self) -> usize {
            self.c_coverage_row_count
        }
    }
    impl BuildCoverageExecution {
        pub fn matrix(&self) -> &BuildCoverageMatrix {
            &self.matrix
        }
    }
    impl BuildCoverageExecution {
        pub fn union(&self) -> &BuildUnionCoverage {
            &self.union
        }
    }
    impl BuildCoverageExecution {
        pub fn result(&self) -> &BuildCoverageResult {
            &self.result
        }
    }
}
mod execution_builder {
    use clearra_core_domain::solution::StandardBoard64TilingIdentity;
    use clearra_coverage::{
        pattern::weighted_pattern_set::WeightedPatternSet, row::coverage_row::CoverageRow,
    };

    use crate::{
        coverage::{
            build_coverage_matrix::BuildCoverageMatrix, build_coverage_result::BuildCoverageResult,
            build_union_coverage::BuildUnionCoverage,
        },
        query::build_coverage_query::BuildCoverageQuery,
    };

    use super::{
        assignment_solver::{solve_assignments_from_exact_report, solve_exact_cover},
        coverage_identity_factory::coverage_identity_for_query_or_rows,
        coverage_row_validator::validate_c_coverage_rows,
        exact_cover_summary::ExactCoverSummary,
        row_coverage_extractor::coverage_for_assignments,
        verified_assignment_coverage::{
            join_verified_rows, validate_verified_query, ConcreteAssignmentTemplate,
        },
        BuildCoverageExecution, BuildCoverageExecutionError, ConcreteAssignmentCoverageRow,
        ConcreteCoverageScope,
    };

    /// One solved Legacy template. Producing and reducing physical coverage
    /// must consume the same bounded assignment enumeration.
    pub struct PreparedConcreteAssignmentCoverage<'a> {
        query: &'a BuildCoverageQuery,
        scope: ConcreteCoverageScope,
        template: ConcreteAssignmentTemplate,
        exact: ExactCoverSummary,
    }

    impl<'a> PreparedConcreteAssignmentCoverage<'a> {
        pub fn new(
            query: &'a BuildCoverageQuery,
            scope: ConcreteCoverageScope,
        ) -> Result<Self, BuildCoverageExecutionError> {
            let template = validate_verified_query(query, scope)?;
            let exact = solve_exact_cover(query)?;
            if !exact.complete {
                return Err(BuildCoverageExecutionError::ConcreteAssignmentSearchIncomplete);
            }
            if exact.assignments.is_empty() {
                return Err(BuildCoverageExecutionError::NoAssignments);
            }
            Ok(Self {
                query,
                scope,
                template,
                exact,
            })
        }

        pub fn identities(
            &self,
        ) -> impl ExactSizeIterator<
            Item = Result<StandardBoard64TilingIdentity, BuildCoverageExecutionError>,
        > + '_ {
            self.exact.assignments.iter().map(|assignment| {
                self.template
                    .identity(assignment, self.scope.initial_occupancy())
            })
        }

        pub fn reduce_rows(
            self,
            rows: Vec<ConcreteAssignmentCoverageRow>,
        ) -> Result<BuildCoverageExecution, BuildCoverageExecutionError> {
            let matrix = join_verified_rows(
                self.template,
                &self.exact.assignments,
                self.query.pattern_count(),
                self.scope,
                rows,
            )?;
            finish_execution(
                self.query,
                self.exact.assignments,
                true,
                self.exact.searched_nodes,
                0,
                matrix,
            )
        }
    }

    impl BuildCoverageExecution {
        /// Reduces complete, independently verified queue rows by concrete
        /// assignment identity. Every solved assignment needs an explicit
        /// row, including an exact empty row. The provider's scope must be
        /// resolved from the request, not inferred from the first row.
        ///
        /// This uses the legacy uniform denominator and does not execute or
        /// certify physical reachability. Neither native candidate IDs nor
        /// per-color union masks can stand in for exact assignment identity.
        pub fn from_verified_concrete_assignment_rows(
            query: &BuildCoverageQuery,
            expected_scope: ConcreteCoverageScope,
            rows: Vec<ConcreteAssignmentCoverageRow>,
        ) -> Result<Self, BuildCoverageExecutionError> {
            PreparedConcreteAssignmentCoverage::new(query, expected_scope)?.reduce_rows(rows)
        }

        /// Uses the legacy assignment reducer after an independent physical
        /// impossibility proof. No provider row is needed in this case.
        pub fn from_proved_empty_packing(
            query: &BuildCoverageQuery,
        ) -> Result<Self, BuildCoverageExecutionError> {
            Self::from_legacy_positional_rows(query, &[])
        }

        pub fn from_c_buildup_rows(
            query: &BuildCoverageQuery,
            rows: &[CoverageRow],
        ) -> Result<Self, BuildCoverageExecutionError> {
            Self::from_legacy_positional_rows(query, rows)
        }

        /// Compatibility for placeholder templates whose assignment slots have
        /// no concrete four-cell identity. Preserve the old row-count/union
        /// semantics, not a claim of an assignment-keyed physical join.
        pub fn from_legacy_positional_rows(
            query: &BuildCoverageQuery,
            rows: &[CoverageRow],
        ) -> Result<Self, BuildCoverageExecutionError> {
            let exact = solve_exact_cover(query)?;
            let assignments = solve_assignments_from_exact_report(query, &exact);
            if assignments.is_empty() {
                return Err(BuildCoverageExecutionError::NoAssignments);
            }
            let identity = coverage_identity_for_query_or_rows(query, rows);
            if rows.is_empty() {
                return build_empty_coverage_execution(
                    query,
                    assignments,
                    exact.complete,
                    exact.searched_nodes,
                    identity,
                );
            }
            if assignments.len() != rows.len() {
                return Err(
                    BuildCoverageExecutionError::CoverageRowAssignmentCountMismatch {
                        assignments: assignments.len(),
                        c_coverage_rows: rows.len(),
                    },
                );
            }
            validate_c_coverage_rows(query.pattern_count(), identity, rows)?;
            let coverages = coverage_for_assignments(&assignments, rows);
            let matrix = BuildCoverageMatrix::from_assignments_with_coverages(
                identity.piece_source_id,
                identity.pattern_universe_id,
                identity.pattern_weight_model_id,
                query.pattern_count(),
                &assignments,
                &coverages,
            )?;
            finish_execution(
                query,
                assignments,
                exact.complete,
                exact.searched_nodes,
                rows.len(),
                matrix,
            )
        }
    }

    fn build_empty_coverage_execution(
        query: &BuildCoverageQuery,
        assignments: Vec<crate::assignment::slot_assignment::SlotAssignment>,
        complete: bool,
        searched_nodes: usize,
        identity: super::coverage_identity::BuildCoverageIdentity,
    ) -> Result<BuildCoverageExecution, BuildCoverageExecutionError> {
        let matrix = BuildCoverageMatrix::from_assignment_coverages(
            identity.piece_source_id,
            identity.pattern_universe_id,
            identity.pattern_weight_model_id,
            query.pattern_count(),
            Vec::new(),
        )?;
        finish_execution(query, assignments, complete, searched_nodes, 0, matrix)
    }

    fn finish_execution(
        query: &BuildCoverageQuery,
        assignments: Vec<crate::assignment::slot_assignment::SlotAssignment>,
        exact_cover_complete: bool,
        exact_cover_searched_nodes: usize,
        c_coverage_row_count: usize,
        matrix: BuildCoverageMatrix,
    ) -> Result<BuildCoverageExecution, BuildCoverageExecutionError> {
        let union = BuildUnionCoverage::from_matrix(matrix.matrix());
        let weights = WeightedPatternSet::uniform(query.pattern_count())?;
        let result = BuildCoverageResult::from_union(union.clone(), &weights)?;
        Ok(BuildCoverageExecution {
            assignments,
            exact_cover_complete,
            exact_cover_searched_nodes,
            c_coverage_row_count,
            matrix,
            union,
            result,
        })
    }
}
mod row_coverage_extractor {
    use clearra_coverage::{
        pattern::pattern_bitset::PatternBitSet, row::coverage_row::CoverageRow,
    };

    use crate::assignment::slot_assignment::SlotAssignment;

    pub(super) fn coverage_for_assignments(
        assignments: &[SlotAssignment],
        rows: &[CoverageRow],
    ) -> Vec<PatternBitSet> {
        assignments
            .iter()
            .zip(rows.iter())
            .map(|(_, row)| row.coverage_bits().clone())
            .collect()
    }
}
mod stable_identity {
    pub(super) fn stable_nonzero_identity(material: &str) -> u64 {
        const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
        const FNV_PRIME: u64 = 1_099_511_628_211;

        let mut hash = FNV_OFFSET;
        for byte in material.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        if hash == 0 {
            1
        } else {
            hash
        }
    }
}
#[path = "verified_assignment_coverage.rs"]
mod verified_assignment_coverage;

pub use error::BuildCoverageExecutionError;
pub use execution::BuildCoverageExecution;
pub use execution_builder::PreparedConcreteAssignmentCoverage;
pub use verified_assignment_coverage::{
    ConcreteAssignmentCoverageRow, ConcreteCoverageCompleteness, ConcreteCoverageScope,
};

#[cfg(test)]
use clearra_coverage::{
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[cfg(test)]
#[path = "build_coverage_executor_tests.rs"]
mod tests;
