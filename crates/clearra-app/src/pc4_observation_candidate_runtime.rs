// SRP rationale: bind the existing complete observation union to one qualified
// graph cache. Range transport, lookup lifetime, fallback and product reduction
// remain with their existing owners; a cache miss only requests a record.
use core::{convert::Infallible, num::NonZeroUsize};

use clearra_pc4_tablebase::{
    prepare_pc4_observation_graph_family, ActivatedSnapshot, ConcretePathMaterializationBudgets,
    ConcretePathMaterializationError, FixedQueueTraversalBudgets, FixedQueueTraversalPageBudgets,
    FixedQueueTraversalPageError, Pc4ObservationFrontierFamily, Pc4ObservationGraphBudgets,
    Pc4ObservationGraphPageError, Pc4ObservationGraphPrepareError, Pc4ObservationGraphRequest,
    PlacementMaterializationError, QualifiedPc4TargetIdentity, TerminalDepthContract,
};

use crate::{
    online_pc4_lookup_session::AppQualifiedPc4LookupHit,
    pc4_input_disclosure_policy::Pc4PreparedOnlineInput,
    pc4_lookup_graph_runtime_adapter::{
        Pc4LookupAdjacencyError, Pc4LookupGraphCache, Pc4LookupGraphCacheAdmission,
        Pc4LookupGraphCacheError, Pc4LookupGraphCacheLimits, Pc4LookupGraphCacheStartError,
        Pc4LookupMaterializationError,
    },
    pc4_observation_candidate_adapter::{
        prepare_pc4_observation_candidate_session, ManifestQualifiedPc4ObservationTerminal,
        Pc4CompleteObservationCandidateFamily, Pc4ObservationCandidateAdapterRequest,
        Pc4ObservationCandidateBudgets, Pc4ObservationCandidateError, Pc4ObservationCandidateGuard,
        Pc4ObservationCandidateSession, Pc4ObservationCandidateSessionError,
    },
    pc_candidate_page_boundary::{
        PcCandidateBoundaryError, PcCandidatePageGuard, PcCandidateReducerInput,
        PcCandidateSourceBinding,
    },
};

type CandidateAdvanceError = Pc4ObservationCandidateError<
    Pc4LookupAdjacencyError,
    Infallible,
    Pc4LookupMaterializationError,
>;

pub(crate) struct Pc4ObservationCandidateRuntimeRequest<'a> {
    pub activated_snapshot: &'a ActivatedSnapshot,
    pub prepared_input: &'a Pc4PreparedOnlineInput,
    pub source: &'a PcCandidateSourceBinding,
    pub start_field_id: u32,
    pub frontier: Pc4ObservationFrontierFamily,
    pub terminal_depth_contract: TerminalDepthContract,
    pub traversal_budgets: FixedQueueTraversalBudgets,
    pub traversal_page_budgets: FixedQueueTraversalPageBudgets,
    pub graph_budgets: Pc4ObservationGraphBudgets,
    pub materialization_budgets: ConcretePathMaterializationBudgets,
    pub candidate_budgets: Pc4ObservationCandidateBudgets,
    pub cache_limits: Pc4LookupGraphCacheLimits,
    pub observation_page_size: NonZeroUsize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Pc4ObservationCandidateRuntimeStartError {
    Cache(Pc4LookupGraphCacheStartError),
    Graph(Pc4ObservationGraphPrepareError),
    Candidate(Pc4ObservationCandidateSessionError),
}

impl Pc4ObservationCandidateRuntimeStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cache(error) => error.reason(),
            Self::Graph(error) => error.reason(),
            Self::Candidate(error) => error.reason(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Pc4ObservationCandidateRuntimeAdvanceError {
    CompletionUnavailable,
    UnresolvedCachedRecord { field_id: u32 },
    Candidate(CandidateAdvanceError),
    Session(Pc4ObservationCandidateSessionError),
    Completion(PcCandidateBoundaryError),
}

impl Pc4ObservationCandidateRuntimeAdvanceError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::CompletionUnavailable => "pc4_observation_runtime_completion_unavailable",
            Self::UnresolvedCachedRecord { .. } => {
                "pc4_observation_runtime_cached_record_unresolved"
            }
            Self::Candidate(error) => error.reason(),
            Self::Session(error) => error.reason(),
            Self::Completion(error) => error.reason(),
        }
    }
}

pub(crate) enum Pc4ObservationCandidateRuntimeStep {
    NeedLookup(u32),
    Advanced {
        concrete_paths: usize,
        reveal_outcomes: usize,
        candidate_memberships: usize,
    },
    Complete {
        replay_provenances: usize,
        canonical_candidates: usize,
    },
}

enum State {
    Running(Box<Pc4ObservationCandidateSession>),
    Complete(Box<Completion>),
    Poisoned,
}

struct Completion {
    family: Pc4CompleteObservationCandidateFamily,
    reducer_input: PcCandidateReducerInput,
}

pub(crate) struct Pc4ObservationCandidateRuntime {
    cache: Pc4LookupGraphCache,
    lookup_frontier: Vec<u32>,
    terminal: ManifestQualifiedPc4ObservationTerminal,
    page_size: NonZeroUsize,
    state: State,
}

impl Pc4ObservationCandidateRuntime {
    pub(crate) fn prepare<G: Pc4ObservationCandidateGuard>(
        request: Pc4ObservationCandidateRuntimeRequest<'_>,
        guard: &G,
    ) -> Result<Self, Pc4ObservationCandidateRuntimeStartError> {
        let target = request.prepared_input.target();
        let cache = Pc4LookupGraphCache::new(
            request.activated_snapshot,
            target.clone(),
            request.cache_limits,
        )
        .map_err(Pc4ObservationCandidateRuntimeStartError::Cache)?;
        let graph = prepare_pc4_observation_graph_family(
            Pc4ObservationGraphRequest::new(
                target.clone(),
                request.start_field_id,
                request.frontier,
                request.terminal_depth_contract,
                request.traversal_budgets,
                request.traversal_page_budgets,
                request.graph_budgets,
            ),
            guard,
        )
        .map_err(Pc4ObservationCandidateRuntimeStartError::Graph)?;
        let session = prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(
                target,
                request.prepared_input,
                request.source,
                request.start_field_id,
                request.materialization_budgets,
                request.candidate_budgets,
            ),
            &graph,
            guard,
        )
        .map_err(Pc4ObservationCandidateRuntimeStartError::Candidate)?;
        Ok(Self {
            cache,
            lookup_frontier: Vec::new(),
            terminal: ManifestQualifiedPc4ObservationTerminal::new(target.clone()),
            page_size: request.observation_page_size,
            state: State::Running(Box::new(session)),
        })
    }

    pub(crate) const fn target(&self) -> &QualifiedPc4TargetIdentity {
        self.cache.target()
    }

    pub(crate) const fn source(&self) -> Option<&PcCandidateSourceBinding> {
        match &self.state {
            State::Running(session) => Some(session.source()),
            State::Complete(done) => Some(done.family.source()),
            State::Poisoned => None,
        }
    }

    pub(crate) fn admit_lookup_hit(
        &mut self,
        hit: AppQualifiedPc4LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4LookupGraphCacheError> {
        let (target, lookup) = hit.into_parts();
        // Cache admission validates the same terminal ID/hash pair, profile,
        // snapshot, payload format and aggregate capacity for every branch.
        self.cache.admit(&target, lookup)
    }

    pub(crate) const fn completed_reducer_input(&self) -> Option<&PcCandidateReducerInput> {
        match &self.state {
            State::Complete(done) => Some(&done.reducer_input),
            _ => None,
        }
    }

    pub(crate) const fn completed_family(&self) -> Option<&Pc4CompleteObservationCandidateFamily> {
        match &self.state {
            State::Complete(done) => Some(&done.family),
            _ => None,
        }
    }

    pub(crate) fn into_completed_reducer_input(self) -> Option<PcCandidateReducerInput> {
        match self.state {
            State::Complete(done) => Some(done.reducer_input),
            _ => None,
        }
    }

    pub(crate) fn advance<G: Pc4ObservationCandidateGuard>(
        &mut self,
        guard: &G,
    ) -> Result<Pc4ObservationCandidateRuntimeStep, Pc4ObservationCandidateRuntimeAdvanceError>
    {
        self.lookup_frontier.clear();
        let State::Running(session) = &mut self.state else {
            return match &self.state {
                State::Complete(done) => Ok(Pc4ObservationCandidateRuntimeStep::Complete {
                    replay_provenances: done.family.replay_provenance_count(),
                    canonical_candidates: done.family.canonical_candidates().len(),
                }),
                _ => Err(Pc4ObservationCandidateRuntimeAdvanceError::CompletionUnavailable),
            };
        };
        let result = session.advance(
            self.page_size,
            &mut self
                .cache
                .adjacency_provider_with_frontier(&mut self.lookup_frontier),
            &mut self.terminal,
            &mut self.cache.placement_materializer(),
            guard,
        );
        match result {
            Ok(_) if session.is_exhausted() => self.seal(guard),
            Ok(_) => Ok(Pc4ObservationCandidateRuntimeStep::Advanced {
                concrete_paths: session.observed_concrete_path_count(),
                reveal_outcomes: session.observed_reveal_outcome_count(),
                candidate_memberships: session.observed_candidate_membership_count(),
            }),
            Err(error) => {
                if let Some(field_id) = required_lookup_field(&error) {
                    if !self.cache.contains_field_id(field_id) {
                        return Ok(Pc4ObservationCandidateRuntimeStep::NeedLookup(field_id));
                    }
                    self.state = State::Poisoned;
                    return Err(
                        Pc4ObservationCandidateRuntimeAdvanceError::UnresolvedCachedRecord {
                            field_id,
                        },
                    );
                }
                self.state = State::Poisoned;
                Err(Pc4ObservationCandidateRuntimeAdvanceError::Candidate(error))
            }
        }
    }

    pub(crate) fn lookup_frontier(&self) -> &[u32] {
        &self.lookup_frontier
    }

    fn seal<G: Pc4ObservationCandidateGuard>(
        &mut self,
        guard: &G,
    ) -> Result<Pc4ObservationCandidateRuntimeStep, Pc4ObservationCandidateRuntimeAdvanceError>
    {
        let State::Running(session) = core::mem::replace(&mut self.state, State::Poisoned) else {
            return Err(Pc4ObservationCandidateRuntimeAdvanceError::CompletionUnavailable);
        };
        let family = session
            .finish(guard)
            .map_err(Pc4ObservationCandidateRuntimeAdvanceError::Session)?;
        let reducer_input = family
            .reducer_input()
            .map_err(Pc4ObservationCandidateRuntimeAdvanceError::Completion)?;
        if PcCandidatePageGuard::is_cancelled(guard) {
            return Err(Pc4ObservationCandidateRuntimeAdvanceError::Session(
                Pc4ObservationCandidateError::Cancelled,
            ));
        }
        if !PcCandidatePageGuard::is_current_source(guard, family.source()) {
            return Err(Pc4ObservationCandidateRuntimeAdvanceError::Session(
                Pc4ObservationCandidateError::StaleSource,
            ));
        }
        if !PcCandidatePageGuard::is_current_snapshot(guard, family.target().snapshot()) {
            return Err(Pc4ObservationCandidateRuntimeAdvanceError::Session(
                Pc4ObservationCandidateError::StaleSnapshot,
            ));
        }
        let step = Pc4ObservationCandidateRuntimeStep::Complete {
            replay_provenances: family.replay_provenance_count(),
            canonical_candidates: family.canonical_candidates().len(),
        };
        self.state = State::Complete(Box::new(Completion {
            family,
            reducer_input,
        }));
        Ok(step)
    }
}

fn required_lookup_field(error: &CandidateAdvanceError) -> Option<u32> {
    match error {
        Pc4ObservationCandidateError::ObservationGraph(
            Pc4ObservationGraphPageError::Traversal(FixedQueueTraversalPageError::Provider(
                Pc4LookupAdjacencyError::RecordRequired { field_id },
            )),
        ) => Some(*field_id),
        Pc4ObservationCandidateError::Materialization(ConcretePathMaterializationError::Edge {
            source:
                PlacementMaterializationError::Materializer(
                    Pc4LookupMaterializationError::SourceRecordRequired { field_id }
                    | Pc4LookupMaterializationError::TargetRecordRequired { field_id },
                ),
            ..
        }) => Some(*field_id),
        _ => None,
    }
}
