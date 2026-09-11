//! Experimental Oracle/compact physical probes for a lazy coverage provider.
//! A result proves only one (full logical identity, full PatternId) proposition.
//! It never claims a complete candidate source, full row, or legacy v2 authority.

use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    universe::{pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId},
};
use clearra_problem::SearchProblem;
use clearra_supply::pattern_universe::{PatternPiecePositionIndex, PieceMultisetKey};

use crate::{
    resource::{admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission},
    WasmCpuSearchError, WasmCpuTerminalResourceAuthority,
};
use super::{
    buildup::{checked_candidate_verification_peak_upper_bound, verify_candidate, BuildUpWorkspace, CandidateWitnessMode},
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    distributed::{map_typed_error, WasmCandidatePacket},
    exact_collections::ExactHashSet,
    geometry::{GeometryCandidate, TargetGroup},
    piece_index, WasmExactSearchError,
};

struct SourceOwner {
    problem: Arc<SearchProblem>,
    catalog: Arc<GeometryCatalog>,
    targets: Option<Arc<[TargetGroup]>>,
    external_bytes: u128,
    admission: ExecutionAdmission,
}

/// Conservative shared-owner bound. Repeated Arcs may be counted more than
/// once; callers never supply zero for these real retained allocations.
pub(super) fn checked_shared_target_owner_bytes(targets: Option<&[TargetGroup]>) -> Option<u128> {
    let Some(targets) = targets else { return Some(0); };
    let arc_bytes = 2 * core::mem::size_of::<usize>() as u128;
    let mut bytes = (core::mem::size_of_val(targets) as u128).checked_add(arc_bytes)?;
    for target in targets {
        bytes = bytes.checked_add(core::mem::size_of::<PatternBitSet>() as u128)?
            .checked_add(target.possible_patterns.checked_storage_retained_bytes()?)?
            .checked_add(arc_bytes)?;
        if let Some(index) = &target.pattern_index {
            bytes = bytes.checked_add(core::mem::size_of::<PatternPiecePositionIndex>() as u128)?
                .checked_add(index.retained_bytes() as u128)?.checked_add(arc_bytes)?;
        }
    }
    Some(bytes)
}

pub(super) fn validate_problem(problem: &SearchProblem) -> Result<(), WasmExactSearchError> {
    if problem.queue_observation_policy().requires_observation_policy()
        || problem.objective().score().requested()
        || problem.objective().execution_constraints().requested()
        || problem.count_policy() != clearra_pc_graph::request::PcCountPolicy::CountUnique
        || problem.build_query().is_some()
    { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_adapter_not_connected")); }
    if problem.piece_source().materialized_universe().is_none() {
        return Err(WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"));
    }
    Ok(())
}

/// Remaining controller authority in addition to the request parent lease.
/// Zero is exhausted, never an unlimited-budget sentinel. Peak bytes include
/// this provider's resident allocations plus the conservative probe future.
#[derive(Clone, Copy, Debug)]
pub struct WasmRequiredQueueProbeLimits {
    pub remaining_work_steps: u64,
    pub remaining_provider_peak_bytes: u128,
}

/// The controller must use this observation, including on errors, to charge
/// its aggregate ledger. An error is Unknown and never negative evidence.
pub struct WasmRequiredQueueProbeObservation {
    pub result: Result<WasmRequiredPatternEvidence, WasmCpuSearchError>,
    pub work_steps: u64,
    pub peak_retained_bytes: u128,
}

#[derive(Default)]
struct ProbeUsage {
    work_steps: u64,
    peak_retained_bytes: u128,
}

/// Conservative structural work, not a claimed CPU instruction count.
/// Dependency closure grows a placed set at most n times. Projection storage
/// visits O(2^n) slots. Feasibility expands each subset at most once per pass;
/// a memoized failed child may be revisited only by a parent realization.
/// Three full DAG passes, storage/generation clears, a defensive DFS fallback,
/// every candidate realization and its finite row scans are included below.
/// The bound deliberately remains valid for Legacy, Off, RelaxationOnly and
/// dependency-cache hits. Actual feasibility/witness counters stay separate.
fn checked_pre_witness_work_upper_bound(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    sequence_len: usize,
) -> Option<u64> {
    let n = u64::try_from(candidate.row_ids().len()).ok()?;
    let height = u64::from(catalog.height());
    let states = 1_u64.checked_shl(u32::try_from(n).ok()?)?;
    let realizations = candidate.row_ids().iter().try_fold(0_u64, |sum, row| {
        sum.checked_add(u64::from(catalog.skeleton(*row).realization_count))
    })?;
    let relaxation = n.checked_mul(height)?.checked_add(
        n.checked_add(1)?.checked_mul(height.checked_add(n)?.checked_add(realizations)?)?,
    )?;
    let projection = n.checked_add(2)?.checked_mul(height)?.checked_add(n)?
        .checked_add(states.checked_mul(4)?)?;
    let per_state = 32_u64.checked_add(n.checked_mul(6)?)?.checked_add(
        realizations.checked_mul(height.checked_mul(16)?.checked_add(32)?)?,
    )?;
    let queue_index = u64::try_from(sequence_len).ok()?.checked_mul(8)?.checked_add(16)?;
    relaxation.checked_add(projection)?.checked_add(states.checked_mul(per_state)?)?
        .checked_add(queue_index)
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct FailureKey {
    identity: StandardBoard64TilingIdentity,
    pattern: PatternId,
}

/// The immutable source owner contains every semantic policy (rules, kicks,
/// spawn, supply, initial hold, queue observation and completion context).
/// Cache entries never leave this owner or use a hash/ordinal as equality.
pub struct WasmRequiredQueueVerifier {
    source: Arc<SourceOwner>,
    workspace: BuildUpWorkspace,
    evaluator: CoverageProductEvaluator,
    failed: ExactHashSet<FailureKey>,
    max_cached_failures: usize,
    identity_pattern_index: Option<(Arc<PatternBitSet>, Arc<PatternPiecePositionIndex>)>,
    poisoned: bool,
}

/// Runtime proof only. original_row is the caller dictionary's label; the
/// expected full canonical identity is checked before that label is returned.
/// Parent authority remains alive through SourceOwner while evidence is held.
pub struct WasmRequiredPatternEvidence {
    source: Arc<SourceOwner>,
    original_row: usize,
    identity: StandardBoard64TilingIdentity,
    pattern: PatternId,
    supported: bool,
    precharged_work_steps: u64,
    witness_nodes: usize,
    feasibility_states: usize,
    provider_peak_upper_bound_bytes: u128,
}

impl WasmRequiredPatternEvidence {
    pub fn original_row(&self) -> usize { self.original_row }
    pub fn identity(&self) -> StandardBoard64TilingIdentity { self.identity }
    pub fn pattern(&self) -> PatternId { self.pattern }
    pub fn supported(&self) -> bool { self.supported }
    pub fn precharged_work_steps(&self) -> u64 { self.precharged_work_steps }
    pub fn witness_nodes(&self) -> usize { self.witness_nodes }
    pub fn feasibility_states(&self) -> usize { self.feasibility_states }
    pub fn work_steps(&self) -> u64 { self.precharged_work_steps + self.witness_nodes as u64 }
    pub fn provider_peak_upper_bound_bytes(&self) -> u128 { self.provider_peak_upper_bound_bytes }
    pub fn universe_id(&self) -> PatternUniverseId {
        self.source.problem.piece_source().materialized_universe().unwrap().pattern_universe_id()
    }
    pub fn weight_model_id(&self) -> PatternWeightModelId {
        self.source.problem.piece_source().materialized_universe().unwrap().pattern_weight_model_id()
    }
}

/// Before canonical dense IDs exist, this proves only whether this exact
/// geometry identity has any physical queue witness in the bound source.
/// An admitted identity still needs complete source enumeration, deduplication
/// and the existing normalized-key sort before it receives a public dense ID.
pub struct WasmCandidateIdentityEvidence {
    source: Arc<SourceOwner>,
    identity: StandardBoard64TilingIdentity,
    witness_pattern: Option<PatternId>,
    precharged_work_steps: u64,
    witness_nodes: usize,
    feasibility_states: usize,
}

impl WasmCandidateIdentityEvidence {
    pub fn identity(&self) -> StandardBoard64TilingIdentity { self.identity }
    pub fn admitted(&self) -> bool { self.witness_pattern.is_some() }
    pub fn witness_pattern(&self) -> Option<PatternId> { self.witness_pattern }
    pub fn precharged_work_steps(&self) -> u64 { self.precharged_work_steps }
    pub fn witness_nodes(&self) -> usize { self.witness_nodes }
    pub fn feasibility_states(&self) -> usize { self.feasibility_states }
}

pub struct WasmCandidateIdentityProbeObservation {
    pub result: Result<WasmCandidateIdentityEvidence, WasmCpuSearchError>,
    pub work_steps: u64,
    pub peak_retained_bytes: u128,
}

impl WasmRequiredQueueVerifier {
    pub(super) fn checked_implicit_source_retained_bytes(&self) -> Option<u128> {
        self.source.problem.checked_build_probability_pointee_retained_bytes()?
            .checked_add(self.source.catalog.retained_bytes() as u128)?
            .checked_add(core::mem::size_of::<GeometryCatalog>() as u128)?
            .checked_add(4 * core::mem::size_of::<usize>() as u128)?
            .checked_add(checked_shared_target_owner_bytes(self.source.targets.as_deref())?)
    }

    pub(super) fn checked_implicit_runtime_retained_bytes(&self) -> Option<u128> {
        self.provider_retained_upper_bound().ok()
    }

    pub(super) fn checked_source_candidate_identity(
        &self, packet: &WasmCandidatePacket,
    ) -> Result<StandardBoard64TilingIdentity, WasmCpuSearchError> {
        if packet.pass_index() != 0 || packet.is_extended() || packet.row_ids().is_empty() || packet.row_ids().len() > 15 {
            return Err(WasmCpuSearchError::InvalidProblem { reason: "wasm_required_queue_candidate_adapter_not_connected" });
        }
        let candidate = GeometryCandidate::from_rows(&self.source.catalog, packet.target_index(), packet.row_ids())
            .ok_or(WasmCpuSearchError::InvalidProblem { reason: "wasm_required_queue_candidate_invalid" })?;
        Ok(candidate.identity)
    }

    pub fn owns_identity_evidence(&self, evidence: &WasmCandidateIdentityEvidence) -> bool {
        Arc::ptr_eq(&self.source, &evidence.source)
    }

    /// external_bytes must cover every producer/verifier/problem/catalog owner
    /// retained by the parent, including shared allocations counted there once.
    pub(super) fn from_shared_inputs(
        problem: Arc<SearchProblem>,
        catalog: Arc<GeometryCatalog>,
        external_bytes: u128,
        max_cached_failures: usize,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        Self::from_shared_inputs_with_targets(problem, catalog, None, external_bytes, max_cached_failures, authority)
    }

    pub(super) fn from_shared_inputs_with_targets(
        problem: Arc<SearchProblem>, catalog: Arc<GeometryCatalog>,
        targets: Option<Arc<[TargetGroup]>>, external_bytes: u128,
        max_cached_failures: usize, authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        Self::from_shared_inputs_exact(problem, catalog, targets, external_bytes, max_cached_failures, authority)
            .map_err(map_typed_error)
    }

    fn from_shared_inputs_exact(
        problem: Arc<SearchProblem>, catalog: Arc<GeometryCatalog>,
        targets: Option<Arc<[TargetGroup]>>, external_bytes: u128,
        max_cached_failures: usize, authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        // Phase-one semantics are explicit. These errors are Unknown to the
        // implicit source controller, never negative candidate evidence.
        validate_problem(problem.as_ref())?;
        let minimum_external = problem.checked_build_probability_pointee_retained_bytes()
            .and_then(|bytes| bytes.checked_add(catalog.retained_bytes() as u128))
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<GeometryCatalog>() as u128 + 4 * core::mem::size_of::<usize>() as u128))
            .and_then(|bytes| bytes.checked_add(checked_shared_target_owner_bytes(targets.as_deref())?))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_source_accounting_unavailable"))?;
        if external_bytes < minimum_external {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_source_accounting_understated"));
        }
        let admission = admit_budget_bound_search_execution_under_terminal_authority(
            problem.as_ref(), external_bytes, authority, 1,
        ).map_err(WasmExactSearchError::resource_admission)?;
        admission.ensure_memory_bound(external_bytes,
            core::mem::size_of::<Self>() as u128 + core::mem::size_of::<SourceOwner>() as u128
                + 2 * core::mem::size_of::<usize>() as u128,
        ).map_err(WasmExactSearchError::resource_admission)?;
        let source = Arc::new(SourceOwner { problem, catalog, targets, external_bytes, admission });
        let result = Self { source, workspace: BuildUpWorkspace::default(), evaluator: CoverageProductEvaluator::default(), failed: ExactHashSet::default(), max_cached_failures, identity_pattern_index: None, poisoned: false };
        result.ensure_memory(0)?;
        Ok(result)
    }

    /// Experimental provider-local A/B. Default is the existing fresh analysis.
    /// Each change starts a new reuse epoch; queue and supply searches remain
    /// independent. Call once after constructing this immutable source owner.
    pub fn set_shared_candidate_feasibility(&mut self, enabled: bool) {
        self.workspace.configure_required_feasibility_reuse(
            enabled.then(|| Arc::clone(&self.source.catalog)),
        );
    }

    /// (reused exact candidate proofs, fresh analyses while reuse is enabled).
    pub fn shared_candidate_feasibility_counters(&self) -> (u64, u64) {
        self.workspace.required_feasibility_reuse_counters()
    }

    /// Private provider-local A/B; ordinary product workspaces remain unchanged.
    /// transition_policy: 0 = OFF, 1 = fresh per probe, 2 = shared exact candidate.
    /// Configure once before probing; reconfiguration drops retained memo state.
    pub fn set_candidate_physical_reuse(
        &mut self, reuse_projection: bool, transition_policy: u8,
    ) -> Result<(), WasmCpuSearchError> {
        self.workspace.configure_required_physical_reuse(
            Arc::clone(&self.source.problem), Arc::clone(&self.source.catalog),
            reuse_projection, transition_policy,
        ).map_err(map_typed_error)
    }

    /// Projection reused/fresh, then transition table reused/fresh/OFF.
    /// These are table/generation counts, not individual transition entry hits.
    pub fn candidate_physical_reuse_counters(&self) -> [u64; 5] {
        self.workspace.required_physical_reuse_counters()
    }

    pub fn owns_evidence(&self, evidence: &WasmRequiredPatternEvidence) -> bool {
        Arc::ptr_eq(&self.source, &evidence.source)
    }

    fn ensure_memory(&self, future: u128) -> Result<(), WasmExactSearchError> {
        let retained = self.source.external_bytes.checked_add(self.provider_retained_upper_bound()?)
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))?;
        self.source.admission.ensure_memory_bound(retained, future)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn provider_retained_upper_bound(&self) -> Result<u128, WasmExactSearchError> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(0)
            .and_then(|n| n.checked_add(core::mem::size_of::<SourceOwner>() as u128 + 2 * core::mem::size_of::<usize>() as u128))
            .and_then(|n| n.checked_add(self.workspace.retained_bytes() as u128))
            .and_then(|n| n.checked_add(self.evaluator.retained_bytes() as u128))
            .and_then(|n| n.checked_add(Self::cache_bytes(self.failed.capacity())?))
            .and_then(|n| n.checked_add(self.identity_pattern_index.as_ref().map_or(0, |(_, index)|
                index.retained_bytes() as u128 + core::mem::size_of::<PatternPiecePositionIndex>() as u128
                    + 2 * core::mem::size_of::<usize>() as u128)))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))
    }

    fn ensure_probe_memory(&self, future: u128, limits: WasmRequiredQueueProbeLimits) -> Result<u128, WasmExactSearchError> {
        self.ensure_memory(future)?;
        let peak = self.provider_retained_upper_bound()?.checked_add(future)
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))?;
        if peak > limits.remaining_provider_peak_bytes {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_controller_peak_exhausted"));
        }
        Ok(peak)
    }


    /// Any-queue membership for the identity-catalog admission pass. This does
    /// not assign a row ordinal, a source-complete flag, or a coverage union.
    #[allow(clippy::too_many_arguments)]
    pub fn probe_candidate_identity_observed(
        &mut self, packet: &WasmCandidatePacket,
        expected_identity: StandardBoard64TilingIdentity,
        universe_id: PatternUniverseId, weight_model_id: PatternWeightModelId,
        limits: WasmRequiredQueueProbeLimits, control: &ExecutionControl,
    ) -> WasmCandidateIdentityProbeObservation {
        let mut usage = ProbeUsage::default();
        let result = self.probe_candidate_identity_exact(packet, expected_identity,
            universe_id, weight_model_id, limits, control, &mut usage).map_err(map_typed_error);
        WasmCandidateIdentityProbeObservation {
            result, work_steps: usage.work_steps, peak_retained_bytes: usage.peak_retained_bytes,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn probe_candidate_identity_exact(
        &mut self, packet: &WasmCandidatePacket,
        expected_identity: StandardBoard64TilingIdentity,
        universe_id: PatternUniverseId, weight_model_id: PatternWeightModelId,
        limits: WasmRequiredQueueProbeLimits, control: &ExecutionControl,
        usage: &mut ProbeUsage,
    ) -> Result<WasmCandidateIdentityEvidence, WasmExactSearchError> {
        if control.is_cancelled() { return Err(WasmExactSearchError::Cancelled); }
        if self.poisoned { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_provider_poisoned")); }
        usage.peak_retained_bytes = self.provider_retained_upper_bound()?;
        self.ensure_probe_memory(0, limits)?;
        let source = Arc::clone(&self.source);
        let problem = source.problem.as_ref();
        let universe = problem.piece_source().materialized_universe().unwrap();
        if universe.pattern_universe_id() != universe_id || universe.pattern_weight_model_id() != weight_model_id {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_universe_binding_mismatch"));
        }
        if packet.pass_index() != 0 || packet.is_extended() || packet.row_ids().is_empty() || packet.row_ids().len() > 15 {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_adapter_not_connected"));
        }
        let original_target = source.targets.as_ref().and_then(|targets| targets.get(packet.target_index() as usize))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_identity_original_target_unavailable"))?;
        let candidate = GeometryCandidate::from_rows(&source.catalog, packet.target_index(), packet.row_ids())
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_invalid"))?;
        let mut occupied = 0_u64;
        let mut counts = [0_u8; 7];
        for row in candidate.row_ids() {
            let placement = source.catalog.skeleton(*row);
            occupied |= placement.cells;
            counts[piece_index(placement.piece)] += 1;
        }
        if candidate.identity != expected_identity || occupied != source.catalog.required_cells()
            || original_target.key != PieceMultisetKey::from_counts(counts)
            || original_target.possible_patterns.pattern_count() != universe.pattern_count()
            || !problem.allows_solution_identity(&candidate.identity)
        { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_identity_mismatch")); }

        let parent_limit = u64::try_from(problem.budget().max_nodes()).unwrap_or(u64::MAX);
        let total_limit = if parent_limit == 0 { limits.remaining_work_steps } else { parent_limit.min(limits.remaining_work_steps) };
        // Admit the worst metadata/bitset scan before count_ones or any queue
        // iteration. Four full word scans cover count + iterator in both this
        // preflight and the borrowed compiler, including a sparse selection.
        let scan_charge = (universe.pattern_count().div_ceil(64) as u64).checked_mul(4)
            .and_then(|n| n.checked_add(universe.pattern_count() as u64))
            .and_then(|n| n.checked_add(16))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?;
        if scan_charge >= total_limit {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_identity_scan_budget_exceeded"));
        }
        usage.work_steps = scan_charge;
        let pattern_count = u64::try_from(original_target.possible_patterns.count_ones())
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?;
        let cached_index = original_target.pattern_index.as_ref().map(Arc::clone).or_else(||
            self.identity_pattern_index.as_ref().filter(|(mask, _)| Arc::ptr_eq(mask, &original_target.possible_patterns))
                .map(|(_, index)| Arc::clone(index)));
        let mut sequence_len = cached_index.as_ref().map_or(0, |index| index.sequence_len());
        if cached_index.is_none() {
            for pattern in original_target.possible_patterns.covered_patterns_before(universe.pattern_count()) {
                if control.is_cancelled() { return Err(WasmExactSearchError::Cancelled); }
                sequence_len = sequence_len.max(universe.sequence_len_at(pattern.index()));
            }
        }
        let words = pattern_count.div_ceil(64);
        let index_slots = (sequence_len as u64).checked_mul(7).and_then(|n| n.checked_mul(words))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?;
        let index_charge = if cached_index.is_some() { 0 } else {
            pattern_count.checked_mul((sequence_len as u64).checked_add(3)
                .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?)
                .and_then(|n| n.checked_add(index_slots))
                .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?
        };
        let precharged_work_steps = checked_pre_witness_work_upper_bound(&source.catalog, &candidate, sequence_len)
            .and_then(|n| n.checked_add(scan_charge))
            .and_then(|n| n.checked_add(index_charge))
            .and_then(|n| n.checked_add(words.checked_mul(16)?))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?;
        let witness_limit = total_limit.checked_sub(precharged_work_steps).filter(|remaining| *remaining != 0)
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_pre_witness_budget_exceeded"))?;
        let witness_limit = usize::try_from(witness_limit).unwrap_or(usize::MAX);
        let index_future = if cached_index.is_some() { 0 } else {
            (index_slots as u128).checked_mul(8)
                .and_then(|n| n.checked_add((pattern_count as u128).checked_mul(4)?))
                .and_then(|n| n.checked_add((sequence_len as u128).checked_mul(64)?))
                .and_then(|n| n.checked_add(core::mem::size_of::<PatternPiecePositionIndex>() as u128 + 128))
                .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))?
        };
        let future = checked_candidate_verification_peak_upper_bound(problem, &source.catalog, &candidate, false)
            .and_then(|n| n.checked_add(index_future))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))?;
        usage.peak_retained_bytes = self.ensure_probe_memory(future, limits)?;
        usage.work_steps = precharged_work_steps;
        let index = match cached_index {
            Some(index) => index,
            None => {
                let index = PatternPiecePositionIndex::compile_subset_borrowed_with_control(
                    universe, &original_target.possible_patterns, sequence_len, || control.is_cancelled(),
                ).map_err(|error| if error == clearra_supply::pattern_universe::PatternPiecePositionIndexError::Cancelled {
                    WasmExactSearchError::Cancelled
                } else { WasmExactSearchError::InvalidProblem("wasm_required_identity_pattern_index_unavailable") })?;
                let index = Arc::new(index);
                self.identity_pattern_index = Some((Arc::clone(&original_target.possible_patterns), Arc::clone(&index)));
                index
            }
        };
        if index.global_pattern_count() != universe.pattern_count() || index.local_pattern_count() as u64 != pattern_count {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_identity_pattern_index_binding_mismatch"));
        }
        let target = TargetGroup { key: original_target.key, pattern_index_id: original_target.pattern_index_id,
            possible_patterns: Arc::clone(&original_target.possible_patterns), pattern_index: Some(index) };
        let previous_budget = self.workspace.replace_required_probe_node_budget(Some(witness_limit));
        self.poisoned = true;
        let checked = verify_candidate(problem, &source.catalog, &candidate, &target,
            &mut self.workspace, &mut self.evaluator, CandidateWitnessMode::MembershipOnly, false, 0, control);
        let remaining = self.workspace.replace_required_probe_node_budget(previous_budget).unwrap_or(witness_limit);
        self.poisoned = false;
        let witness_nodes = witness_limit - remaining;
        usage.work_steps += witness_nodes as u64;
        let checked = checked?;
        if control.is_cancelled() { return Err(WasmExactSearchError::Cancelled); }
        usage.peak_retained_bytes = usage.peak_retained_bytes.max(self.ensure_probe_memory(checked.retained_bytes as u128, limits)?);
        let witness_pattern = checked.witness_pattern_id.map(|id| PatternId::new(id as usize));
        if checked.buildable != witness_pattern.is_some() || witness_pattern.is_some_and(|pattern|
            !target.possible_patterns.contains(pattern) || target.pattern_index.as_ref().unwrap().local_pattern_index(pattern.index()).is_none())
        { return Err(WasmExactSearchError::InvalidProblem("wasm_required_identity_witness_binding_mismatch")); }
        // No full-row coverage, reachable union, dense ordinal or completeness
        // claim is created. Only this identity's any-queue membership is known.
        Ok(WasmCandidateIdentityEvidence { source, identity: candidate.identity, witness_pattern,
            precharged_work_steps, witness_nodes, feasibility_states: checked.feasibility_states })
    }

    fn cache_bytes(capacity: usize) -> Option<u128> {
        if capacity == 0 { return Some(0); }
        (capacity as u128).checked_add(1)?.checked_mul(2)?
            .checked_mul(core::mem::size_of::<FailureKey>() as u128 + 1)?.checked_add(64)
    }

    /// One incremental unit for joint(bounds, required): repeat only within
    /// the same full candidate identity. Each queue may have another history.
    /// Convenience for a caller which stops the whole request on any error.
    /// A continuing controller must use probe_pattern_observed so an error's
    /// work and peak are charged before it retries or issues another query.
    #[allow(clippy::too_many_arguments)]
    pub fn probe_pattern(
        &mut self, original_row: usize, packet: &WasmCandidatePacket,
        expected_identity: StandardBoard64TilingIdentity, pattern: PatternId,
        universe_id: PatternUniverseId, weight_model_id: PatternWeightModelId,
        limits: WasmRequiredQueueProbeLimits,
        control: &ExecutionControl,
    ) -> Result<WasmRequiredPatternEvidence, WasmCpuSearchError> {
        self.probe_pattern_observed(original_row, packet, expected_identity, pattern,
            universe_id, weight_model_id, limits, control).result
    }

    #[allow(clippy::too_many_arguments)]
    pub fn probe_pattern_observed(
        &mut self, original_row: usize, packet: &WasmCandidatePacket,
        expected_identity: StandardBoard64TilingIdentity, pattern: PatternId,
        universe_id: PatternUniverseId, weight_model_id: PatternWeightModelId,
        limits: WasmRequiredQueueProbeLimits,
        control: &ExecutionControl,
    ) -> WasmRequiredQueueProbeObservation {
        let mut usage = ProbeUsage::default();
        let result = self.probe_pattern_exact(original_row, packet, expected_identity,
            pattern, universe_id, weight_model_id, limits, control, &mut usage)
            .map_err(map_typed_error);
        WasmRequiredQueueProbeObservation {
            result, work_steps: usage.work_steps, peak_retained_bytes: usage.peak_retained_bytes,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn probe_pattern_exact(
        &mut self, original_row: usize, packet: &WasmCandidatePacket,
        expected_identity: StandardBoard64TilingIdentity, pattern: PatternId,
        universe_id: PatternUniverseId, weight_model_id: PatternWeightModelId,
        limits: WasmRequiredQueueProbeLimits,
        control: &ExecutionControl,
        usage: &mut ProbeUsage,
    ) -> Result<WasmRequiredPatternEvidence, WasmExactSearchError> {
        if control.is_cancelled() { return Err(WasmExactSearchError::Cancelled); }
        if self.poisoned { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_provider_poisoned")); }
        usage.peak_retained_bytes = self.provider_retained_upper_bound()?;
        let mut provider_peak_upper_bound_bytes = self.ensure_probe_memory(0, limits)?;
        let source = Arc::clone(&self.source);
        let problem = source.problem.as_ref();
        let universe = problem.piece_source().materialized_universe().unwrap();
        if universe.pattern_universe_id() != universe_id || universe.pattern_weight_model_id() != weight_model_id
            || pattern.index() >= universe.pattern_count()
        { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_universe_binding_mismatch")); }
        if packet.pass_index() != 0 || packet.is_extended() || packet.row_ids().is_empty() || packet.row_ids().len() > 15 {
            return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_adapter_not_connected"));
        }
        let candidate = GeometryCandidate::from_rows(&source.catalog, packet.target_index(), packet.row_ids())
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_invalid"))?;
        let mut occupied = 0_u64;
        let mut counts = [0_u8; 7];
        for row in candidate.row_ids() {
            let placement = source.catalog.skeleton(*row);
            occupied |= placement.cells;
            counts[piece_index(placement.piece)] += 1;
        }
        if candidate.identity != expected_identity || occupied != source.catalog.required_cells()
            || !problem.allows_solution_identity(&candidate.identity)
        { return Err(WasmExactSearchError::InvalidProblem("wasm_required_queue_candidate_identity_mismatch")); }
        let key = FailureKey { identity: candidate.identity, pattern };
        if self.failed.contains(&key) {
            return Ok(WasmRequiredPatternEvidence { source, original_row, identity: candidate.identity, pattern, supported: false, precharged_work_steps: 0, witness_nodes: 0, feasibility_states: 0, provider_peak_upper_bound_bytes });
        }

        let precharged_work_steps = checked_pre_witness_work_upper_bound(
            &source.catalog, &candidate, universe.sequence_len_at(pattern.index()),
        ).ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_work_projection_overflow"))?;
        let parent_limit = u64::try_from(problem.budget().max_nodes()).unwrap_or(u64::MAX);
        let total_limit = if parent_limit == 0 { limits.remaining_work_steps }
            else { parent_limit.min(limits.remaining_work_steps) };
        let witness_limit = total_limit.checked_sub(precharged_work_steps)
            .filter(|remaining| *remaining != 0)
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_pre_witness_budget_exceeded"))?;
        let witness_limit = usize::try_from(witness_limit).unwrap_or(usize::MAX);
        let future = checked_candidate_verification_peak_upper_bound(problem, &source.catalog, &candidate, false)
            .and_then(|bytes| bytes.checked_add((universe.sequence_len_at(pattern.index()) as u128).checked_mul(64)?))
            .and_then(|bytes| bytes.checked_add(1024))
            .ok_or(WasmExactSearchError::InvalidProblem("wasm_required_queue_memory_projection_overflow"))?;
        provider_peak_upper_bound_bytes = self.ensure_probe_memory(future, limits)?;
        usage.peak_retained_bytes = provider_peak_upper_bound_bytes;
        // Commit the conservative precharge before singleton allocation or
        // any dependency/projection/feasibility work can begin. Errors keep it.
        usage.work_steps = precharged_work_steps;
        let full_id = u32::try_from(pattern.index()).map_err(|_| WasmExactSearchError::InvalidProblem("wasm_required_queue_pattern_id_overflow"))?;
        let index = PatternPiecePositionIndex::compile_one_borrowed(universe, full_id)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_required_queue_pattern_index_unavailable"))?;
        let mut pattern_ids = Vec::new();
        pattern_ids.try_reserve_exact(1).map_err(|_| WasmExactSearchError::InvalidProblem("wasm_required_queue_pattern_storage_unavailable"))?;
        pattern_ids.push(full_id);
        let patterns = PatternBitSet::from_pattern_indices(universe.pattern_count(), pattern_ids)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_required_queue_pattern_selection_invalid"))?;
        let target = TargetGroup { key: PieceMultisetKey::from_counts(counts), pattern_index_id: 0,
            possible_patterns: Arc::new(patterns), pattern_index: Some(Arc::new(index)) };

        // MembershipOnly + CountUnique + singleton concrete index enters
        // find_first_pattern_witness before BuildOrderGraph/canonical language/
        // standard-bag/full-coverage cache. active_patterns is only local bit 0.
        let previous_budget = self.workspace.replace_required_probe_node_budget(Some(witness_limit));
        self.poisoned = true;
        let checked = verify_candidate(problem, &source.catalog, &candidate, &target,
            &mut self.workspace, &mut self.evaluator, CandidateWitnessMode::MembershipOnly,
            false, 0, control);
        let remaining = self.workspace.replace_required_probe_node_budget(previous_budget).unwrap_or(witness_limit);
        self.poisoned = false;
        let witness_nodes = witness_limit - remaining;
        usage.work_steps += witness_nodes as u64;
        let checked = checked?;
        if control.is_cancelled() { return Err(WasmExactSearchError::Cancelled); }
        drop(target);
        let retained_peak = self.ensure_probe_memory(checked.retained_bytes as u128, limits)?;
        provider_peak_upper_bound_bytes = provider_peak_upper_bound_bytes.max(retained_peak);
        usage.peak_retained_bytes = provider_peak_upper_bound_bytes;
        let supported = checked.buildable;
        let feasibility_states = checked.feasibility_states;
        if !supported && self.failed.len() < self.max_cached_failures {
            let next_capacity = self.failed.capacity().saturating_mul(2).max(self.failed.len().saturating_add(1));
            if let Some(cache_peak) = Self::cache_bytes(next_capacity)
                .and_then(|bytes| self.ensure_probe_memory(bytes, limits).ok())
            {
                provider_peak_upper_bound_bytes = provider_peak_upper_bound_bytes.max(cache_peak);
                usage.peak_retained_bytes = provider_peak_upper_bound_bytes;
                if self.failed.try_reserve(1).is_ok() { self.failed.insert(key); }
            }
        }
        Ok(WasmRequiredPatternEvidence { source, original_row, identity: candidate.identity, pattern, supported, precharged_work_steps, witness_nodes, feasibility_states, provider_peak_upper_bound_bytes })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind};
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;
    use crate::resource::ExecutionMemoryBound;

    fn admit_fixture(problem: &SearchProblem, authority: &WasmCpuTerminalResourceAuthority, retained: u128, future: u128) {
        ExecutionMemoryBound::unbounded_for_problem(problem)
            .and_then(|bound| bound.with_cap(authority.memory_capacity_bytes()))
            .and_then(|bound| bound.ensure(retained, future))
            .expect("actual parent-owned fixture memory admission");
    }

    /// Existing one-O (0xf3fcf) and two-O terminal-hold micro fixtures,
    /// A two-piece field has OI and IO queues requiring distinct histories.
    /// A P7 source retains 5040 original IDs, including late supported and
    /// unsupported queues that are all singleton local bit zero in the probe.
    /// No result, queue mask, or physical transition is mocked.
    #[test]
    fn singleton_physical_probes_match_legacy_full_rows_and_preserve_scope() {
        // Run this authority-owning test in isolation with --test-threads=1.
        for fixture in 0..5 {
            let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
                .expect("real request memory and compute authority");
            let (initial, queue, hold, placements, expected_successes) = match fixture {
                0 => (0xf3fcf, "[IO]", None, vec![(PieceKind::O, 0xc030)], 1),
                1 => (0xf3fcf, "[IO]", Some(PieceKind::O), vec![(PieceKind::O, 0xc030)], 2),
                2 => (0xfffff ^ (0x0c03 | 0x300c), "[IO]", Some(PieceKind::O),
                    vec![(PieceKind::O, 0x0c03), (PieceKind::O, 0x300c)], 1),
                3 => (0xfffff ^ (0x0c03 | 0x3c000), "[OI]!", None,
                    vec![(PieceKind::O, 0x0c03), (PieceKind::I, 0x3c000)], 2),
                _ => (0xf3fcf, "P7", None, vec![(PieceKind::O, 0xc030)], 720),
            };
            let problem = {
                let query = PcScenarioQuery::new(PcScenarioBoard::standard_10(2, initial),
                    PcQueueInput::pattern_expression(QueuePatternExpression::parse(queue, 5040).expect("canonical queue fixture")),
                    PieceWindow::new(placements.len()))
                    .with_allow_hold(hold.is_some()).with_hold_piece(hold)
                    .with_exact_pieces(Some(placements.len()))
                    .with_count_policy(PcCountPolicy::CountUnique)
                    .with_objective(ObjectivePolicy::minimum_cover())
                    .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1).with_max_memory_mib(Some(64)));
                let query = if fixture == 4 { query.with_supply_window_size(SupplyWindowSize::new(7)) } else { query };
                Arc::new(ProblemCompiler::compile_scenario_pc(&query).expect("fixture problem"))
            };
            let problem_bytes = problem.checked_build_probability_pointee_retained_bytes()
                .expect("measured complete fixture problem graph")
                + 2 * core::mem::size_of::<usize>() as u128;
            admit_fixture(&problem, &authority, problem_bytes,
                GeometryCatalog::checked_compile_peak_upper_bound(&problem).expect("checked catalog constructor peak"));
            let catalog = Arc::new(GeometryCatalog::compile(&problem).expect("actual fixture catalog"));
            let row_ids = placements.iter().map(|&(piece, cells)| catalog.skeleton_id(piece, cells).expect("fixture logical placement belongs to real catalog")).collect::<Vec<_>>();
            let packet = WasmCandidatePacket::new(0, 0, row_ids);
            let candidate = GeometryCandidate::from_rows(&catalog, 0, packet.row_ids()).expect("full fixture identity");
            let universe = problem.piece_source().materialized_universe().unwrap();
            let base = problem_bytes + core::mem::size_of::<GeometryCatalog>() as u128
                + catalog.retained_bytes() as u128 + 2 * core::mem::size_of::<usize>() as u128
                + packet.checked_nested_retained_bytes().unwrap() + core::mem::size_of::<WasmCandidatePacket>() as u128
                + (placements.capacity() * core::mem::size_of::<(PieceKind, u64)>()) as u128
                + 4 * core::mem::size_of::<usize>() as u128;
            let max_len = (0..universe.pattern_count()).map(|id| universe.sequence_len_at(id)).max().unwrap_or(0) as u128;
            let index_peak = (core::mem::size_of::<PatternPiecePositionIndex>() as u128
                + universe.pattern_count() as u128 * 4
                + max_len * 7 * universe.pattern_count().div_ceil(64) as u128 * 8
                + max_len * core::mem::size_of::<PieceKind>() as u128
                + 2 * universe.checked_retained_capacity_bytes().unwrap()) * 2;
            let row_peak = PatternBitSet::checked_all_projection(universe.pattern_count()).unwrap().constructor_peak_bytes;
            let physical_peak = checked_candidate_verification_peak_upper_bound(&problem, &catalog, &candidate, false).unwrap();
            admit_fixture(&problem, &authority, base, index_peak + row_peak + physical_peak);
            let legacy_row = {
                let mut counts = [0; 7];
                for &(piece, _) in &placements { counts[piece_index(piece)] += 1; }
                let target = TargetGroup { key: PieceMultisetKey::from_counts(counts), pattern_index_id: 0,
                    possible_patterns: Arc::new(PatternBitSet::all(universe.pattern_count())),
                    pattern_index: Some(Arc::new(PatternPiecePositionIndex::compile(universe).unwrap())) };
                let mut workspace = BuildUpWorkspace::default();
                let mut evaluator = CoverageProductEvaluator::default();
                let full = verify_candidate(&problem, &catalog, &candidate, &target, &mut workspace,
                    &mut evaluator, CandidateWitnessMode::Disabled, false, 0, &ExecutionControl::default())
                    .expect("legacy full-row physical verifier");
                assert!(full.graph_nodes > 0, "legacy path constructed actual BuildOrderGraph");
                if let Some(row) = full.covered_patterns { row } else {
                    workspace.materialize_standard_bag_root(
                        full.symbolic_coverage_root.expect("successful actual symbolic row"),
                    ).expect("materialized legacy symbolic coverage")
                }
            };
            assert_eq!(legacy_row.count_ones() as usize, expected_successes);
            let mut selected_ids = [0_usize; 4];
            let selected_count = if fixture == 4 {
                assert_eq!(universe.pattern_count(), 5040, "unprojected original P7 source");
                let supported = (64..universe.pattern_count()).rev()
                    .find(|id| legacy_row.contains(PatternId::new(*id))).unwrap();
                let unsupported = (64..universe.pattern_count()).rev()
                    .find(|id| !legacy_row.contains(PatternId::new(*id))).unwrap();
                assert!(supported >= 64 && unsupported >= 64);
                assert!(supported.max(unsupported) >= 5000, "late P7 original ID");
                assert_ne!(universe.sequence_at(supported), universe.sequence_at(unsupported));
                selected_ids = [0, 64, supported, unsupported];
                4
            } else {
                assert!(universe.pattern_count() <= selected_ids.len());
                for (id, slot) in selected_ids.iter_mut().take(universe.pattern_count()).enumerate() { *slot = id; }
                universe.pattern_count()
            };
            let external = base + legacy_row.checked_storage_retained_bytes().unwrap();
            let mut provider = WasmRequiredQueueVerifier::from_shared_inputs(
                Arc::clone(&problem), Arc::clone(&catalog), external, 64, &authority,
            ).expect("one-compute-child provider under actual parent authority");
            let limits = WasmRequiredQueueProbeLimits { remaining_work_steps: 1_000_000, remaining_provider_peak_bytes: 32 * 1024 * 1024 };
            let uid = universe.pattern_universe_id();
            let wid = universe.pattern_weight_model_id();
            let mut remaining_work_steps = limits.remaining_work_steps;
            for id in selected_ids.into_iter().take(selected_count) {
                let pattern = PatternId::new(id);
                {
                    // Demonstrate the PPI mapping independently of supported
                    // status; local bit zero must not alias global ID zero.
                    admit_fixture(&problem, &authority, external + provider.provider_retained_upper_bound().unwrap(), index_peak + row_peak);
                    let one = PatternPiecePositionIndex::compile_one_borrowed(universe, id as u32).unwrap();
                    assert_eq!(one.word_count(), 1);
                    assert_eq!(one.active_word(0), 1);
                    assert_eq!(one.global_pattern_index(0), Some(id));
                    assert_eq!(one.local_pattern_index(id), Some(0));
                    let expanded = one.expand_coverage_words(&[1]).unwrap();
                    assert_eq!(expanded.count_ones(), 1);
                    assert!(expanded.contains(pattern));
                    if id != 0 { assert!(!expanded.contains(PatternId::new(0))); }
                }
                let current = WasmRequiredQueueProbeLimits { remaining_work_steps, ..limits };
                let observed = provider.probe_pattern_observed(7, &packet, candidate.identity, pattern, uid, wid, current, &ExecutionControl::default());
                let proof = observed.result.unwrap();
                assert_eq!(proof.supported(), legacy_row.contains(pattern));
                assert_eq!(proof.original_row(), 7);
                assert_eq!(proof.identity(), candidate.identity);
                assert_eq!(proof.pattern(), pattern);
                assert!(provider.owns_evidence(&proof));
                assert!(proof.provider_peak_upper_bound_bytes() <= current.remaining_provider_peak_bytes);
                assert_eq!(observed.peak_retained_bytes, proof.provider_peak_upper_bound_bytes());
                assert_eq!(observed.work_steps, proof.work_steps());
                assert!(proof.precharged_work_steps() >= proof.feasibility_states() as u64);
                remaining_work_steps = remaining_work_steps.checked_sub(observed.work_steps).unwrap();
                if !proof.supported() {
                    let cached = provider.probe_pattern(7, &packet, candidate.identity, pattern, uid, wid, limits, &ExecutionControl::default()).unwrap();
                    assert!(!cached.supported());
                    assert_eq!(cached.witness_nodes(), 0);
                    assert_eq!(cached.work_steps(), 0);
                }
            }
            let successful = legacy_row.first_pattern().unwrap();
            if fixture == 4 {
                use super::super::realization_feasibility::{
                    realization_feasibility_policy, set_realization_feasibility_policy,
                    RealizationFeasibilityPolicy,
                };
                let previous_policy = realization_feasibility_policy();
                // Full P7 IDs share local bit zero in singleton compilation.
                // Compare physical cache/projection ablations to the existing
                // complete row, including both supported and unsupported IDs.
                for policy in [RealizationFeasibilityPolicy::Legacy,
                    RealizationFeasibilityPolicy::Off,
                    RealizationFeasibilityPolicy::RelaxationOnly] {
                    set_realization_feasibility_policy(policy);
                    for projection in [false, true] {
                        for transition in 0..=2 {
                            provider.failed.clear();
                            provider.set_shared_candidate_feasibility(true);
                            provider.set_candidate_physical_reuse(projection, transition).unwrap();
                            for id in selected_ids.into_iter().take(selected_count) {
                                let pattern = PatternId::new(id);
                                let proof = provider.probe_pattern(7, &packet, candidate.identity,
                                    pattern, uid, wid, limits, &ExecutionControl::default()).unwrap();
                                assert_eq!(proof.supported(), legacy_row.contains(pattern));
                                assert_eq!(proof.pattern(), pattern);
                            }
                            // A fresh successful queue must remain valid after
                            // another queue failed in the same candidate owner.
                            assert!(provider.probe_pattern(7, &packet, candidate.identity,
                                successful, uid, wid, limits, &ExecutionControl::default())
                                .unwrap().supported());
                            let counts = provider.candidate_physical_reuse_counters();
                            if projection { assert!(counts[0] > 0); }
                            if transition == 2 { assert!(counts[2] > 0); }
                        }
                    }
                }
                set_realization_feasibility_policy(previous_policy);
                provider.set_shared_candidate_feasibility(false);
                provider.set_candidate_physical_reuse(false, 1).unwrap();
            }
            let failures_before = provider.failed.len();
            let exhausted = WasmRequiredQueueProbeLimits { remaining_work_steps: 0, ..limits };
            assert!(provider.probe_pattern(7, &packet, candidate.identity, successful, uid, wid, exhausted, &ExecutionControl::default()).is_err());
            assert_eq!(provider.failed.len(), failures_before);
            let precharge = checked_pre_witness_work_upper_bound(&catalog, &candidate,
                universe.sequence_len_at(successful.index())).unwrap();
            let exhausted = WasmRequiredQueueProbeLimits { remaining_work_steps: precharge, ..limits };
            let rejected = provider.probe_pattern_observed(7, &packet, candidate.identity, successful, uid, wid, exhausted, &ExecutionControl::default());
            assert!(rejected.result.is_err(), "no feasibility begins without its full precharge and witness authority");
            assert_eq!(rejected.work_steps, 0);
            assert_eq!(provider.failed.len(), failures_before);
            if fixture == 0 {
                let exhausted = WasmRequiredQueueProbeLimits { remaining_work_steps: precharge + 1, ..limits };
                let rejected = provider.probe_pattern_observed(7, &packet, candidate.identity, successful, uid, wid, exhausted, &ExecutionControl::default());
                assert!(rejected.result.is_err(), "one witness visit cannot finish the one-O path");
                assert_eq!(rejected.work_steps, precharge + 1, "error retains both prework and witness charge");
                assert_eq!(provider.failed.len(), failures_before);
                assert_eq!(provider.workspace.replace_required_probe_node_budget(None), None, "ordinary error restored the previous budget");
                assert!(!provider.poisoned);
            }
            assert!(provider.probe_pattern(7, &packet, candidate.identity, successful, uid, wid, limits, &ExecutionControl::default()).unwrap().supported());
            let exhausted = WasmRequiredQueueProbeLimits { remaining_provider_peak_bytes: 0, ..limits };
            assert!(provider.probe_pattern(7, &packet, candidate.identity, successful, uid, wid, exhausted, &ExecutionControl::default()).is_err());
            assert_eq!(provider.failed.len(), failures_before);
            assert!(provider.probe_pattern(7, &packet, candidate.identity, successful, PatternUniverseId::new(uid.get() ^ 1), wid, limits, &ExecutionControl::default()).is_err());
            let wrong_identity = StandardBoard64TilingIdentity::from_placements(initial, core::iter::empty()).unwrap();
            assert!(provider.probe_pattern(7, &packet, wrong_identity, successful, uid, wid, limits, &ExecutionControl::default()).is_err());
            let cancelled = ExecutionCancellationToken::new();
            cancelled.handle().cancel();
            assert!(matches!(provider.probe_pattern(7, &packet, candidate.identity, successful, uid, wid, limits, &ExecutionControl::new(cancelled)), Err(WasmCpuSearchError::Cancelled)));
            assert_eq!(provider.failed.len(), failures_before);
            if fixture == 0 || fixture == 4 {
                // Exercise the public producer -> probe transition while the
                // parent owns exactly one compute slot. No false-zero bound.
                drop(provider);
                let mut packet_buffer = Vec::with_capacity(1);
                let packet_buffer_bytes = (packet_buffer.capacity() * core::mem::size_of::<WasmCandidatePacket>()) as u128;
                let mut producer = super::super::distributed::WasmCpuCandidateProducer::new_shared_required_queue_source_under_terminal_authority(
                    Arc::clone(&problem), external + packet_buffer_bytes + 4, &authority,
                ).expect("minimum geometry source admitted under the same parent");
                loop {
                    match producer.advance(&ExecutionControl::default()).unwrap() {
                        super::super::distributed::WasmCandidateProducerAdvance::Pending => {}
                        super::super::distributed::WasmCandidateProducerAdvance::Candidate(packet) => packet_buffer.push(packet),
                        super::super::distributed::WasmCandidateProducerAdvance::Completed(summary) => {
                            assert!(summary.truncated_reason.is_none());
                            break;
                        }
                        super::super::distributed::WasmCandidateProducerAdvance::Cancelled => panic!("fixture cancellation"),
                    }
                }
                assert_eq!(packet_buffer.len(), 1);
                let transition_external = external + producer.checked_required_queue_source_retained_upper_bound_bytes().unwrap()
                    + packet_buffer_bytes + packet_buffer.iter().map(|packet| packet.checked_nested_retained_bytes().unwrap()).sum::<u128>();
                let mut selected = producer.into_required_queue_verifier_under_terminal_authority(
                    transition_external, 64, &authority,
                ).expect("consuming source returns its compute child before probe admission");
                let identity_observation = selected.probe_candidate_identity_observed(
                    &packet_buffer[0], candidate.identity, uid, wid, limits, &ExecutionControl::default(),
                );
                let identity_proof = identity_observation.result.unwrap();
                assert!(identity_proof.admitted());
                assert!(legacy_row.contains(identity_proof.witness_pattern().unwrap()));
                assert_eq!(identity_proof.identity(), candidate.identity);
                assert!(selected.owns_identity_evidence(&identity_proof));
                assert_eq!(identity_observation.work_steps, identity_proof.precharged_work_steps() + identity_proof.witness_nodes() as u64);
                assert!(identity_observation.peak_retained_bytes <= limits.remaining_provider_peak_bytes);
                if fixture == 4 {
                    assert!(identity_proof.witness_pattern().unwrap().index() >= 64);
                    // Whichever PPI preparation route this source uses, a
                    // repeat has exactly the same admitted full identity.
                    let repeated = selected.probe_candidate_identity_observed(
                        &packet_buffer[0], candidate.identity, uid, wid, limits, &ExecutionControl::default(),
                    ).result.unwrap();
                    assert_eq!(repeated.identity(), identity_proof.identity());
                    assert_eq!(repeated.witness_pattern(), identity_proof.witness_pattern());
                }
                assert!(selected.probe_pattern(7, &packet_buffer[0], candidate.identity, successful, uid, wid, limits, &ExecutionControl::default()).unwrap().supported());
            }
        }
    }
}
