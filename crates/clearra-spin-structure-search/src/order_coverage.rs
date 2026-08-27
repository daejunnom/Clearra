use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    model::{PieceInventory, SpinStructureOutcome, SpinStructureQuery, SpinStructureReport},
    structural_verify::StructuralBuildVerifier,
};

/// Default hard ceiling for exact no-hold order materialization. Seven-piece
/// bags (7! = 5,040) and eight distinct pieces (40,320) fit; factorial inputs
/// beyond the governed ceiling fail closed before becoming a coverage claim.
pub const DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureCoverageRow {
    covered_pattern_indices: Vec<u32>,
}

impl SpinStructureCoverageRow {
    pub fn covered_pattern_indices(&self) -> &[u32] {
        &self.covered_pattern_indices
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureCoverageAnalysis {
    pattern_count: usize,
    covered_pattern_count: usize,
    rows: Vec<SpinStructureCoverageRow>,
}

impl SpinStructureCoverageAnalysis {
    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    /// Rows follow `report.outcomes()` exactly: Regular first, then Mini, with
    /// the canonical order already proven by the ranked-family promoter.
    pub fn rows(&self) -> &[SpinStructureCoverageRow] {
        &self.rows
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinStructureOrderCoverageError {
    InvalidPatternLimit,
    PatternCountOverflow,
    PatternLimitExceeded { limit: usize },
    PatternIndexOverflow,
    ReportIncomplete,
    QueryIdentityMismatch,
}

/// Computes each structure's exact no-hold coverage over every unique order of
/// the complete unordered inventory. A structure that uses fewer operations
/// consumes the corresponding queue prefix; unused suffix pieces do not alter
/// the already completed terminal spin.
pub fn analyze_spin_structure_coverage(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
    max_patterns: usize,
) -> Result<SpinStructureCoverageAnalysis, SpinStructureOrderCoverageError> {
    validate_report(query, report)?;
    let patterns = enumerate_inventory_orders(query.inventory, max_patterns)?;
    let mut verifier = StructuralBuildVerifier::new(query);
    let mut rows = Vec::with_capacity(report.outcome_count());
    let mut covered = vec![false; patterns.len()];
    for outcome in report.outcomes() {
        let operation_count = outcome.logical_operations().len();
        let mut covered_pattern_indices = Vec::new();
        for (pattern_index, pattern) in patterns.iter().enumerate() {
            if operation_count <= pattern.len()
                && verifier.accepts_piece_order(
                    query,
                    outcome.logical_operations(),
                    outcome.logical_spin(),
                    &pattern[..operation_count],
                )
            {
                let pattern_index = u32::try_from(pattern_index)
                    .map_err(|_| SpinStructureOrderCoverageError::PatternIndexOverflow)?;
                covered_pattern_indices.push(pattern_index);
                covered[pattern_index as usize] = true;
            }
        }
        rows.push(SpinStructureCoverageRow {
            covered_pattern_indices,
        });
    }
    Ok(SpinStructureCoverageAnalysis {
        pattern_count: patterns.len(),
        covered_pattern_count: covered.into_iter().filter(|value| *value).count(),
        rows,
    })
}

/// Filters the ordinary structure family to candidates whose reserved terminal
/// piece can be locked last after *every* unique no-hold order of their exact
/// non-target operation multiset. This is an ordinary family, not a portfolio
/// tie set.
pub fn guaranteed_spin_structure_family(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
    final_piece: PieceKind,
    max_patterns: usize,
) -> Result<SpinStructureReport, SpinStructureOrderCoverageError> {
    validate_report(query, report)?;
    if max_patterns == 0 {
        return Err(SpinStructureOrderCoverageError::InvalidPatternLimit);
    }
    let mut verifier = StructuralBuildVerifier::new(query);
    let mut retained = |outcome: &SpinStructureOutcome| {
        if outcome.logical_spin().piece() != final_piece {
            return Ok(false);
        }
        let mut inventory = PieceInventory::EMPTY;
        for operation in outcome.logical_operations() {
            if *operation == outcome.logical_spin() {
                continue;
            }
            let mut counts = inventory.counts();
            let Some(index) = PieceKind::STANDARD_TETROMINOES
                .iter()
                .position(|piece| *piece == operation.piece())
            else {
                return Err(SpinStructureOrderCoverageError::PatternCountOverflow);
            };
            counts[index] = counts[index]
                .checked_add(1)
                .ok_or(SpinStructureOrderCoverageError::PatternCountOverflow)?;
            inventory = PieceInventory::from_counts(counts);
        }
        let orders = enumerate_inventory_orders(inventory, max_patterns)?;
        let mut full_order = Vec::with_capacity(outcome.logical_operations().len());
        for order in orders {
            full_order.clear();
            full_order.extend(order);
            full_order.push(final_piece);
            if !verifier.accepts_piece_order(
                query,
                outcome.logical_operations(),
                outcome.logical_spin(),
                &full_order,
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    };

    let regular = report
        .regular
        .iter()
        .filter_map(|outcome| match retained(outcome) {
            Ok(true) => Some(Ok(outcome.clone())),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mini = report
        .mini
        .iter()
        .filter_map(|outcome| match retained(outcome) {
            Ok(true) => Some(Ok(outcome.clone())),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let minimum_placements = regular
        .iter()
        .chain(&mini)
        .map(SpinStructureOutcome::placement_count)
        .min()
        .and_then(|count| u8::try_from(count).ok());
    Ok(SpinStructureReport {
        regular,
        mini,
        minimum_placements,
        layers: report.layers.clone(),
        stages: report.stages,
        timings: report.timings,
        workers_used: report.workers_used,
        complete: true,
        query: report.query.clone(),
    })
}

fn validate_report(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
) -> Result<(), SpinStructureOrderCoverageError> {
    if !report.complete {
        return Err(SpinStructureOrderCoverageError::ReportIncomplete);
    }
    if report.query.as_ref() != Some(query) {
        return Err(SpinStructureOrderCoverageError::QueryIdentityMismatch);
    }
    Ok(())
}

fn enumerate_inventory_orders(
    inventory: PieceInventory,
    max_patterns: usize,
) -> Result<Vec<Vec<PieceKind>>, SpinStructureOrderCoverageError> {
    if max_patterns == 0 {
        return Err(SpinStructureOrderCoverageError::InvalidPatternLimit);
    }
    let length = usize::from(inventory.total());
    let mut orders = Vec::new();
    let mut prefix = Vec::with_capacity(length);
    enumerate_orders_recursive(
        inventory.counts(),
        length,
        max_patterns,
        &mut prefix,
        &mut orders,
    )?;
    Ok(orders)
}

fn enumerate_orders_recursive(
    mut counts: [u8; 7],
    length: usize,
    max_patterns: usize,
    prefix: &mut Vec<PieceKind>,
    orders: &mut Vec<Vec<PieceKind>>,
) -> Result<(), SpinStructureOrderCoverageError> {
    if prefix.len() == length {
        if orders.len() == max_patterns {
            return Err(SpinStructureOrderCoverageError::PatternLimitExceeded {
                limit: max_patterns,
            });
        }
        orders.push(prefix.clone());
        return Ok(());
    }
    for (index, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
        if counts[index] == 0 {
            continue;
        }
        counts[index] -= 1;
        prefix.push(piece);
        enumerate_orders_recursive(counts, length, max_patterns, prefix, orders)?;
        prefix.pop();
        counts[index] += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SpinLineRequirement, SpinStructureMode, SpinStructureSearcher, StructureBoard};

    fn one_piece_query() -> SpinStructureQuery {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T]).expect("inventory"),
            SpinStructureMode::TSpins,
        );
        query.initial_board = StructureBoard::from_words([0x14000043ff, 0, 0, 0]);
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query
    }

    #[test]
    fn exact_order_coverage_is_query_bound_and_complete() {
        let query = SpinStructureSearcher::normalize_query(one_piece_query()).expect("normalize");
        let report = SpinStructureSearcher::run(query.clone()).expect("search");
        let coverage = analyze_spin_structure_coverage(&query, &report, 8).expect("coverage");
        assert_eq!(coverage.pattern_count(), 1);
        assert_eq!(coverage.rows().len(), report.outcome_count());
        assert_eq!(coverage.covered_pattern_count(), 1);
        assert!(coverage
            .rows()
            .iter()
            .all(|row| row.covered_pattern_indices() == [0]));
    }

    #[test]
    fn guaranteed_family_checks_every_non_target_order_and_keeps_final_piece_last() {
        let query = SpinStructureSearcher::normalize_query(one_piece_query()).expect("normalize");
        let report = SpinStructureSearcher::run(query.clone()).expect("search");
        let guaranteed = guaranteed_spin_structure_family(
            &query,
            &report,
            PieceKind::T,
            DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS,
        )
        .expect("guaranteed");
        assert_eq!(guaranteed.outcome_count(), report.outcome_count());
        assert!(guaranteed.complete);
        assert_eq!(guaranteed.query.as_ref(), Some(&query));
    }

    #[test]
    fn exact_order_materialization_fails_closed_at_the_bound() {
        let inventory =
            PieceInventory::from_pieces([PieceKind::I, PieceKind::O]).expect("inventory");
        assert_eq!(
            enumerate_inventory_orders(inventory, 1),
            Err(SpinStructureOrderCoverageError::PatternLimitExceeded { limit: 1 })
        );
    }
}
