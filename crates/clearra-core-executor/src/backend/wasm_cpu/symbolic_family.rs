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
    geometry_partial_family::{GeometryDownView, ZddArena, ZddError, ZddId, ZddLimits},
    reachability::{symbolic_family::SymbolicOccupancy, SharedReachabilityTemplates},
    symbolic_boolean::{BooleanArena, BooleanError, BooleanId, BooleanLimits},
    symbolic_family_bridge::FamilyBridge,
    symbolic_family_supply::{SymbolicFamilySupply, SymbolicSupplyError, SymbolicSupplyState},
    WasmExactSearchError,
};

#[cfg(test)]
#[path = "symbolic_family_tests.rs"]
mod tests;

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
    pub placed_pieces: usize,
    pub supply_states: usize,
    pub joint_members: u128,
    pub set_nodes_before_compaction: usize,
    pub set_nodes_after_compaction: usize,
    pub set_compaction_peak_bytes_estimate: usize,
    pub predicate_nodes: usize,
    pub derivative_nodes: usize,
    pub movement_poses: usize,
    pub guarded_movement_edges: usize,
    pub movement_propagations: u64,
    pub legal_row_queries: u64,
    pub geometry_filters: u64,
}

/// Counts refer to unique normalized placement identities and original source
/// identities. They do not count operation orders, Build paths or spin traces.
/// Capacity estimates below exclude allocator and operation scratch; they are
/// neither peak RSS nor a process memory upper bound.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymbolicFamilyReport {
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
    pub predicate_peak_nodes: usize,
    pub predicate_work_steps: u64,
    pub bridge_work_steps: u64,
    pub derivative_work_steps: u64,
    pub derivative_cache_misses: u64,
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
    report: SymbolicFamilyReport,
}

impl SymbolicFamilyResult {
    pub fn report(&self) -> &SymbolicFamilyReport {
        &self.report
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
pub fn solve_symbolic_family(
    problem: &SearchProblem,
    limits: SymbolicFamilyLimits,
    control: &ExecutionControl,
) -> Result<SymbolicFamilyResult, SymbolicFamilyError> {
    validate_scope(problem, control)?;
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
    let mut supply = SymbolicFamilySupply::compile(problem, row_count, &mut sets)?;
    let keys = geometry_keys(problem, target_depth, &mut supply, &mut sets)?;
    let geometry = compile_symbolic_geometry(
        &catalog,
        keys,
        limits.max_geometry_nodes,
        limits.max_work_per_arena,
        control,
    )
    .map_err(geometry_error)?;
    let mut report = SymbolicFamilyReport {
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
    let mut down = GeometryDownView::new(&geometry.family, geometry.root, row_count, set_limits)?;
    down.set_cancellation(control.cancellation.clone());
    let mut row_cells = Vec::new();
    row_cells
        .try_reserve_exact(row_count as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_catalog_mask_allocation"))?;
    row_cells.extend((0..row_count).map(|row| catalog.skeleton(row).cells));
    down.set_row_cells(&row_cells)?;
    drop(row_cells);
    let mut predicates = BooleanArena::new(
        row_count,
        BooleanLimits {
            max_nodes: limits.max_predicate_nodes,
            max_work: limits.max_work_per_arena,
            max_memo_entries: limits.max_memo_entries,
        },
    )?;
    predicates.set_cancellation(control.cancellation.clone());
    let occupancy = SymbolicOccupancy::compile(&catalog, &mut predicates, control)?;
    let persistent_predicates = predicates.checkpoint();
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
    let mut bridge = FamilyBridge::new(
        row_count,
        limits.max_work_per_arena,
        limits.max_memo_entries,
    );
    let initial = down.filter_extendible(&mut sets, supply.root())?;
    let mut current = BTreeMap::new();
    if initial != ZddId::EMPTY {
        current.insert(supply.initial_state(), initial);
    }
    for rank in 0..target_depth {
        check_cancelled(control)?;
        let mut joint = ZddId::EMPTY;
        for &root in current.values() {
            joint = sets.union(joint, root)?;
        }
        if joint == ZddId::EMPTY {
            break;
        }
        let domain = bridge.domain(&sets, &mut predicates, joint, control)?;
        let board = occupancy.for_domain(&mut predicates, domain, control)?;
        let mut next = BTreeMap::new();
        let mut rank_report = SymbolicFamilyRankReport {
            placed_pieces: rank + 1,
            ..SymbolicFamilyRankReport::default()
        };
        for piece in PieceKind::STANDARD_TETROMINOES {
            // Compile the small supply product once per kind, not per row.
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
            let relation = board.compile_piece(&mut predicates, &templates, piece, control)?;
            let work = relation.work();
            rank_report.movement_poses += work.poses;
            rank_report.guarded_movement_edges += work.guarded_edges;
            rank_report.movement_propagations += work.propagation_edges;
            for row_id in 0..row_count {
                if catalog.skeleton(row_id).piece != piece {
                    continue;
                }
                check_cancelled(control)?;
                rank_report.legal_row_queries += 1;
                let legal = relation.legal_row(&mut predicates, row_id, control)?;
                if legal == BooleanId::FALSE {
                    continue;
                }
                for (&state, &root) in &ready {
                    let selected = bridge.filter(&mut sets, &predicates, root, legal, control)?;
                    if selected == ZddId::EMPTY {
                        continue;
                    }
                    let extended = sets.add(selected, row_id)?;
                    rank_report.geometry_filters += 1;
                    let extendible = down.filter_extendible(&mut sets, extended)?;
                    merge_root(&mut next, state, extendible, &mut sets)?;
                }
            }
        }
        let mut joint_next = ZddId::EMPTY;
        for &root in next.values() {
            joint_next = sets.union(joint_next, root)?;
        }
        rank_report.joint_members = sets.count(joint_next)?;
        rank_report.supply_states = next.len();
        rank_report.predicate_nodes = predicates.node_count();
        rank_report.derivative_nodes = down.generated_node_count();
        rank_report.set_nodes_before_compaction = sets.node_count();
        rank_report.set_compaction_peak_bytes_estimate =
            compact_layer(&mut sets, &mut supply, &mut next)?;
        rank_report.set_nodes_after_compaction = sets.node_count();
        report.ranks.push(rank_report);
        // Only request-lifetime U(S) expressions survive. The arena keeps its
        // capacity for the next rank; this is logical reuse, not an RSS claim.
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
    let mut source_variables = Vec::new();
    source_variables
        .try_reserve_exact((supply.variable_count() - row_count) as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_projection_allocation"))?;
    source_variables.extend(supply.source_variables());
    let identities = sets.project(accepted, &source_variables)?;
    let mut row_variables = Vec::new();
    row_variables
        .try_reserve_exact(row_count as usize)
        .map_err(|_| SymbolicFamilyError::Resource("symbolic_projection_allocation"))?;
    row_variables.extend(0..row_count);
    let covered_sources = sets.project(accepted, &row_variables)?;
    report.unique_solution_count = sets.count(identities)?;
    report.covered_source_count = sets.count(covered_sources)?;
    report.solution_source_pairs = sets.count(accepted)?;
    report.predicate_peak_nodes = predicates.peak_node_count();
    report.predicate_work_steps = predicates.work_steps();
    report.bridge_work_steps = bridge.work_steps();
    report.derivative_work_steps = down.work_steps();
    report.derivative_cache_misses = down.metrics().derivative_misses;
    report.retained_predicate_bytes_estimate = predicates.retained_bytes();
    report.retained_derivative_bytes_estimate = down.retained_bytes();
    let mut roots = [accepted, identities, covered_sources];
    report.result_compaction_peak_bytes_estimate = sets.compact(&mut roots)?.peak_retained_bytes;
    report.set_nodes_created = sets.metrics().created_nodes;
    report.set_apply_misses = sets.metrics().apply_misses;
    report.set_compactions = sets.metrics().compactions;
    report.set_work_steps = sets.work_steps();
    report.retained_set_bytes_estimate = sets.retained_bytes();
    // The result owns neither the original geometry DAG, its derivative memo,
    // nor movement predicates. Only accepted sets and the catalog survive.
    drop(down);
    drop(geometry);
    drop(occupancy);
    Ok(SymbolicFamilyResult {
        catalog,
        sets,
        accepted: roots[0],
        identities: roots[1],
        covered_sources: roots[2],
        report,
    })
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
