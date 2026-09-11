//! Private CPU/Oracle PC minimum adapter with a complete admitted identity
//! dictionary and physically evaluated lazy coverage. No v2 source is minted.

use std::{fmt, ops::Range, sync::Arc};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::{
    cover::implicit_minimum::{ImplicitDiagramOracle, ImplicitJointDecision, ImplicitMinimumError,
        ImplicitOracleBudget, ImplicitOracleObservation, ImplicitSourceBinding},
    pattern::pattern_id::PatternId,
    universe::{pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId},
};
use clearra_problem::SearchProblem;
use sha2::{Digest, Sha256};

use crate::{WasmCpuSearchError, WasmCpuTerminalResourceAuthority};
use super::{WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuCandidateProducer,
    WasmRequiredQueueProbeLimits, WasmRequiredQueueVerifier};

/// Preparation is separate from the CEGIS controller's per-query ledger.
/// Geometry still obeys the request's node/memory limits; these additional
/// finite caps prevent unbounded capture and membership preparation.
#[derive(Clone, Copy, Debug)]
pub struct WasmImplicitPcSourceLimits {
    pub max_geometry_candidates: usize,
    pub max_geometry_advances: u64,
    /// None = ordinary complete-family producer; Some(false/true) = bounded
    /// deferred/open root publication. The choice is fixed for preparation.
    pub bounded_root_streaming: Option<bool>,
    /// Used only by the bounded root path. Zero means exhausted.
    pub max_geometry_work_steps: u64,
    pub max_identity_work_steps: u64,
    pub max_cached_failures: usize,
    pub shared_candidate_feasibility: bool,
    pub reuse_candidate_projection: bool,
    /// 0 = no transition table, 1 = fresh per probe, 2 = same-candidate reuse.
    pub candidate_transition_policy: u8,
}

pub struct WasmImplicitPcSourceObservation {
    pub result: Result<WasmImplicitPcMinimumOracle, ImplicitMinimumError>,
    /// Exact producer.advance calls, not a claim about CPU instructions.
    pub geometry_advances: u64,
    /// Conservative bounded-root precharges; None means the ordinary producer
    /// was not metered with this work model, not that it did no work.
    pub geometry_work_steps: Option<u64>,
    /// Conservative precharges plus visited physical witness nodes.
    pub identity_work_steps: u64,
}

struct OriginalIdentity {
    identity: StandardBoard64TilingIdentity,
    packet: WasmCandidatePacket,
}

pub struct WasmImplicitPcMinimumOracle {
    verifier: WasmRequiredQueueVerifier,
    original: Vec<OriginalIdentity>,
    /// 0 unknown; 1 proved outside U*; 2 has a physical witness in the source.
    /// This is one byte per queue, never a candidate-by-queue matrix.
    reachability: Vec<u8>,
    binding: ImplicitSourceBinding,
    universe_id: PatternUniverseId,
    weight_model_id: PatternWeightModelId,
}

fn map_error(error: WasmCpuSearchError) -> ImplicitMinimumError {
    match error {
        WasmCpuSearchError::Cancelled => ImplicitMinimumError::Cancelled,
        WasmCpuSearchError::ResourceAdmission { .. } => ImplicitMinimumError::CapacityExceeded,
        WasmCpuSearchError::InvalidProblem { reason } => match reason {
            "wasm_required_queue_pre_witness_budget_exceeded"
            | "wasm_required_queue_witness_budget_exceeded"
            | "wasm_required_identity_scan_budget_exceeded" => ImplicitMinimumError::WorkLimit,
            "wasm_required_queue_controller_peak_exhausted" => ImplicitMinimumError::CapacityExceeded,
            _ => ImplicitMinimumError::Unknown,
        },
        _ => ImplicitMinimumError::Unknown,
    }
}

fn reserve<T>(count: usize) -> Result<Vec<T>, ImplicitMinimumError> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| ImplicitMinimumError::CapacityExceeded)?;
    Ok(result)
}

struct CanonicalSha<'a>(&'a mut Sha256);
impl fmt::Write for CanonicalSha<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.update(value.as_bytes());
        Ok(())
    }
}

fn canonical_dictionary_sha256(original: &[OriginalIdentity]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"clearra-implicit-original-normalized-keys-v1\0");
    digest.update((original.len() as u64).to_le_bytes());
    for row in original {
        // The domain type owns the canonical spelling; NUL is not in a key.
        row.identity.write_canonical(&mut CanonicalSha(&mut digest))
            .expect("digest formatting has no allocation or failure");
        digest.update([0]);
    }
    digest.finalize().into()
}

impl WasmImplicitPcMinimumOracle {
    /// Drive the real producer to verified normal completion, admit every
    /// distinct geometry identity using an any-queue physical witness, and
    /// retain exactly the buildable dictionary in legacy normalized-key order.
    /// query_sha256 is the transport's canonical request digest, never a
    /// substitute for the immutable problem/catalog owner used by all probes.
    /// external_bytes must account the caller's actual retained graph including
    /// the shared SearchProblem; capture buffers are preadmitted here as well.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_under_terminal_authority(
        problem: Arc<SearchProblem>, query_sha256: [u8; 32], external_bytes: u128,
        limits: WasmImplicitPcSourceLimits, authority: &WasmCpuTerminalResourceAuthority,
        control: &ExecutionControl,
    ) -> WasmImplicitPcSourceObservation {
        let mut observation = WasmImplicitPcSourceObservation {
            result: Err(ImplicitMinimumError::Unknown), geometry_advances: 0, identity_work_steps: 0,
            geometry_work_steps: limits.bounded_root_streaming.map(|_| 0),
        };
        observation.result = Self::prepare_exact(problem, query_sha256, external_bytes, limits,
            authority, control, &mut observation.geometry_advances, &mut observation.geometry_work_steps,
            &mut observation.identity_work_steps);
        observation
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_exact(
        problem: Arc<SearchProblem>, query_sha256: [u8; 32], external_bytes: u128,
        limits: WasmImplicitPcSourceLimits, authority: &WasmCpuTerminalResourceAuthority,
        control: &ExecutionControl, geometry_advances: &mut u64,
        geometry_work_steps: &mut Option<u64>, identity_work_steps: &mut u64,
    ) -> Result<Self, ImplicitMinimumError> {
        if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
        if limits.max_geometry_candidates == 0 || limits.max_geometry_advances == 0 {
            return Err(ImplicitMinimumError::WorkLimit);
        }
        if limits.candidate_transition_policy > 2 { return Err(ImplicitMinimumError::InvalidOracle); }
        if problem.objective().kind() != clearra_core_domain::objective::objective_kind::ObjectiveKind::MinimumCover
            || problem.solution_probability_policy().requested()
        { return Err(ImplicitMinimumError::Unknown); }
        let universe = problem.piece_source().materialized_universe().ok_or(ImplicitMinimumError::Unknown)?;
        if !universe.complete() { return Err(ImplicitMinimumError::Unknown); }
        let pattern_count = universe.pattern_count();
        let universe_id = universe.pattern_universe_id();
        let weight_model_id = universe.pattern_weight_model_id();
        // Reserve both capture and admitted-record outer vectors and every
        // candidate's <=15 u32 row payload before creating any of them. One
        // extra returned packet fits even if a cap is reached on that call.
        let capture_future = (limits.max_geometry_candidates as u128)
            .checked_mul((core::mem::size_of::<WasmCandidatePacket>() + core::mem::size_of::<OriginalIdentity>()
                + 16 * core::mem::size_of::<u32>()) as u128)
            .and_then(|n| n.checked_add(pattern_count as u128))
            .and_then(|n| n.checked_add(core::mem::size_of::<Self>() as u128 + 4096))
            .ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let captured_external = external_bytes.checked_add(capture_future).ok_or(ImplicitMinimumError::CapacityExceeded)?;
        // This constructor validates the real external graph, obtains a child
        // of the parent authority, and admits catalog/family preparation.
        let mut producer = WasmCpuCandidateProducer::new_shared_required_queue_source_under_terminal_authority(
            Arc::clone(&problem), captured_external, authority,
        ).map_err(map_error)?;
        let mut packets = reserve(limits.max_geometry_candidates)?;
        loop {
            if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
            if *geometry_advances >= limits.max_geometry_advances { return Err(ImplicitMinimumError::WorkLimit); }
            *geometry_advances += 1;
            let advance = if let Some(stream) = limits.bounded_root_streaming {
                let used = geometry_work_steps.as_mut().ok_or(ImplicitMinimumError::InvalidOracle)?;
                let remaining = limits.max_geometry_work_steps.checked_sub(*used)
                    .ok_or(ImplicitMinimumError::WorkLimit)?;
                let (result, work, _) = producer.advance_bounded_root(stream, remaining, control);
                *used = used.checked_add(work).ok_or(ImplicitMinimumError::WorkLimit)?;
                if work > remaining { return Err(ImplicitMinimumError::WorkLimit); }
                result.map_err(|reason| match reason {
                    "open_family_work_limit" | "open_family_preparation_work_limit" => ImplicitMinimumError::WorkLimit,
                    _ if control.is_cancelled() => ImplicitMinimumError::Cancelled,
                    _ => ImplicitMinimumError::Unknown,
                })?
            } else {
                producer.advance(control).map_err(|_| ImplicitMinimumError::Unknown)?
            };
            match advance {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Cancelled => return Err(ImplicitMinimumError::Cancelled),
                WasmCandidateProducerAdvance::Candidate(packet) => {
                    if packets.len() == limits.max_geometry_candidates { return Err(ImplicitMinimumError::CapacityExceeded); }
                    if packet.ordinal() != packets.len() as u64 || packet.pass_index() != 0 || packet.is_extended()
                        || packet.row_ids().is_empty() || packet.row_ids().len() > 15
                        || packet.checked_nested_retained_bytes().ok_or(ImplicitMinimumError::CapacityExceeded)? > 16 * 4
                    { return Err(ImplicitMinimumError::InvalidOracle); }
                    packets.push(packet);
                }
                WasmCandidateProducerAdvance::Completed(summary) => {
                    if summary.truncated_reason.is_some() { return Err(ImplicitMinimumError::Unknown); }
                    if summary.candidate_count != packets.len() { return Err(ImplicitMinimumError::InvalidOracle); }
                    break;
                }
            }
        }
        // Includes retained original TargetGroups; consuming the producer
        // releases its single compute child before admitting the physical one.
        let transition_external = captured_external.checked_add(
            producer.checked_required_queue_source_retained_upper_bound_bytes().ok_or(ImplicitMinimumError::CapacityExceeded)?,
        ).ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let mut verifier = producer.into_required_queue_verifier_under_terminal_authority(
            transition_external, limits.max_cached_failures, authority,
        ).map_err(map_error)?;
        verifier.set_shared_candidate_feasibility(limits.shared_candidate_feasibility);
        verifier.set_candidate_physical_reuse(limits.reuse_candidate_projection,
            limits.candidate_transition_policy).map_err(map_error)?;
        let mut original = reserve(limits.max_geometry_candidates)?;
        for packet in packets.drain(..) {
            if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
            let identity = verifier.checked_source_candidate_identity(&packet).map_err(map_error)?;
            original.push(OriginalIdentity { identity, packet });
        }
        // StandardBoard64TilingIdentity::Ord compares exactly the canonical
        // key's piece characters and fixed-width hexadecimal masks. It is not
        // a producer ordinal or a hash-bucket ordering (regression below).
        original.sort_unstable_by_key(|entry| entry.identity);
        if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
        let mut reachability = reserve(pattern_count)?;
        reachability.resize(pattern_count, 0_u8);
        let mut write = 0;
        for read in 0..original.len() {
            let entry = &original[read];
            let remaining_work_steps = limits.max_identity_work_steps.checked_sub(*identity_work_steps)
                .ok_or(ImplicitMinimumError::WorkLimit)?;
            let observed = verifier.probe_candidate_identity_observed(&entry.packet, entry.identity,
                universe_id, weight_model_id, WasmRequiredQueueProbeLimits {
                    remaining_work_steps,
                    // The parent lease still includes source and all buffers;
                    // each physical allocation is independently checked there.
                    remaining_provider_peak_bytes: authority.memory_capacity_bytes(),
                }, control);
            *identity_work_steps = identity_work_steps.checked_add(observed.work_steps).ok_or(ImplicitMinimumError::WorkLimit)?;
            if observed.work_steps > remaining_work_steps { return Err(ImplicitMinimumError::WorkLimit); }
            let evidence = observed.result.map_err(map_error)?;
            if !verifier.owns_identity_evidence(&evidence) || evidence.identity() != entry.identity {
                return Err(ImplicitMinimumError::InvalidOracle);
            }
            if let Some(witness) = evidence.witness_pattern() {
                let state = reachability.get_mut(witness.index()).ok_or(ImplicitMinimumError::InvalidOracle)?;
                *state = 2;
                original.swap(write, read);
                write += 1;
            }
        }
        original.truncate(write);
        // Admission precedes deduplication. Equal logical identities can have
        // different producer target memberships; one rejected membership must
        // not hide an admitted realization in another complete target group.
        original.dedup_by_key(|entry| entry.identity);
        drop(packets);
        if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
        let binding = ImplicitSourceBinding {
            query_sha256, original_identity_sha256: canonical_dictionary_sha256(&original),
            original_row_count: original.len(), pattern_universe_count: pattern_count,
        };
        Ok(Self { verifier, original, reachability, binding, universe_id, weight_model_id })
    }

    /// Cache choice is explicit for A/B; it never changes dictionary membership,
    /// canonical IDs, per-queue histories or the required reachable universe.
    pub fn set_shared_candidate_feasibility(&mut self, enabled: bool) {
        self.verifier.set_shared_candidate_feasibility(enabled);
    }

    pub fn shared_candidate_feasibility_counters(&self) -> (u64, u64) {
        self.verifier.shared_candidate_feasibility_counters()
    }

    pub fn candidate_physical_reuse_counters(&self) -> [u64; 5] {
        self.verifier.candidate_physical_reuse_counters()
    }

    pub fn original_identity(&self, row: usize) -> Option<StandardBoard64TilingIdentity> {
        self.original.get(row).map(|entry| entry.identity)
    }

    fn fixed_retained_bytes(&self) -> Option<u128> {
        let packets = self.original.iter().try_fold(0_u128, |sum, entry|
            sum.checked_add(entry.packet.checked_nested_retained_bytes()?))?;
        (core::mem::size_of::<Self>() as u128)
            .checked_add((self.original.capacity() as u128).checked_mul(core::mem::size_of::<OriginalIdentity>() as u128)?)?
            .checked_add(packets)?.checked_add(self.reachability.capacity() as u128)?
            .checked_add(self.verifier.checked_implicit_source_retained_bytes()?)
    }

    fn start_usage(&self, budget: ImplicitOracleBudget) -> Result<QueryUsage, ImplicitMinimumError> {
        let fixed = self.fixed_retained_bytes().ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let runtime = self.verifier.checked_implicit_runtime_retained_bytes().ok_or(ImplicitMinimumError::CapacityExceeded)?;
        let peak = fixed.checked_add(runtime).ok_or(ImplicitMinimumError::CapacityExceeded)?;
        if peak > budget.max_retained_bytes { return Err(ImplicitMinimumError::CapacityExceeded); }
        Ok(QueryUsage { work_steps: 0, peak_retained_bytes: peak, fixed_retained_bytes: fixed })
    }

    fn pair(
        &mut self, row: usize, pattern: usize, budget: ImplicitOracleBudget,
        control: &ExecutionControl, usage: &mut QueryUsage,
    ) -> Result<bool, ImplicitMinimumError> {
        if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
        usage.charge(budget)?;
        let entry = self.original.get(row).ok_or(ImplicitMinimumError::InvalidOracle)?;
        let limits = WasmRequiredQueueProbeLimits {
            remaining_work_steps: budget.remaining_work_steps.checked_sub(usage.work_steps).ok_or(ImplicitMinimumError::WorkLimit)?,
            remaining_provider_peak_bytes: budget.max_retained_bytes.checked_sub(usage.fixed_retained_bytes)
                .ok_or(ImplicitMinimumError::CapacityExceeded)?,
        };
        let observed = self.verifier.probe_pattern_observed(row, &entry.packet, entry.identity,
            PatternId::new(pattern), self.universe_id, self.weight_model_id, limits, control);
        usage.work_steps = usage.work_steps.checked_add(observed.work_steps).ok_or(ImplicitMinimumError::WorkLimit)?;
        usage.peak_retained_bytes = usage.peak_retained_bytes.max(usage.fixed_retained_bytes
            .checked_add(observed.peak_retained_bytes).ok_or(ImplicitMinimumError::CapacityExceeded)?);
        if usage.work_steps > budget.remaining_work_steps { return Err(ImplicitMinimumError::WorkLimit); }
        if usage.peak_retained_bytes > budget.max_retained_bytes { return Err(ImplicitMinimumError::CapacityExceeded); }
        let evidence = observed.result.map_err(map_error)?;
        if !self.verifier.owns_evidence(&evidence) || evidence.original_row() != row
            || evidence.identity() != entry.identity || evidence.pattern().index() != pattern
        { return Err(ImplicitMinimumError::InvalidOracle); }
        if evidence.supported() { self.reachability[pattern] = 2; }
        Ok(evidence.supported())
    }

    fn joint_exact(
        &mut self, rows: Range<usize>, patterns: &[usize], budget: ImplicitOracleBudget,
        control: &ExecutionControl, usage: &mut QueryUsage,
    ) -> Result<ImplicitJointDecision, ImplicitMinimumError> {
        if rows.start > rows.end || rows.end > self.original.len()
            || patterns.iter().any(|pattern| *pattern >= self.reachability.len())
        { return Err(ImplicitMinimumError::InvalidOracle); }
        if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
        usage.charge(budget)?;
        'candidate: for row in rows {
            for pattern in patterns {
                if !self.pair(row, *pattern, budget, control, usage)? { continue 'candidate; }
            }
            return Ok(ImplicitJointDecision::Witness(row));
        }
        // Every known-full logical identity in the complete dictionary was
        // rejected by an exhaustive physical probe for at least one queue.
        Ok(ImplicitJointDecision::ProvedEmpty)
    }

    fn counterexample_exact(
        &mut self, rows: &[usize], budget: ImplicitOracleBudget,
        control: &ExecutionControl, usage: &mut QueryUsage,
    ) -> Result<Option<usize>, ImplicitMinimumError> {
        if rows.iter().any(|row| *row >= self.original.len()) { return Err(ImplicitMinimumError::InvalidOracle); }
        'queue: for pattern in 0..self.reachability.len() {
            if control.is_cancelled() { return Err(ImplicitMinimumError::Cancelled); }
            usage.charge(budget)?;
            if self.reachability[pattern] == 1 { continue; }
            for row in rows {
                if self.pair(*row, pattern, budget, control, usage)? { continue 'queue; }
            }
            // PC Minimum covers U*, the complete source's reachable union.
            // A chosen set's failure alone never proves a queue is outside U*.
            if self.reachability[pattern] == 2 { return Ok(Some(pattern)); }
            match self.joint_exact(0..self.original.len(), &[pattern], budget, control, usage)? {
                ImplicitJointDecision::Witness(_) => return Ok(Some(pattern)),
                ImplicitJointDecision::ProvedEmpty => self.reachability[pattern] = 1,
                ImplicitJointDecision::Unknown => return Err(ImplicitMinimumError::Unknown),
            }
        }
        Ok(None)
    }
}

struct QueryUsage {
    work_steps: u64,
    peak_retained_bytes: u128,
    fixed_retained_bytes: u128,
}

impl QueryUsage {
    fn charge(&mut self, budget: ImplicitOracleBudget) -> Result<(), ImplicitMinimumError> {
        if self.work_steps >= budget.remaining_work_steps { return Err(ImplicitMinimumError::WorkLimit); }
        self.work_steps += 1;
        Ok(())
    }
}

impl ImplicitDiagramOracle for WasmImplicitPcMinimumOracle {
    fn binding(&self) -> ImplicitSourceBinding { self.binding }
    fn checked_retained_bytes(&self) -> Option<u128> {
        self.fixed_retained_bytes()?.checked_add(self.verifier.checked_implicit_runtime_retained_bytes()?)
    }
    fn joint(&mut self, rows: Range<usize>, patterns: &[usize], budget: ImplicitOracleBudget,
        control: &ExecutionControl) -> ImplicitOracleObservation<ImplicitJointDecision> {
        let mut usage = match self.start_usage(budget) {
            Ok(usage) => usage,
            Err(error) => return ImplicitOracleObservation { result: Err(error), work_steps: 0,
                peak_retained_bytes: self.checked_retained_bytes().unwrap_or(u128::MAX) },
        };
        let result = self.joint_exact(rows, patterns, budget, control, &mut usage);
        ImplicitOracleObservation { result, work_steps: usage.work_steps, peak_retained_bytes: usage.peak_retained_bytes }
    }
    fn counterexample(&mut self, rows: &[usize], budget: ImplicitOracleBudget,
        control: &ExecutionControl) -> ImplicitOracleObservation<Option<usize>> {
        let mut usage = match self.start_usage(budget) {
            Ok(usage) => usage,
            Err(error) => return ImplicitOracleObservation { result: Err(error), work_steps: 0,
                peak_retained_bytes: self.checked_retained_bytes().unwrap_or(u128::MAX) },
        };
        let result = self.counterexample_exact(rows, budget, control, &mut usage);
        ImplicitOracleObservation { result, work_steps: usage.work_steps, peak_retained_bytes: usage.peak_retained_bytes }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::PiecePlacementMask};
    use clearra_coverage::cover::implicit_minimum::{ImplicitMinimumLimits, ImplicitMinimumSearch};
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard,
        PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    #[test]
    fn dictionary_identity_order_matches_legacy_normalized_key_order() {
        // J's domain piece code comes after O, but canonical text orders J
        // before O. This prevents either enum order or source ordinal becoming
        // the externally meaningful original candidate ordering.
        let mut identities = [
            StandardBoard64TilingIdentity::from_placements(0, [PiecePlacementMask::new(PieceKind::O, 0x303)]).unwrap(),
            StandardBoard64TilingIdentity::from_placements(0, [PiecePlacementMask::new(PieceKind::J, 0x407)]).unwrap(),
            StandardBoard64TilingIdentity::from_placements(0, [PiecePlacementMask::new(PieceKind::I, 0xf)]).unwrap(),
        ];
        let canonical = |identity: StandardBoard64TilingIdentity| {
            let mut key = String::new();
            identity.write_canonical(&mut key).unwrap();
            key
        };
        let mut expected = identities.map(canonical);
        expected.sort_unstable();
        identities.sort_unstable();
        assert_eq!(identities.map(canonical), expected);
    }

    /// Isolated test, --test-threads=1: owns real request memory/compute.
    /// The source has two queues but U* contains only O. Every dictionary
    /// entry comes from the actual producer and actual any-queue BuildUp.
    #[test]
    fn actual_pc_oracle_uses_reachable_union_and_preserves_reuse_ab_results() {
        use super::super::inverse_parent::{inverse_parent_policy, set_inverse_parent_policy, InverseParentPolicy};
        struct RestoreParents(InverseParentPolicy);
        impl Drop for RestoreParents { fn drop(&mut self) { set_inverse_parent_policy(self.0); } }
        let _restore_parents = RestoreParents(inverse_parent_policy());
        let mut bindings = [None; 18];
        let cases = [InverseParentPolicy::EagerTable, InverseParentPolicy::EagerRaw, InverseParentPolicy::Deferred]
            .into_iter().flat_map(|parent| [(false, None), (true, None),
                (false, Some(false)), (true, Some(false)), (false, Some(true)), (true, Some(true))]
                .into_iter().map(move |(reuse, root)| (parent, reuse, root)));
        for (trial, (parent, reuse, root)) in cases.enumerate() {
            set_inverse_parent_policy(parent);
            let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity().unwrap();
            let query = PcScenarioQuery::new(PcScenarioBoard::standard_10(2, 0xf3fcf),
                PcQueueInput::pattern_expression(QueuePatternExpression::parse("[IO]", 2).unwrap()),
                PieceWindow::new(1))
                .with_allow_hold(false).with_exact_pieces(Some(1))
                .with_count_policy(PcCountPolicy::CountUnique)
                .with_objective(ObjectivePolicy::minimum_cover())
                .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1).with_max_memory_mib(Some(64)));
            let problem = Arc::new(ProblemCompiler::compile_scenario_pc(&query).unwrap());
            let universe = problem.piece_source().materialized_universe().unwrap();
            let o = (0..universe.pattern_count()).find(|id| universe.sequence_at(*id)[0] == PieceKind::O).unwrap();
            let i = (0..universe.pattern_count()).find(|id| universe.sequence_at(*id)[0] == PieceKind::I).unwrap();
            let external = problem.checked_build_probability_pointee_retained_bytes().unwrap()
                + 2 * core::mem::size_of::<usize>() as u128;
            let prepared = WasmImplicitPcMinimumOracle::prepare_under_terminal_authority(
                Arc::clone(&problem), Sha256::digest(b"pc/0xf3fcf/[IO]/no-hold/minimum/exact1").into(),
                external, WasmImplicitPcSourceLimits {
                    max_geometry_candidates: 8, max_geometry_advances: 100_000,
                    bounded_root_streaming: root, max_geometry_work_steps: 1_000_000_000_000_000_000,
                    max_identity_work_steps: 1_000_000, max_cached_failures: 64,
                    shared_candidate_feasibility: reuse,
                    reuse_candidate_projection: reuse, candidate_transition_policy: if reuse { 2 } else { 1 },
                }, &authority, &ExecutionControl::default(),
            );
            assert!(prepared.geometry_advances > 0);
            assert_eq!(prepared.geometry_work_steps.is_some(), root.is_some());
            if let Some(work) = prepared.geometry_work_steps { assert!(work > 0); }
            assert!(prepared.identity_work_steps > 0);
            let mut oracle = prepared.result.unwrap();
            assert_eq!(oracle.binding().original_row_count, 1);
            assert_eq!(oracle.binding().pattern_universe_count, 2);
            bindings[trial] = Some(oracle.binding());
            let budget = ImplicitOracleBudget { remaining_work_steps: 1_000_000, max_retained_bytes: 64 * 1024 * 1024 };
            assert_eq!(oracle.joint(0..1, &[i], budget, &ExecutionControl::default()).result.unwrap(), ImplicitJointDecision::ProvedEmpty);
            assert_eq!(oracle.joint(0..1, &[o], budget, &ExecutionControl::default()).result.unwrap(), ImplicitJointDecision::Witness(0));
            assert_eq!(oracle.counterexample(&[], budget, &ExecutionControl::default()).result.unwrap(), Some(o));
            assert_eq!(oracle.counterexample(&[0], budget, &ExecutionControl::default()).result.unwrap(), None);
            assert_eq!(oracle.reachability[i], 1, "outside U* only after all original identities reject I");
            let stopped = oracle.joint(0..1, &[o], ImplicitOracleBudget { remaining_work_steps: 0, ..budget }, &ExecutionControl::default());
            assert_eq!(stopped.result, Err(ImplicitMinimumError::WorkLimit));
            assert_eq!(stopped.work_steps, 0);
            let mut search = ImplicitMinimumSearch::new(oracle, ImplicitMinimumLimits {
                max_work_steps: 2_000_000, max_retained_bytes: 64 * 1024 * 1024,
            }).unwrap();
            let result = search.solve_minimum(&ExecutionControl::default()).unwrap().unwrap();
            assert_eq!(result.minimum_cardinality(), 1);
            assert_eq!(result.original_rows(), &[0]);
            assert!(search.next_alternative(&ExecutionControl::default()).unwrap().is_none());
            let expected = search.oracle().original_identity(0).unwrap();
            drop(search);
            // Exercise the real full-row objective path too. Partial physical
            // oracle evidence above never enters this legacy complete result.
            for stream in [false, true].into_iter().filter(|_| root.is_none()) {
                let mut session = crate::WasmCpuSearchSession::new_bounded_root_ab_under_authority(
                    Arc::clone(&problem), external, &authority, stream, 1_000_000_000_000_000_000,
                ).unwrap();
                let mut completed = false;
                for _ in 0..100_000 {
                    match session.advance(1, &ExecutionControl::default()).unwrap() {
                        crate::WasmCpuSearchAdvance::Pending => {}
                        crate::WasmCpuSearchAdvance::Cancelled => panic!("unexpected cancellation"),
                        crate::WasmCpuSearchAdvance::Completed(full) => {
                            assert_eq!(full.bool_field("minimum_cover_complete"), Some(true));
                            assert_eq!(full.bool_field("minimum_cover_proven_minimum"), Some(true));
                            assert_eq!(full.normalized_solution_identities(), &[expected]);
                            completed = true;
                            break;
                        }
                    }
                }
                assert!(completed, "bounded source failed to complete");
                let (work, candidates) = session.bounded_root_ab_progress().unwrap();
                assert!(work > 0 && candidates > 0);
            }
            let mut exhausted = crate::WasmCpuSearchSession::new_bounded_root_ab_under_authority(
                Arc::clone(&problem), external, &authority, true, 0,
            ).unwrap();
            assert!(exhausted.advance(1, &ExecutionControl::default()).is_err());
            assert!(exhausted.advance(1, &ExecutionControl::default()).is_err(),
                "resource interruption must not turn into complete source evidence");
            drop(exhausted);
            if root == Some(true) {
                let denied = WasmImplicitPcMinimumOracle::prepare_under_terminal_authority(
                    Arc::clone(&problem), Sha256::digest(b"pc/zero-source-work").into(),
                    external, WasmImplicitPcSourceLimits {
                        max_geometry_candidates: 8, max_geometry_advances: 100_000,
                        bounded_root_streaming: root, max_geometry_work_steps: 0,
                        max_identity_work_steps: 1_000_000, max_cached_failures: 64,
                        shared_candidate_feasibility: reuse, reuse_candidate_projection: reuse,
                        candidate_transition_policy: if reuse { 2 } else { 1 },
                    }, &authority, &ExecutionControl::default(),
                );
                assert!(matches!(denied.result, Err(ImplicitMinimumError::WorkLimit)));
                assert_eq!(denied.geometry_work_steps, Some(0));
                assert_eq!(denied.identity_work_steps, 0,
                    "no dictionary membership may be minted from an unfinished source");
            }
        }
        assert!(bindings.iter().all(|binding| *binding == bindings[0]),
            "source publication and cache policies preserve the complete canonical dictionary");
    }
}
