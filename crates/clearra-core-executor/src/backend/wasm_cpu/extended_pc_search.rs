//! Full-height PC execution through the shared ILC/BuildUp/language primitives.
//! The typed minimum producer retains coverage proof for the common reducer;
//! score and replay authorities are not interchangeable with that family.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{
    BuildProbabilityField, PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem,
    SearchProblemPreset,
};

use super::{
    build_probability::BuildProbabilityAdvance,
    extended_build_probability::ExtendedBuildProbabilitySession, ExactSearchAdvance,
    WasmExactSearchError,
};
use crate::{
    resource::{
        admit_budget_bound_search_execution,
        admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission,
        WasmCpuTerminalResourceAuthority,
    },
    CoreExecutionResult,
};

pub(crate) struct ExtendedPcSearchSession {
    owner: ExtendedPcSearchOwner,
    terminal_authority_supplied: bool,
}

struct ExtendedPcDirectOwner {
    engine: ExtendedBuildProbabilitySession,
    admission: ExecutionAdmission,
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    original_lease: clearra_core_domain::resource::ResourceLeaseToken,
}

#[allow(clippy::large_enum_variant)]
enum ExtendedPcSearchOwner {
    Direct(ExtendedPcDirectOwner),
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    NativeCooperative(Box<native_cooperative::Driver>),
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
pub use native_cooperative::{NativePcCooperativeAdvance, NativePcCooperativeResult};

pub(super) fn validate_pc_family_problem(
    problem: &SearchProblem,
) -> Result<(), WasmExactSearchError> {
    let ordinary_source = problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::Disabled
        && !problem.objective().score().requested()
        && matches!(
            problem.objective().kind(),
            ObjectiveKind::All | ObjectiveKind::Unique
        );
    let minimum_source = problem.pc_chance_evidence_policy()
        == PcChanceEvidencePolicy::PcMinimumCoverV2
        && problem.objective().kind() == ObjectiveKind::MinimumCover
        && matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        && problem.output_policy() == SearchOutputPolicy::Trace;
    let chance_source = problem.pc_chance_evidence_policy()
        == PcChanceEvidencePolicy::PcProbabilityV2
        && problem.objective().kind() == ObjectiveKind::Unique
        && problem.count_policy() == PcCountPolicy::CountUnique
        && problem.output_policy() == SearchOutputPolicy::CoverageSummary
        && !problem.solution_probability_policy().requested();
    // The canonical failed-queue command deliberately retains its existing
    // All + CountAll input contract. Its complement proof is purpose-separated
    // from chance; do not normalize the user's query into a different source.
    let failed_source = problem
        .pc_chance_evidence_policy()
        .pc_failed_queue_example_limit()
        .is_some()
        && matches!(
            (problem.objective().kind(), problem.count_policy()),
            (ObjectiveKind::All, PcCountPolicy::CountAll)
                | (ObjectiveKind::Unique, PcCountPolicy::CountUnique)
        )
        && problem.output_policy() == SearchOutputPolicy::CoverageSummary
        && !problem.solution_probability_policy().requested();
    let score_source = problem.objective().score().requested()
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && match problem.pc_chance_evidence_policy() {
            PcChanceEvidencePolicy::Disabled => problem.objective().kind() == ObjectiveKind::All,
            PcChanceEvidencePolicy::PcScorePortfolioV2 => {
                problem.objective().kind() == ObjectiveKind::MinimumCover
            }
            _ => false,
        };
    let path_source = problem
        .pc_chance_evidence_policy()
        .retains_pc_path_v2_evidence()
        && problem.objective().kind() == ObjectiveKind::All
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && !problem.objective().score().requested();
    // Match the compact engine's ordinary observation-policy family. Typed
    // minimum/chance/score/replay products keep their separate oracle proof
    // contracts; neither a local worker coverage nor an oracle union can stand
    // in for a VisibleSeven policy over the complete candidate language.
    let observation_source =
        ordinary_source && problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::Disabled;
    if !matches!(
        problem.preset(),
        SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc
    ) || (problem.preset() == SearchProblemPreset::OpeningPc
        && problem.initial_board().occupied_words() != [0; 4])
        || problem.goal().as_str() != "clear-to-empty"
        || problem.initial_board().width() != 10
        || !(7..=24).contains(&problem.visible_height())
        || !(ordinary_source
            || minimum_source
            || chance_source
            || failed_source
            || score_source
            || path_source)
        || !matches!(
            problem.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        || !(matches!(
            problem.output_policy(),
            SearchOutputPolicy::Summary | SearchOutputPolicy::Trace
        ) || chance_source
            || failed_source)
        || (problem.objective().score().requested() && !score_source)
        || problem.allowed_colored_solution_identities().is_some()
        || (problem
            .queue_observation_policy()
            .requires_observation_policy()
            && !observation_source)
        || problem.backend_policy().tablebase_requested()
        || problem.backend_policy().precompute_build_dependencies()
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "extended_pc_family_contract_not_connected",
        ));
    }
    super::ensure_connected_kick_profile(problem)
}

pub(super) fn checked_pc_family_problem_nested_retained_bytes(
    problem: &SearchProblem,
) -> Option<u128> {
    // Constructor validation owns execution compatibility. This projection
    // only counts owners and does not revalidate kicks in the solver loop.
    let pointee = if problem.output_policy() == SearchOutputPolicy::TilingOnly {
        problem.checked_pc_tiling_pointee_retained_bytes()?
    } else if problem.objective().score().requested() {
        problem.checked_pc_score_pointee_retained_bytes()?
    } else {
        problem.checked_pc_family_pointee_retained_bytes()?
    };
    pointee.checked_sub(core::mem::size_of::<SearchProblem>() as u128)
}

impl ExtendedPcSearchSession {
    pub(super) fn failed_queue_memory_bound(&self) -> crate::resource::ExecutionMemoryBound {
        self.direct_owner()
            .expect("typed failed queues are not in the ordinary cooperative cohort")
            .admission
            .memory_bound()
    }

    pub(super) fn checked_failed_queue_retained_bytes(&self) -> Option<u128> {
        self.direct_owner()?
            .engine
            .checked_retained_bytes_with_coexisting_owners()
    }

    pub fn new(problem: &SearchProblem) -> Result<Self, WasmExactSearchError> {
        Self::new_with_coexisting_retained_bytes(problem, 0)
    }

    pub(super) fn new_with_coexisting_retained_bytes(
        problem: &SearchProblem,
        additional_coexisting_retained_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_with_admission(problem, additional_coexisting_retained_bytes, None)
    }

    pub(crate) fn new_under_authority(
        problem: &SearchProblem,
        external_retained_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmExactSearchError> {
        if !problem.objective().score().requested()
            && !problem
                .pc_chance_evidence_policy()
                .retains_pc_path_v2_evidence()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_terminal_authority_requires_score_or_path",
            ));
        }
        Self::new_with_admission(problem, external_retained_bytes, Some(authority))
    }

    fn new_with_admission(
        problem: &SearchProblem,
        additional_coexisting_retained_bytes: u128,
        authority: Option<&WasmCpuTerminalResourceAuthority>,
    ) -> Result<Self, WasmExactSearchError> {
        let field = Self::validated_family_field(problem)?;
        let direct = Self::admit_direct_owner(
            problem,
            field,
            additional_coexisting_retained_bytes,
            authority,
        )?;
        Ok(Self {
            owner: ExtendedPcSearchOwner::Direct(direct),
            terminal_authority_supplied: authority.is_some(),
        })
    }

    fn validated_family_field(
        problem: &SearchProblem,
    ) -> Result<BuildProbabilityField, WasmExactSearchError> {
        validate_pc_family_problem(problem)?;
        // Do not silently turn a multiworker product into a serial search.
        if problem.backend_policy().workers() != 1
            && !cfg!(all(feature = "parallel", not(target_family = "wasm")))
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parallel_not_connected",
            ));
        }
        // The engine owns the shared Geometry/BuildUp node debit. A requested
        // multiworker execution must not be downgraded for an explicit cap.
        let height = problem.visible_height() as u8;
        let initial = Board256Mask::from_words(problem.initial_board().occupied_words());
        let full = Board256Mask::all_cells(u16::from(height) * 10)
            .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_height_invalid"))?;
        let required = full.without(initial);
        if required.is_empty()
            || !required.count_ones().is_multiple_of(4)
            || problem.exact_pieces() != Some(required.count_ones() as usize / 4)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_area_invalid",
            ));
        }
        BuildProbabilityField::from_words_preserving_height(
            height,
            initial.words(),
            required.words(),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("extended_pc_field_invalid"))
    }

    fn admit_direct_owner(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        additional_coexisting_retained_bytes: u128,
        authority: Option<&WasmCpuTerminalResourceAuthority>,
    ) -> Result<ExtendedPcDirectOwner, WasmExactSearchError> {
        let admission = if let Some(authority) = authority {
            admit_budget_bound_search_execution_under_terminal_authority(
                problem,
                additional_coexisting_retained_bytes,
                authority,
                problem.backend_policy().workers(),
            )
        } else {
            admit_budget_bound_search_execution(problem, problem.backend_policy().workers())
        }
        .map_err(WasmExactSearchError::resource_admission)?;
        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        let original_lease = admission.lease_token();
        // The legacy ordinary execution API borrows a caller-owned problem;
        // both that input and the engine's owned snapshot coexist.
        let external = checked_pc_family_problem_nested_retained_bytes(problem)
            .and_then(|bytes| bytes.checked_add(additional_coexisting_retained_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    core::mem::size_of::<Self>() as u128
                        + core::mem::size_of::<SearchProblem>() as u128,
                )
            })
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        let engine = ExtendedBuildProbabilitySession::new_pc_family(
            problem,
            field,
            admission.memory_bound(),
            external,
        )?;
        Ok(ExtendedPcDirectOwner {
            engine,
            admission,
            #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
            original_lease,
        })
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub(crate) fn try_new_native_cooperative(
        problem: &SearchProblem,
        checked_external_retained_bytes: u128,
    ) -> Result<Option<Self>, WasmExactSearchError> {
        if !native_cooperative::Driver::is_problem_eligible(problem) {
            return Ok(None);
        }
        let field = Self::validated_family_field(problem)?;
        let external = checked_external_retained_bytes
            .checked_add(core::mem::size_of::<native_cooperative::Driver>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        let direct = Self::admit_direct_owner(problem, field, external, None)?;
        Ok(Some(Self {
            owner: ExtendedPcSearchOwner::NativeCooperative(Box::new(
                native_cooperative::Driver::new(direct)?,
            )),
            terminal_authority_supplied: false,
        }))
    }

    #[cfg(all(test, feature = "parallel", not(target_family = "wasm")))]
    pub(crate) fn native_cooperative_leases_for_test(
        &self,
    ) -> Option<(
        clearra_core_domain::resource::ResourceLeaseToken,
        Option<clearra_core_domain::resource::ResourceLeaseToken>,
    )> {
        let ExtendedPcSearchOwner::NativeCooperative(owner) = &self.owner else {
            return None;
        };
        Some((owner.original_lease(), owner.publication_lease()))
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub(crate) fn advance_native_cooperative(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<NativePcCooperativeAdvance, WasmExactSearchError> {
        match &mut self.owner {
            ExtendedPcSearchOwner::NativeCooperative(owner) => owner.advance(budget, control),
            ExtendedPcSearchOwner::Direct(_) => Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_native_cooperative_explicit_opt_in_required",
            )),
        }
    }

    fn direct_owner(&self) -> Option<&ExtendedPcDirectOwner> {
        match &self.owner {
            ExtendedPcSearchOwner::Direct(owner) => Some(owner),
            #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
            ExtendedPcSearchOwner::NativeCooperative(_) => None,
        }
    }

    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        match &mut self.owner {
            ExtendedPcSearchOwner::Direct(owner) => owner.advance(budget, control),
            #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
            ExtendedPcSearchOwner::NativeCooperative(_) => {
                Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_native_cooperative_requires_leased_result_advance",
                ))
            }
        }
    }

    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub fn execute_parallel_if_worthwhile(
        &mut self,
        requested_workers: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmExactSearchError> {
        match &mut self.owner {
            ExtendedPcSearchOwner::Direct(owner) => owner
                .engine
                .execute_pc_family_parallel(requested_workers, control),
            ExtendedPcSearchOwner::NativeCooperative(_) => {
                Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_native_cooperative_requires_bounded_advance",
                ))
            }
        }
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        future: u128,
    ) -> Result<(), WasmExactSearchError> {
        // This ordinary borrowed-input path never manufactures the parent
        // authority required by typed score/Tiling terminal callbacks.
        let owner = self
            .direct_owner()
            .filter(|_| self.terminal_authority_supplied)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_family_parent_authority_not_supplied",
            ))?;
        let retained = owner
            .engine
            .checked_retained_bytes_with_coexisting_owners()
            .and_then(|bytes| bytes.checked_add(result.checked_resource_retained_bytes()?))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_memory_projection_unavailable",
            ))?;
        owner
            .admission
            .ensure_memory_bound(retained, future)
            .map_err(WasmExactSearchError::resource_admission)
    }
}

impl ExtendedPcDirectOwner {
    fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<ExactSearchAdvance, WasmExactSearchError> {
        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        {
            let requested_workers = self.engine.requested_pc_workers();
            if requested_workers > 1 {
                if let Some(result) = self
                    .engine
                    .execute_pc_family_parallel(requested_workers, control)?
                {
                    return Ok(ExactSearchAdvance::Completed(result));
                }
            }
        }
        advance_serial(&mut self.engine, budget, control)
    }
}

fn advance_serial(
    engine: &mut ExtendedBuildProbabilitySession,
    budget: usize,
    control: &ExecutionControl,
) -> Result<ExactSearchAdvance, WasmExactSearchError> {
    let advance = engine.advance(budget, control)?;
    let progress = engine.distributed_progress();
    control.report_progress("geometry", progress.geometry_nodes as u64, None);
    control.report_progress("buildup", progress.build_nodes as u64, None);
    Ok(match advance {
        BuildProbabilityAdvance::Pending => ExactSearchAdvance::Pending,
        BuildProbabilityAdvance::Completed(result) => ExactSearchAdvance::Completed(result),
        BuildProbabilityAdvance::Cancelled => ExactSearchAdvance::Cancelled,
    })
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
mod native_cooperative {
    use clearra_core_domain::resource::ResourceLeaseToken;

    use super::*;
    use crate::backend::wasm_cpu::extended_build_probability::{
        DryRunOwner, ParallelTerminalRetention, SerialHandoff,
    };

    #[allow(clippy::large_enum_variant)]
    enum State {
        Parallel(DryRunOwner),
        Serial(SerialHandoff),
        Published(ResourceLeaseToken),
        Stopped,
    }

    #[allow(clippy::large_enum_variant)]
    pub enum NativePcCooperativeAdvance {
        Pending,
        Completed(NativePcCooperativeResult),
        Cancelled,
    }

    enum PublicationRetention {
        Parallel(ParallelTerminalRetention),
        Serial(ExecutionAdmission),
    }

    /// A complete native result together with its original admission.
    /// Terminal owners consume the result in a callback while that same
    /// admission is still live; no request is readmitted.
    pub struct NativePcCooperativeResult {
        result: CoreExecutionResult,
        retention: PublicationRetention,
    }

    impl NativePcCooperativeResult {
        #[cfg(test)]
        pub(crate) fn result(&self) -> &CoreExecutionResult {
            &self.result
        }

        pub(crate) fn lease_token(&self) -> ResourceLeaseToken {
            match &self.retention {
                PublicationRetention::Parallel(retention) => retention.lease_token(),
                PublicationRetention::Serial(admission) => admission.lease_token(),
            }
        }

        /// Move the completed result through its terminal transformation.
        /// The original execution lease remains held for the whole callback.
        pub fn finish<R>(self, terminal: impl FnOnce(CoreExecutionResult) -> R) -> R {
            let Self { result, retention } = self;
            let output = terminal(result);
            drop(retention);
            output
        }
    }

    pub(super) struct Driver {
        state: State,
        original_lease: ResourceLeaseToken,
    }

    impl Driver {
        pub(super) fn is_problem_eligible(problem: &SearchProblem) -> bool {
            DryRunOwner::is_problem_eligible(problem)
        }

        pub(super) fn new(direct: ExtendedPcDirectOwner) -> Result<Self, WasmExactSearchError> {
            let original_lease = direct.original_lease;
            Ok(Self {
                state: State::Parallel(DryRunOwner::from_admitted(
                    direct.engine,
                    direct.admission,
                    original_lease,
                )?),
                original_lease,
            })
        }

        pub(super) fn publication_lease(&self) -> Option<ResourceLeaseToken> {
            match &self.state {
                State::Published(lease) => Some(*lease),
                _ => None,
            }
        }

        #[cfg(test)]
        pub(super) fn original_lease(&self) -> ResourceLeaseToken {
            self.original_lease
        }

        pub(super) fn advance(
            &mut self,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<NativePcCooperativeAdvance, WasmExactSearchError> {
            if self.publication_lease().is_some() || matches!(&self.state, State::Stopped) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_native_cooperative_session_already_finished",
                ));
            }
            if control.is_cancelled() {
                self.state = State::Stopped;
                return Ok(NativePcCooperativeAdvance::Cancelled);
            }
            if budget == 0 {
                return Ok(NativePcCooperativeAdvance::Pending);
            }
            let result = self.advance_admitted(budget, control);
            match result {
                Err(WasmExactSearchError::Cancelled) => Ok(NativePcCooperativeAdvance::Cancelled),
                other => other,
            }
        }

        fn advance_admitted(
            &mut self,
            budget: usize,
            control: &ExecutionControl,
        ) -> Result<NativePcCooperativeAdvance, WasmExactSearchError> {
            // Taking the current owner ensures a failed or canceled step
            // cannot reuse a partially consumed admission or retry another
            // route after it has performed work.
            match core::mem::replace(&mut self.state, State::Stopped) {
                State::Parallel(mut owner) => {
                    let (ready, used) = owner.advance(budget, control)?;
                    if control.is_cancelled() {
                        return Err(WasmExactSearchError::Cancelled);
                    }
                    let fits_publication = budget
                        .checked_sub(used)
                        .ok_or_else(lease_or_transfer_error)?
                        >= 1;
                    if !ready || !fits_publication {
                        self.state = State::Parallel(owner);
                        return Ok(NativePcCooperativeAdvance::Pending);
                    }
                    // The final move-out or the no-Geometry handoff consumes
                    // one unit. The handoff advances its existing serial
                    // engine only on a subsequent poll, never recursively.
                    if let Some(actual) = owner.serial_handoff_pending() {
                        if actual != self.original_lease {
                            return Err(lease_or_transfer_error());
                        }
                        let handoff = owner.into_serial_handoff(control)?;
                        if handoff.admission.lease_token() != self.original_lease {
                            return Err(lease_or_transfer_error());
                        }
                        self.state = State::Serial(handoff);
                        return Ok(NativePcCooperativeAdvance::Pending);
                    }
                    if owner.parallel_result_pending() != Some(self.original_lease) {
                        return Err(lease_or_transfer_error());
                    }
                    let (result, retention) = owner
                        .into_parallel_result(control)?
                        .into_result_and_retention();
                    self.publish(result, PublicationRetention::Parallel(retention), control)
                }
                State::Serial(mut handoff) => {
                    let advance = advance_serial(&mut handoff.engine, budget, control)?;
                    if control.is_cancelled() {
                        return Err(WasmExactSearchError::Cancelled);
                    }
                    match advance {
                        ExactSearchAdvance::Pending => {
                            self.state = State::Serial(handoff);
                            Ok(NativePcCooperativeAdvance::Pending)
                        }
                        ExactSearchAdvance::Cancelled => Ok(NativePcCooperativeAdvance::Cancelled),
                        ExactSearchAdvance::Completed(result) => self.publish(
                            result,
                            PublicationRetention::Serial(handoff.admission),
                            control,
                        ),
                    }
                }
                State::Published(_) | State::Stopped => Err(lease_or_transfer_error()),
            }
        }

        fn publish(
            &mut self,
            result: CoreExecutionResult,
            retention: PublicationRetention,
            control: &ExecutionControl,
        ) -> Result<NativePcCooperativeAdvance, WasmExactSearchError> {
            // Keep the result and its admission in one value before any
            // checks which can reject publication and drop the value.
            let publication = NativePcCooperativeResult { result, retention };
            let lease = publication.lease_token();
            if lease != self.original_lease {
                return Err(lease_or_transfer_error());
            }
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            self.state = State::Published(lease);
            Ok(NativePcCooperativeAdvance::Completed(publication))
        }
    }

    fn lease_or_transfer_error() -> WasmExactSearchError {
        WasmExactSearchError::InvalidProblem("extended_pc_native_cooperative_transfer_invalid")
    }
}
