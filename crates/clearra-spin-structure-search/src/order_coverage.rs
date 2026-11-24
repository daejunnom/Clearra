use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{try_for_each_chunk, ParallelWorkError},
    piece::piece_kind::PieceKind,
};

use crate::{
    model::{PieceInventory, SpinStructureOutcome, SpinStructureQuery, SpinStructureReport},
    structural_verify::StructuralBuildVerifier,
};

/// The public coverage rows use `u32` pattern identities. Orders are decoded
/// lazily from their canonical rank; there is no factorial queue allocation or
/// smaller product-only pattern ceiling.
pub const DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS: usize = u32::MAX as usize;

const ORDER_TILE: usize = 64;

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
    Cancelled,
    WorkerFailed(&'static str),
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
    analyze_spin_structure_coverage_with_control(
        query,
        report,
        max_patterns,
        &ExecutionControl::default(),
    )
}

/// Runs the exact candidate × order matrix on the request's shared compute
/// budget. Splitting both dimensions also distributes a single heavy candidate.
/// Every accepted index is the original rank, independent of worker order.
pub fn analyze_spin_structure_coverage_with_control(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
    max_patterns: usize,
    control: &ExecutionControl,
) -> Result<SpinStructureCoverageAnalysis, SpinStructureOrderCoverageError> {
    validate_report(query, report)?;
    check_cancelled(control)?;
    let source = InventoryOrderSource::new(query.inventory, max_patterns)?;
    let outcomes = report.outcomes().collect::<Vec<_>>();
    if outcomes.is_empty() {
        return Ok(SpinStructureCoverageAnalysis {
            pattern_count: source.len(),
            covered_pattern_count: 0,
            rows: Vec::new(),
        });
    }
    let tiles_per_candidate = source.len().div_ceil(ORDER_TILE);
    let tile_count = outcomes
        .len()
        .checked_mul(tiles_per_candidate)
        .ok_or(SpinStructureOrderCoverageError::PatternCountOverflow)?;
    let rows = (0..outcomes.len())
        .map(|_| Mutex::new(Vec::<u32>::new()))
        .collect::<Vec<_>>();
    let covered = (0..tiles_per_candidate)
        .map(|_| AtomicU64::new(0))
        .collect::<Vec<_>>();
    let completed = AtomicU64::new(0);
    try_for_each_chunk(
        control,
        tile_count,
        1,
        || Ok((StructuralBuildVerifier::new(query), Vec::new())),
        |(verifier, pattern), tasks| {
            for task in tasks {
                let candidate = task / tiles_per_candidate;
                let tile = task % tiles_per_candidate;
                let first = tile * ORDER_TILE;
                let end = (first + ORDER_TILE).min(source.len());
                let outcome = outcomes[candidate];
                let operation_count = outcome.logical_operations().len();
                let mut indices = Vec::new();
                let mut bits = 0_u64;
                for rank in first..end {
                    check_cancelled(control)?;
                    source.decode(rank, pattern);
                    if operation_count <= pattern.len()
                        && verifier.accepts_piece_order_with_control(
                            query,
                            outcome.logical_operations(),
                            outcome.logical_spin(),
                            &pattern[..operation_count],
                            control,
                        )
                    {
                        indices.push(rank as u32);
                        bits |= 1_u64 << (rank - first);
                    }
                }
                check_cancelled(control)?;
                if !indices.is_empty() {
                    rows[candidate]
                        .lock()
                        .map_err(|_| {
                            SpinStructureOrderCoverageError::WorkerFailed("spin_order_row_poisoned")
                        })?
                        .extend(indices);
                    covered[tile].fetch_or(bits, Ordering::Relaxed);
                }
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                control.report_progress(
                    "spin-structure-order-coverage",
                    done,
                    Some(tile_count as u64),
                );
            }
            Ok(())
        },
    )
    .map_err(parallel_error)?;
    // Completion may arrive in any order. Canonicalize separate rows in the
    // same pool instead of moving the entire terminal sort onto one caller.
    try_for_each_chunk(
        control,
        rows.len(),
        1,
        || Ok(()),
        |_, range| {
            for index in range {
                rows[index]
                    .lock()
                    .map_err(|_| {
                        SpinStructureOrderCoverageError::WorkerFailed("spin_order_row_poisoned")
                    })?
                    .sort_unstable();
            }
            Ok(())
        },
    )
    .map_err(parallel_error)?;
    check_cancelled(control)?;
    let covered_pattern_count = covered
        .iter()
        .map(|word| word.load(Ordering::Relaxed).count_ones() as usize)
        .sum();
    let rows = rows
        .into_iter()
        .map(|row| {
            row.into_inner()
                .map(|covered_pattern_indices| SpinStructureCoverageRow {
                    covered_pattern_indices,
                })
                .map_err(|_| {
                    SpinStructureOrderCoverageError::WorkerFailed("spin_order_row_poisoned")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SpinStructureCoverageAnalysis {
        pattern_count: source.len(),
        covered_pattern_count,
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
    guaranteed_spin_structure_family_with_control(
        query,
        report,
        final_piece,
        max_patterns,
        &ExecutionControl::default(),
    )
}

pub fn guaranteed_spin_structure_family_with_control(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
    final_piece: PieceKind,
    max_patterns: usize,
    control: &ExecutionControl,
) -> Result<SpinStructureReport, SpinStructureOrderCoverageError> {
    validate_report(query, report)?;
    check_cancelled(control)?;
    if max_patterns == 0 {
        return Err(SpinStructureOrderCoverageError::InvalidPatternLimit);
    }
    let outcomes = report.outcomes().collect::<Vec<_>>();
    let mut sources = Vec::with_capacity(outcomes.len());
    let mut tile_ends = Vec::with_capacity(outcomes.len());
    let mut total_tiles = 0_usize;
    for outcome in &outcomes {
        if outcome.logical_spin().piece() != final_piece {
            sources.push(None);
            tile_ends.push(total_tiles);
            continue;
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
        let source = InventoryOrderSource::new(inventory, max_patterns)?;
        total_tiles = total_tiles
            .checked_add(source.len().div_ceil(ORDER_TILE))
            .ok_or(SpinStructureOrderCoverageError::PatternCountOverflow)?;
        sources.push(Some(source));
        tile_ends.push(total_tiles);
    }
    let retained = sources
        .iter()
        .map(|source| AtomicBool::new(source.is_some()))
        .collect::<Vec<_>>();
    try_for_each_chunk(
        control,
        total_tiles,
        1,
        || Ok((StructuralBuildVerifier::new(query), Vec::new())),
        |(verifier, full_order), tasks| {
            for task in tasks {
                let candidate = tile_ends.partition_point(|end| *end <= task);
                if !retained[candidate].load(Ordering::Relaxed) {
                    continue;
                }
                let source = sources[candidate]
                    .as_ref()
                    .expect("task belongs to a source");
                let first_tile = candidate.checked_sub(1).map_or(0, |index| tile_ends[index]);
                let first = (task - first_tile) * ORDER_TILE;
                let end = (first + ORDER_TILE).min(source.len());
                let outcome = outcomes[candidate];
                for rank in first..end {
                    check_cancelled(control)?;
                    if !retained[candidate].load(Ordering::Relaxed) {
                        break;
                    }
                    source.decode(rank, full_order);
                    full_order.push(final_piece);
                    if !verifier.accepts_piece_order_with_control(
                        query,
                        outcome.logical_operations(),
                        outcome.logical_spin(),
                        full_order,
                        control,
                    ) {
                        check_cancelled(control)?;
                        retained[candidate].store(false, Ordering::Relaxed);
                        break;
                    }
                }
            }
            Ok(())
        },
    )
    .map_err(parallel_error)?;
    check_cancelled(control)?;
    let regular = report
        .regular
        .iter()
        .enumerate()
        .filter(|(index, _)| retained[*index].load(Ordering::Relaxed))
        .map(|(_, outcome)| outcome.clone())
        .collect::<Vec<_>>();
    let mini = report
        .mini
        .iter()
        .enumerate()
        .filter(|(index, _)| retained[report.regular.len() + *index].load(Ordering::Relaxed))
        .map(|(_, outcome)| outcome.clone())
        .collect::<Vec<_>>();
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

fn check_cancelled(control: &ExecutionControl) -> Result<(), SpinStructureOrderCoverageError> {
    if control.is_cancelled() {
        Err(SpinStructureOrderCoverageError::Cancelled)
    } else {
        Ok(())
    }
}

fn parallel_error(
    error: ParallelWorkError<SpinStructureOrderCoverageError>,
) -> SpinStructureOrderCoverageError {
    match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => SpinStructureOrderCoverageError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            SpinStructureOrderCoverageError::WorkerFailed("spin_order_worker_spawn_failed")
        }
        ParallelWorkError::WorkerPanicked => {
            SpinStructureOrderCoverageError::WorkerFailed("spin_order_worker_panicked")
        }
    }
}

/// Canonical multiset permutations with exact rank decoding. Only seven
/// multiplicities and one reusable queue are retained, independent of N!.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryOrderSource {
    inventory: PieceInventory,
    count: usize,
}

impl InventoryOrderSource {
    pub fn new(
        inventory: PieceInventory,
        max_patterns: usize,
    ) -> Result<Self, SpinStructureOrderCoverageError> {
        if max_patterns == 0 {
            return Err(SpinStructureOrderCoverageError::InvalidPatternLimit);
        }
        let limit = max_patterns.min(u32::MAX as usize);
        let mut count = 1_u128;
        let mut length = 0_u128;
        for multiplicity in inventory.counts() {
            for add in 1..=u128::from(multiplicity) {
                // C(length + multiplicity, multiplicity) is folded into the
                // previous multiset count without any factorial intermediate.
                count = count
                    .checked_mul(length + add)
                    .ok_or(SpinStructureOrderCoverageError::PatternCountOverflow)?
                    / add;
                if count > limit as u128 {
                    return Err(if max_patterns > u32::MAX as usize {
                        SpinStructureOrderCoverageError::PatternIndexOverflow
                    } else {
                        SpinStructureOrderCoverageError::PatternLimitExceeded {
                            limit: max_patterns,
                        }
                    });
                }
            }
            length += u128::from(multiplicity);
        }
        Ok(Self {
            inventory,
            count: count as usize,
        })
    }

    pub const fn len(self) -> usize {
        self.count
    }

    pub const fn is_empty(self) -> bool {
        false
    }

    pub fn decode(self, rank: usize, output: &mut Vec<PieceKind>) -> bool {
        if rank >= self.count {
            return false;
        }
        output.clear();
        let mut counts = self.inventory.counts();
        let mut remaining = usize::from(self.inventory.total());
        let mut permutations = self.count as u128;
        let mut rank = rank as u128;
        while remaining != 0 {
            for (index, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
                if counts[index] == 0 {
                    continue;
                }
                let branch = permutations * u128::from(counts[index]) / remaining as u128;
                if rank < branch {
                    output.push(piece);
                    counts[index] -= 1;
                    remaining -= 1;
                    permutations = branch;
                    break;
                }
                rank -= branch;
            }
        }
        true
    }
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

#[cfg(test)]
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

#[cfg(test)]
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

    #[test]
    fn ranked_inventory_orders_preserve_recursive_identity_without_the_product_ceiling() {
        for pieces in [
            vec![],
            vec![PieceKind::I, PieceKind::O, PieceKind::I],
            PieceKind::STANDARD_TETROMINOES.to_vec(),
        ] {
            let inventory = PieceInventory::from_pieces(pieces).unwrap();
            let expected = enumerate_inventory_orders(inventory, 5040).unwrap();
            let source = InventoryOrderSource::new(inventory, 5040).unwrap();
            assert_eq!(source.len(), expected.len());
            let mut actual = Vec::new();
            for (rank, order) in expected.iter().enumerate() {
                assert!(source.decode(rank, &mut actual));
                assert_eq!(&actual, order);
            }
            assert!(!source.decode(source.len(), &mut actual));
        }
        let inventory = PieceInventory::from_counts([2, 2, 1, 1, 1, 1, 2]);
        let source =
            InventoryOrderSource::new(inventory, DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS).unwrap();
        assert_eq!(source.len(), 453_600);
        let mut first = Vec::new();
        let mut last = Vec::new();
        assert!(source.decode(0, &mut first));
        assert!(source.decode(source.len() - 1, &mut last));
        first.reverse();
        assert_eq!(first, last);
    }

    #[test]
    fn parallel_order_matrix_and_guaranteed_filter_preserve_original_rows() {
        let mut query = one_piece_query();
        query.inventory = PieceInventory::from_counts([1, 1, 1, 0, 0, 0, 0]);
        query.max_placements = Some(1);
        let query = SpinStructureSearcher::normalize_query(query).unwrap();
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        assert!(report.outcome_count() > 0);
        let patterns = enumerate_inventory_orders(query.inventory, 8).unwrap();
        let mut verifier = StructuralBuildVerifier::new(&query);
        let expected = report
            .outcomes()
            .map(|outcome| {
                patterns
                    .iter()
                    .enumerate()
                    .filter_map(|(rank, pattern)| {
                        let count = outcome.logical_operations().len();
                        verifier
                            .accepts_piece_order(
                                &query,
                                outcome.logical_operations(),
                                outcome.logical_spin(),
                                &pattern[..count],
                            )
                            .then_some(rank as u32)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for workers in [1, 2, 4, 8] {
            let control = ExecutionControl::default().with_parallelism(workers);
            let actual =
                analyze_spin_structure_coverage_with_control(&query, &report, 8, &control).unwrap();
            assert_eq!(actual.pattern_count(), patterns.len());
            assert_eq!(
                actual
                    .rows()
                    .iter()
                    .map(|row| row.covered_pattern_indices().to_vec())
                    .collect::<Vec<_>>(),
                expected
            );
            let guaranteed = guaranteed_spin_structure_family_with_control(
                &query,
                &report,
                PieceKind::T,
                8,
                &control,
            )
            .unwrap();
            assert_eq!(guaranteed.regular, report.regular);
            assert_eq!(guaranteed.mini, report.mini);
        }
        let cancelled = ExecutionControl::default().with_parallelism(8);
        cancelled.cancellation.handle().cancel();
        assert_eq!(
            analyze_spin_structure_coverage_with_control(&query, &report, 8, &cancelled),
            Err(SpinStructureOrderCoverageError::Cancelled)
        );
        let mut wrong = query.clone();
        wrong.fill_bottom = 1;
        assert_eq!(
            analyze_spin_structure_coverage_with_control(
                &wrong,
                &report,
                8,
                &ExecutionControl::default()
            ),
            Err(SpinStructureOrderCoverageError::QueryIdentityMismatch)
        );
    }
}
