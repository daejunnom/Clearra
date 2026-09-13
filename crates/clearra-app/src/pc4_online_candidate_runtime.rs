// SRP rationale: select the candidate producer behind one existing Range
// lifecycle. This dispatch layer contains no graph search or I/O policy.
use clearra_pc4_tablebase::QualifiedPc4TargetIdentity;

use crate::{
    online_pc4_lookup_session::AppQualifiedPc4LookupHit,
    pc4_fixed_queue_candidate_runtime::{
        Pc4FixedQueueCandidateRuntime, Pc4FixedQueueCandidateRuntimeAdmissionError,
        Pc4FixedQueueCandidateRuntimeAdvanceError, Pc4FixedQueueCandidateRuntimeStep,
    },
    pc4_lookup_graph_runtime_adapter::Pc4LookupGraphCacheAdmission,
    pc4_observation_candidate_adapter::{
        Pc4CompleteObservationCandidateFamily, Pc4ObservationCandidateGuard,
    },
    pc4_observation_candidate_runtime::{
        Pc4ObservationCandidateRuntime, Pc4ObservationCandidateRuntimeAdvanceError,
        Pc4ObservationCandidateRuntimeStep,
    },
    pc_candidate_page_boundary::{PcCandidateReducerInput, PcCandidateSourceBinding},
};

pub(crate) enum Pc4OnlineCandidateRuntime {
    Fixed(Pc4FixedQueueCandidateRuntime),
    Observation(Pc4ObservationCandidateRuntime),
}

pub(crate) enum Pc4OnlineCandidateRuntimeStep {
    NeedLookup(u32),
    Progress {
        observed_replays: usize,
        observed_candidates: usize,
    },
    ObservationProgress {
        concrete_paths: usize,
        reveal_outcomes: usize,
        candidate_memberships: usize,
    },
    Complete {
        replay_provenances: usize,
        canonical_candidates: usize,
    },
}

pub(crate) enum Pc4OnlineCandidateRuntimeAdvanceError {
    Fixed(Pc4FixedQueueCandidateRuntimeAdvanceError),
    Observation(Pc4ObservationCandidateRuntimeAdvanceError),
}

impl Pc4OnlineCandidateRuntimeAdvanceError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Fixed(error) => error.reason(),
            Self::Observation(error) => error.reason(),
        }
    }
}

impl Pc4OnlineCandidateRuntime {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        match self {
            Self::Fixed(runtime) => runtime.target(),
            Self::Observation(runtime) => runtime.target(),
        }
    }

    pub const fn source(&self) -> Option<&PcCandidateSourceBinding> {
        match self {
            Self::Fixed(runtime) => runtime.source(),
            Self::Observation(runtime) => runtime.source(),
        }
    }

    pub const fn completed_reducer_input(&self) -> Option<&PcCandidateReducerInput> {
        match self {
            Self::Fixed(runtime) => runtime.completed_reducer_input(),
            Self::Observation(runtime) => runtime.completed_reducer_input(),
        }
    }

    pub fn into_completed_reducer_input(self) -> Option<PcCandidateReducerInput> {
        match self {
            Self::Fixed(runtime) => runtime.into_completed_reducer_input(),
            Self::Observation(runtime) => runtime.into_completed_reducer_input(),
        }
    }

    #[cfg(test)]
    pub const fn completed_candidate_family(
        &self,
    ) -> Option<&crate::pc_candidate_page_boundary::graph_candidate_adapter::Pc4GraphCandidateFamily>
    {
        match self {
            Self::Fixed(runtime) => runtime.completed_candidate_family(),
            Self::Observation(_) => None,
        }
    }

    pub const fn completed_observation_family(
        &self,
    ) -> Option<&Pc4CompleteObservationCandidateFamily> {
        match self {
            Self::Observation(runtime) => runtime.completed_family(),
            Self::Fixed(_) => None,
        }
    }

    pub fn admit_lookup_hit(
        &mut self,
        hit: AppQualifiedPc4LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4FixedQueueCandidateRuntimeAdmissionError> {
        match self {
            Self::Fixed(runtime) => runtime.admit_lookup_hit(hit),
            Self::Observation(runtime) => runtime
                .admit_lookup_hit(hit)
                .map_err(Pc4FixedQueueCandidateRuntimeAdmissionError::Cache),
        }
    }

    pub fn advance<G: Pc4ObservationCandidateGuard>(
        &mut self,
        guard: &G,
    ) -> Result<Pc4OnlineCandidateRuntimeStep, Pc4OnlineCandidateRuntimeAdvanceError> {
        match self {
            Self::Fixed(runtime) => runtime
                .advance(guard)
                .map(|step| match step {
                    Pc4FixedQueueCandidateRuntimeStep::NeedLookup(id) => {
                        Pc4OnlineCandidateRuntimeStep::NeedLookup(id)
                    }
                    Pc4FixedQueueCandidateRuntimeStep::Advanced {
                        observed_replays,
                        observed_candidates,
                    } => Pc4OnlineCandidateRuntimeStep::Progress {
                        observed_replays,
                        observed_candidates,
                    },
                    Pc4FixedQueueCandidateRuntimeStep::Complete {
                        replay_provenances,
                        canonical_candidates,
                    } => Pc4OnlineCandidateRuntimeStep::Complete {
                        replay_provenances,
                        canonical_candidates,
                    },
                })
                .map_err(Pc4OnlineCandidateRuntimeAdvanceError::Fixed),
            Self::Observation(runtime) => runtime
                .advance(guard)
                .map(|step| match step {
                    Pc4ObservationCandidateRuntimeStep::NeedLookup(id) => {
                        Pc4OnlineCandidateRuntimeStep::NeedLookup(id)
                    }
                    Pc4ObservationCandidateRuntimeStep::Advanced {
                        concrete_paths,
                        reveal_outcomes,
                        candidate_memberships,
                    } => Pc4OnlineCandidateRuntimeStep::ObservationProgress {
                        concrete_paths,
                        reveal_outcomes,
                        candidate_memberships,
                    },
                    Pc4ObservationCandidateRuntimeStep::Complete {
                        replay_provenances,
                        canonical_candidates,
                    } => Pc4OnlineCandidateRuntimeStep::Complete {
                        replay_provenances,
                        canonical_candidates,
                    },
                })
                .map_err(Pc4OnlineCandidateRuntimeAdvanceError::Observation),
        }
    }
}
