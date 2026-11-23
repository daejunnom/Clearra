//! Opt-in direct evaluation of a compressed geometry family.
//!
//! The live relation contains (selected placements, original source). Supply
//! states are a small external automaton. Every rank adds one legal row and
//! intersects Down(F), without constructing a complete geometric candidate or
//! a candidate-specific Build graph. At the fixed target rank, Down membership
//! is exact F membership. Results retain this shared relation and page only at
//! the explicit output boundary.

use std::{collections::BTreeMap, fmt};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_problem::{goal::SearchGoal, SearchProblem, SearchProblemPreset};
use clearra_supply::pattern_universe::PieceMultisetKey;

use super::{
    catalog::GeometryCatalog,
    geometry::compile_symbolic_geometry,
    geometry_compatibility_filter::GeometryCompatibilityFilter,
    geometry_partial_family::{ZddArena, ZddError, ZddId, ZddLimits},
    reachability::{symbolic_family::SymbolicOccupancy, SharedReachabilityTemplates},
    symbolic_boolean::{BooleanArena, BooleanError, BooleanId, BooleanLimits},
    symbolic_cell_bridge::CellFamilyBridge,
    symbolic_family_bridge::FamilyBridge,
    symbolic_family_supply::{SymbolicFamilySupply, SymbolicSupplyError, SymbolicSupplyState},
    WasmExactSearchError,
};

#[cfg(test)]
#[path = "symbolic_family_tests.rs"]
mod tests;

#[path = "symbolic_family_archive.rs"]
mod archive;
pub use archive::{SymbolicFamilyArchive, SymbolicFamilyArchiveError, SymbolicFamilyArchiveScope};

/// Explicit research representation bounds. Every zero means unlimited.
/// These are logical allocation/work bounds, not timeouts or a process RSS
/// quota. The enclosing native process still needs its ordinary supervision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SymbolicFamilyLimits {
    pub max_geometry_nodes: usize,
    pub max_set_nodes: usize,
    pub max_predicate_nodes: usize,
    pub max_work_per_arena: u64,
    pub max_memo_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolicFamilyError {
    Cancelled,
    Unsupported(&'static str),
    InvalidProblem(&'static str),
    Resource(&'static str),
}

impl fmt::Display for SymbolicFamilyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("symbolic_family_cancelled"),
            Self::Unsupported(reason) => write!(formatter, "unsupported: {reason}"),
            Self::InvalidProblem(reason) => write!(formatter, "invalid: {reason}"),
            Self::Resource(reason) => write!(formatter, "resource: {reason}"),
        }
    }
}

impl std::error::Error for SymbolicFamilyError {}

impl From<ZddError> for SymbolicFamilyError {
    fn from(error: ZddError) -> Self {
        match error {
            ZddError::Cancelled => Self::Cancelled,
            ZddError::InvalidFamily => Self::InvalidProblem("symbolic_set_family"),
            ZddError::InvalidVariableOrder => Self::InvalidProblem("symbolic_variable_order"),
            ZddError::InvalidGeometry => Self::InvalidProblem("symbolic_geometry_contract"),
            ZddError::NodeLimit => Self::Resource("symbolic_set_node_limit"),
            ZddError::MemoLimit => Self::Resource("symbolic_set_memo_limit"),
            ZddError::WorkLimit => Self::Resource("symbolic_set_work_limit"),
            ZddError::Allocation => Self::Resource("symbolic_set_allocation"),
            ZddError::NodeIdExhausted => Self::Resource("symbolic_set_node_id_exhausted"),
            ZddError::CountOverflow => Self::Resource("symbolic_exact_count_overflow"),
        }
    }
}

impl From<BooleanError> for SymbolicFamilyError {
    fn from(error: BooleanError) -> Self {
        match error {
            BooleanError::Cancelled => Self::Cancelled,
            BooleanError::Allocation => Self::Resource("symbolic_predicate_allocation"),
            BooleanError::NodeLimit => Self::Resource("symbolic_predicate_node_limit"),
            BooleanError::MemoLimit => Self::Resource("symbolic_predicate_memo_limit"),
            BooleanError::WorkLimit => Self::Resource("symbolic_predicate_work_limit"),
            BooleanError::CountOverflow => Self::Resource("symbolic_predicate_count_overflow"),
            BooleanError::InvalidInput(reason) => Self::InvalidProblem(reason),
            BooleanError::Unsupported(reason) => Self::Unsupported(reason),
        }
    }
}

impl From<SymbolicSupplyError> for SymbolicFamilyError {
    fn from(error: SymbolicSupplyError) -> Self {
        match error {
            SymbolicSupplyError::Unsupported(reason) => Self::Unsupported(reason),
            SymbolicSupplyError::InvalidSource(reason) => Self::InvalidProblem(reason),
            SymbolicSupplyError::Allocation => Self::Resource("symbolic_supply_allocation"),
            SymbolicSupplyError::Zdd(error) => error.into(),
            SymbolicSupplyError::Execution(_) => Self::InvalidProblem("symbolic_supply_transition"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolicFamilyRankReport {
    pub cell_support_work_steps: u64,
    pub cell_support_cached_nodes: usize,
    pub placed_pieces: usize,
    pub supply_states: usize,
    pub joint_members: u128,
    pub set_nodes_before_compaction: usize,
    pub set_nodes_after_compaction: usize,
    pub set_compaction_peak_bytes_estimate: usize,
    pub predicate_nodes: usize,
    pub derivative_nodes: usize,
    pub movement_poses: usize,
    /// Retained transfer descriptors, not materialized Boolean guard families.
    pub guarded_movement_edges: usize,
    /// Transfers considered while propagating actual reached deltas.
    pub movement_propagations: u64,
    pub vertical_seed_edges: u64,
    pub vertical_seed_updates: u64,
    pub early_reached_rejections: u64,
    pub rotation_candidate_checks: u64,
    pub rotation_candidates_pruned: u64,
    pub rotation_guard_no_prior: u64,
    pub rotation_guard_single_bit: u64,
    pub rotation_guard_single_mask: u64,
    pub rotation_guard_ordered: u64,
    pub rotation_ordered_prior_terms: u64,
    pub legal_row_queries: u64,
    pub geometry_filters: u64,
    /// Ready source roots projected while preparing the shared piece family.
    pub ready_row_projections: u64,
    /// Additional distinct piece domains, excluding the common rank domain.
    pub piece_domain_builds: u64,
    /// Exact row/source semijoins after a shared legal/geometry decision.
    pub source_semijoins: u64,
    /// Retained capacity after completed-piece memo releases in this rank.
    pub compatibility_cache_bytes_estimate: usize,
    /// Largest retained capacity when a piece's geometry work finishes, before
    /// release. Cell batching releases it before the original-source joins.
    /// Excludes allocator and transient rehash storage; this is not peak RSS.
    pub compatibility_cache_peak_bytes_estimate: usize,
    pub compatibility_cache_releases: usize,
    pub compatibility_filter_misses: u64,
    pub compatibility_guard_misses: u64,
    pub movement_parallel: SymbolicFamilyParallelReport,
}

/// Movement worker accounting. This does not claim parallel joint-ZDD updates.
/// Private storage sums are conservative accounting, not simultaneous RSS.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolicFamilyParallelReport {
    pub requested_workers: usize,
    pub eligible_pieces: usize,
    pub spawned_workers: usize,
    pub active_workers: usize,
    pub peak_concurrent_movement_tasks: usize,
    pub fallback_reason: Option<&'static str>,
    pub shared_prefix_nodes: usize,
    pub shared_prefix_retained_bytes_estimate: usize,
    pub private_node_peak_sum_upper_bound: usize,
    pub private_retained_bytes_sum_estimate: usize,
    /// Construction-only private indexes released before worker publication.
    pub released_builder_bytes_sum_estimate: usize,
    pub predicate_work_steps: u64,
    pub predicate_compactions: u64,
    pub predicate_reclaimed_nodes: u64,
    /// Included in predicate_work_steps; do not add it a second time.
    pub predicate_compaction_work_steps: u64,
    pub predicate_discarded_apply_entries: u64,
    /// Sum of individual scratch peaks, not a simultaneous memory peak.
    pub compaction_peak_temporary_bytes_sum_estimate: usize,
}

/// Counts refer to unique normalized placement identities and original source
/// identities. They do not count operation orders, Build paths or spin traces.
/// Capacity estimates below exclude allocator and operation scratch; they are
/// neither peak RSS nor a process memory upper bound.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolicFamilyReport {
    /// Physical predicates use placement identities or original occupied cells.
    pub movement_variable_space: &'static str,
    /// Unit of the measured geometry_filters calls, not a candidate count.
    pub geometry_filter_scope: &'static str,
    pub catalog_rows: usize,
    pub geometry_nodes: usize,
    pub geometry_bytes_estimate: usize,
    pub geometry_expanded_nodes: usize,
    pub geometry_tablebase_pruned_states: usize,
    pub geometry_released_construction_bytes: usize,
    pub source_count: u128,
    pub source_residual_states: usize,
    pub unique_solution_count: u128,
    pub covered_source_count: u128,
    pub solution_source_pairs: u128,
    pub ranks: Vec<SymbolicFamilyRankReport>,
    pub set_nodes_created: u64,
    pub set_apply_misses: u64,
    pub set_work_steps: u64,
    pub set_compactions: u64,
    /// Coordinator arena only; worker suffix peaks are reported per rank.
    pub predicate_peak_nodes: usize,
    pub predicate_work_steps: u64,
    pub bridge_work_steps: u64,
    pub derivative_work_steps: u64,
    pub derivative_cache_misses: u64,
    pub compatibility_work_steps: u64,
    pub compatibility_filter_misses: u64,
    pub compatibility_guard_misses: u64,
    pub compatibility_saturated_unions: u64,
    pub compatibility_union_remainder_calls: u64,
    pub compatibility_union_remainder_created_nodes: u64,
    pub compatibility_union_remainder_work_steps: u64,
    pub retained_compatibility_bytes_estimate: usize,
    pub retained_set_bytes_estimate: usize,
    pub retained_predicate_bytes_estimate: usize,
    pub retained_derivative_bytes_estimate: usize,
    pub result_compaction_peak_bytes_estimate: usize,
}

pub struct SymbolicFamilyResult {
    catalog: GeometryCatalog,
    sets: ZddArena,
    accepted: ZddId,
    identities: ZddId,
    covered_sources: ZddId,
    source_length: usize,
    report: SymbolicFamilyReport,
}

impl SymbolicFamilyResult {
    pub fn report(&self) -> &SymbolicFamilyReport {
        &self.report
    }

    /// Full original queue length, including draws hidden by source projection.
    pub const fn source_length(&self) -> usize {
        self.source_length
    }

    /// Whether at least one accepted identity covers this full original queue.
    /// This is global source membership, not a per-identity relation check.
    /// It allocates no nodes or scratch storage and leaves counts and caches
    /// unchanged. `PieceKind` admits only the seven standard tetrominoes.
    pub fn covers_source(&self, source: &[PieceKind]) -> Result<bool, SymbolicFamilyError> {
        if source.len() != self.source_length {
            return Err(SymbolicFamilyError::InvalidProblem(
                "symbolic_output_source_length",
            ));
        }
        let row_count = u32::try_from(self.catalog.skeleton_count())
            .map_err(|_| SymbolicFamilyError::InvalidProblem("symbolic_output_variable_range"))?;
        let piece_count = PieceKind::STANDARD_TETROMINOES.len() as u32;
        // Supply compilation checked the entire original-source variable
        // range. Each queue selects one variable from its position's block.
        let variables = source.iter().enumerate().map(|(position, &piece)| {
            row_count + position as u32 * piece_count + super::piece_index(piece) as u32
        });
        Ok(self.sets.contains(self.covered_sources, variables)?)
    }

    /// Extract only the requested page. `offset` skips counted DAG branches;
    /// it does not enumerate earlier identities. The order is stable for this
    /// catalog/arena and is not the legacy normalized-set digest order.
    pub fn identity_page(
        &mut self,
        offset: u128,
        limit: usize,
    ) -> Result<Vec<StandardBoard64TilingIdentity>, SymbolicFamilyError> {
        let page = self.sets.page(self.identities, offset, limit)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(page.len())
            .map_err(|_| SymbolicFamilyError::Resource("symbolic_output_allocation"))?;
        for rows in page {
            let placements = rows.into_iter().map(|row| {
                let skeleton = self.catalog.skeleton(row);
                PiecePlacementMask::new(skeleton.piece, skeleton.cells)
            });
            let identity = StandardBoard64TilingIdentity::from_placements(
                self.catalog.initial_board(),
                placements,
            )
            .map_err(|_| SymbolicFamilyError::InvalidProblem("symbolic_output_identity"))?;
            output.push(identity);
        }
        Ok(output)
    }

    /// Count original sources for one requested complete identity, using the
    /// retained joint relation. No source page or queue list is materialized.
    pub fn source_count_for_identity(
        &mut self,
        identity: &StandardBoard64TilingIdentity,
    ) -> Result<u128, SymbolicFamilyError> {
        if identity.initial_board_mask() != self.catalog.initial_board()
            || identity.placement_count() * 4 != self.catalog.required_cells().count_ones() as usize
        {
            return Err(SymbolicFamilyError::InvalidProblem(
                "symbolic_output_foreign_identity",
            ));
        }
        let mut root = self.accepted;
        for index in 0..identity.placement_count() {
            let placement = identity
                .placement(index)
                .expect("identity placement bounds");
            let Some(row) = self
                .catalog
                .skeleton_id(placement.piece(), placement.cells_mask())
            else {
                return Ok(0);
            };
            root = self.sets.select(root, row, true)?;
        }
        Ok(self.sets.count(root)?)
    }

    /// Reclaim query scratch while retaining all three published relation roots.
    pub fn compact(&mut self) -> Result<(), SymbolicFamilyError> {
        let mut roots = [self.accepted, self.identities, self.covered_sources];
        self.sets.compact(&mut roots)?;
        [self.accepted, self.identities, self.covered_sources] = roots;
        Ok(())
    }
}

/// Research execution for a complete, fixed-rank, standard Board64 PC family.
/// It has no implicit time limit and no candidate-enumeration fallback. The
/// feature does not activate this path in product dispatch. First-success
/// ordered kicks use the active verified table, including imported tables.
/// Diagnostic sinks receive frequent phase/context markers and coarse work
/// pulses. Keep these callbacks lightweight; sampling, clocks and logging
/// belong to the diagnostic owner rather than a row-operation callback.
pub fn solve_symbolic_family(
    problem: &SearchProblem,
    limits: SymbolicFamilyLimits,
    control: &ExecutionControl,
) -> Result<SymbolicFamilyResult, SymbolicFamilyError> {
    solve_with_occupancy::<false>(problem, limits, control)
}

/// Opt-in physical quotient. Original placement/source identities remain in
/// the joint ZDD; only movement predicates use original cell occupancy.
/// No candidate enumeration or fallback to the row-variable path is used.
pub fn solve_symbolic_family_with_cell_occupancy(
    problem: &SearchProblem,
    limits: SymbolicFamilyLimits,
    control: &ExecutionControl,
) -> Result<SymbolicFamilyResult, SymbolicFamilyError> {
    solve_with_occupancy::<true>(problem, limits, control)
}

fn solve_with_occupancy<const CELLS: bool>(
    problem: &SearchProblem,
    limits: SymbolicFamilyLimits,
    control: &ExecutionControl,
) -> Result<SymbolicFamilyResult, SymbolicFamilyError> {
    validate_scope(problem, control)?;
    // Markers update an optional diagnostic context; the sink owns sampling.
    // No clock, formatting or logging belongs in these operation boundaries.
    let phase = |stage| control.report_progress(stage, 0, None);
    phase("symbolic_phase_catalog");
    let catalog = GeometryCatalog::compile(problem).map_err(geometry_error)?;
    let target_depth = catalog.required_cells().count_ones() as usize / 4;
    if problem.exact_pieces() != Some(target_depth) || target_depth == 0 {
        return Err(SymbolicFamilyError::Unsupported(
            "symbolic_fixed_target_rank",
        ));
    }
    let row_count = u32::try_from(catalog.skeleton_count())
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_catalog_variable_count"))?;
    let set_limits = ZddLimits {
        max_nodes: limits.max_set_nodes,
        max_work: limits.max_work_per_arena,
        max_memo_entries: limits.max_memo_entries,
    };
    let mut sets = ZddArena::new(set_limits);
    sets.set_cancellation(control.cancellation.clone());
    sets.set_progress_sink(control.progress_sink.clone());
    phase("symbolic_phase_source");
    let mut supply = SymbolicFamilySupply::compile(problem, row_count, &mut sets)?;
    phase("symbolic_phase_targets");
    let keys = geometry_keys(problem, target_depth, &mut supply, &mut sets)?;
    phase("symbolic_phase_geometry");
    let geometry = compile_symbolic_geometry(
        &catalog,
        keys,
        limits.max_geometry_nodes,
        limits.max_work_per_arena,
        control,
    )
    .map_err(geometry_error)?;
    let mut report = SymbolicFamilyReport {
        movement_variable_space: if CELLS {
            "original-cells"
        } else {
            "catalog-rows"
        },
        geometry_filter_scope: if CELLS {
            "piece-union"
        } else {
            "row-extension"
        },
        catalog_rows: catalog.skeleton_count(),
        geometry_nodes: geometry.family.node_count() as usize,
        geometry_bytes_estimate: geometry.family.retained_bytes(),
        geometry_expanded_nodes: geometry.expanded_nodes,
        geometry_tablebase_pruned_states: geometry.tablebase_pruned_states,
        geometry_released_construction_bytes: geometry.released_construction_bytes,
        source_count: supply.source_count(),
        source_residual_states: supply.compiled_residual_states(),
        ..SymbolicFamilyReport::default()
    };
    report
        .ranks
        .try_reserve_exact(target_depth)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_rank_report_allocation"))?;
    let mut row_cells = Vec::new();
    row_cells
        .try_reserve_exact(row_count as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_catalog_mask_allocation"))?;
    row_cells.extend((0..row_count).map(|row| catalog.skeleton(row).cells));
    let mut bridge = if CELLS {
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(row_cells.len())
            .map_err(|_| SymbolicFamilyError::Resource("symbolic_catalog_mask_allocation"))?;
        cells.extend_from_slice(&row_cells);
        MovementBridge::Cells(CellFamilyBridge::new(
            cells,
            catalog.initial_board(),
            u32::from(catalog.width()) * u32::from(catalog.height()),
            limits.max_work_per_arena,
            limits.max_memo_entries,
        )?)
    } else {
        MovementBridge::Rows(FamilyBridge::new(
            row_count,
            limits.max_work_per_arena,
            limits.max_memo_entries,
        ))
    };
    phase("symbolic_phase_compatibility_certificate");
    let mut down = GeometryCompatibilityFilter::new(
        &geometry.family,
        geometry.root,
        row_cells,
        catalog.required_cells(),
        set_limits,
        control,
    )?;
    phase("symbolic_phase_occupancy");
    let mut predicates = BooleanArena::new(
        if CELLS {
            u32::from(catalog.width()) * u32::from(catalog.height())
        } else {
            row_count
        },
        BooleanLimits {
            max_nodes: limits.max_predicate_nodes,
            max_work: limits.max_work_per_arena,
            max_memo_entries: limits.max_memo_entries,
        },
    )?;
    predicates.set_cancellation(control.cancellation.clone());
    predicates.set_progress_sink(control.progress_sink.clone());
    let occupancy = if CELLS {
        SymbolicOccupancy::compile_cells(&catalog, &mut predicates, control)?
    } else {
        SymbolicOccupancy::compile(&catalog, &mut predicates, control)?
    };
    let persistent_predicates = predicates.checkpoint();
    phase("symbolic_phase_templates");
    let templates = match super::ordered_kick_authority(problem, catalog.width(), catalog.height())
        .map_err(geometry_error)?
    {
        Some(authority) => SharedReachabilityTemplates::with_authority(authority),
        None => {
            super::ensure_canonical_kick_profile(problem).map_err(geometry_error)?;
            SharedReachabilityTemplates::new(
                catalog.width(),
                catalog.height(),
                problem.kick_profile().profile_id(),
            )
        }
    };
    phase("symbolic_phase_initial_down");
    let initial = down.filter_extendible(&mut sets, supply.root())?;
    let mut current = BTreeMap::new();
    if initial != ZddId::EMPTY {
        current.insert(supply.initial_state(), initial);
    }
    for rank in 0..target_depth {
        check_cancelled(control)?;
        control.report_progress(
            "symbolic_context_rank",
            (rank + 1) as u64,
            Some(target_depth as u64),
        );
        phase("symbolic_phase_rank_domain");
        let mut rows = ZddId::EMPTY;
        for &root in current.values() {
            let projected = sets.project_prefix(root, row_count)?;
            rows = sets.union(rows, projected)?;
        }
        if rows == ZddId::EMPTY {
            break;
        }
        let domain = bridge.domain(&sets, &mut predicates, rows, control)?;
        phase("symbolic_phase_board");
        let board = occupancy.for_domain(&mut predicates, domain, control)?;
        let mut next = BTreeMap::new();
        let compatibility_start = down.metrics();
        let cell_support_start = bridge.support_work_steps();
        let mut rank_report = SymbolicFamilyRankReport {
            placed_pieces: rank + 1,
            compatibility_cache_peak_bytes_estimate: down.retained_bytes(),
            ..SymbolicFamilyRankReport::default()
        };
        // Project the ready row family once per piece. Movement depends on
        // this physical domain; the original state/source relation remains
        // attached until the exact semijoin when its rows are extended.
        let mut ready_by_piece = BTreeMap::new();
        let mut ready_rows_by_piece = BTreeMap::new();
        let mut domains_by_rows = BTreeMap::from([(rows, domain)]);
        let mut pieces = Vec::new();
        pieces
            .try_reserve_exact(PieceKind::STANDARD_TETROMINOES.len())
            .map_err(|_| SymbolicFamilyError::Resource("symbolic_piece_task_allocation"))?;
        for piece in PieceKind::STANDARD_TETROMINOES {
            control.report_progress("symbolic_context_piece", piece.as_ascii() as u64, None);
            phase("symbolic_phase_supply");
            let mut ready = BTreeMap::new();
            for (&state, &root) in &current {
                for transition in
                    supply.successors(&mut sets, root, state, piece, rank + 1 == target_depth)?
                {
                    merge_root(&mut ready, transition.state, transition.root, &mut sets)?;
                }
            }
            if ready.is_empty() {
                continue;
            }
            let mut ready_rows = ZddId::EMPTY;
            for &root in ready.values() {
                let projected = sets.project_prefix(root, row_count)?;
                rank_report.ready_row_projections += 1;
                ready_rows = sets.union(ready_rows, projected)?;
            }
            let piece_domain = if let Some(&cached) = domains_by_rows.get(&ready_rows) {
                cached
            } else {
                let compiled = bridge.domain(&sets, &mut predicates, ready_rows, control)?;
                domains_by_rows.insert(ready_rows, compiled);
                rank_report.piece_domain_builds += 1;
                compiled
            };
            ready_by_piece.insert(piece, ready);
            ready_rows_by_piece.insert(piece, ready_rows);
            pieces.push((piece, piece_domain));
        }
        let requested_workers = problem.backend_policy().workers();
        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        let parallel_reason = super::symbolic_family_parallel::parallel_movement_fallback_reason(
            &predicates,
            requested_workers,
            pieces.len(),
        );
        #[cfg(not(all(feature = "parallel", not(target_family = "wasm"))))]
        let parallel_reason = Some("native-parallel-feature-unavailable");
        let mut parallel_report = SymbolicFamilyParallelReport {
            requested_workers,
            eligible_pieces: pieces.len(),
            fallback_reason: parallel_reason,
            ..SymbolicFamilyParallelReport::default()
        };
        {
            let mut consume_piece =
                |piece: PieceKind,
                 predicate_owner: &mut BooleanArena,
                 legal_rows: &[(u32, BooleanId)],
                 work: super::reachability::symbolic_family::SymbolicReachabilityWork|
                 -> Result<(), SymbolicFamilyError> {
                    control.report_progress(
                        "symbolic_context_piece",
                        piece.as_ascii() as u64,
                        None,
                    );
                    rank_report.movement_poses += work.poses;
                    rank_report.guarded_movement_edges += work.guarded_edges;
                    rank_report.movement_propagations += work.propagation_edges;
                    rank_report.vertical_seed_edges += work.vertical_seed_edges;
                    rank_report.vertical_seed_updates += work.vertical_seed_updates;
                    rank_report.early_reached_rejections += work.early_reached_rejections;
                    rank_report.rotation_candidate_checks += work.rotation_candidate_checks;
                    rank_report.rotation_candidates_pruned += work.rotation_candidates_pruned;
                    rank_report.rotation_guard_no_prior += work.rotation_guard_no_prior;
                    rank_report.rotation_guard_single_bit += work.rotation_guard_single_bit;
                    rank_report.rotation_guard_single_mask += work.rotation_guard_single_mask;
                    rank_report.rotation_guard_ordered += work.rotation_guard_ordered;
                    rank_report.rotation_ordered_prior_terms += work.rotation_ordered_prior_terms;
                    phase("symbolic_phase_rows");
                    let ready = &ready_by_piece[&piece];
                    let ready_rows = ready_rows_by_piece[&piece];
                    let mut consume_extension =
                        |sets: &mut ZddArena,
                         row_id: u32,
                         selected: ZddId,
                         extended: ZddId,
                         allowed: ZddId,
                         rank_report: &mut SymbolicFamilyRankReport|
                         -> Result<(), SymbolicFamilyError> {
                            if allowed == ZddId::EMPTY {
                                return Ok(());
                            }
                            // Legality excludes an already-selected row, so
                            // add/remove is a bijection. Keep the original
                            // state/source relation through the semijoin.
                            let allowed_before = if allowed == extended {
                                selected
                            } else {
                                sets.remove(allowed, row_id)?
                            };
                            for (&state, &root) in ready {
                                check_cancelled(control)?;
                                phase("symbolic_phase_row_bridge");
                                let selected = if allowed_before == ready_rows {
                                    root
                                } else {
                                    rank_report.source_semijoins += 1;
                                    sets.filter_prefix(root, allowed_before, row_count)?
                                };
                                if selected == ZddId::EMPTY {
                                    continue;
                                }
                                phase("symbolic_phase_row_add");
                                let extendible = sets.add(selected, row_id)?;
                                phase("symbolic_phase_row_merge");
                                merge_root(&mut next, state, extendible, sets)?;
                            }
                            Ok(())
                        };
                    let mut extensions = Vec::new();
                    if CELLS {
                        extensions
                            .try_reserve_exact(legal_rows.len())
                            .map_err(|_| {
                                SymbolicFamilyError::Resource(
                                    "symbolic_piece_extensions_allocation",
                                )
                            })?;
                    }
                    let mut all_extended = ZddId::EMPTY;
                    for &(row_id, legal) in legal_rows {
                        check_cancelled(control)?;
                        control.report_progress(
                            "symbolic_context_row",
                            u64::from(row_id),
                            Some(u64::from(row_count)),
                        );
                        rank_report.legal_row_queries += 1;
                        if legal == BooleanId::FALSE {
                            continue;
                        }
                        phase("symbolic_phase_row_bridge");
                        let selected = bridge.filter(
                            &mut sets,
                            predicate_owner,
                            ready_rows,
                            legal,
                            control,
                        )?;
                        if selected == ZddId::EMPTY {
                            continue;
                        }
                        phase("symbolic_phase_row_add");
                        let extended = sets.add(selected, row_id)?;
                        if CELLS {
                            // Geometry is a predicate of the extended row set,
                            // independent of which legal row was added last.
                            phase("symbolic_phase_piece_union");
                            all_extended = sets.union(all_extended, extended)?;
                            extensions.push((row_id, selected, extended));
                            continue;
                        }
                        rank_report.geometry_filters += 1;
                        phase("symbolic_phase_row_down");
                        let allowed = down.filter_extendible(&mut sets, extended)?;
                        consume_extension(
                            &mut sets,
                            row_id,
                            selected,
                            extended,
                            allowed,
                            &mut rank_report,
                        )?;
                    }
                    if CELLS {
                        control.report_progress(
                            "symbolic_context_piece",
                            piece.as_ascii() as u64,
                            None,
                        );
                        let accepted = if all_extended == ZddId::EMPTY {
                            ZddId::EMPTY
                        } else {
                            rank_report.geometry_filters += 1;
                            phase("symbolic_phase_piece_down");
                            down.filter_extendible(&mut sets, all_extended)?
                        };
                        // A = (union E_r) intersect Down(F). Since E_r is a
                        // subset of that union, A intersect E_r equals
                        // E_r intersect Down(F) exactly.
                        // Release derived geometry maps before reconstructing
                        // the original row/source transitions from these roots.
                        release_compatibility_memo(&mut down, &mut rank_report, control);
                        if accepted != ZddId::EMPTY {
                            for (row_id, selected, extended) in extensions {
                                check_cancelled(control)?;
                                control.report_progress(
                                    "symbolic_context_row",
                                    u64::from(row_id),
                                    Some(u64::from(row_count)),
                                );
                                phase("symbolic_phase_row_intersect");
                                let allowed = if accepted == all_extended {
                                    extended
                                } else {
                                    sets.intersection(accepted, extended)?
                                };
                                consume_extension(
                                    &mut sets,
                                    row_id,
                                    selected,
                                    extended,
                                    allowed,
                                    &mut rank_report,
                                )?;
                            }
                        }
                    } else {
                        release_compatibility_memo(&mut down, &mut rank_report, control);
                    }
                    Ok(())
                };
            #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
            if parallel_reason.is_none() {
                control.report_progress(
                    "symbolic_context_rank",
                    (rank + 1) as u64,
                    Some(target_depth as u64),
                );
                phase("symbolic_phase_movement");
                let parallel = super::symbolic_family_parallel::run_piece_movements(
                    &board,
                    &mut predicates,
                    &templates,
                    &pieces,
                    requested_workers,
                    control,
                    |movement| {
                        consume_piece(
                            movement.piece,
                            &mut movement.predicates,
                            &movement.legal_rows,
                            movement.work,
                        )
                    },
                )?;
                parallel_report.spawned_workers = parallel.spawned_workers;
                parallel_report.active_workers = parallel.active_workers;
                parallel_report.peak_concurrent_movement_tasks =
                    parallel.peak_concurrent_movement_tasks;
                parallel_report.shared_prefix_nodes = parallel.shared_prefix_nodes;
                parallel_report.shared_prefix_retained_bytes_estimate =
                    parallel.shared_prefix_retained_bytes_estimate;
                parallel_report.private_node_peak_sum_upper_bound =
                    parallel.private_node_peak_sum_upper_bound;
                parallel_report.private_retained_bytes_sum_estimate =
                    parallel.private_retained_bytes_sum_estimate;
                parallel_report.released_builder_bytes_sum_estimate =
                    parallel.released_builder_bytes_sum_estimate;
                parallel_report.predicate_work_steps = parallel.predicate_work_steps;
                parallel_report.predicate_compactions = parallel.predicate_compactions;
                parallel_report.predicate_reclaimed_nodes = parallel.predicate_reclaimed_nodes;
                parallel_report.predicate_compaction_work_steps =
                    parallel.predicate_compaction_work_steps;
                parallel_report.predicate_discarded_apply_entries =
                    parallel.predicate_discarded_apply_entries;
                parallel_report.compaction_peak_temporary_bytes_sum_estimate =
                    parallel.compaction_peak_temporary_bytes_sum_estimate;
            }
            if parallel_reason.is_some() {
                for (piece, piece_domain) in pieces {
                    control.report_progress(
                        "symbolic_context_piece",
                        piece.as_ascii() as u64,
                        None,
                    );
                    phase("symbolic_phase_movement");
                    let relation = board.compile_piece_in_domain(
                        &mut predicates,
                        &templates,
                        piece,
                        piece_domain,
                        control,
                    )?;
                    let work = relation.work();
                    let mut legal_rows = Vec::new();
                    super::symbolic_movement_progress::report_phase(
                        control,
                        piece,
                        super::symbolic_movement_progress::MovementPhase::LegalRows,
                        &predicates,
                    );
                    for row_id in 0..row_count {
                        if catalog.skeleton(row_id).piece != piece {
                            continue;
                        }
                        check_cancelled(control)?;
                        control.report_progress(
                            "symbolic_context_row",
                            u64::from(row_id),
                            Some(u64::from(row_count)),
                        );
                        phase("symbolic_phase_row_legal");
                        let legal = relation.legal_row(&mut predicates, row_id, control)?;
                        legal_rows.try_reserve(1).map_err(|_| {
                            SymbolicFamilyError::Resource("symbolic_legal_rows_allocation")
                        })?;
                        legal_rows.push((row_id, legal));
                    }
                    drop(relation);
                    consume_piece(piece, &mut predicates, &legal_rows, work)?;
                }
            }
        }
        rank_report.movement_parallel = parallel_report;
        // Reassert the rank to clear stale piece/row context for shared work.
        control.report_progress(
            "symbolic_context_rank",
            (rank + 1) as u64,
            Some(target_depth as u64),
        );
        phase("symbolic_phase_rank_count");
        let mut joint_next = ZddId::EMPTY;
        for &root in next.values() {
            joint_next = sets.union(joint_next, root)?;
        }
        rank_report.joint_members = sets.count(joint_next)?;
        rank_report.supply_states = next.len();
        rank_report.predicate_nodes = predicates.node_count();
        rank_report.compatibility_cache_bytes_estimate = down.retained_bytes();
        let compatibility_end = down.metrics();
        rank_report.compatibility_filter_misses = compatibility_end
            .filter_misses
            .saturating_sub(compatibility_start.filter_misses);
        rank_report.compatibility_guard_misses = compatibility_end
            .guard_misses
            .saturating_sub(compatibility_start.guard_misses);
        rank_report.set_nodes_before_compaction = sets.node_count();
        rank_report.cell_support_work_steps = bridge.support_work_steps() - cell_support_start;
        rank_report.cell_support_cached_nodes = bridge.support_cache_entries();
        phase("symbolic_phase_rank_compact");
        down.clear_memo();
        bridge.clear_support_cache();
        if CELLS {
            control.report_progress("symbolic_cell_support_entries", 0, None);
        }
        rank_report.set_compaction_peak_bytes_estimate =
            compact_layer(&mut sets, &mut supply, &mut next)?;
        rank_report.set_nodes_after_compaction = sets.node_count();
        report.ranks.push(rank_report);
        // Only request-lifetime U(S) expressions survive. The arena keeps its
        // capacity for the next rank; this is logical reuse, not an RSS claim.
        phase("symbolic_phase_predicate_rewind");
        drop(board);
        predicates.rewind(persistent_predicates)?;
        current = next;
        control.report_progress(
            "symbolic_rank",
            (rank + 1) as u64,
            Some(target_depth as u64),
        );
    }
    check_cancelled(control)?;
    control.report_progress("symbolic_context_clear", 0, None);
    phase("symbolic_phase_accepted");
    let mut accepted = ZddId::EMPTY;
    // An early empty layer is complete emptiness. A resource error above never
    // reaches this completion path and cannot publish an empty success result.
    if report.ranks.len() == target_depth {
        for (&state, &root) in &current {
            if supply.is_accepting(state) {
                accepted = sets.union(accepted, root)?;
            }
        }
    }
    phase("symbolic_phase_identity_project");
    let mut source_variables = Vec::new();
    source_variables
        .try_reserve_exact((supply.variable_count() - row_count) as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_projection_allocation"))?;
    source_variables.extend(supply.source_variables());
    let identities = sets.project(accepted, &source_variables)?;
    phase("symbolic_phase_source_project");
    let mut row_variables = Vec::new();
    row_variables
        .try_reserve_exact(row_count as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_projection_allocation"))?;
    row_variables.extend(0..row_count);
    let covered_sources = sets.project(accepted, &row_variables)?;
    phase("symbolic_phase_final_count");
    report.unique_solution_count = sets.count(identities)?;
    report.covered_source_count = sets.count(covered_sources)?;
    report.solution_source_pairs = sets.count(accepted)?;
    report.predicate_peak_nodes = predicates.peak_node_count();
    report.predicate_work_steps = predicates.work_steps();
    report.bridge_work_steps = bridge.work_steps();
    report.compatibility_work_steps = down.work_steps();
    report.compatibility_filter_misses = down.metrics().filter_misses;
    report.compatibility_guard_misses = down.metrics().guard_misses;
    report.compatibility_saturated_unions = down.metrics().saturated_unions;
    report.compatibility_union_remainder_calls = down.metrics().union_remainder_calls;
    report.compatibility_union_remainder_created_nodes =
        down.metrics().union_remainder_created_nodes;
    report.compatibility_union_remainder_work_steps = down.metrics().union_remainder_work_steps;
    report.retained_predicate_bytes_estimate = predicates.retained_bytes();
    report.retained_compatibility_bytes_estimate = down.retained_bytes();
    // All these owners are dead before the final arena copies its live roots.
    // Retaining them until result construction inflated the compaction peak.
    drop(down);
    drop(geometry);
    drop(occupancy);
    drop(predicates);
    drop(bridge);
    let mut roots = [accepted, identities, covered_sources];
    phase("symbolic_phase_final_compact");
    report.result_compaction_peak_bytes_estimate = sets.compact(&mut roots)?.peak_retained_bytes;
    report.set_nodes_created = sets.metrics().created_nodes;
    report.set_apply_misses = sets.metrics().apply_misses;
    report.set_compactions = sets.metrics().compactions;
    report.set_work_steps = sets.work_steps();
    report.retained_set_bytes_estimate = sets.retained_bytes();
    // Only accepted sets and the catalog survive in the result.
    phase("symbolic_phase_result_release");
    Ok(SymbolicFamilyResult {
        catalog,
        sets,
        accepted: roots[0],
        identities: roots[1],
        covered_sources: roots[2],
        source_length: (supply.variable_count() - row_count) as usize
            / PieceKind::STANDARD_TETROMINOES.len(),
        report,
    })
}

fn release_compatibility_memo(
    down: &mut GeometryCompatibilityFilter<'_>,
    report: &mut SymbolicFamilyRankReport,
    control: &ExecutionControl,
) {
    report.compatibility_cache_peak_bytes_estimate = report
        .compatibility_cache_peak_bytes_estimate
        .max(down.retained_bytes());
    down.clear_memo();
    report.compatibility_cache_releases += 1;
    control.report_progress("symbolic_storage_compatibility_memo_entries", 0, None);
    control.report_progress(
        "symbolic_storage_compatibility_retained_bytes",
        down.retained_bytes() as u64,
        None,
    );
}

/// The two spaces have different variable meanings. Keep dispatch at the
/// domain/filter boundary so cell IDs can never enter the row bridge.
enum MovementBridge {
    Rows(FamilyBridge),
    Cells(CellFamilyBridge),
}

impl MovementBridge {
    fn domain(
        &mut self,
        sets: &ZddArena,
        predicates: &mut BooleanArena,
        root: ZddId,
        control: &ExecutionControl,
    ) -> Result<BooleanId, SymbolicFamilyError> {
        match self {
            Self::Rows(bridge) => bridge.domain(sets, predicates, root, control),
            Self::Cells(bridge) => bridge.domain(sets, predicates, root, control),
        }
    }

    fn filter(
        &mut self,
        sets: &mut ZddArena,
        predicates: &mut BooleanArena,
        root: ZddId,
        condition: BooleanId,
        control: &ExecutionControl,
    ) -> Result<ZddId, SymbolicFamilyError> {
        match self {
            Self::Rows(bridge) => bridge.filter(sets, predicates, root, condition, control),
            Self::Cells(bridge) => {
                // All legal row roots precede this checkpoint. Cofactor
                // residuals and local memo IDs die before rewind, including
                // on refusal. Only the original ZDD owns the returned result.
                let checkpoint = predicates.checkpoint();
                let result = bridge.filter(sets, predicates, root, condition, control);
                let cleanup = predicates
                    .rewind(checkpoint)
                    .map_err(SymbolicFamilyError::from);
                match result {
                    Err(error) => Err(error),
                    Ok(result) => {
                        cleanup?;
                        Ok(result)
                    }
                }
            }
        }
    }

    fn work_steps(&self) -> u64 {
        match self {
            Self::Rows(bridge) => bridge.work_steps(),
            Self::Cells(bridge) => bridge.work_steps(),
        }
    }

    fn support_work_steps(&self) -> u64 {
        match self {
            Self::Rows(_) => 0,
            Self::Cells(bridge) => bridge.support_work_steps(),
        }
    }

    fn support_cache_entries(&self) -> usize {
        match self {
            Self::Rows(_) => 0,
            Self::Cells(bridge) => bridge.support_cache_entries(),
        }
    }

    fn clear_support_cache(&mut self) {
        if let Self::Cells(bridge) = self {
            bridge.clear_support_cache();
        }
    }
}

fn geometry_keys(
    problem: &SearchProblem,
    target_depth: usize,
    supply: &mut SymbolicFamilySupply,
    arena: &mut ZddArena,
) -> Result<Vec<PieceMultisetKey>, SymbolicFamilyError> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or(SymbolicFamilyError::Unsupported("symbolic_source_universe"))?;
    if let Some(family) = universe
        .symbolic_factorized_full_draw_family_for_execution(
            target_depth,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            super::packing_hold_projection(problem),
        )
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_target_multiset_allocation"))?
    {
        let mut keys = Vec::new();
        keys.try_reserve_exact(family.groups().len())
            .map_err(|_| SymbolicFamilyError::Resource("symbolic_target_multiset_allocation"))?;
        keys.extend(family.groups().iter().map(|group| group.key()));
        return Ok(keys);
    }
    Ok(supply.reachable_multisets(arena, target_depth)?)
}

fn compact_layer(
    arena: &mut ZddArena,
    supply: &mut SymbolicFamilySupply,
    layer: &mut BTreeMap<SymbolicSupplyState, ZddId>,
) -> Result<usize, SymbolicFamilyError> {
    let mut roots = Vec::new();
    roots
        .try_reserve_exact(layer.len() + 1)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_gc_root_allocation"))?;
    roots.push(supply.root());
    roots.extend(layer.values().copied());
    let compaction = arena.compact(&mut roots)?;
    supply.remap_root(roots[0]);
    for (root, remapped) in layer.values_mut().zip(roots.into_iter().skip(1)) {
        *root = remapped;
    }
    Ok(compaction.peak_retained_bytes)
}

fn merge_root(
    layer: &mut BTreeMap<SymbolicSupplyState, ZddId>,
    state: SymbolicSupplyState,
    root: ZddId,
    arena: &mut ZddArena,
) -> Result<(), SymbolicFamilyError> {
    if root != ZddId::EMPTY {
        let previous = layer.get(&state).copied().unwrap_or(ZddId::EMPTY);
        layer.insert(state, arena.union(previous, root)?);
    }
    Ok(())
}

fn validate_scope(
    problem: &SearchProblem,
    control: &ExecutionControl,
) -> Result<(), SymbolicFamilyError> {
    check_cancelled(control)?;
    if control.partition().count() != 1 {
        return Err(SymbolicFamilyError::Unsupported(
            "symbolic_whole_relation_required",
        ));
    }
    if problem.initial_board().width() != 10
        || !(1..=6).contains(&problem.compact_pc_geometry_height())
    {
        return Err(SymbolicFamilyError::Unsupported(
            "symbolic_standard_board64",
        ));
    }
    if !matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
        || !matches!(
            problem.preset(),
            SearchProblemPreset::OpeningPc | SearchProblemPreset::ScenarioPc
        )
        || !matches!(
            problem.objective().kind(),
            ObjectiveKind::All | ObjectiveKind::Unique
        )
        || problem.setup_query().is_some()
        || problem.build_query().is_some()
        || problem.build_replay_evidence_requested()
        || problem.allowed_colored_solution_identities().is_some()
        || problem.checkpoint_schedule().is_some()
        || problem.continuation_policy().min_remaining_queue() != 0
    {
        return Err(SymbolicFamilyError::Unsupported(
            "symbolic_plain_boolean_pc_contract",
        ));
    }
    Ok(())
}

fn check_cancelled(control: &ExecutionControl) -> Result<(), SymbolicFamilyError> {
    if control.is_cancelled() {
        Err(SymbolicFamilyError::Cancelled)
    } else {
        Ok(())
    }
}

fn geometry_error(error: WasmExactSearchError) -> SymbolicFamilyError {
    match error {
        WasmExactSearchError::Cancelled => SymbolicFamilyError::Cancelled,
        WasmExactSearchError::InvalidProblem(reason)
            if reason.starts_with("symbolic_geometry_")
                && reason != "symbolic_geometry_multiset_depth" =>
        {
            SymbolicFamilyError::Resource(reason)
        }
        WasmExactSearchError::InvalidProblem(reason) => SymbolicFamilyError::InvalidProblem(reason),
        error @ WasmExactSearchError::ResourceAdmission(_) => {
            SymbolicFamilyError::Resource(error.reason())
        }
    }
}
