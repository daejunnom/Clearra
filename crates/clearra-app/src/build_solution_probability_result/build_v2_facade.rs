// SRP rationale: this module has one behavior-level change reason: executing admitted Build v2 requests and returning query-bound validated portfolio results.

//! App-owned execution seam for the first actual Build v2 vertical.
//!
//! The legacy `build-probability` command still owns its byte-compatible
//! response. This facade adds a nominal, query-bound v2 result without
//! reinterpreting that response at a Host boundary. The facade deliberately
//! exposes only portfolio forms that the current producer can prove end to
//! end: target-search `build.cover` and supplied-solution
//! `build.evaluate.minimals`. Colored-target contracts and the other supplied
//! evaluators remain unavailable until their full typed producers exist.

use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::union_probability::union_probability,
};
use clearra_objectives::policy::score_objective_policy::{
    ScoreProfileSelection, SpinProfileSelection,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
    ProblemCompiler,
};
use clearra_supply::QueueObservationPolicy;

use crate::{
    app_services::AppCoreExecutorService,
    portfolio_alternative_store::CoveragePortfolioAlternativeSet,
};

pub use super::{
    build_v2_colored_result::{
        BuildColoredTargetCandidateCoverageV1, BuildColoredTargetScoreWinnerV1,
        BuildColoredTargetSetError, BuildColoredTargetSetV1,
    },
    build_v2_options::{BuildObjective, BuildQueueKnowledge, BuildScoreProfile},
    build_v2_supplied_result::{
        BuildSuppliedCandidateCoverageV1, BuildSuppliedScoreWinnerV1,
        BuildSuppliedSolutionSetError, BuildSuppliedSolutionSetV1,
    },
};

use super::{
    build_v2_colored_result::{
        validate_build_colored_family_v1_result, validate_build_colored_portfolio_v1_result,
        validate_build_colored_probability_v1_result, validate_build_colored_score_v1_result,
        BuildColoredTargetFamilyV1Result, BuildColoredTargetPortfolioV1Result,
        BuildColoredTargetProbabilityV1Result, BuildColoredTargetScoreV1Result,
    },
    build_v2_contract::{
        BuildColoredTargetDocumentSnapshot, BuildColoredTargetScoreQuerySnapshot,
        BuildSuppliedSolutionDocumentSnapshot, BuildSuppliedSolutionEvaluationContract,
        BuildSuppliedSolutionEvaluationQuerySnapshot, BuildSuppliedSolutionScoreQuerySnapshot,
        BuildTargetSearchContract, BuildTargetSearchQuerySnapshot,
        ReportedBuildSuppliedSolutionEvaluationResultIdentity,
        ReportedBuildTargetSearchResultIdentity,
        ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
        ValidatedBuildTargetSearchResultAuthority,
    },
    build_v2_options::{BuildExecutionSemantics, BuildV2OptionRequest},
    build_v2_result::{
        validate_build_coverage_portfolio_v2_result, BuildCoveragePortfolioV2Result,
    },
    build_v2_supplied_result::{
        validate_build_colored_replay_allow_empty, validate_build_supplied_cover_percent_v1_result,
        validate_build_supplied_coverage_v1_result,
        validate_build_supplied_minimum_cover_v1_result, validate_build_supplied_score_v1_result,
        BuildSuppliedCoverPercentV1Result, BuildSuppliedCoverageV1Result,
        BuildSuppliedMinimumCoverV1Result, BuildSuppliedScoreV1Result,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildCoverV2FacadeError {
    ObjectiveUnavailable,
    QueryNotPortfolioCapable,
    OptionsRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvaluateMinimalsV1FacadeError {
    QueryNotPortfolioCapable,
    SuppliedInputDoesNotMatchQuery,
    QuerySnapshotRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvaluateCoverPercentV1FacadeError {
    QueryNotProbabilityCapable,
    SuppliedInputDoesNotMatchQuery,
    QuerySnapshotRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvaluateCoverV1FacadeError {
    QueryNotCoverageCapable,
    SuppliedInputDoesNotMatchQuery,
    QuerySnapshotRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildEvaluateScoreV1FacadeError {
    QueryNotScoreCapable,
    SuppliedInputDoesNotMatchQuery,
    QuerySnapshotRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildColoredTargetV1FacadeError {
    QueryNotReplayCapable,
    ColoredInputDoesNotMatchQuery,
    ObjectiveUnavailable,
    QuerySnapshotRejected { detail: String },
    QueryCompileFailed { detail: String },
    ExecutionFailed { detail: String },
    ResultRejected { detail: String },
}

#[derive(Clone, Debug, PartialEq)]
struct BuildColoredTargetV1Request {
    query: BuildProbabilityQuery,
    target: BuildColoredTargetSetV1,
    snapshot: BuildTargetSearchQuerySnapshot,
}

impl BuildColoredTargetV1Request {
    fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        contract: BuildTargetSearchContract,
        objective: BuildObjective,
        score: Option<(BuildScoreProfile, u16)>,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        let objective_supported = match contract {
            BuildTargetSearchContract::Setup | BuildTargetSearchContract::Congruent => {
                matches!(objective, BuildObjective::All | BuildObjective::Unique)
            }
            BuildTargetSearchContract::CongruentCover | BuildTargetSearchContract::SetupCover => {
                matches!(
                    objective,
                    BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
                )
            }
            BuildTargetSearchContract::SetupCoverPercent => {
                matches!(objective, BuildObjective::All | BuildObjective::Unique)
            }
            BuildTargetSearchContract::SetupCoverScore => {
                objective == BuildObjective::MaxScoreCover
            }
            BuildTargetSearchContract::Cover => false,
        };
        if !objective_supported {
            return Err(BuildColoredTargetV1FacadeError::ObjectiveUnavailable);
        }
        if query.aggregation() != BuildProbabilityAggregation::Buildability
            || query.finesse_metric().requested()
            || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        {
            return Err(BuildColoredTargetV1FacadeError::QueryNotReplayCapable);
        }
        if !target.matches_query(&query) {
            return Err(BuildColoredTargetV1FacadeError::ColoredInputDoesNotMatchQuery);
        }
        let query =
            query.with_allowed_colored_solution_identities(target.identities().iter().copied());
        let document = BuildColoredTargetDocumentSnapshot::new(
            target.initial_board_mask(),
            target.visible_height(),
            target.page_count(),
            target.identities().len(),
            true,
            target.input_identity_sha256(),
        )
        .map_err(
            |error| BuildColoredTargetV1FacadeError::QuerySnapshotRejected {
                detail: format!("document:{error:?}"),
            },
        )?;
        let snapshot = match (contract, score) {
            (BuildTargetSearchContract::Setup, None) => {
                BuildTargetSearchQuerySnapshot::setup(query.clone(), document)
            }
            (BuildTargetSearchContract::Congruent, None) => {
                BuildTargetSearchQuerySnapshot::congruent(query.clone(), document)
            }
            (BuildTargetSearchContract::CongruentCover, None) => {
                BuildTargetSearchQuerySnapshot::congruent_cover(query.clone(), document)
            }
            (BuildTargetSearchContract::SetupCover, None) => {
                BuildTargetSearchQuerySnapshot::setup_cover(query.clone(), document)
            }
            (BuildTargetSearchContract::SetupCoverPercent, None) => {
                BuildTargetSearchQuerySnapshot::setup_cover_percent(query.clone(), document)
            }
            (BuildTargetSearchContract::SetupCoverScore, Some((profile, initial_b2b))) => {
                BuildTargetSearchQuerySnapshot::setup_cover_score(
                    BuildColoredTargetScoreQuerySnapshot::new(
                        query.clone(),
                        document,
                        profile,
                        initial_b2b,
                    ),
                )
            }
            _ => return Err(BuildColoredTargetV1FacadeError::ObjectiveUnavailable),
        };
        let queue_knowledge = match query.queue_observation_policy() {
            QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
            QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
        };
        let mut options = BuildV2OptionRequest::default()
            .with_queue_knowledge(queue_knowledge)
            .with_execution_semantics(BuildExecutionSemantics::Reachable)
            .with_objective(objective);
        if let Some((profile, initial_b2b)) = score {
            options = options
                .with_score_profile(profile)
                .with_initial_b2b(initial_b2b);
        }
        let snapshot = snapshot.with_options(options).map_err(|error| {
            BuildColoredTargetV1FacadeError::QuerySnapshotRejected {
                detail: format!("options:{error:?}"),
            }
        })?;
        Ok(Self {
            query,
            target,
            snapshot,
        })
    }

    fn authority(
        &self,
    ) -> Result<ValidatedBuildTargetSearchResultAuthority, BuildColoredTargetV1FacadeError> {
        let contract = self.snapshot.contract();
        ValidatedBuildTargetSearchResultAuthority::validate(
            self.snapshot.clone(),
            ReportedBuildTargetSearchResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("identity:{error:?}"),
        })
    }

    fn execute(
        &self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<clearra_core_executor::CoreExecutionResult, BuildColoredTargetV1FacadeError> {
        let problem =
            ProblemCompiler::compile_scenario_pc(self.query.core_query()).map_err(|error| {
                BuildColoredTargetV1FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        executor
            .execute_build_probability_with_control(
                &problem,
                self.query.field(),
                self.query.aggregation(),
                self.query.finesse_request().clone(),
                self.query.solution_probability_policy(),
                control,
            )
            .map_err(|error| BuildColoredTargetV1FacadeError::ExecutionFailed {
                detail: format!("{error:?}"),
            })
    }

    fn execute_score(
        &self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<
        (
            clearra_core_executor::CoreExecutionResult,
            crate::pc_score_postprocess::PcScoreDerivation,
        ),
        BuildColoredTargetV1FacadeError,
    > {
        let problem =
            ProblemCompiler::compile_scenario_pc(self.query.core_query()).map_err(|error| {
                BuildColoredTargetV1FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        executor
            .execute_build_probability_with_score_derivation_with_control(
                &problem,
                self.query.field(),
                self.query.aggregation(),
                self.query.finesse_request().clone(),
                self.query.solution_probability_policy(),
                control,
            )
            .map_err(|error| BuildColoredTargetV1FacadeError::ExecutionFailed {
                detail: format!("{error:?}"),
            })
    }

    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.query
            .checked_retained_capacity_bytes()?
            .checked_add(self.target.checked_retained_capacity_bytes()?)?
            .checked_add(self.snapshot.checked_retained_capacity_bytes()?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildColoredTargetCompleteness {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    score_evidence_complete: bool,
    exact_minimum_proven: bool,
}

impl BuildColoredTargetCompleteness {
    pub const fn input_identity_bound(self) -> bool {
        self.input_identity_bound
    }

    pub const fn producer_filter_bound(self) -> bool {
        self.producer_filter_bound
    }

    pub const fn buildability_replay_complete(self) -> bool {
        self.buildability_replay_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub const fn score_evidence_complete(self) -> bool {
        self.score_evidence_complete
    }

    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub const fn replay_complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }

    pub const fn portfolio_complete(self) -> bool {
        self.replay_complete() && self.exact_minimum_proven
    }

    pub const fn score_portfolio_complete(self) -> bool {
        self.portfolio_complete() && self.score_evidence_complete
    }
}

fn colored_completeness(
    evidence: super::build_v2_colored_result::BuildColoredTargetCompletenessEvidence,
) -> BuildColoredTargetCompleteness {
    BuildColoredTargetCompleteness {
        input_identity_bound: evidence.input_identity_bound(),
        producer_filter_bound: evidence.producer_filter_bound(),
        buildability_replay_complete: evidence.buildability_replay_complete(),
        coverage_rows_complete: evidence.coverage_rows_complete(),
        probability_weights_complete: evidence.probability_weights_complete(),
        score_evidence_complete: evidence.score_evidence_complete(),
        exact_minimum_proven: evidence.exact_minimum_proven(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupV1Request {
    request: BuildColoredTargetV1Request,
}

/// Complete producer-bound coverage evidence used by `setup.score` before its
/// score-only reduction. This is intentionally crate-private: public callers
/// receive the closed Setup score payload, never mutable coverage rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetupScoreCoverageEvidence {
    row: PatternBitSet,
    weights: WeightedPatternSet,
    pattern_universe_identity: String,
    covered_probability: String,
}

impl SetupScoreCoverageEvidence {
    pub(crate) fn row(&self) -> &PatternBitSet {
        &self.row
    }

    pub(crate) fn weights(&self) -> &WeightedPatternSet {
        &self.weights
    }

    pub(crate) fn pattern_universe_identity(&self) -> &str {
        &self.pattern_universe_identity
    }

    pub(crate) fn covered_probability(&self) -> &str {
        &self.covered_probability
    }
}

impl BuildSetupV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        objective: BuildObjective,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::Setup,
                objective,
                None,
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSetupV1, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let validated = validate_build_colored_family_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSetupV1 { result: validated })
    }

    /// Runs the same actual Build producer and fieldwise replay validator as
    /// `build.setup`, retaining the one candidate's complete pattern row for
    /// Setup score reduction. An unreachable candidate is a valid empty row;
    /// producer incompleteness and identity drift still fail closed.
    pub(crate) fn execute_setup_score_coverage(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<SetupScoreCoverageEvidence, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let authority = self.request.authority()?;
        super::build_v2_colored_result::validate_target_binding_for_setup_score(
            &authority,
            &self.request.query,
            &self.request.target,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        let replay = validate_build_colored_replay_allow_empty(
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("replay:{error:?}"),
        })?;
        if replay.rows.len() != 1 || replay.pattern_count != replay.weights.len() {
            return Err(BuildColoredTargetV1FacadeError::ResultRejected {
                detail: "setup-score coverage row cardinality mismatch".to_owned(),
            });
        }
        let row = replay
            .rows
            .into_iter()
            .next()
            .expect("validated one-row cardinality");
        let covered_probability = union_probability(&row, &replay.weights)
            .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
                detail: format!("setup-score probability:{error:?}"),
            })?
            .get()
            .to_string();
        Ok(SetupScoreCoverageEvidence {
            row,
            weights: replay.weights,
            pattern_universe_identity: replay.pattern_universe_identity,
            covered_probability,
        })
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.request.checked_retained_capacity_bytes()
    }

    pub(crate) fn setup_score_max_patterns(&self) -> usize {
        self.request
            .query
            .core_query()
            .execution_policy()
            .max_patterns()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildCongruentV1Request {
    request: BuildColoredTargetV1Request,
}

impl BuildCongruentV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        objective: BuildObjective,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::Congruent,
                objective,
                None,
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildCongruentV1, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let validated = validate_build_colored_family_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildCongruentV1 { result: validated })
    }
}

macro_rules! colored_family_result {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {
            result: BuildColoredTargetFamilyV1Result,
        }

        impl $name {
            pub fn contract_id(&self) -> &'static str {
                self.result.contract_id()
            }

            pub fn input_identity_sha256(&self) -> &str {
                self.result.input_identity_sha256()
            }

            pub fn evaluation_identity_sha256(&self) -> &str {
                self.result.evaluation_identity_sha256()
            }

            pub fn objective(&self) -> BuildObjective {
                self.result.objective()
            }

            pub fn source_candidate_count(&self) -> usize {
                self.result.source_candidate_count()
            }

            pub fn reachable_candidate_count(&self) -> usize {
                self.result.reachable_candidate_count()
            }

            pub fn pattern_count(&self) -> usize {
                self.result.pattern_count()
            }

            pub fn covered_pattern_count(&self) -> usize {
                self.result.covered_pattern_count()
            }

            pub fn union_probability(&self) -> &str {
                self.result.union_probability()
            }

            pub fn candidates(&self) -> &[BuildColoredTargetCandidateCoverageV1] {
                self.result.candidates()
            }

            pub fn completeness(&self) -> BuildColoredTargetCompleteness {
                colored_completeness(self.result.completeness())
            }
        }
    };
}

colored_family_result!(BuildSetupV1);
colored_family_result!(BuildCongruentV1);

#[derive(Clone, Debug, PartialEq)]
pub struct BuildCongruentCoverV1Request {
    request: BuildColoredTargetV1Request,
}

impl BuildCongruentCoverV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        objective: BuildObjective,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::CongruentCover,
                objective,
                None,
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildCongruentCoverV1, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let validated = validate_build_colored_portfolio_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildCongruentCoverV1 { result: validated })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupCoverV1Request {
    request: BuildColoredTargetV1Request,
}

impl BuildSetupCoverV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        objective: BuildObjective,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::SetupCover,
                objective,
                None,
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSetupCoverV1, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let validated = validate_build_colored_portfolio_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSetupCoverV1 { result: validated })
    }
}

macro_rules! colored_portfolio_result {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {
            result: BuildColoredTargetPortfolioV1Result,
        }

        impl $name {
            pub fn contract_id(&self) -> &'static str {
                self.result.contract_id()
            }

            pub fn input_identity_sha256(&self) -> &str {
                self.result.input_identity_sha256()
            }

            pub fn objective(&self) -> BuildObjective {
                self.result.objective()
            }

            pub fn source_candidate_count(&self) -> usize {
                self.result.source_candidate_count()
            }

            pub fn reachable_candidate_count(&self) -> usize {
                self.result.reachable_candidate_count()
            }

            pub fn selected_candidate_count(&self) -> usize {
                self.result.selected_candidate_count()
            }

            pub fn pattern_count(&self) -> usize {
                self.result.pattern_count()
            }

            pub fn required_pattern_count(&self) -> usize {
                self.result.required_pattern_count()
            }

            pub fn union_probability(&self) -> &str {
                self.result.union_probability()
            }

            pub fn canonical_candidate_keys(&self) -> &[String] {
                self.result.canonical_candidate_keys()
            }

            pub fn portfolio_alternative_owner(
                &self,
            ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
                self.result.portfolio_alternative_owner()
            }

            pub fn completeness(&self) -> BuildColoredTargetCompleteness {
                colored_completeness(self.result.completeness())
            }
        }
    };
}

colored_portfolio_result!(BuildCongruentCoverV1);
colored_portfolio_result!(BuildSetupCoverV1);

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupCoverPercentV1Request {
    request: BuildColoredTargetV1Request,
}

impl BuildSetupCoverPercentV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        objective: BuildObjective,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::SetupCoverPercent,
                objective,
                None,
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSetupCoverPercentV1, BuildColoredTargetV1FacadeError> {
        let result = self.request.execute(executor, control)?;
        let validated = validate_build_colored_probability_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSetupCoverPercentV1 { result: validated })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupCoverPercentV1 {
    result: BuildColoredTargetProbabilityV1Result,
}

impl BuildSetupCoverPercentV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn evaluation_identity_sha256(&self) -> &str {
        self.result.evaluation_identity_sha256()
    }

    pub fn objective(&self) -> BuildObjective {
        self.result.objective()
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn covered_pattern_count(&self) -> usize {
        self.result.covered_pattern_count()
    }

    pub fn union_probability(&self) -> &str {
        self.result.union_probability()
    }

    pub fn completeness(&self) -> BuildColoredTargetCompleteness {
        colored_completeness(self.result.completeness())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupCoverScoreV1Request {
    request: BuildColoredTargetV1Request,
}

impl BuildSetupCoverScoreV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        target: BuildColoredTargetSetV1,
        score_profile: BuildScoreProfile,
        initial_b2b: u16,
    ) -> Result<Self, BuildColoredTargetV1FacadeError> {
        let query = query.with_score_summary(score_profile_selection(score_profile), initial_b2b);
        Ok(Self {
            request: BuildColoredTargetV1Request::new(
                query,
                target,
                BuildTargetSearchContract::SetupCoverScore,
                BuildObjective::MaxScoreCover,
                Some((score_profile, initial_b2b)),
            )?,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSetupCoverScoreV1, BuildColoredTargetV1FacadeError> {
        let (result, derivation) = self.request.execute_score(executor, control)?;
        let validated = validate_build_colored_score_v1_result(
            self.request.authority()?,
            &self.request.query,
            &self.request.target,
            &result,
            &derivation,
        )
        .map_err(|error| BuildColoredTargetV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSetupCoverScoreV1 { result: validated })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSetupCoverScoreV1 {
    result: BuildColoredTargetScoreV1Result,
}

impl BuildSetupCoverScoreV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn score_profile(&self) -> &str {
        self.result.score_profile()
    }

    pub fn initial_b2b(&self) -> u16 {
        self.result.initial_b2b()
    }

    pub const fn score_accuracy(&self) -> &'static str {
        "basic-approximation"
    }

    pub const fn profile_specific_exact(&self) -> bool {
        false
    }

    pub const fn score_equality_basis(&self) -> &'static str {
        "score-only"
    }

    pub const fn informational_attack_basis(&self) -> &'static str {
        "canonical-equal-score-trace"
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn selected_candidate_count(&self) -> usize {
        self.result.selected_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn required_pattern_count(&self) -> usize {
        self.result.required_pattern_count()
    }

    pub fn canonical_candidate_keys(&self) -> &[String] {
        self.result.canonical_candidate_keys()
    }

    pub fn winners(&self) -> &[BuildColoredTargetScoreWinnerV1] {
        self.result.winners()
    }

    pub fn portfolio_alternative_owner(&self) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.result.portfolio_alternative_owner()
    }

    pub fn completeness(&self) -> BuildColoredTargetCompleteness {
        colored_completeness(self.result.completeness())
    }
}

/// Fully validated request for the actual `build.cover` portfolio vertical.
///
/// Validation happens before compilation or solver execution. In particular,
/// a caller cannot smuggle `visible-7` through a tiling-only request, request
/// a non-portfolio objective, or omit the coverage/weight evidence required
/// by the exact reducer.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildCoverV2Request {
    query: BuildProbabilityQuery,
    objective: BuildObjective,
}

impl BuildCoverV2Request {
    pub fn new(
        query: BuildProbabilityQuery,
        objective: BuildObjective,
    ) -> Result<Self, BuildCoverV2FacadeError> {
        if !matches!(
            objective,
            BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
        ) {
            return Err(BuildCoverV2FacadeError::ObjectiveUnavailable);
        }
        if query.aggregation() != BuildProbabilityAggregation::Buildability
            || query.finesse_metric().requested()
            || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        {
            return Err(BuildCoverV2FacadeError::QueryNotPortfolioCapable);
        }
        validated_query_snapshot(&query, objective)?;
        Ok(Self { query, objective })
    }

    pub const fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }

    pub const fn objective(&self) -> BuildObjective {
        self.objective
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildCoveragePortfolioV2, BuildCoverV2FacadeError> {
        let problem =
            ProblemCompiler::compile_scenario_pc(self.query.core_query()).map_err(|error| {
                BuildCoverV2FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        let result = executor
            .execute_build_probability_with_control(
                &problem,
                self.query.field(),
                self.query.aggregation(),
                self.query.finesse_request().clone(),
                self.query.solution_probability_policy(),
                control,
            )
            .map_err(|error| BuildCoverV2FacadeError::ExecutionFailed {
                detail: format!("{error:?}"),
            })?;

        let snapshot = validated_query_snapshot(&self.query, self.objective)?;
        let contract = snapshot.contract();
        let authority = ValidatedBuildTargetSearchResultAuthority::validate(
            snapshot,
            ReportedBuildTargetSearchResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .map_err(|error| BuildCoverV2FacadeError::ResultRejected {
            detail: format!("identity:{error:?}"),
        })?;
        let result =
            validate_build_coverage_portfolio_v2_result(authority, &result).map_err(|error| {
                BuildCoverV2FacadeError::ResultRejected {
                    detail: format!("evidence:{error:?}"),
                }
            })?;
        Ok(BuildCoveragePortfolioV2 { result })
    }
}

/// Fully bound request for the actual `build.evaluate.minimals` vertical.
///
/// The supplied colored identities do not become candidates by declaration.
/// Execution installs them as an allow-list on the real Build producer, which
/// must rediscover each retained solution through geometry and buildability
/// replay before its coverage row is eligible for the exact reducer.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvaluateMinimalsV1Request {
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
}

impl BuildEvaluateMinimalsV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        supplied: BuildSuppliedSolutionSetV1,
    ) -> Result<Self, BuildEvaluateMinimalsV1FacadeError> {
        if query.aggregation() != BuildProbabilityAggregation::Buildability
            || query.finesse_metric().requested()
            || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        {
            return Err(BuildEvaluateMinimalsV1FacadeError::QueryNotPortfolioCapable);
        }
        if !supplied.matches_query(&query) {
            return Err(BuildEvaluateMinimalsV1FacadeError::SuppliedInputDoesNotMatchQuery);
        }
        validated_supplied_minimals_snapshot(&query, &supplied)?;
        Ok(Self { query, supplied })
    }

    pub const fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }

    pub const fn supplied(&self) -> &BuildSuppliedSolutionSetV1 {
        &self.supplied
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSuppliedMinimumCoverV1, BuildEvaluateMinimalsV1FacadeError> {
        let query = self
            .query
            .with_allowed_colored_solution_identities(self.supplied.identities().iter().copied());
        let snapshot = validated_supplied_minimals_snapshot(&query, &self.supplied)?;
        let contract = snapshot.contract();
        let authority = ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
            snapshot,
            ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .map_err(|error| BuildEvaluateMinimalsV1FacadeError::ResultRejected {
            detail: format!("identity:{error:?}"),
        })?;
        let problem =
            ProblemCompiler::compile_scenario_pc(query.core_query()).map_err(|error| {
                BuildEvaluateMinimalsV1FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        let result = executor
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                control,
            )
            .map_err(
                |error| BuildEvaluateMinimalsV1FacadeError::ExecutionFailed {
                    detail: format!("{error:?}"),
                },
            )?;
        let result = validate_build_supplied_minimum_cover_v1_result(
            authority,
            &query,
            &self.supplied,
            &result,
        )
        .map_err(|error| BuildEvaluateMinimalsV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSuppliedMinimumCoverV1 { result })
    }
}

/// Public finite descriptor for one replay-validated supplied-solution
/// minimum-cover result. Tie alternatives retain the same immutable shared
/// owner consumed by the product pager seam.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildSuppliedMinimumCoverV1 {
    result: BuildSuppliedMinimumCoverV1Result,
}

impl BuildSuppliedMinimumCoverV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn replay_basis(&self) -> &'static str {
        self.result.replay_basis()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn selected_candidate_count(&self) -> usize {
        self.result.selected_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn required_pattern_count(&self) -> usize {
        self.result.required_pattern_count()
    }

    pub fn union_probability(&self) -> &str {
        self.result.union_probability()
    }

    pub fn canonical_candidate_keys(&self) -> &[String] {
        self.result.canonical_candidate_keys()
    }

    pub fn completeness(&self) -> BuildSuppliedReplayCompleteness {
        let evidence = self.result.completeness();
        BuildSuppliedReplayCompleteness {
            input_identity_bound: evidence.input_identity_bound(),
            producer_filter_bound: evidence.producer_filter_bound(),
            buildability_replay_complete: evidence.buildability_replay_complete(),
            coverage_rows_complete: evidence.coverage_rows_complete(),
            probability_weights_complete: evidence.probability_weights_complete(),
            exact_minimum_proven: evidence.exact_minimum_proven(),
        }
    }

    pub fn portfolio_alternative_owner(&self) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.result.portfolio_alternative_owner()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildSuppliedReplayCompleteness {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
}

impl BuildSuppliedReplayCompleteness {
    pub const fn input_identity_bound(self) -> bool {
        self.input_identity_bound
    }

    pub const fn producer_filter_bound(self) -> bool {
        self.producer_filter_bound
    }

    pub const fn buildability_replay_complete(self) -> bool {
        self.buildability_replay_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub const fn complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
            && self.exact_minimum_proven
    }
}

/// Query-bound default-`unique` execution seam for
/// `build.evaluate.cover-percent`. Probability is computed from the OR union
/// of replay-validated coverage rows; candidate probabilities are never
/// summed.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvaluateCoverPercentV1Request {
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
}

impl BuildEvaluateCoverPercentV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        supplied: BuildSuppliedSolutionSetV1,
    ) -> Result<Self, BuildEvaluateCoverPercentV1FacadeError> {
        if query.aggregation() != BuildProbabilityAggregation::Buildability
            || query.finesse_metric().requested()
            || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        {
            return Err(BuildEvaluateCoverPercentV1FacadeError::QueryNotProbabilityCapable);
        }
        if !supplied.matches_query(&query) {
            return Err(BuildEvaluateCoverPercentV1FacadeError::SuppliedInputDoesNotMatchQuery);
        }
        validated_supplied_cover_percent_snapshot(&query, &supplied)?;
        Ok(Self { query, supplied })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSuppliedCoverPercentV1, BuildEvaluateCoverPercentV1FacadeError> {
        let query = self
            .query
            .with_allowed_colored_solution_identities(self.supplied.identities().iter().copied());
        let snapshot = validated_supplied_cover_percent_snapshot(&query, &self.supplied)?;
        let contract = snapshot.contract();
        let authority = ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
            snapshot,
            ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .map_err(
            |error| BuildEvaluateCoverPercentV1FacadeError::ResultRejected {
                detail: format!("identity:{error:?}"),
            },
        )?;
        let problem =
            ProblemCompiler::compile_scenario_pc(query.core_query()).map_err(|error| {
                BuildEvaluateCoverPercentV1FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        let result = executor
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                control,
            )
            .map_err(
                |error| BuildEvaluateCoverPercentV1FacadeError::ExecutionFailed {
                    detail: format!("{error:?}"),
                },
            )?;
        let result = validate_build_supplied_cover_percent_v1_result(
            authority,
            &query,
            &self.supplied,
            &result,
        )
        .map_err(
            |error| BuildEvaluateCoverPercentV1FacadeError::ResultRejected {
                detail: format!("evidence:{error:?}"),
            },
        )?;
        Ok(BuildSuppliedCoverPercentV1 { result })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSuppliedCoverPercentV1 {
    result: BuildSuppliedCoverPercentV1Result,
}

impl BuildSuppliedCoverPercentV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn replay_basis(&self) -> &'static str {
        self.result.replay_basis()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn evaluation_identity_sha256(&self) -> &str {
        self.result.evaluation_identity_sha256()
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn covered_pattern_count(&self) -> usize {
        self.result.covered_pattern_count()
    }

    pub fn union_probability(&self) -> &str {
        self.result.union_probability()
    }

    pub fn completeness(&self) -> BuildSuppliedProbabilityCompleteness {
        let evidence = self.result.completeness();
        BuildSuppliedProbabilityCompleteness {
            input_identity_bound: evidence.input_identity_bound(),
            producer_filter_bound: evidence.producer_filter_bound(),
            buildability_replay_complete: evidence.buildability_replay_complete(),
            coverage_rows_complete: evidence.coverage_rows_complete(),
            probability_weights_complete: evidence.probability_weights_complete(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildSuppliedProbabilityCompleteness {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
}

impl BuildSuppliedProbabilityCompleteness {
    pub const fn input_identity_bound(self) -> bool {
        self.input_identity_bound
    }

    pub const fn producer_filter_bound(self) -> bool {
        self.producer_filter_bound
    }

    pub const fn buildability_replay_complete(self) -> bool {
        self.buildability_replay_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub const fn complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }
}

/// Actual supplied-solution coverage replay. Every public row is rediscovered
/// by the Build producer; merely appearing in the input document is never
/// sufficient to make a candidate reachable.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvaluateCoverV1Request {
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
}

impl BuildEvaluateCoverV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        supplied: BuildSuppliedSolutionSetV1,
    ) -> Result<Self, BuildEvaluateCoverV1FacadeError> {
        validate_supplied_coverage_query(&query, &supplied)?;
        validated_supplied_coverage_snapshot(
            &query,
            &supplied,
            BuildSuppliedSolutionEvaluationContract::Cover,
        )
        .map_err(|detail| BuildEvaluateCoverV1FacadeError::QuerySnapshotRejected { detail })?;
        Ok(Self { query, supplied })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSuppliedCoverageV1, BuildEvaluateCoverV1FacadeError> {
        execute_supplied_coverage(
            self.query,
            self.supplied,
            BuildSuppliedSolutionEvaluationContract::Cover,
            executor,
            control,
        )
        .map(|result| BuildSuppliedCoverageV1 { result })
        .map_err(|detail| BuildEvaluateCoverV1FacadeError::ResultRejected { detail })
    }
}

/// Actual B2B-preserving supplied-solution coverage replay. The T-spin B2B
/// execution constraint is compiled into the query and materialized before
/// coverage rows become public.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvaluateB2bCoverV1Request {
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
}

impl BuildEvaluateB2bCoverV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        supplied: BuildSuppliedSolutionSetV1,
    ) -> Result<Self, BuildEvaluateCoverV1FacadeError> {
        validate_supplied_coverage_query(&query, &supplied)?;
        let query = query.with_back_to_back_preservation(SpinProfileSelection::TSpins);
        validated_supplied_coverage_snapshot(
            &query,
            &supplied,
            BuildSuppliedSolutionEvaluationContract::B2bCover,
        )
        .map_err(|detail| BuildEvaluateCoverV1FacadeError::QuerySnapshotRejected { detail })?;
        Ok(Self { query, supplied })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSuppliedCoverageV1, BuildEvaluateCoverV1FacadeError> {
        execute_supplied_coverage(
            self.query,
            self.supplied,
            BuildSuppliedSolutionEvaluationContract::B2bCover,
            executor,
            control,
        )
        .map(|result| BuildSuppliedCoverageV1 { result })
        .map_err(|detail| BuildEvaluateCoverV1FacadeError::ResultRejected { detail })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSuppliedCoverageV1 {
    result: BuildSuppliedCoverageV1Result,
}

impl BuildSuppliedCoverageV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn evaluation_identity_sha256(&self) -> &str {
        self.result.evaluation_identity_sha256()
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn covered_pattern_count(&self) -> usize {
        self.result.covered_pattern_count()
    }

    pub fn union_probability(&self) -> &str {
        self.result.union_probability()
    }

    pub fn b2b_preservation_required(&self) -> bool {
        self.result.b2b_preservation_required()
    }

    pub fn candidates(&self) -> &[BuildSuppliedCandidateCoverageV1] {
        self.result.candidates()
    }

    pub fn completeness(&self) -> BuildSuppliedProbabilityCompleteness {
        let evidence = self.result.completeness();
        BuildSuppliedProbabilityCompleteness {
            input_identity_bound: evidence.input_identity_bound(),
            producer_filter_bound: evidence.producer_filter_bound(),
            buildability_replay_complete: evidence.buildability_replay_complete(),
            coverage_rows_complete: evidence.coverage_rows_complete(),
            probability_weights_complete: evidence.probability_weights_complete(),
        }
    }
}

/// B-option maximum-score cover over actual supplied-solution replay. Score is
/// the only eligibility/equality relation. Attack belongs solely to the
/// canonical equal-score trace retained for display.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvaluateScoreV1Request {
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
    score_profile: BuildScoreProfile,
    initial_b2b: u16,
}

/// Internal request-profile authority shared by the Build v2 facades.  Every
/// family retains the same typed Build query that is executed, so profile
/// selection can be derived from that query instead of being reconstructed
/// from frontend text or result metadata.
pub(crate) trait BuildV2RequestProfileQuery {
    fn request_profile_query(&self) -> &BuildProbabilityQuery;
}

macro_rules! impl_colored_request_profile_query {
    ($($request:ty),+ $(,)?) => {
        $(
            impl BuildV2RequestProfileQuery for $request {
                fn request_profile_query(&self) -> &BuildProbabilityQuery {
                    &self.request.query
                }
            }
        )+
    };
}

impl_colored_request_profile_query!(
    BuildSetupV1Request,
    BuildCongruentV1Request,
    BuildCongruentCoverV1Request,
    BuildSetupCoverV1Request,
    BuildSetupCoverPercentV1Request,
    BuildSetupCoverScoreV1Request,
);

macro_rules! impl_direct_request_profile_query {
    ($($request:ty),+ $(,)?) => {
        $(
            impl BuildV2RequestProfileQuery for $request {
                fn request_profile_query(&self) -> &BuildProbabilityQuery {
                    &self.query
                }
            }
        )+
    };
}

impl_direct_request_profile_query!(
    BuildCoverV2Request,
    BuildEvaluateMinimalsV1Request,
    BuildEvaluateCoverPercentV1Request,
    BuildEvaluateCoverV1Request,
    BuildEvaluateB2bCoverV1Request,
    BuildEvaluateScoreV1Request,
);

impl BuildEvaluateScoreV1Request {
    pub fn new(
        query: BuildProbabilityQuery,
        supplied: BuildSuppliedSolutionSetV1,
        score_profile: BuildScoreProfile,
        initial_b2b: u16,
    ) -> Result<Self, BuildEvaluateScoreV1FacadeError> {
        if query.aggregation() != BuildProbabilityAggregation::Buildability
            || query.finesse_metric().requested()
            || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        {
            return Err(BuildEvaluateScoreV1FacadeError::QueryNotScoreCapable);
        }
        if !supplied.matches_query(&query) {
            return Err(BuildEvaluateScoreV1FacadeError::SuppliedInputDoesNotMatchQuery);
        }
        let query = query.with_score_summary(score_profile_selection(score_profile), initial_b2b);
        validated_supplied_score_snapshot(&query, &supplied, score_profile, initial_b2b)
            .map_err(|detail| BuildEvaluateScoreV1FacadeError::QuerySnapshotRejected { detail })?;
        Ok(Self {
            query,
            supplied,
            score_profile,
            initial_b2b,
        })
    }

    pub fn execute(
        self,
        executor: &AppCoreExecutorService,
        control: &ExecutionControl,
    ) -> Result<BuildSuppliedScoreV1, BuildEvaluateScoreV1FacadeError> {
        let query = self
            .query
            .with_allowed_colored_solution_identities(self.supplied.identities().iter().copied());
        let snapshot = validated_supplied_score_snapshot(
            &query,
            &self.supplied,
            self.score_profile,
            self.initial_b2b,
        )
        .map_err(|detail| BuildEvaluateScoreV1FacadeError::QuerySnapshotRejected { detail })?;
        let contract = snapshot.contract();
        let authority = ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
            snapshot,
            ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .map_err(|error| BuildEvaluateScoreV1FacadeError::ResultRejected {
            detail: format!("identity:{error:?}"),
        })?;
        let problem =
            ProblemCompiler::compile_scenario_pc(query.core_query()).map_err(|error| {
                BuildEvaluateScoreV1FacadeError::QueryCompileFailed {
                    detail: format!("{error:?}"),
                }
            })?;
        let (result, derivation) = executor
            .execute_build_probability_with_score_derivation_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                control,
            )
            .map_err(|error| BuildEvaluateScoreV1FacadeError::ExecutionFailed {
                detail: format!("{error:?}"),
            })?;
        let result = validate_build_supplied_score_v1_result(
            authority,
            &query,
            &self.supplied,
            &result,
            &derivation,
        )
        .map_err(|error| BuildEvaluateScoreV1FacadeError::ResultRejected {
            detail: format!("evidence:{error:?}"),
        })?;
        Ok(BuildSuppliedScoreV1 { result })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildSuppliedScoreV1 {
    result: BuildSuppliedScoreV1Result,
}

impl BuildSuppliedScoreV1 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn input_identity_sha256(&self) -> &str {
        self.result.input_identity_sha256()
    }

    pub fn score_profile(&self) -> &str {
        self.result.score_profile()
    }

    pub fn initial_b2b(&self) -> u16 {
        self.result.initial_b2b()
    }

    pub fn score_accuracy(&self) -> &'static str {
        "basic-approximation"
    }

    pub fn profile_specific_exact(&self) -> bool {
        false
    }

    pub fn score_equality_basis(&self) -> &'static str {
        "score-only"
    }

    pub fn informational_attack_basis(&self) -> &'static str {
        "canonical-equal-score-trace"
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn reachable_candidate_count(&self) -> usize {
        self.result.reachable_candidate_count()
    }

    pub fn selected_candidate_count(&self) -> usize {
        self.result.selected_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn required_pattern_count(&self) -> usize {
        self.result.required_pattern_count()
    }

    pub fn canonical_candidate_keys(&self) -> &[String] {
        self.result.canonical_candidate_keys()
    }

    pub fn winners(&self) -> &[BuildSuppliedScoreWinnerV1] {
        self.result.winners()
    }

    pub fn portfolio_alternative_owner(&self) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.result.portfolio_alternative_owner()
    }

    pub fn completeness(&self) -> BuildSuppliedReplayCompleteness {
        let evidence = self.result.completeness();
        BuildSuppliedReplayCompleteness {
            input_identity_bound: evidence.input_identity_bound(),
            producer_filter_bound: evidence.producer_filter_bound(),
            buildability_replay_complete: evidence.buildability_replay_complete(),
            coverage_rows_complete: evidence.coverage_rows_complete(),
            probability_weights_complete: evidence.probability_weights_complete(),
            exact_minimum_proven: evidence.exact_minimum_proven(),
        }
    }
}

/// Public, finite descriptor plus one immutable shared owner for lazy tie
/// paging. The source `CoreExecutionResult` and producer-private replay tables
/// are not retained by this boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildCoveragePortfolioV2 {
    result: BuildCoveragePortfolioV2Result,
}

impl BuildCoveragePortfolioV2 {
    pub fn contract_id(&self) -> &'static str {
        self.result.contract_id()
    }

    pub fn probability_basis(&self) -> &'static str {
        self.result.probability_basis()
    }

    pub fn objective(&self) -> BuildObjective {
        self.result.objective()
    }

    pub fn source_candidate_count(&self) -> usize {
        self.result.source_candidate_count()
    }

    pub fn selected_candidate_count(&self) -> usize {
        self.result.selected_candidate_count()
    }

    pub fn pattern_count(&self) -> usize {
        self.result.pattern_count()
    }

    pub fn required_pattern_count(&self) -> usize {
        self.result.required_pattern_count()
    }

    pub fn union_probability(&self) -> &str {
        self.result.union_probability()
    }

    pub fn normalized_solution_set_hash(&self) -> &str {
        self.result.normalized_solution_set_hash()
    }

    pub fn canonical_candidate_keys(&self) -> &[String] {
        self.result.canonical_candidate_keys()
    }

    pub fn completeness(&self) -> BuildCoveragePortfolioCompleteness {
        let evidence = self.result.completeness();
        BuildCoveragePortfolioCompleteness {
            source_universe_complete: evidence.source_universe_complete(),
            coverage_rows_complete: evidence.coverage_rows_complete(),
            probability_weights_complete: evidence.probability_weights_complete(),
            exact_minimum_proven: evidence.exact_minimum_proven(),
            query_bound: evidence.query_bound(),
        }
    }

    pub fn portfolio_alternative_owner(&self) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.result.portfolio_alternative_owner()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildCoveragePortfolioCompleteness {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
    query_bound: bool,
}

impl BuildCoveragePortfolioCompleteness {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub const fn query_bound(self) -> bool {
        self.query_bound
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
            && self.exact_minimum_proven
            && self.query_bound
    }
}

fn validated_query_snapshot(
    query: &BuildProbabilityQuery,
    objective: BuildObjective,
) -> Result<BuildTargetSearchQuerySnapshot, BuildCoverV2FacadeError> {
    let queue_knowledge = match query.queue_observation_policy() {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    };
    let execution_semantics = if query.aggregation().is_tiling_only() {
        BuildExecutionSemantics::TilingOnly
    } else {
        BuildExecutionSemantics::Reachable
    };
    BuildTargetSearchQuerySnapshot::cover(query.clone())
        .and_then(|snapshot| {
            snapshot.with_options(
                BuildV2OptionRequest::default()
                    .with_queue_knowledge(queue_knowledge)
                    .with_execution_semantics(execution_semantics)
                    .with_objective(objective),
            )
        })
        .map_err(|error| BuildCoverV2FacadeError::OptionsRejected {
            detail: format!("{error:?}"),
        })
}

fn validated_supplied_minimals_snapshot(
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
) -> Result<BuildSuppliedSolutionEvaluationQuerySnapshot, BuildEvaluateMinimalsV1FacadeError> {
    let document = BuildSuppliedSolutionDocumentSnapshot::new(
        supplied.initial_board_mask(),
        supplied.visible_height(),
        supplied.page_count(),
        supplied.identities().len(),
        true,
        supplied.input_identity_sha256(),
    )
    .map_err(
        |error| BuildEvaluateMinimalsV1FacadeError::QuerySnapshotRejected {
            detail: format!("document:{error:?}"),
        },
    )?;
    let queue_knowledge = match query.queue_observation_policy() {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    };
    BuildSuppliedSolutionEvaluationQuerySnapshot::minimals(document)
        .and_then(|snapshot| {
            snapshot.with_options(
                BuildV2OptionRequest::default()
                    .with_queue_knowledge(queue_knowledge)
                    .with_execution_semantics(BuildExecutionSemantics::Reachable)
                    .with_objective(BuildObjective::MinCover),
            )
        })
        .map_err(
            |error| BuildEvaluateMinimalsV1FacadeError::QuerySnapshotRejected {
                detail: format!("options:{error:?}"),
            },
        )
}

fn validated_supplied_cover_percent_snapshot(
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
) -> Result<BuildSuppliedSolutionEvaluationQuerySnapshot, BuildEvaluateCoverPercentV1FacadeError> {
    let document = BuildSuppliedSolutionDocumentSnapshot::new(
        supplied.initial_board_mask(),
        supplied.visible_height(),
        supplied.page_count(),
        supplied.identities().len(),
        true,
        supplied.input_identity_sha256(),
    )
    .map_err(
        |error| BuildEvaluateCoverPercentV1FacadeError::QuerySnapshotRejected {
            detail: format!("document:{error:?}"),
        },
    )?;
    let queue_knowledge = match query.queue_observation_policy() {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    };
    BuildSuppliedSolutionEvaluationQuerySnapshot::cover_percent(document)
        .and_then(|snapshot| {
            snapshot.with_options(
                BuildV2OptionRequest::default()
                    .with_queue_knowledge(queue_knowledge)
                    .with_execution_semantics(BuildExecutionSemantics::Reachable),
            )
        })
        .map_err(
            |error| BuildEvaluateCoverPercentV1FacadeError::QuerySnapshotRejected {
                detail: format!("options:{error:?}"),
            },
        )
}

fn validate_supplied_coverage_query(
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
) -> Result<(), BuildEvaluateCoverV1FacadeError> {
    if query.aggregation() != BuildProbabilityAggregation::Buildability
        || query.finesse_metric().requested()
        || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
    {
        return Err(BuildEvaluateCoverV1FacadeError::QueryNotCoverageCapable);
    }
    if !supplied.matches_query(query) {
        return Err(BuildEvaluateCoverV1FacadeError::SuppliedInputDoesNotMatchQuery);
    }
    Ok(())
}

fn validated_supplied_coverage_snapshot(
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    contract: BuildSuppliedSolutionEvaluationContract,
) -> Result<BuildSuppliedSolutionEvaluationQuerySnapshot, String> {
    let document = BuildSuppliedSolutionDocumentSnapshot::new(
        supplied.initial_board_mask(),
        supplied.visible_height(),
        supplied.page_count(),
        supplied.identities().len(),
        true,
        supplied.input_identity_sha256(),
    )
    .map_err(|error| format!("document:{error:?}"))?;
    let snapshot = match contract {
        BuildSuppliedSolutionEvaluationContract::Cover => {
            BuildSuppliedSolutionEvaluationQuerySnapshot::cover(document)
        }
        BuildSuppliedSolutionEvaluationContract::B2bCover => {
            BuildSuppliedSolutionEvaluationQuerySnapshot::b2b_cover(document)
        }
        _ => return Err("unsupported-coverage-contract".to_owned()),
    }
    .map_err(|error| format!("contract:{error:?}"))?;
    let queue_knowledge = match query.queue_observation_policy() {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    };
    snapshot
        .with_options(
            BuildV2OptionRequest::default()
                .with_queue_knowledge(queue_knowledge)
                .with_execution_semantics(BuildExecutionSemantics::Reachable),
        )
        .map_err(|error| format!("options:{error:?}"))
}

fn execute_supplied_coverage(
    query: BuildProbabilityQuery,
    supplied: BuildSuppliedSolutionSetV1,
    contract: BuildSuppliedSolutionEvaluationContract,
    executor: &AppCoreExecutorService,
    control: &ExecutionControl,
) -> Result<BuildSuppliedCoverageV1Result, String> {
    let query =
        query.with_allowed_colored_solution_identities(supplied.identities().iter().copied());
    let snapshot = validated_supplied_coverage_snapshot(&query, &supplied, contract)?;
    let reported_contract = snapshot.contract();
    let authority = ValidatedBuildSuppliedSolutionEvaluationResultAuthority::validate(
        snapshot,
        ReportedBuildSuppliedSolutionEvaluationResultIdentity::new(
            reported_contract.capability_id(),
            reported_contract.problem_contract_id(),
            reported_contract.input_schema_id(),
            reported_contract.result_contract_id(),
        ),
    )
    .map_err(|error| format!("identity:{error:?}"))?;
    let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|error| format!("compile:{error:?}"))?;
    let result = executor
        .execute_build_probability_with_control(
            &problem,
            query.field(),
            query.aggregation(),
            query.finesse_request().clone(),
            query.solution_probability_policy(),
            control,
        )
        .map_err(|error| format!("execution:{error:?}"))?;
    validate_build_supplied_coverage_v1_result(authority, &query, &supplied, &result)
        .map_err(|error| format!("evidence:{error:?}"))
}

fn validated_supplied_score_snapshot(
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    score_profile: BuildScoreProfile,
    initial_b2b: u16,
) -> Result<BuildSuppliedSolutionEvaluationQuerySnapshot, String> {
    let document = BuildSuppliedSolutionDocumentSnapshot::new(
        supplied.initial_board_mask(),
        supplied.visible_height(),
        supplied.page_count(),
        supplied.identities().len(),
        true,
        supplied.input_identity_sha256(),
    )
    .map_err(|error| format!("document:{error:?}"))?;
    let queue_knowledge = match query.queue_observation_policy() {
        QueueObservationPolicy::FullQueueOracle => BuildQueueKnowledge::Oracle,
        QueueObservationPolicy::VisibleSeven => BuildQueueKnowledge::VisibleSeven,
    };
    BuildSuppliedSolutionEvaluationQuerySnapshot::score(
        BuildSuppliedSolutionScoreQuerySnapshot::new(document, score_profile, initial_b2b),
    )
    .and_then(|snapshot| {
        snapshot.with_options(
            BuildV2OptionRequest::default()
                .with_queue_knowledge(queue_knowledge)
                .with_execution_semantics(BuildExecutionSemantics::Reachable)
                .with_objective(BuildObjective::MaxScoreCover)
                .with_score_profile(score_profile)
                .with_initial_b2b(initial_b2b),
        )
    })
    .map_err(|error| format!("options:{error:?}"))
}

const fn score_profile_selection(profile: BuildScoreProfile) -> ScoreProfileSelection {
    match profile {
        BuildScoreProfile::Tetrio => ScoreProfileSelection::Tetrio,
        BuildScoreProfile::Guideline => ScoreProfileSelection::Guideline,
        BuildScoreProfile::JstrisUltra => ScoreProfileSelection::JstrisUltra,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_objectives::policy::score_objective_policy::ScoreProfileSelection;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery,
        BuildSolutionProbabilityPolicy,
    };
    use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

    use super::{
        BuildColoredTargetSetV1, BuildColoredTargetV1FacadeError, BuildCongruentCoverV1Request,
        BuildCongruentV1Request, BuildCoverV2FacadeError, BuildCoverV2Request,
        BuildEvaluateB2bCoverV1Request, BuildEvaluateCoverPercentV1Request,
        BuildEvaluateCoverV1Request, BuildEvaluateMinimalsV1FacadeError,
        BuildEvaluateMinimalsV1Request, BuildEvaluateScoreV1Request, BuildObjective,
        BuildQueueKnowledge, BuildScoreProfile, BuildSetupCoverPercentV1Request,
        BuildSetupCoverScoreV1Request, BuildSetupCoverV1Request, BuildSetupV1Request,
        BuildSuppliedSolutionSetV1,
    };
    use crate::{
        build_solution_probability_result::build_probability_resource_test_guard,
        AppCoreExecutorService,
    };

    fn one_piece_query() -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("canonical target");
        BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include)
    }

    fn colored_identity(piece_index: usize, cells: u64) -> StandardBoard64ColoredTilingIdentity {
        let mut piece_masks = [0_u64; 7];
        piece_masks[piece_index] = cells;
        StandardBoard64ColoredTilingIdentity::from_piece_masks(0, piece_masks)
            .expect("four colored cells")
    }

    fn colored_target(label: &str) -> BuildColoredTargetSetV1 {
        BuildColoredTargetSetV1::new(
            4,
            2,
            label,
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask target candidates")
    }

    #[test]
    fn public_option_enums_keep_the_closed_spellings() {
        assert_eq!(
            BuildQueueKnowledge::parse("visible-7"),
            Some(BuildQueueKnowledge::VisibleSeven)
        );
        assert_eq!(
            BuildObjective::parse("minimum-cover"),
            Some(BuildObjective::MinCover)
        );
        assert_eq!(BuildObjective::parse("minimum"), None);
        assert_eq!(
            BuildScoreProfile::parse("jstris-ultra"),
            Some(BuildScoreProfile::JstrisUltra)
        );
    }

    #[test]
    fn facade_rejects_invalid_forms_before_execution() {
        assert_eq!(
            BuildCoverV2Request::new(one_piece_query(), BuildObjective::Unique),
            Err(BuildCoverV2FacadeError::ObjectiveUnavailable)
        );
        assert_eq!(
            BuildCoverV2Request::new(
                one_piece_query()
                    .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Omit),
                BuildObjective::MinCover,
            ),
            Err(BuildCoverV2FacadeError::QueryNotPortfolioCapable)
        );
        assert!(matches!(
            BuildCoverV2Request::new(
                one_piece_query()
                    .with_aggregation(BuildProbabilityAggregation::TilingOnly)
                    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
                BuildObjective::MinCover,
            ),
            Err(BuildCoverV2FacadeError::QueryNotPortfolioCapable)
                | Err(BuildCoverV2FacadeError::OptionsRejected { .. })
        ));
    }

    #[test]
    fn app_facade_executes_and_retains_one_shared_page_source() {
        let _resource_guard = build_probability_resource_test_guard();
        let output = BuildCoverV2Request::new(one_piece_query(), BuildObjective::MinCover)
            .expect("valid portfolio request")
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            )
            .expect("complete Build portfolio execution");

        assert_eq!(output.contract_id(), "build-coverage-portfolio.v2");
        assert_eq!(output.objective(), BuildObjective::MinCover);
        assert_eq!(output.source_candidate_count(), 1);
        assert_eq!(output.selected_candidate_count(), 1);
        assert_eq!(output.union_probability(), "1");
        assert!(output.completeness().complete());
        let owner = output
            .portfolio_alternative_owner()
            .expect("portfolio forms own a page source");
        assert_eq!(Arc::strong_count(owner), 1);
        assert_eq!(owner.canonical_page().portfolio().candidate_ids(), &[1]);

        let owner_identity = owner.set_identity_sha256().to_owned();
        let canonical_first_candidate = output.canonical_candidate_keys()[0].clone();
        let response = crate::AppResponse::success(crate::AppRenderModel::Verify(
            crate::AppMessage::new(crate::AppResultKind::Verify, Vec::new()),
        ))
        .with_build_coverage_portfolio_v2(output)
        .expect("validated Build App response");
        let product = response
            .product_capability_result()
            .expect("Build product result");
        assert_eq!(product.contract().as_str(), "build.cover");
        assert_eq!(
            product.result_kind().as_str(),
            "build-coverage-portfolio.v2"
        );
        let crate::ProductPageSourceOwner::CoveragePortfolio(public_owner) = product
            .public_page_source_owner()
            .expect("one shared public page owner")
        else {
            panic!("Build uses coverage portfolio owner");
        };
        assert_eq!(public_owner.set_identity_sha256(), owner_identity);
        let public_payload = product
            .public_result_payload()
            .expect("finite Build payload");
        let clearra_host_contract::ProductResultPayloadContent::BuildCoveragePortfolioV2(
            build_payload,
        ) = public_payload.content()
        else {
            panic!("Build coverage payload kind");
        };
        assert_eq!(
            build_payload.canonical_first_candidate_id(),
            canonical_first_candidate
        );
        assert_eq!(
            build_payload.page_source_identity_sha256(),
            Some(owner_identity.as_str())
        );
        let host = response.to_host_response();
        assert_eq!(
            host.result().map(|result| result.kind()),
            Some("build-coverage-portfolio.v2")
        );
        assert_eq!(
            host.product_result_payload()
                .expect("Host Build payload")
                .content(),
            public_payload.content()
        );

        let app = crate::AppContext::new(
            crate::AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let dispatched = app.run(crate::AppRequest::new(crate::AppCommand::BuildV2(
            crate::commands::BuildV2AppCommand::build_cover(
                BuildCoverV2Request::new(one_piece_query(), BuildObjective::MinCover)
                    .expect("checked Build command request"),
            ),
        )));
        assert_eq!(dispatched.status(), crate::AppStatus::Success);
        assert_eq!(
            dispatched.command(),
            Some(clearra_host_contract::AppCommandKind::BuildProbability)
        );
        assert_eq!(
            dispatched
                .product_capability_result()
                .expect("checked dispatch product result")
                .contract()
                .as_str(),
            "build.cover"
        );
        assert!(matches!(
            dispatched
                .to_host_response()
                .product_result_payload()
                .expect("checked dispatch Host payload")
                .content(),
            clearra_host_contract::ProductResultPayloadContent::BuildCoveragePortfolioV2(_)
        ));
    }

    #[test]
    fn colored_setup_and_congruent_are_actual_replayed_families() {
        let _resource_guard = build_probability_resource_test_guard();
        let setup = BuildSetupV1Request::new(
            one_piece_query(),
            colored_target("colored-target:setup"),
            BuildObjective::Unique,
        )
        .expect("setup request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("setup replay");
        let congruent = BuildCongruentV1Request::new(
            one_piece_query(),
            colored_target("colored-target:congruent"),
            BuildObjective::All,
        )
        .expect("congruent request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("congruent replay");

        assert_eq!(setup.contract_id(), "build-target-family.v2");
        assert_eq!(setup.objective(), BuildObjective::Unique);
        assert_eq!(congruent.contract_id(), "build-congruence-family.v1");
        assert_eq!(congruent.objective(), BuildObjective::All);
        for (source, reachable, probability, candidates, complete) in [
            (
                setup.source_candidate_count(),
                setup.reachable_candidate_count(),
                setup.union_probability(),
                setup.candidates(),
                setup.completeness(),
            ),
            (
                congruent.source_candidate_count(),
                congruent.reachable_candidate_count(),
                congruent.union_probability(),
                congruent.candidates(),
                congruent.completeness(),
            ),
        ] {
            assert_eq!(source, 2);
            assert_eq!(reachable, 1);
            assert_eq!(probability, "1");
            assert_eq!(candidates.len(), 2);
            assert_eq!(
                candidates
                    .iter()
                    .map(|candidate| candidate.covered_pattern_count())
                    .sum::<usize>(),
                1
            );
            assert!(complete.replay_complete());
            assert!(!complete.exact_minimum_proven());
            assert!(!complete.score_evidence_complete());
        }
    }

    #[test]
    fn build_setup_app_command_projects_the_actual_family_into_the_host_contract() {
        let _resource_guard = build_probability_resource_test_guard();
        let app = crate::AppContext::new(
            crate::AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let response = app.run(crate::AppRequest::new(crate::AppCommand::BuildV2(
            crate::commands::BuildV2AppCommand::build_setup(
                BuildSetupV1Request::new(
                    one_piece_query(),
                    colored_target("colored-target:app-build-setup"),
                    BuildObjective::Unique,
                )
                .expect("checked build.setup request"),
            ),
        )));

        assert_eq!(response.status(), crate::AppStatus::Success);
        assert_eq!(
            response.command(),
            Some(clearra_host_contract::AppCommandKind::BuildProbability)
        );
        let product = response
            .product_capability_result()
            .expect("validated build.setup product result");
        assert_eq!(product.contract().as_str(), "build.setup");
        assert_eq!(product.result_kind().as_str(), "build-target-family.v2");
        assert!(product.public_page_source_owner().is_none());
        assert!(response.public_page_source_owner().is_none());

        let host = response.to_host_response();
        assert_eq!(
            host.result().map(|result| result.kind()),
            Some("build-target-family.v2")
        );
        let clearra_host_contract::ProductResultPayloadContent::BuildSetupFamilyV1(payload) = host
            .product_result_payload()
            .expect("public build.setup Host payload")
            .content()
        else {
            panic!("build.setup must use the typed family payload");
        };
        assert_eq!(payload.contract(), "build-target-family.v2");
        assert_eq!(payload.objective(), "unique");
        assert_eq!(payload.source_candidate_count(), "2");
        assert_eq!(payload.reachable_candidate_count(), "1");
        assert_eq!(payload.pattern_count(), "1");
        assert_eq!(payload.covered_pattern_count(), "1");
        assert_eq!(payload.union_probability(), "1");
        assert_eq!(payload.candidates().len(), 2);
        assert_eq!(
            payload
                .candidates()
                .iter()
                .filter(|candidate| candidate.covered_pattern_count() == "1")
                .count(),
            1
        );
        assert!(payload.completeness().complete());
    }

    #[test]
    fn colored_cover_forms_retain_the_shared_all_optimal_store() {
        let _resource_guard = build_probability_resource_test_guard();
        let congruent = BuildCongruentCoverV1Request::new(
            one_piece_query(),
            colored_target("colored-target:congruent-cover"),
            BuildObjective::MinCover,
        )
        .expect("congruent-cover request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("congruent-cover replay");
        let setup = BuildSetupCoverV1Request::new(
            one_piece_query(),
            colored_target("colored-target:setup-cover"),
            BuildObjective::MaxProbabilityMinimum,
        )
        .expect("setup-cover request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("setup-cover replay");

        assert_eq!(congruent.contract_id(), "build-congruence-coverage.v1");
        assert_eq!(setup.contract_id(), "build-setup-cover.v1");
        for (output_objective, selected, probability, complete, owner) in [
            (
                congruent.objective(),
                congruent.selected_candidate_count(),
                congruent.union_probability(),
                congruent.completeness(),
                congruent.portfolio_alternative_owner(),
            ),
            (
                setup.objective(),
                setup.selected_candidate_count(),
                setup.union_probability(),
                setup.completeness(),
                setup.portfolio_alternative_owner(),
            ),
        ] {
            assert!(matches!(
                output_objective,
                BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
            ));
            assert_eq!(selected, 1);
            assert_eq!(probability, "1");
            assert!(complete.portfolio_complete());
            let owner = owner.expect("complete portfolio owner");
            assert_eq!(Arc::strong_count(owner), 1);
            assert_eq!(owner.canonical_page().portfolio().candidate_ids(), &[1]);
        }
    }

    #[test]
    fn colored_setup_cover_percent_uses_or_union_and_has_no_portfolio_claim() {
        let _resource_guard = build_probability_resource_test_guard();
        let output = BuildSetupCoverPercentV1Request::new(
            one_piece_query(),
            colored_target("colored-target:setup-cover-percent"),
            BuildObjective::Unique,
        )
        .expect("percent request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("percent replay");
        assert_eq!(output.contract_id(), "build-setup-cover-probability.v1");
        assert_eq!(output.source_candidate_count(), 2);
        assert_eq!(output.reachable_candidate_count(), 1);
        assert_eq!(output.covered_pattern_count(), 1);
        assert_eq!(output.union_probability(), "1");
        assert!(output.completeness().replay_complete());
        assert!(!output.completeness().exact_minimum_proven());
    }

    #[test]
    fn colored_setup_cover_score_uses_score_only_and_retains_approximation_contract() {
        let _resource_guard = build_probability_resource_test_guard();
        let output = BuildSetupCoverScoreV1Request::new(
            one_piece_query(),
            colored_target("colored-target:setup-cover-score"),
            BuildScoreProfile::Guideline,
            u16::MAX,
        )
        .expect("score request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("score replay");
        assert_eq!(output.contract_id(), "build-setup-cover-score.v1");
        assert_eq!(output.score_profile(), "guideline");
        assert_eq!(output.initial_b2b(), u16::MAX);
        assert_eq!(output.score_accuracy(), "basic-approximation");
        assert!(!output.profile_specific_exact());
        assert_eq!(output.score_equality_basis(), "score-only");
        assert_eq!(
            output.informational_attack_basis(),
            "canonical-equal-score-trace"
        );
        assert_eq!(output.winners().len(), 1);
        assert_eq!(output.selected_candidate_count(), 1);
        assert!(output.completeness().score_portfolio_complete());
        assert!(output.portfolio_alternative_owner().is_some());
    }

    #[test]
    fn every_colored_target_form_rejects_wrong_objective_or_identity_before_execution() {
        let wrong_height = BuildColoredTargetSetV1::new(
            1,
            1,
            "colored-target:wrong-height",
            [colored_identity(0, 0xf)],
        )
        .expect("internally valid target");
        for rejected in [
            BuildSetupV1Request::new(
                one_piece_query(),
                colored_target("colored-target:setup-objective"),
                BuildObjective::MinCover,
            )
            .map(|_| ()),
            BuildCongruentV1Request::new(
                one_piece_query(),
                colored_target("colored-target:congruent-objective"),
                BuildObjective::MaxProbabilityMinimum,
            )
            .map(|_| ()),
            BuildCongruentCoverV1Request::new(
                one_piece_query(),
                colored_target("colored-target:congruent-cover-objective"),
                BuildObjective::Unique,
            )
            .map(|_| ()),
            BuildSetupCoverV1Request::new(
                one_piece_query(),
                colored_target("colored-target:setup-cover-objective"),
                BuildObjective::All,
            )
            .map(|_| ()),
            BuildSetupCoverPercentV1Request::new(
                one_piece_query(),
                colored_target("colored-target:percent-objective"),
                BuildObjective::MinCover,
            )
            .map(|_| ()),
        ] {
            assert_eq!(
                rejected,
                Err(BuildColoredTargetV1FacadeError::ObjectiveUnavailable)
            );
        }
        assert_eq!(
            BuildSetupV1Request::new(one_piece_query(), wrong_height, BuildObjective::Unique),
            Err(BuildColoredTargetV1FacadeError::ColoredInputDoesNotMatchQuery)
        );
    }

    #[test]
    fn supplied_minimals_replays_actual_candidates_and_excludes_nominal_geometry() {
        let _resource_guard = build_probability_resource_test_guard();
        let horizontal_i = colored_identity(0, 0xf);
        let nominal_o_line = colored_identity(1, 0xf);
        let supplied = BuildSuppliedSolutionSetV1::new(
            4,
            2,
            "fumen-document:test-i-plus-nominal-o",
            [horizontal_i, nominal_o_line],
        )
        .expect("same-board colored candidates");
        let input_identity = supplied.input_identity_sha256().to_owned();

        let output = BuildEvaluateMinimalsV1Request::new(one_piece_query(), supplied)
            .expect("query-bound supplied request")
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            )
            .expect("complete supplied-solution replay");

        assert_eq!(output.contract_id(), "build-supplied-minimum-cover.v1");
        assert_eq!(
            output.replay_basis(),
            "supplied-colored-identity-filter-plus-buildability-replay"
        );
        assert_eq!(output.input_identity_sha256(), input_identity);
        assert_eq!(output.source_candidate_count(), 2);
        assert_eq!(output.reachable_candidate_count(), 1);
        assert_eq!(output.selected_candidate_count(), 1);
        assert_eq!(output.union_probability(), "1");
        assert!(output.completeness().complete());
        assert_eq!(output.canonical_candidate_keys().len(), 1);
        assert!(output.canonical_candidate_keys()[0].contains("colors=I:"));
        let owner = output
            .portfolio_alternative_owner()
            .expect("complete minimum cover owns its shared page source");
        assert_eq!(Arc::strong_count(owner), 1);
        assert_eq!(owner.canonical_page().portfolio().candidate_ids(), &[1]);
    }

    #[test]
    fn supplied_minimals_rejects_document_identity_mismatch_before_execution() {
        let supplied = BuildSuppliedSolutionSetV1::new(
            1,
            1,
            "fumen-document:height-one",
            [colored_identity(0, 0xf)],
        )
        .expect("one-row colored source is internally valid");

        assert_eq!(
            BuildEvaluateMinimalsV1Request::new(one_piece_query(), supplied),
            Err(BuildEvaluateMinimalsV1FacadeError::SuppliedInputDoesNotMatchQuery)
        );
    }

    #[test]
    fn supplied_cover_percent_uses_replayed_or_union_not_candidate_sum() {
        let _resource_guard = build_probability_resource_test_guard();
        let supplied = BuildSuppliedSolutionSetV1::new(
            4,
            2,
            "fumen-document:cover-percent",
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask supplied candidates");
        let input_identity = supplied.input_identity_sha256().to_owned();

        let output = BuildEvaluateCoverPercentV1Request::new(one_piece_query(), supplied)
            .expect("query-bound supplied probability request")
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            )
            .expect("complete supplied probability replay");

        assert_eq!(output.contract_id(), "build-supplied-probability.v1");
        assert_eq!(output.input_identity_sha256(), input_identity);
        assert_eq!(output.evaluation_identity_sha256().len(), 64);
        assert_eq!(output.source_candidate_count(), 2);
        assert_eq!(output.reachable_candidate_count(), 1);
        assert_eq!(output.pattern_count(), 1);
        assert_eq!(output.covered_pattern_count(), 1);
        assert_eq!(output.union_probability(), "1");
        assert!(output.completeness().complete());
    }

    #[test]
    fn build_evaluate_cover_replays_every_public_candidate() {
        let _resource_guard = build_probability_resource_test_guard();
        let supplied = BuildSuppliedSolutionSetV1::new(
            4,
            2,
            "fumen-document:cover",
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask supplied candidates");

        let output = BuildEvaluateCoverV1Request::new(one_piece_query(), supplied)
            .expect("query-bound supplied cover request")
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            )
            .expect("complete supplied coverage replay");

        assert_eq!(output.contract_id(), "build-supplied-coverage.v1");
        assert_eq!(output.source_candidate_count(), 2);
        assert_eq!(output.reachable_candidate_count(), 1);
        assert_eq!(output.pattern_count(), 1);
        assert_eq!(output.covered_pattern_count(), 1);
        assert_eq!(output.union_probability(), "1");
        assert!(!output.b2b_preservation_required());
        assert_eq!(output.candidates().len(), 2);
        assert_eq!(
            output
                .candidates()
                .iter()
                .map(|candidate| candidate.covered_pattern_count())
                .sum::<usize>(),
            1
        );
        assert!(output.candidates().iter().any(|candidate| {
            candidate.candidate_key().contains("colors=I:")
                && candidate.covered_pattern_count() == 1
        }));
        assert!(output.completeness().complete());
    }

    #[test]
    fn build_evaluate_b2b_cover_is_constraint_materialized_before_public_rows() {
        let _resource_guard = build_probability_resource_test_guard();
        let supplied = BuildSuppliedSolutionSetV1::new(
            4,
            1,
            "fumen-document:b2b-cover",
            [colored_identity(0, 0xf)],
        )
        .expect("one replayable supplied candidate");

        let output = BuildEvaluateB2bCoverV1Request::new(one_piece_query(), supplied)
            .expect("query-bound B2B request")
            .execute(
                &AppCoreExecutorService::wasm_cpu(),
                &ExecutionControl::default(),
            );

        match output {
            Ok(output) => {
                assert_eq!(output.contract_id(), "build-supplied-b2b-coverage.v1");
                assert!(output.b2b_preservation_required());
                assert!(output.completeness().complete());
            }
            Err(error) => {
                panic!("complete B2B replay must not downgrade to nominal output: {error:?}")
            }
        }
    }

    #[test]
    fn build_evaluate_score_uses_score_only_for_ties_and_owns_all_minimum_covers() {
        let _resource_guard = build_probability_resource_test_guard();
        let supplied = BuildSuppliedSolutionSetV1::new(
            4,
            2,
            "fumen-document:score",
            [colored_identity(0, 0xf), colored_identity(1, 0xf)],
        )
        .expect("same-mask supplied score candidates");

        let output = BuildEvaluateScoreV1Request::new(
            one_piece_query(),
            supplied,
            BuildScoreProfile::Tetrio,
            0,
        )
        .expect("query-bound supplied score request")
        .execute(
            &AppCoreExecutorService::wasm_cpu(),
            &ExecutionControl::default(),
        )
        .expect("complete supplied score replay");

        assert_eq!(output.contract_id(), "build-supplied-score.v1");
        assert_eq!(output.score_profile(), "tetrio");
        assert_eq!(output.initial_b2b(), 0);
        assert_eq!(output.score_accuracy(), "basic-approximation");
        assert!(!output.profile_specific_exact());
        assert_eq!(output.score_equality_basis(), "score-only");
        assert_eq!(
            output.informational_attack_basis(),
            "canonical-equal-score-trace"
        );
        assert_eq!(output.source_candidate_count(), 2);
        assert_eq!(output.reachable_candidate_count(), 1);
        assert_eq!(output.selected_candidate_count(), 1);
        assert_eq!(output.pattern_count(), 1);
        assert_eq!(output.required_pattern_count(), 1);
        assert_eq!(output.canonical_candidate_keys().len(), 1);
        assert_eq!(output.winners().len(), 1);
        assert_eq!(output.winners()[0].pattern_id(), 0);
        assert!(output.winners()[0].candidate_key().contains("colors=I:"));
        let owner = output
            .portfolio_alternative_owner()
            .expect("complete score result owns the shared lazy page source");
        assert_eq!(Arc::strong_count(owner), 1);
        assert_eq!(owner.canonical_page().portfolio().candidate_ids(), &[1]);
        assert!(output.completeness().complete());
    }

    #[test]
    fn build_score_request_materializes_the_actual_replay_score_matrix() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = one_piece_query().with_score_summary(ScoreProfileSelection::Tetrio, 0);
        let problem = clearra_problem::ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("score Build problem compiles");
        assert!(problem.objective().score().requested());
        assert_eq!(
            problem.objective().score().profile(),
            ScoreProfileSelection::Tetrio
        );
        let (result, derivation) = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_score_derivation_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("score Build execution");

        assert_eq!(result.field("score_summary_complete"), Some("true"));
        assert_eq!(result.field("score_profile_requested"), Some("tetrio"));
        assert_eq!(result.field("score_equality_basis"), Some("score-only"));
        assert!(result.exact_scoring_execution_batch().is_some());
        assert!(derivation.execution_source_complete());
        assert!(!derivation.pattern_winners().is_empty());
    }
}
