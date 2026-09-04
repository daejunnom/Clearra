use std::collections::VecDeque;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_problem::{BuildProbabilityField, SearchProblem};
use clearra_supply::pattern_universe::PackingMultisetFamily;

use crate::tiling_solution_store::{pack_canonical_tiling_row_ids, PackedTilingRows};

use super::{
    build_probability::merge_symmetry_results,
    catalog::GeometryCatalog,
    distributed::{
        WasmCandidatePacket, WasmCandidateProducerAdvance, WasmDistributedBackendExecution,
        WasmDistributedGeometrySummary, WasmDistributedProgress, WasmDistributedResultMerger,
    },
    geometry::{GeometryAdvance, GeometrySearch},
    result::{canonical_tiling_rank_by_source, WasmExactSearchSession},
};

const ROOT_ADVANCE_WORK_BUDGET: usize = 32 * 1024;
const NO_TILING_ROOT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WasmPackedTilingIdentity {
    bucket_hash: u64,
    packed_rows: PackedTilingRows,
}

impl WasmPackedTilingIdentity {
    pub const fn new(bucket_hash: u64, packed_rows: PackedTilingRows) -> Self {
        Self {
            bucket_hash,
            packed_rows,
        }
    }

    pub const fn bucket_hash(self) -> u64 {
        self.bucket_hash
    }

    pub const fn packed_rows(self) -> PackedTilingRows {
        self.packed_rows
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmTilingRootChunk {
    pass_index: u8,
    root_ordinal: u32,
    chunk_sequence: u32,
    root_complete: bool,
    identities: Vec<WasmPackedTilingIdentity>,
    completed_roots: usize,
    candidate_family_count: Option<u128>,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_compositions: usize,
}

impl Default for WasmTilingRootChunk {
    fn default() -> Self {
        Self {
            pass_index: 0,
            root_ordinal: NO_TILING_ROOT,
            chunk_sequence: 0,
            root_complete: false,
            identities: Vec::new(),
            completed_roots: 0,
            candidate_family_count: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
        }
    }
}

impl WasmTilingRootChunk {
    #[allow(clippy::too_many_arguments)]
    pub fn from_wire_parts(
        pass_index: u8,
        root_ordinal: u32,
        chunk_sequence: u32,
        root_complete: bool,
        identities: Vec<WasmPackedTilingIdentity>,
        completed_roots: usize,
        candidate_family_count: Option<u128>,
        expanded_nodes: usize,
        peak_frontier: usize,
        domain_pruned_states: usize,
        hall_pruned_states: usize,
        column_pruned_states: usize,
        component_compositions: usize,
    ) -> Self {
        Self {
            pass_index,
            root_ordinal,
            chunk_sequence,
            root_complete,
            identities,
            completed_roots,
            candidate_family_count,
            expanded_nodes,
            peak_frontier,
            domain_pruned_states,
            hall_pruned_states,
            column_pruned_states,
            component_compositions,
        }
    }

    pub const fn pass_index(&self) -> u8 {
        self.pass_index
    }

    pub const fn root_ordinal(&self) -> Option<u32> {
        if self.root_ordinal == NO_TILING_ROOT {
            None
        } else {
            Some(self.root_ordinal)
        }
    }

    pub const fn chunk_sequence(&self) -> u32 {
        self.chunk_sequence
    }

    pub const fn root_complete(&self) -> bool {
        self.root_complete
    }

    pub fn identities(&self) -> &[WasmPackedTilingIdentity] {
        &self.identities
    }

    pub fn into_identities(self) -> Vec<WasmPackedTilingIdentity> {
        self.identities
    }

    pub const fn completed_roots(&self) -> usize {
        self.completed_roots
    }

    pub const fn candidate_family_count(&self) -> Option<u128> {
        self.candidate_family_count
    }

    pub const fn expanded_nodes(&self) -> usize {
        self.expanded_nodes
    }

    pub const fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }

    pub const fn domain_pruned_states(&self) -> usize {
        self.domain_pruned_states
    }

    pub const fn hall_pruned_states(&self) -> usize {
        self.hall_pruned_states
    }

    pub const fn column_pruned_states(&self) -> usize {
        self.column_pruned_states
    }

    pub const fn component_compositions(&self) -> usize {
        self.component_compositions
    }

    pub fn is_empty(&self) -> bool {
        self.identities.is_empty() && !self.root_complete && self.completed_roots == 0
    }
}

pub enum WasmTilingRootAdvance {
    Pending(WasmTilingRootChunk),
    Completed(WasmTilingRootChunk),
    Cancelled,
}

pub struct WasmTilingRootProducer {
    passes: Vec<TilingRootProducerPass>,
    next_pass: usize,
    build_probability: Option<TilingBuildProbabilityMerge>,
    root_count: usize,
    finished: bool,
    shared_external_retained_upper_bound_bytes: Option<u128>,
}

struct TilingRootProducerPass {
    merger: WasmDistributedResultMerger,
    root_order: Vec<u32>,
    next_root: usize,
}

struct TilingBuildProbabilityMerge {
    mirror_included: bool,
    mirror_distinct: bool,
    pattern_weights: WeightedPatternSet,
}

pub struct WasmTilingRootResultMerger {
    passes: Vec<WasmDistributedResultMerger>,
    build_probability: Option<TilingBuildProbabilityMerge>,
}

impl WasmTilingRootProducer {
    /// Allocation-free conservative projection for the producer-owned surface
    /// retained outside the shared exact-search session. The typed App
    /// authority includes this value in its fixed external envelope before the
    /// child session is admitted.
    pub fn checked_shared_external_retained_upper_bound(problem: &SearchProblem) -> Option<u128> {
        let universe = problem.piece_source().materialized_universe()?;
        let root_count = universe
            .checked_packing_multiset_family_build_projection(
                problem.exact_pieces()?,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                super::packing_hold_projection(problem),
                1,
            )?
            .max_group_count;
        (core::mem::size_of::<Self>() as u128)
            .checked_add(core::mem::size_of::<WasmTilingRootResultMerger>() as u128)?
            .checked_add(core::mem::size_of::<TilingRootProducerPass>() as u128)?
            .checked_add(core::mem::size_of::<WasmDistributedResultMerger>() as u128)?
            .checked_add(root_count.checked_mul(core::mem::size_of::<u32>() as u128)?)
    }

    pub fn build_probability_root_count(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> Result<usize, &'static str> {
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or("wasm_piece_source_not_materialized")?;
        let family_count = universe
            .packing_multiset_family_for_execution(
                field.target_piece_count(),
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                super::packing_hold_projection(problem),
            )
            .len();
        let pass_count = if field.includes_applicable_horizontal_mirror()
            && field.original_only().mirrored_horizontally() != field.original_only()
        {
            2
        } else {
            1
        };
        family_count
            .checked_mul(pass_count)
            .ok_or("wasm_tiling_root_count_overflow")
    }

    pub fn new(problem: &SearchProblem) -> Result<Self, &'static str> {
        let session = WasmExactSearchSession::new_external_geometry(problem)
            .map_err(super::distributed::map_error)?;
        Self::from_sessions(vec![session], None, None)
    }

    pub fn new_shared_under_authority(
        problem: std::sync::Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &crate::WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, &'static str> {
        let shared_external_retained_upper_bound_bytes =
            Self::checked_shared_external_retained_upper_bound(problem.as_ref())
                .ok_or("wasm_tiling_root_external_retained_projection_unavailable")?;
        let session = WasmExactSearchSession::new_shared_external_geometry_under_authority(
            problem,
            checked_external_retained_upper_bound_bytes,
            authority,
        )
        .map_err(super::distributed::map_error)?;
        Self::from_sessions(
            vec![session],
            None,
            Some(shared_external_retained_upper_bound_bytes),
        )
    }

    pub fn new_for_build_probability(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> Result<Self, &'static str> {
        let mirror_included = field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mirrored = mirror_included.then(|| original.mirrored_horizontally());
        let mirror_distinct = mirrored.is_some_and(|candidate| candidate != original);
        let mut sessions = Vec::with_capacity(usize::from(mirror_distinct) + 1);
        sessions.push(Self::session_for_build_probability_field(
            problem, original,
        )?);
        if let Some(mirrored) = mirrored.filter(|candidate| *candidate != original) {
            sessions.push(Self::session_for_build_probability_field(
                problem, mirrored,
            )?);
        }
        let pattern_weights = problem
            .piece_source()
            .materialized_pattern_weights()
            .ok_or("wasm_piece_source_not_materialized")?
            .clone();
        Self::from_sessions(
            sessions,
            Some(TilingBuildProbabilityMerge {
                mirror_included,
                mirror_distinct,
                pattern_weights,
            }),
            None,
        )
    }

    fn session_for_build_probability_field(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> Result<WasmExactSearchSession, &'static str> {
        let initial_board = field
            .compact_base_mask()
            .ok_or("wasm_tiling_root_compact_base_missing")?;
        let required_cells = field
            .compact_target_mask()
            .ok_or("wasm_tiling_root_compact_target_missing")?;
        WasmExactSearchSession::new_external_geometry_for_required_cells_on_board(
            problem,
            initial_board,
            required_cells,
        )
        .map_err(super::distributed::map_error)
    }

    fn from_sessions(
        sessions: Vec<WasmExactSearchSession>,
        build_probability: Option<TilingBuildProbabilityMerge>,
        shared_external_retained_upper_bound_bytes: Option<u128>,
    ) -> Result<Self, &'static str> {
        let mut passes = Vec::new();
        passes
            .try_reserve_exact(sessions.len())
            .map_err(|_| "wasm_tiling_root_pass_storage_unavailable")?;
        let mut root_count = 0_usize;
        for mut session in sessions {
            let root_order = session
                .distributed_tiling_root_order()
                .map_err(super::distributed::map_error)?;
            if root_order.is_empty() {
                return Err("wasm_tiling_root_set_empty");
            }
            root_count = root_count
                .checked_add(root_order.len())
                .ok_or("wasm_tiling_root_count_overflow")?;
            session
                .prepare_distributed_tiling_root_runs(root_order.len())
                .map_err(super::distributed::map_error)?;
            passes.push(TilingRootProducerPass {
                merger: WasmDistributedResultMerger::from_session(
                    session
                        .into_distributed_finalizer()
                        .map_err(super::distributed::map_error)?,
                ),
                root_order,
                next_root: 0,
            });
        }
        let producer = Self {
            passes,
            next_pass: 0,
            build_probability,
            root_count,
            finished: false,
            shared_external_retained_upper_bound_bytes,
        };
        producer.validate_shared_external_retained_bytes(0)?;
        Ok(producer)
    }

    fn checked_shared_external_retained_bytes(
        &self,
        future_merger_capacity: usize,
    ) -> Option<u128> {
        let mut bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add(core::mem::size_of::<WasmTilingRootResultMerger>() as u128)?
            .checked_add(
                (self.passes.capacity() as u128)
                    .checked_mul(core::mem::size_of::<TilingRootProducerPass>() as u128)?,
            )?
            .checked_add(
                (future_merger_capacity as u128)
                    .checked_mul(core::mem::size_of::<WasmDistributedResultMerger>() as u128)?,
            )?;
        for pass in &self.passes {
            bytes = bytes.checked_add(
                (pass.root_order.capacity() as u128)
                    .checked_mul(core::mem::size_of::<u32>() as u128)?,
            )?;
        }
        Some(bytes)
    }

    fn validate_shared_external_retained_bytes(
        &self,
        future_merger_capacity: usize,
    ) -> Result<(), &'static str> {
        let Some(limit) = self.shared_external_retained_upper_bound_bytes else {
            return Ok(());
        };
        let retained = self
            .checked_shared_external_retained_bytes(future_merger_capacity)
            .ok_or("wasm_tiling_root_external_retained_projection_overflow")?;
        if retained > limit {
            return Err("wasm_tiling_root_external_retained_envelope_exceeded");
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, &'static str> {
        if self.finished {
            return Err("wasm_tiling_root_producer_already_finished");
        }
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        for offset in 0..self.passes.len() {
            let pass_index = (self.next_pass + offset) % self.passes.len();
            let pass = &mut self.passes[pass_index];
            if pass.next_root >= pass.root_order.len() {
                continue;
            }
            let root_ordinal = pass.next_root;
            let family_index = pass.root_order[root_ordinal];
            pass.next_root += 1;
            self.next_pass = (pass_index + 1) % self.passes.len();
            return Ok(WasmCandidateProducerAdvance::Candidate(
                WasmCandidatePacket::for_pass(
                    root_ordinal as u64,
                    u8::try_from(pass_index).map_err(|_| "wasm_tiling_pass_index_overflow")?,
                    family_index,
                    Vec::new(),
                ),
            ));
        }
        self.finished = true;
        Ok(WasmCandidateProducerAdvance::Completed(
            WasmDistributedGeometrySummary {
                candidate_count: 0,
                candidate_digest: 0,
                candidate_family_count: Some(0),
                expanded_nodes: 0,
                peak_frontier: 0,
                domain_pruned_states: 0,
                hall_pruned_states: 0,
                column_pruned_states: 0,
                component_compositions: 0,
                truncated_reason: None,
                backend_execution: WasmDistributedBackendExecution::Cpu,
            },
        ))
    }

    pub fn into_merger(self) -> Result<WasmTilingRootResultMerger, &'static str> {
        if !self.finished {
            return Err("wasm_tiling_root_producer_not_finished");
        }
        let mut mergers = Vec::new();
        mergers
            .try_reserve_exact(self.passes.len())
            .map_err(|_| "wasm_tiling_root_merger_storage_unavailable")?;
        self.validate_shared_external_retained_bytes(mergers.capacity())?;
        let Self {
            passes,
            build_probability,
            ..
        } = self;
        for pass in passes {
            mergers.push(pass.merger);
        }
        Ok(WasmTilingRootResultMerger {
            passes: mergers,
            build_probability,
        })
    }

    pub fn absorb(&mut self, chunk: &WasmTilingRootChunk) -> Result<(), &'static str> {
        self.passes
            .get_mut(usize::from(chunk.pass_index()))
            .ok_or("wasm_tiling_root_pass_invalid")?
            .merger
            .absorb_tiling_chunk(chunk)
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            candidates: self
                .passes
                .iter()
                .map(|pass| pass.merger.tiling_candidate_count())
                .fold(0_usize, usize::saturating_add),
            candidate_family_count: Some(self.root_count as u128),
            coverage_checks: self
                .passes
                .iter()
                .map(|pass| pass.next_root)
                .fold(0_usize, usize::saturating_add),
            pass_count: self.passes.len(),
            ..WasmDistributedProgress::default()
        }
    }

    pub const fn root_count(&self) -> usize {
        self.root_count
    }
}

impl WasmTilingRootResultMerger {
    pub fn absorb(&mut self, chunk: &WasmTilingRootChunk) -> Result<(), &'static str> {
        self.passes
            .get_mut(usize::from(chunk.pass_index()))
            .ok_or("wasm_tiling_root_pass_invalid")?
            .absorb_tiling_chunk(chunk)
    }

    pub fn progress(&self) -> Option<WasmDistributedProgress> {
        let mut progress = WasmDistributedProgress {
            pass_count: self.passes.len(),
            ..WasmDistributedProgress::default()
        };
        let mut any = false;
        for pass in &self.passes {
            if let Some(pass_progress) = pass.tiling_progress() {
                progress.merge(pass_progress);
                any = true;
            }
        }
        any.then_some(progress)
    }

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<crate::CoreExecutionResult, &'static str> {
        let mut results = Vec::new();
        results
            .try_reserve_exact(self.passes.len())
            .map_err(|_| "wasm_tiling_pass_result_storage_unavailable")?;
        for pass in &mut self.passes {
            results.push(pass.finish(summary, workers_used)?);
        }
        let Some(build_probability) = &self.build_probability else {
            return match results.len() {
                1 => Ok(results.pop().expect("one tiling pass result")),
                _ => Err("wasm_tiling_result_pass_mismatch"),
            };
        };
        if !build_probability.mirror_distinct {
            return match results.len() {
                1 => Ok(results.pop().expect("one build tiling pass result")),
                _ => Err("wasm_tiling_result_pass_mismatch"),
            };
        }
        for result in &mut results {
            let owned = core::mem::replace(
                result,
                crate::CoreExecutionResult::new(Vec::new(), Vec::new()),
            );
            *result = owned.with_replaced_fields(vec![(
                "build_probability_aggregation".to_owned(),
                "tiling".to_owned(),
            )]);
        }
        merge_symmetry_results(
            results,
            build_probability.mirror_included,
            build_probability.mirror_distinct,
            &build_probability.pattern_weights,
            false,
        )
        .map_err(super::distributed::map_error)
    }
}

#[derive(Clone, Copy)]
struct TilingRootTask {
    pass_index: u8,
    ordinal: u32,
    family_index: u32,
}

struct ActiveTilingRoot {
    task: TilingRootTask,
    search: GeometrySearch,
    identities: Vec<WasmPackedTilingIdentity>,
}

struct CompletedTilingRoot {
    task: TilingRootTask,
    identities: Vec<WasmPackedTilingIdentity>,
    next_offset: usize,
    chunk_sequence: u32,
    candidate_family_count: Option<u128>,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_compositions: usize,
}

pub struct WasmTilingRootWorker {
    problem: SearchProblem,
    passes: Vec<TilingRootWorkerPass>,
    pending_roots: VecDeque<TilingRootTask>,
    active_root: Option<ActiveTilingRoot>,
    completed_root: Option<CompletedTilingRoot>,
    candidate_count: usize,
    completed_roots: usize,
    expanded_nodes: usize,
    peak_frontier: usize,
}

struct TilingRootWorkerPass {
    catalog: GeometryCatalog,
    canonical_rank_by_source: Vec<u32>,
    family: PackingMultisetFamily,
}

impl WasmTilingRootWorker {
    pub fn new(problem: &SearchProblem) -> Result<Self, &'static str> {
        if problem.objective().kind()
            != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
        {
            return Err("wasm_tiling_root_worker_requires_tiling_objective");
        }
        super::ensure_connected_kick_profile(problem).map_err(super::distributed::map_error)?;
        let catalog = GeometryCatalog::compile(problem).map_err(super::distributed::map_error)?;
        Self::with_catalogs(problem, vec![catalog])
    }

    pub fn new_for_build_probability(
        problem: &SearchProblem,
        field: BuildProbabilityField,
    ) -> Result<Self, &'static str> {
        if problem.objective().kind()
            != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
        {
            return Err("wasm_tiling_root_worker_requires_tiling_objective");
        }
        super::ensure_connected_kick_profile(problem).map_err(super::distributed::map_error)?;
        let mirror_included = field.includes_applicable_horizontal_mirror();
        let original = field.original_only();
        let mut fields = vec![original];
        if let Some(mirrored) = mirror_included
            .then(|| original.mirrored_horizontally())
            .filter(|candidate| *candidate != original)
        {
            fields.push(mirrored);
        }
        let mut catalogs = Vec::new();
        catalogs
            .try_reserve_exact(fields.len())
            .map_err(|_| "wasm_tiling_root_pass_storage_unavailable")?;
        for field in fields {
            let initial_board = field
                .compact_base_mask()
                .ok_or("wasm_tiling_root_compact_base_missing")?;
            let required_cells = field
                .compact_target_mask()
                .ok_or("wasm_tiling_root_compact_target_missing")?;
            catalogs.push(
                GeometryCatalog::compile_for_required_cells_on_board(
                    problem,
                    initial_board,
                    required_cells,
                )
                .map_err(super::distributed::map_error)?,
            );
        }
        Self::with_catalogs(problem, catalogs)
    }

    fn with_catalogs(
        problem: &SearchProblem,
        catalogs: Vec<GeometryCatalog>,
    ) -> Result<Self, &'static str> {
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or("wasm_piece_source_not_materialized")?;
        let mut passes = Vec::new();
        passes
            .try_reserve_exact(catalogs.len())
            .map_err(|_| "wasm_tiling_root_pass_storage_unavailable")?;
        for catalog in catalogs {
            let canonical_rank_by_source =
                canonical_tiling_rank_by_source(&catalog).map_err(super::distributed::map_error)?;
            let target_piece_count = catalog.required_cells().count_ones() as usize / 4;
            let family = universe.packing_multiset_family_for_execution(
                target_piece_count,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                super::packing_hold_projection(problem),
            );
            if family.is_empty() {
                return Err("wasm_supply_has_no_reachable_piece_multiset");
            }
            passes.push(TilingRootWorkerPass {
                catalog,
                canonical_rank_by_source,
                family,
            });
        }
        Ok(Self {
            problem: problem.clone(),
            passes,
            pending_roots: VecDeque::new(),
            active_root: None,
            completed_root: None,
            candidate_count: 0,
            completed_roots: 0,
            expanded_nodes: 0,
            peak_frontier: 0,
        })
    }

    pub fn enqueue(&mut self, roots: &[(u8, u32, u32)]) -> Result<(), &'static str> {
        self.pending_roots
            .try_reserve(roots.len())
            .map_err(|_| "wasm_tiling_root_queue_storage_unavailable")?;
        for (pass_index, ordinal, family_index) in roots.iter().copied() {
            let pass = self
                .passes
                .get(usize::from(pass_index))
                .ok_or("wasm_tiling_root_pass_invalid")?;
            if family_index as usize >= pass.family.len() {
                return Err("wasm_tiling_root_index_invalid");
            }
            self.pending_roots.push_back(TilingRootTask {
                pass_index,
                ordinal,
                family_index,
            });
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        identity_capacity: usize,
        control: &ExecutionControl,
    ) -> Result<WasmTilingRootAdvance, &'static str> {
        if control.is_cancelled() {
            return Ok(WasmTilingRootAdvance::Cancelled);
        }
        if self.completed_root.is_some() {
            return self.emit_completed_root(identity_capacity);
        }

        for work in 0..ROOT_ADVANCE_WORK_BUDGET {
            if work & 1023 == 0 && control.is_cancelled() {
                return Ok(WasmTilingRootAdvance::Cancelled);
            }
            if self.active_root.is_none() {
                let Some(task) = self.pending_roots.pop_front() else {
                    return Ok(WasmTilingRootAdvance::Completed(
                        WasmTilingRootChunk::default(),
                    ));
                };
                let pass = self
                    .passes
                    .get(usize::from(task.pass_index))
                    .ok_or("wasm_tiling_root_pass_invalid")?;
                let family = pass
                    .family
                    .single_group(task.family_index as usize)
                    .ok_or("wasm_tiling_root_index_invalid")?;
                let universe = self
                    .problem
                    .piece_source()
                    .materialized_universe()
                    .ok_or("wasm_piece_source_not_materialized")?;
                let search =
                    GeometrySearch::new(universe, &family, pass.catalog.required_cells(), false)
                        .map_err(super::distributed::map_error)?;
                self.active_root = Some(ActiveTilingRoot {
                    task,
                    search,
                    identities: Vec::new(),
                });
            }

            let active = self
                .active_root
                .as_mut()
                .ok_or("wasm_tiling_root_search_missing")?;
            let pass = self
                .passes
                .get(usize::from(active.task.pass_index))
                .ok_or("wasm_tiling_root_pass_invalid")?;
            match active.search.advance(&pass.catalog) {
                GeometryAdvance::Pending => {}
                GeometryAdvance::Candidate(candidate) => {
                    let packed_rows = pack_canonical_tiling_row_ids(
                        candidate.row_ids(),
                        &pass.canonical_rank_by_source,
                    )
                    .ok_or("wasm_tiling_root_identity_invalid")?;
                    if active.identities.len() == active.identities.capacity() {
                        active
                            .identities
                            .try_reserve(identity_capacity.max(1))
                            .map_err(|_| "wasm_tiling_root_identity_storage_unavailable")?;
                    }
                    active.identities.push(WasmPackedTilingIdentity::new(
                        candidate.identity.bucket_hash(),
                        packed_rows,
                    ));
                    self.candidate_count = self.candidate_count.saturating_add(1);
                }
                GeometryAdvance::Complete => {
                    let mut active = self
                        .active_root
                        .take()
                        .ok_or("wasm_tiling_root_search_missing")?;
                    active
                        .identities
                        .sort_unstable_by_key(|identity| identity.packed_rows());
                    active
                        .identities
                        .dedup_by_key(|identity| identity.packed_rows());
                    let completed = active.search;
                    self.completed_roots = self.completed_roots.saturating_add(1);
                    self.expanded_nodes = self
                        .expanded_nodes
                        .saturating_add(completed.expanded_nodes());
                    self.peak_frontier = self.peak_frontier.max(completed.peak_frontier());
                    self.completed_root = Some(CompletedTilingRoot {
                        task: active.task,
                        identities: active.identities,
                        next_offset: 0,
                        chunk_sequence: 0,
                        candidate_family_count: completed.candidate_family_count(),
                        expanded_nodes: completed.expanded_nodes(),
                        peak_frontier: completed.peak_frontier(),
                        domain_pruned_states: completed.domain_pruned_states(),
                        hall_pruned_states: completed.hall_pruned_states(),
                        column_pruned_states: completed.column_pruned_states(),
                        component_compositions: completed.component_compositions(),
                    });
                    return self.emit_completed_root(identity_capacity);
                }
                GeometryAdvance::ResourceIncomplete(reason) => return Err(reason),
            }
        }
        Ok(WasmTilingRootAdvance::Pending(
            WasmTilingRootChunk::default(),
        ))
    }

    fn emit_completed_root(
        &mut self,
        identity_capacity: usize,
    ) -> Result<WasmTilingRootAdvance, &'static str> {
        let completed = self
            .completed_root
            .as_mut()
            .ok_or("wasm_tiling_completed_root_missing")?;
        let begin = completed.next_offset;
        let end = begin
            .saturating_add(identity_capacity.max(1))
            .min(completed.identities.len());
        let root_complete = end == completed.identities.len();
        let mut identities = Vec::new();
        identities
            .try_reserve_exact(end - begin)
            .map_err(|_| "wasm_tiling_root_chunk_storage_unavailable")?;
        identities.extend_from_slice(&completed.identities[begin..end]);
        let chunk = WasmTilingRootChunk {
            pass_index: completed.task.pass_index,
            root_ordinal: completed.task.ordinal,
            chunk_sequence: completed.chunk_sequence,
            root_complete,
            identities,
            completed_roots: usize::from(root_complete),
            candidate_family_count: if root_complete {
                completed.candidate_family_count
            } else {
                Some(0)
            },
            expanded_nodes: usize::from(root_complete) * completed.expanded_nodes,
            peak_frontier: usize::from(root_complete) * completed.peak_frontier,
            domain_pruned_states: usize::from(root_complete) * completed.domain_pruned_states,
            hall_pruned_states: usize::from(root_complete) * completed.hall_pruned_states,
            column_pruned_states: usize::from(root_complete) * completed.column_pruned_states,
            component_compositions: usize::from(root_complete) * completed.component_compositions,
        };
        completed.next_offset = end;
        completed.chunk_sequence = completed
            .chunk_sequence
            .checked_add(1)
            .ok_or("wasm_tiling_root_chunk_sequence_overflow")?;
        if root_complete {
            self.completed_root = None;
        }
        if self.has_pending_work() {
            Ok(WasmTilingRootAdvance::Pending(chunk))
        } else {
            Ok(WasmTilingRootAdvance::Completed(chunk))
        }
    }

    pub fn has_pending_work(&self) -> bool {
        self.active_root.is_some()
            || self.completed_root.is_some()
            || !self.pending_roots.is_empty()
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.expanded_nodes.saturating_add(
                self.active_root
                    .as_ref()
                    .map_or(0, |active| active.search.expanded_nodes()),
            ),
            candidates: self.candidate_count,
            candidate_family_count: self.passes.iter().try_fold(0_u128, |total, pass| {
                total.checked_add(pass.family.len() as u128)
            }),
            coverage_checks: self.completed_roots,
            pass_count: self.passes.len(),
            ..WasmDistributedProgress::default()
        }
    }
}
