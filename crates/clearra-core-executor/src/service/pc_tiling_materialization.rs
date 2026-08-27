use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    resource::ResourceReport,
    solution::{
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_core_ffi::PackingCandidateIdentityError;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_problem::{SearchOutputPolicy, SearchProblem};

use crate::{packing::PackingRunResult, tiling_solution_store::TilingSolutionPageStore};

pub(crate) const ACTUAL_TILING_SOLUTION_SET_CONTRACT: &str = "normalized-tiling-set";
pub(crate) const PC_TILING_INITIAL_PAGE_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcTilingMaterializationError {
    ExecutionCancelled,
    CandidateUnavailable { candidate_index: usize },
    CandidateIdentity(PackingCandidateIdentityError),
    AllocationFailed,
    MemoryAccountingUnavailable,
    ResourceIncomplete(ResourceReport),
    PageStore(&'static str),
}

const TILING_MATERIALIZATION_CANCELLATION_POLL_STRIDE: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PcTilingMaterialization {
    initial_page_keys: Vec<String>,
    page_store: Arc<TilingSolutionPageStore>,
    packing_source_raw_geometry: bool,
    canonical_tiling_objective: bool,
    memory_admission_accounted: bool,
    complete: bool,
    incomplete_reason: &'static str,
}

impl PcTilingMaterialization {
    pub(crate) fn from_packing(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
    ) -> Result<Self, PcTilingMaterializationError> {
        Self::from_packing_with_production_poll(problem, packing, control, &mut |_, _, _| {})
    }

    fn from_packing_with_production_poll(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
        observer: &mut impl FnMut(&'static str, usize, usize),
    ) -> Result<Self, PcTilingMaterializationError> {
        Self::from_packing_with_poll(
            problem,
            packing,
            control,
            TILING_MATERIALIZATION_CANCELLATION_POLL_STRIDE,
            observer,
        )
    }

    fn from_packing_with_poll(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
        poll_stride: usize,
        observer: &mut impl FnMut(&'static str, usize, usize),
    ) -> Result<Self, PcTilingMaterializationError> {
        let poll_stride = poll_stride.max(1);
        let canonical_tiling_objective = problem.output_policy() == SearchOutputPolicy::TilingOnly
            && problem.objective() == ObjectivePolicy::tiling();
        poll_cancellation(
            control,
            observer,
            "identities",
            0,
            packing.candidate_count(),
        )?;

        let identity_memory_admitted = ensure_materialization_memory(
            problem,
            packing,
            checked_identity_bytes(packing.candidate_count())?,
            canonical_tiling_objective,
        )?;

        let mut identities = Vec::new();
        identities
            .try_reserve_exact(packing.candidate_count())
            .map_err(|_| PcTilingMaterializationError::AllocationFailed)?;
        for candidate_index in 0..packing.candidate_count() {
            if candidate_index != 0 && candidate_index % poll_stride == 0 {
                poll_cancellation(
                    control,
                    observer,
                    "identities",
                    candidate_index,
                    packing.candidate_count(),
                )?;
            }
            let candidate = packing
                .candidate_view_at(candidate_index)
                .ok_or(PcTilingMaterializationError::CandidateUnavailable { candidate_index })?;
            identities.push(
                candidate
                    .standard_board64_tiling_identity(problem.initial_board().occupied_mask())
                    .map_err(PcTilingMaterializationError::CandidateIdentity)?,
            );
        }
        poll_cancellation(
            control,
            observer,
            "identities",
            packing.candidate_count(),
            packing.candidate_count(),
        )?;

        poll_cancellation(control, observer, "canonicalize", 0, 2)?;
        identities.sort_unstable();
        poll_cancellation(control, observer, "canonicalize", 1, 2)?;
        identities.dedup();
        poll_cancellation(control, observer, "canonicalize", 2, 2)?;

        poll_cancellation(control, observer, "hash", 0, 1)?;
        let store_peak_bytes =
            checked_store_construction_peak_bytes(&identities, identities.capacity())?;
        let store_memory_admitted = ensure_materialization_memory(
            problem,
            packing,
            store_peak_bytes,
            canonical_tiling_objective,
        )?;
        let page_store = Arc::new(
            TilingSolutionPageStore::from_standard_identities(
                problem.initial_board().occupied_mask(),
                identities,
            )
            .map_err(PcTilingMaterializationError::PageStore)?,
        );
        poll_cancellation(control, observer, "hash", 1, 1)?;

        let initial_page_count = page_store.len().min(PC_TILING_INITIAL_PAGE_LIMIT);
        let page_future_bytes = checked_initial_page_peak_bytes(&page_store, initial_page_count)?;
        let page_memory_admitted = ensure_materialization_memory(
            problem,
            packing,
            page_future_bytes,
            canonical_tiling_objective,
        )?;
        let initial_page_identities = page_store
            .page_identities(0, initial_page_count)
            .map_err(PcTilingMaterializationError::PageStore)?;
        let mut initial_page_keys = Vec::new();
        poll_cancellation(control, observer, "keys", 0, initial_page_count)?;
        initial_page_keys
            .try_reserve_exact(initial_page_count)
            .map_err(|_| PcTilingMaterializationError::AllocationFailed)?;
        for (identity_index, identity) in initial_page_identities.into_iter().enumerate() {
            if identity_index != 0 && identity_index % poll_stride == 0 {
                poll_cancellation(
                    control,
                    observer,
                    "keys",
                    identity_index,
                    initial_page_count,
                )?;
            }
            initial_page_keys.push(
                NormalizedTilingSolutionKey::from_standard_board64_identity(identity).to_string(),
            );
        }
        poll_cancellation(
            control,
            observer,
            "keys",
            initial_page_count,
            initial_page_count,
        )?;

        let packing_source_raw_geometry = packing.candidate_provenance().is_raw_geometry();
        let memory_admission_accounted =
            identity_memory_admitted && store_memory_admitted && page_memory_admitted;
        let incomplete_reason = if !canonical_tiling_objective {
            "noncanonical-tiling-objective"
        } else if !packing_source_raw_geometry {
            "packing-source-not-raw-geometry"
        } else if let Some(reason) = packing.truncation_reason() {
            reason.as_str()
        } else if !problem.piece_source().complete() {
            "piece-source-incomplete"
        } else {
            "none"
        };
        let complete = packing_source_raw_geometry
            && canonical_tiling_objective
            && memory_admission_accounted
            && packing.count_complete()
            && problem.piece_source().complete();

        Ok(Self {
            initial_page_keys,
            page_store,
            packing_source_raw_geometry,
            canonical_tiling_objective,
            memory_admission_accounted,
            complete,
            incomplete_reason,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_packing_with_poll_observer_for_test(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
        poll_stride: usize,
        observer: &mut impl FnMut(&'static str, usize, usize),
    ) -> Result<Self, PcTilingMaterializationError> {
        Self::from_packing_with_poll(problem, packing, control, poll_stride, observer)
    }

    #[cfg(test)]
    pub(crate) fn from_packing_with_production_poll_observer_for_test(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
        observer: &mut impl FnMut(&'static str, usize, usize),
    ) -> Result<Self, PcTilingMaterializationError> {
        Self::from_packing_with_production_poll(problem, packing, control, observer)
    }

    pub(crate) fn normalized_hash(&self) -> &str {
        self.page_store.normalized_hash()
    }

    pub(crate) fn normalized_solution_count(&self) -> usize {
        self.page_store.len()
    }

    pub(crate) fn initial_page_count(&self) -> usize {
        self.initial_page_keys.len()
    }

    pub(crate) fn initial_page_covers_family(&self) -> bool {
        self.initial_page_count() == self.normalized_solution_count()
    }

    pub(crate) fn solution_page_available(&self) -> bool {
        self.initial_page_count() < self.normalized_solution_count()
    }

    pub(crate) const fn packing_source_raw_geometry(&self) -> bool {
        self.packing_source_raw_geometry
    }

    pub(crate) const fn canonical_tiling_objective(&self) -> bool {
        self.canonical_tiling_objective
    }

    pub(crate) const fn memory_admission_accounted(&self) -> bool {
        self.memory_admission_accounted
    }

    pub(crate) const fn native_internal_memory_evidence_authorized(&self) -> bool {
        self.memory_admission_accounted && self.canonical_tiling_objective && self.complete
    }

    pub(crate) const fn complete(&self) -> bool {
        self.complete
    }

    pub(crate) const fn incomplete_reason(&self) -> &'static str {
        self.incomplete_reason
    }

    pub(crate) const fn normalized_key_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM
    }

    pub(crate) const fn normalized_hash_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM
    }

    pub(crate) fn into_result_parts(self) -> (Vec<String>, Arc<TilingSolutionPageStore>) {
        (self.initial_page_keys, self.page_store)
    }
}

fn ensure_materialization_memory(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    future_bytes: u128,
    admission_required: bool,
) -> Result<bool, PcTilingMaterializationError> {
    let Some(bound) = packing.execution_memory_bound() else {
        return if admission_required || problem.backend_request().max_memory_mib().is_some() {
            Err(PcTilingMaterializationError::MemoryAccountingUnavailable)
        } else {
            Ok(false)
        };
    };
    let retained = packing
        .checked_retained_execution_bytes()
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)?;
    bound
        .ensure(retained, future_bytes)
        .map_err(PcTilingMaterializationError::ResourceIncomplete)?;
    Ok(true)
}

fn checked_identity_bytes(count: usize) -> Result<u128, PcTilingMaterializationError> {
    (count as u128)
        .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)
}

fn checked_store_construction_peak_bytes(
    identities: &[StandardBoard64TilingIdentity],
    identity_capacity: usize,
) -> Result<u128, PcTilingMaterializationError> {
    let identity_bytes = checked_identity_bytes(identity_capacity)?;
    let placement_count = identities
        .iter()
        .try_fold(0_usize, |total, identity| {
            total.checked_add(identity.placement_count())
        })
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)?;
    let run_count = usize::from(!identities.is_empty());
    let canonical_store_peak =
        TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
            placement_count,
            identities.len(),
            run_count,
        )
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)?;
    identity_bytes
        .checked_add(canonical_store_peak)
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)
}

fn checked_initial_page_peak_bytes(
    store: &TilingSolutionPageStore,
    page_count: usize,
) -> Result<u128, PcTilingMaterializationError> {
    const TRANSIENT_NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND: u128 =
        42 + (clearra_core_domain::solution::STANDARD_BOARD64_TILING_MAX_PLACEMENTS as u128) * 20;
    let retained_store = store
        .checked_owned_graph_retained_bytes()
        .and_then(|bytes| {
            bytes.checked_add((core::mem::size_of::<usize>() as u128).checked_mul(2)?)
        })
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)?;
    let page_build_peak =
        TilingSolutionPageStore::checked_initial_page_build_peak_upper_bound(page_count)
            .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)?;
    retained_store
        .checked_add(page_build_peak)
        .and_then(|bytes| {
            bytes.checked_add(if page_count == 0 {
                0
            } else {
                TRANSIENT_NORMALIZED_KEY_BACKING_BYTES_UPPER_BOUND
            })
        })
        .ok_or(PcTilingMaterializationError::MemoryAccountingUnavailable)
}

fn poll_cancellation(
    control: &ExecutionControl,
    observer: &mut impl FnMut(&'static str, usize, usize),
    stage: &'static str,
    completed: usize,
    total: usize,
) -> Result<(), PcTilingMaterializationError> {
    observer(stage, completed, total);
    if control.is_cancelled() {
        Err(PcTilingMaterializationError::ExecutionCancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };

    use crate::tiling_solution_store::{PackedTilingRows, TilingSolutionPageStore};

    use super::{
        checked_identity_bytes, checked_initial_page_peak_bytes,
        checked_store_construction_peak_bytes,
    };

    fn unique_single_placement_identities(count: usize) -> Vec<StandardBoard64TilingIdentity> {
        let mut identities = Vec::with_capacity(count);
        'masks: for first in 0..64 {
            for second in first + 1..64 {
                for third in second + 1..64 {
                    for fourth in third + 1..64 {
                        let mask = (1_u64 << first)
                            | (1_u64 << second)
                            | (1_u64 << third)
                            | (1_u64 << fourth);
                        identities.push(
                            StandardBoard64TilingIdentity::from_placements(
                                0,
                                [PiecePlacementMask::new(PieceKind::I, mask)],
                            )
                            .expect("four-cell placement identity"),
                        );
                        if identities.len() == count {
                            break 'masks;
                        }
                    }
                }
            }
        }
        assert_eq!(identities.len(), count);
        identities
    }

    #[test]
    fn native_store_projection_includes_standard_input_and_radix_construction_peak() {
        const ABOVE_RADIX_THRESHOLD: usize = 1_025;
        let identities = unique_single_placement_identities(ABOVE_RADIX_THRESHOLD);
        let projected = checked_store_construction_peak_bytes(&identities, identities.capacity())
            .expect("native store construction projection");
        let expected = checked_identity_bytes(identities.capacity())
            .expect("standard identity backing")
            .checked_add(
                TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
                    identities.len(),
                    identities.len(),
                    1,
                )
                .expect("canonical store construction peak"),
            )
            .expect("combined native construction peak");
        assert_eq!(projected, expected);

        let below_radix = TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
            identities.len(),
            ABOVE_RADIX_THRESHOLD - 1,
            1,
        )
        .expect("below-radix construction peak");
        let above_radix = TilingSolutionPageStore::checked_canonical_construction_peak_upper_bound(
            identities.len(),
            ABOVE_RADIX_THRESHOLD,
            1,
        )
        .expect("above-radix construction peak");
        let one_packed_identity_and_index =
            core::mem::size_of::<PackedTilingRows>() as u128 + core::mem::size_of::<u64>() as u128;
        assert!(above_radix - below_radix > one_packed_identity_and_index);
    }

    #[test]
    fn native_initial_page_projection_keeps_store_arc_and_shared_page_peak_live() {
        let identity = unique_single_placement_identities(1)[0];
        let store = TilingSolutionPageStore::from_standard_identities(0, vec![identity])
            .expect("one-identity store");
        let projected =
            checked_initial_page_peak_bytes(&store, 1).expect("initial page projection");
        let expected = store
            .checked_owned_graph_retained_bytes()
            .expect("retained store")
            .checked_add((core::mem::size_of::<usize>() as u128) * 2)
            .and_then(|bytes| {
                bytes.checked_add(
                    TilingSolutionPageStore::checked_initial_page_build_peak_upper_bound(1)
                        .expect("initial page build peak"),
                )
            })
            .and_then(|bytes| bytes.checked_add(42 + 16 * 20))
            .expect("complete initial page peak");
        assert_eq!(projected, expected);
    }
}
