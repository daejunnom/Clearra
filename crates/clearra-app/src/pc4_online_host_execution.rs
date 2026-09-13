//! SRP: host-driven online execution of the ordinary typed PC request.
//! CLI compilation, supply semantics, candidate sealing and product reduction
//! remain their existing owners. This module performs no HTTP or fallback.
use crate::*;
use clearra_core_domain::board::standard_pc_board::StandardPcBoard;
use clearra_pc4_tablebase::*;
use std::{
    num::{NonZeroU16, NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
};

pub struct Pc4OnlineHostExecution {
    context: AppContext,
    request: AppRequest,
    generation: PinnedPc4Generation,
    source: PcCandidateSourceBinding,
    prepared: Pc4PreparedOnlineInput,
    board: StandardPcBoard,
    hold: FixedQueueHoldState,
    lookup: Option<AppOnlinePc4LookupSession>,
    candidates: Option<AppOnlinePc4CandidateSession>,
    product: Option<Pc4CandidateProductExecution>,
    pending: Option<RangeRequest>,
    ordinal: u32,
    lookup_id: Option<LookupSessionId>,
}

impl AppContext {
    pub fn start_online_pc4_execution(
        &self,
        request: AppRequest,
        snapshot: ActivatedSnapshot,
    ) -> Result<Pc4OnlineHostExecution, &'static str> {
        if !matches!(
            request.command(),
            AppCommand::Pc(_) | AppCommand::Scenario(_)
        ) {
            return Err("pc4_online_product_not_supported");
        }
        if request.resource_budget().memory_mib().is_some()
            || request.resource_budget().max_memory_mib().is_some()
        {
            return Err("pc4_online_finite_memory_handoff_required");
        }
        let compiled = match self.prepare_distributed_search(request.clone()) {
            DistributedSearchPreparation::Search(search) => search,
            _ => return Err("pc4_online_request_rejected"),
        };
        let problem = compiled.problem_arc();
        let profile = Pc4RuleProfile::ALL
            .into_iter()
            .find(|profile| validate_pc4_search_problem_compatibility(*profile, &problem).is_ok())
            .ok_or("pc4_online_rule_not_supported")?;
        let lines = u8::try_from(problem.initial_board().visible_height())
            .map_err(|_| "pc4_online_target_not_supported")?;
        let target = snapshot
            .qualified_target(
                profile,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(lines).map_err(|_| "pc4_online_target_not_supported")?,
            )
            .map_err(|_| "pc4_online_profile_or_target_unavailable")?;
        let board =
            StandardPcBoard::from_words(lines, [problem.initial_board().occupied_mask(), 0, 0, 0])
                .map_err(|_| "pc4_online_initial_board_invalid")?;
        let hold = crate::pc_candidate_execution_bridge::fixed_queue_hold_state(
            problem.core_query().allow_hold(),
            problem.core_query().hold_state(),
        );
        let mut preparation = Pc4CompiledPatternPreparation::begin(
            problem,
            Pc4CompiledPatternLimits::new(nz(5_000_000), nz(11), nz(256)),
        )
        .map_err(|e| e.reason())?;
        // Compact patterns are structurally certified without unranking. The
        // host entry supports that compact representation; explicit large
        // sources must use cooperative preparation rather than block a worker.
        preparation
            .advance(nz(256), &|| false)
            .map_err(|e| e.reason())?;
        if !preparation.is_complete() {
            return Err("pc4_online_explicit_pattern_preparation_required");
        }
        let pattern = preparation.finish().map_err(|e| e.reason())?;
        let input_identity = *pattern.identity().as_bytes();
        let prepared = Pc4PreparedOnlineInput::for_compiled_pattern(
            target.clone(),
            Pc4InputSurface::Gui,
            pattern,
        )
        .map_err(|e| e.reason())?;
        let source = PcCandidateSourceBinding::online_pc4_for_prepared_input(
            PcCandidateSessionId::new(NonZeroU64::new(1).unwrap()),
            PcCandidateSourceIdentity::from_sha256(input_identity),
            &prepared,
            board,
            hold,
        )
        .map_err(|_| "pc4_online_source_binding_failed")?;
        let generation = pin(snapshot)?;
        let hash = clearra_board64_mask_to_hydra_field_hash_v1(
            board.occupied().compact_board64().unwrap(),
        )
        .map_err(|_| "pc4_online_initial_board_invalid")?;
        let lookup = AppOnlinePc4LookupSession::start(
            generation.clone(),
            Pc4OnlineLookupRequest::new(
                LookupSessionId::new(1).unwrap(),
                target,
                hash,
                range_limits(),
                Pc4OfflineFallbackAuthorization::NotAuthorized,
            ),
        )
        .map_err(|e| e.reason())?;
        Ok(Pc4OnlineHostExecution {
            context: self.clone(),
            request,
            generation,
            source,
            prepared,
            board,
            hold,
            lookup: Some(lookup),
            candidates: None,
            product: None,
            pending: None,
            ordinal: 0,
            lookup_id: None,
        })
    }
}

impl Pc4OnlineHostExecution {
    pub fn pending_range(&self) -> Option<&RangeRequest> {
        self.pending.as_ref()
    }

    pub fn advance(
        &mut self,
        work: usize,
        control: &ExecutionControl,
    ) -> Result<CooperativeAppAdvance, &'static str> {
        let guard = HostGuard {
            source: &self.source,
            control,
        };
        if control.is_cancelled() {
            self.pending = None;
            return Ok(CooperativeAppAdvance::Cancelled);
        }
        if self.pending.is_some() {
            return Ok(CooperativeAppAdvance::Pending);
        }
        if let Some(product) = &mut self.product {
            return product
                .advance(work, &guard, control)
                .map_err(|e| e.reason());
        }
        if let Some(lookup) = &self.lookup {
            match lookup.step() {
                AppOnlinePc4LookupStep::NeedRange(range) => self.pending = Some(range),
                AppOnlinePc4LookupStep::Hit(hit) => {
                    let id = hit.lookup().field_id;
                    let request = AppOnlinePc4ObservationCandidateRequest::for_prepared_input(
                        &self.source,
                        &self.prepared,
                        self.board,
                        self.hold,
                        LookupSessionId::new(2).unwrap(),
                        id,
                        frontier_budgets(),
                        TerminalDepthContract::QueueExhaustedOnly,
                        FixedQueueTraversalBudgets::new(
                            nz(50_000_000),
                            nz(100_000),
                            nz(10),
                            nz(5_000_000),
                        ),
                        FixedQueueTraversalPageBudgets::new(nz(32), nz(32)),
                        Pc4ObservationGraphBudgets::new(
                            nz(1_000_000_000),
                            nz(1_000_000_000),
                            nz(1_000_000_000),
                            nz(5_000_000),
                            nz(8),
                            nz(8),
                            nz(8),
                        ),
                        ConcretePathMaterializationBudgets::new(
                            nz(10),
                            nz(256),
                            nz(100_000),
                            nz(32),
                        ),
                        Pc4ObservationCandidateBudgets::new(
                            nz(8),
                            nz(8),
                            nz(32),
                            nz(5_000_000),
                            nz(5_000_000),
                            nz(5_000_000),
                            nz(16_000_000),
                        ),
                        Pc4LookupGraphCacheLimits::new(
                            nz(100_000),
                            nz(32 * 1024 * 1024),
                            nz(64 * 1024 * 1024),
                        ),
                        nz(8),
                        range_limits(),
                        &guard,
                    )
                    .map_err(|e| e.reason())?;
                    self.candidates = Some(
                        AppOnlinePc4CandidateSession::start_observation(
                            self.generation.clone(),
                            request,
                            &guard,
                        )
                        .map_err(|e| e.reason())?,
                    );
                    self.lookup = None;
                }
                AppOnlinePc4LookupStep::Miss => return Err("pc4_online_field_miss"),
                AppOnlinePc4LookupStep::Failed(e) => return Err(e.reason()),
                AppOnlinePc4LookupStep::Cancelled => return Ok(CooperativeAppAdvance::Cancelled),
            }
            return Ok(CooperativeAppAdvance::Pending);
        }
        let session = self
            .candidates
            .as_mut()
            .ok_or("pc4_online_execution_state")?;
        match session.step(&guard) {
            AppOnlinePc4CandidateStep::NeedRange(range) => {
                self.pending = Some(range);
            }
            AppOnlinePc4CandidateStep::Complete { .. } => {
                let input = session
                    .completed_reducer_input()
                    .ok_or("pc4_online_incomplete_candidate_source")?;
                self.product = Some(
                    self.context
                        .start_pc4_candidate_product(self.request.clone(), input, &guard, control)
                        .map_err(|e| e.reason())?,
                );
                self.candidates = None;
            }
            AppOnlinePc4CandidateStep::Failed(e) => return Err(e.reason()),
            AppOnlinePc4CandidateStep::Miss { .. } => return Err("pc4_online_field_miss"),
            AppOnlinePc4CandidateStep::Cancelled => return Ok(CooperativeAppAdvance::Cancelled),
            _ => {}
        }
        Ok(CooperativeAppAdvance::Pending)
    }

    /// The transport forwards the observed status/header/body. Rust performs
    /// its existing admission check too; a whole response cannot be relabelled
    /// as an admitted Range by this application seam.
    pub fn admit_range(
        &mut self,
        lookup_session: u64,
        request_id: u64,
        status: u16,
        content_range: Option<String>,
        bytes: Vec<u8>,
        control: &ExecutionControl,
    ) -> Result<(), &'static str> {
        let range = self.pending.as_ref().ok_or("pc4_online_no_pending_range")?;
        if range.request_id() != request_id || range.lookup_session().get() != lookup_session {
            return Err("pc4_online_response_id_mismatch");
        }
        let guard = HostGuard {
            source: &self.source,
            control,
        };
        if self.lookup_id != Some(range.lookup_session()) {
            self.ordinal = 0;
            self.lookup_id = Some(range.lookup_session());
        }
        let ordinal = self
            .ordinal
            .checked_add(1)
            .and_then(NonZeroU32::new)
            .ok_or("pc4_online_request_count_overflow")?;
        let response = RangeResponse {
            lookup_session: range.lookup_session(),
            request_id: range.request_id(),
            snapshot: range.snapshot().clone(),
            profile: range.profile(),
            artifact: range.artifact(),
            artifact_content_identity: range.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: range.offset(),
            complete_length: range.artifact_descriptor().byte_len(),
            bytes,
        };
        let input = RangeAdmissionInput::http(RangeHttpResponse::new(
            status,
            content_range,
            None,
            Some(response),
        ));
        let attempt = RangeAdmissionAttempt::new(ordinal, NonZeroU16::new(1).unwrap());
        if let Some(lookup) = &mut self.lookup {
            lookup
                .admit_range(attempt, input, &guard)
                .map_err(|_| "pc4_online_range_admission_failed")?;
        } else {
            self.candidates
                .as_mut()
                .ok_or("pc4_online_execution_state")?
                .admit_range(attempt, input, &guard)
                .map_err(|_| "pc4_online_range_admission_failed")?;
        }
        self.ordinal = ordinal.get();
        self.pending = None;
        Ok(())
    }
}

struct HostGuard<'a> {
    source: &'a PcCandidateSourceBinding,
    control: &'a ExecutionControl,
}
impl PcCandidatePageGuard for HostGuard<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        source == self.source
    }
}
impl FixedQueueTraversalGuard for HostGuard<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.source.qualified_snapshot() == Some(snapshot)
    }
}
impl MaterializationGuard for HostGuard<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.source.qualified_snapshot() == Some(snapshot)
    }
}
impl RangeAdmissionGuard for HostGuard<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.source.qualified_snapshot() == Some(snapshot)
    }
}
fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).expect("nonzero online work limit")
}
fn range_limits() -> RangeAdmissionLimits {
    RangeAdmissionLimits::new(
        NonZeroU64::new(65_536).unwrap(),
        NonZeroU64::new(128 * 1024).unwrap(),
        NonZeroU32::new(64).unwrap(),
        NonZeroU16::new(1).unwrap(),
        60,
    )
}
fn frontier_budgets() -> Pc4ObservationFrontierBudgets {
    Pc4ObservationFrontierBudgets::new(
        Pc4BagRevealBudgets::new(
            nz(11),
            nz(1_000_000),
            nz(5_000_000),
            nz(64),
            nz(64),
            nz(5_000_000),
        ),
        FixedQueueHoldBudgets::new(nz(65_536), nz(4096), nz(65_536), nz(4096)),
        nz(11),
        nz(10),
        nz(8),
        nz(8),
        nz(8),
        nz(5_000_000),
    )
}
fn pin(snapshot: ActivatedSnapshot) -> Result<PinnedPc4Generation, &'static str> {
    let mut registry = Pc4GenerationRegistry::new(Pc4GenerationRetentionLimit::new(1).unwrap());
    let Pc4GenerationStageOutcome::Staged { token, .. } = registry
        .stage(registry.version(), Arc::new(snapshot))
        .map_err(|_| "pc4_online_generation_stage_failed")?
    else {
        return Err("pc4_online_generation_stage_failed");
    };
    registry
        .promote(&token)
        .map_err(|_| "pc4_online_generation_promote_failed")?;
    match registry.pin_current() {
        Pc4CurrentGeneration::Current(pinned) => Ok(pinned),
        _ => Err("pc4_online_generation_unavailable"),
    }
}
