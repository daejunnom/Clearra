//! Closed option contract shared by every v2 Build capability.
//!
//! Build options are deliberately nominal instead of reusing PC option text.
//! The validator resolves a capability's fixed objective before execution and
//! keeps absence distinct from an explicitly selected default. This lets a
//! fixed subcommand accept the identical objective while rejecting a conflict
//! before any solver or document decoder is started.

use super::build_v2_contract::{
    BuildSuppliedSolutionEvaluationContract, BuildTargetSearchContract,
};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BuildQueueKnowledge {
    #[default]
    Oracle,
    VisibleSeven,
}

impl BuildQueueKnowledge {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oracle => "oracle",
            Self::VisibleSeven => "visible-7",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "oracle" => Some(Self::Oracle),
            "visible-7" => Some(Self::VisibleSeven),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BuildObjective {
    All,
    #[default]
    Unique,
    MinCover,
    MaxProbabilityMinimum,
    MaxScoreCover,
}

impl BuildObjective {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unique => "unique",
            Self::MinCover => "min-cover",
            Self::MaxProbabilityMinimum => "max-probability-minimum",
            Self::MaxScoreCover => "max-score-cover",
        }
    }

    /// `minimum-cover` is the sole compatibility alias. In particular, open
    /// spellings such as `minimum`, `minimals`, and `score` never acquire a
    /// Build objective through this parser.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "unique" => Some(Self::Unique),
            "min-cover" | "minimum-cover" => Some(Self::MinCover),
            "max-probability-minimum" => Some(Self::MaxProbabilityMinimum),
            "max-score-cover" => Some(Self::MaxScoreCover),
            _ => None,
        }
    }

    pub const fn is_portfolio(self) -> bool {
        matches!(
            self,
            Self::MinCover | Self::MaxProbabilityMinimum | Self::MaxScoreCover
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BuildScoreProfile {
    #[default]
    Tetrio,
    Guideline,
    JstrisUltra,
}

impl BuildScoreProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tetrio => "tetrio",
            Self::Guideline => "guideline",
            Self::JstrisUltra => "jstris-ultra",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tetrio" => Some(Self::Tetrio),
            "guideline" => Some(Self::Guideline),
            "jstris-ultra" => Some(Self::JstrisUltra),
            _ => None,
        }
    }

    pub const fn accuracy(self) -> &'static str {
        let _ = self;
        "basic-approximation"
    }

    pub const fn profile_specific_exact(self) -> bool {
        let _ = self;
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildExecutionSemantics {
    Reachable,
    TilingOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildV2Capability {
    Target(BuildTargetSearchContract),
    Supplied(BuildSuppliedSolutionEvaluationContract),
}

impl BuildV2Capability {
    pub const fn capability_id(self) -> &'static str {
        match self {
            Self::Target(contract) => contract.capability_id(),
            Self::Supplied(contract) => contract.capability_id(),
        }
    }

    pub const fn fixed_objective(self) -> Option<BuildObjective> {
        match self {
            Self::Supplied(BuildSuppliedSolutionEvaluationContract::Minimals) => {
                Some(BuildObjective::MinCover)
            }
            Self::Target(BuildTargetSearchContract::SetupCoverScore)
            | Self::Supplied(BuildSuppliedSolutionEvaluationContract::Score) => {
                Some(BuildObjective::MaxScoreCover)
            }
            _ => None,
        }
    }

    pub const fn score_capable(self) -> bool {
        matches!(
            self,
            Self::Target(BuildTargetSearchContract::SetupCoverScore)
                | Self::Supplied(BuildSuppliedSolutionEvaluationContract::Score)
        )
    }

    pub const fn supports_objective(self, objective: BuildObjective) -> bool {
        match objective {
            BuildObjective::All | BuildObjective::Unique => self.fixed_objective().is_none(),
            BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum => matches!(
                self,
                Self::Target(BuildTargetSearchContract::Cover)
                    | Self::Target(BuildTargetSearchContract::CongruentCover)
                    | Self::Target(BuildTargetSearchContract::SetupCover)
                    | Self::Supplied(BuildSuppliedSolutionEvaluationContract::Minimals)
            ),
            BuildObjective::MaxScoreCover => self.score_capable(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildV2OptionRequest {
    queue_knowledge: BuildQueueKnowledge,
    explicit_objective: Option<BuildObjective>,
    explicit_score_profile: Option<BuildScoreProfile>,
    explicit_initial_b2b: Option<u16>,
    execution_semantics: BuildExecutionSemantics,
}

impl Default for BuildV2OptionRequest {
    fn default() -> Self {
        Self {
            queue_knowledge: BuildQueueKnowledge::Oracle,
            explicit_objective: None,
            explicit_score_profile: None,
            explicit_initial_b2b: None,
            execution_semantics: BuildExecutionSemantics::Reachable,
        }
    }
}

impl BuildV2OptionRequest {
    pub const fn with_queue_knowledge(mut self, value: BuildQueueKnowledge) -> Self {
        self.queue_knowledge = value;
        self
    }

    pub const fn with_objective(mut self, value: BuildObjective) -> Self {
        self.explicit_objective = Some(value);
        self
    }

    pub const fn with_score_profile(mut self, value: BuildScoreProfile) -> Self {
        self.explicit_score_profile = Some(value);
        self
    }

    pub const fn with_initial_b2b(mut self, value: u16) -> Self {
        self.explicit_initial_b2b = Some(value);
        self
    }

    pub const fn with_execution_semantics(mut self, value: BuildExecutionSemantics) -> Self {
        self.execution_semantics = value;
        self
    }

    pub const fn queue_knowledge(self) -> BuildQueueKnowledge {
        self.queue_knowledge
    }

    // Retained for product adapters that need to distinguish defaults from explicit input.
    #[allow(dead_code)]
    pub const fn explicit_objective(self) -> Option<BuildObjective> {
        self.explicit_objective
    }

    // Retained for product adapters that need to distinguish defaults from explicit input.
    #[allow(dead_code)]
    pub const fn explicit_score_profile(self) -> Option<BuildScoreProfile> {
        self.explicit_score_profile
    }

    // Retained for product adapters that need to distinguish defaults from explicit input.
    #[allow(dead_code)]
    pub const fn explicit_initial_b2b(self) -> Option<u16> {
        self.explicit_initial_b2b
    }

    pub const fn execution_semantics(self) -> BuildExecutionSemantics {
        self.execution_semantics
    }

    pub fn validate(
        self,
        capability: BuildV2Capability,
    ) -> Result<ValidatedBuildV2Options, BuildV2OptionError> {
        if self.execution_semantics == BuildExecutionSemantics::TilingOnly
            && self.queue_knowledge == BuildQueueKnowledge::VisibleSeven
        {
            return Err(BuildV2OptionError::VisibleSevenUnavailableWithTilingOnly);
        }

        let fixed_objective = capability.fixed_objective();
        let objective = match (fixed_objective, self.explicit_objective) {
            (Some(fixed), Some(explicit)) if fixed != explicit => {
                return Err(BuildV2OptionError::FixedObjectiveConflict {
                    capability_id: capability.capability_id(),
                    fixed,
                    requested: explicit,
                });
            }
            (Some(fixed), _) => fixed,
            (None, Some(explicit)) => explicit,
            (None, None) => BuildObjective::Unique,
        };
        if !capability.supports_objective(objective) {
            return Err(BuildV2OptionError::ObjectiveUnavailable {
                capability_id: capability.capability_id(),
                requested: objective,
            });
        }

        if !capability.score_capable()
            && (self.explicit_score_profile.is_some() || self.explicit_initial_b2b.is_some())
        {
            return Err(BuildV2OptionError::ScoreOptionUnavailable {
                capability_id: capability.capability_id(),
            });
        }
        let score_profile = capability
            .score_capable()
            .then_some(self.explicit_score_profile.unwrap_or_default());
        let initial_b2b = capability
            .score_capable()
            .then_some(self.explicit_initial_b2b.unwrap_or(0));

        Ok(ValidatedBuildV2Options {
            queue_knowledge: self.queue_knowledge,
            objective,
            score_profile,
            initial_b2b,
            execution_semantics: self.execution_semantics,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedBuildV2Options {
    queue_knowledge: BuildQueueKnowledge,
    objective: BuildObjective,
    score_profile: Option<BuildScoreProfile>,
    initial_b2b: Option<u16>,
    execution_semantics: BuildExecutionSemantics,
}

impl ValidatedBuildV2Options {
    pub const fn queue_knowledge(self) -> BuildQueueKnowledge {
        self.queue_knowledge
    }

    pub const fn objective(self) -> BuildObjective {
        self.objective
    }

    pub const fn score_profile(self) -> Option<BuildScoreProfile> {
        self.score_profile
    }

    pub const fn initial_b2b(self) -> Option<u16> {
        self.initial_b2b
    }

    // Retained for product projections that expose the validated execution policy.
    #[allow(dead_code)]
    pub const fn execution_semantics(self) -> BuildExecutionSemantics {
        self.execution_semantics
    }

    pub const fn score_accuracy(self) -> Option<&'static str> {
        match self.score_profile {
            Some(profile) => Some(profile.accuracy()),
            None => None,
        }
    }

    pub const fn profile_specific_exact(self) -> Option<bool> {
        match self.score_profile {
            Some(profile) => Some(profile.profile_specific_exact()),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildV2OptionError {
    VisibleSevenUnavailableWithTilingOnly,
    FixedObjectiveConflict {
        capability_id: &'static str,
        fixed: BuildObjective,
        requested: BuildObjective,
    },
    ObjectiveUnavailable {
        capability_id: &'static str,
        requested: BuildObjective,
    },
    ScoreOptionUnavailable {
        capability_id: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_accept_only_the_frozen_public_spellings() {
        assert_eq!(
            BuildQueueKnowledge::parse("oracle"),
            Some(BuildQueueKnowledge::Oracle)
        );
        assert_eq!(
            BuildQueueKnowledge::parse("visible-7"),
            Some(BuildQueueKnowledge::VisibleSeven)
        );
        assert_eq!(BuildQueueKnowledge::parse("full"), None);
        assert_eq!(
            BuildObjective::parse("minimum-cover"),
            Some(BuildObjective::MinCover)
        );
        for rejected in ["minimum", "minimals", "max-score", "probability"] {
            assert_eq!(BuildObjective::parse(rejected), None, "{rejected}");
        }
        assert_eq!(
            BuildScoreProfile::parse("jstris-ultra"),
            Some(BuildScoreProfile::JstrisUltra)
        );
        assert_eq!(BuildScoreProfile::parse("jstris"), None);
    }

    #[test]
    fn fixed_subcommands_accept_the_same_objective_and_reject_conflicts() {
        let capability =
            BuildV2Capability::Supplied(BuildSuppliedSolutionEvaluationContract::Minimals);
        let same = BuildV2OptionRequest::default()
            .with_objective(BuildObjective::MinCover)
            .validate(capability)
            .expect("identical fixed objective");
        assert_eq!(same.objective(), BuildObjective::MinCover);

        assert_eq!(
            BuildV2OptionRequest::default()
                .with_objective(BuildObjective::Unique)
                .validate(capability),
            Err(BuildV2OptionError::FixedObjectiveConflict {
                capability_id: "build.evaluate.minimals",
                fixed: BuildObjective::MinCover,
                requested: BuildObjective::Unique,
            })
        );
    }

    #[test]
    fn portfolio_objectives_are_capability_scoped() {
        for contract in [
            BuildTargetSearchContract::Cover,
            BuildTargetSearchContract::CongruentCover,
            BuildTargetSearchContract::SetupCover,
        ] {
            let capability = BuildV2Capability::Target(contract);
            for objective in [
                BuildObjective::MinCover,
                BuildObjective::MaxProbabilityMinimum,
            ] {
                assert_eq!(
                    BuildV2OptionRequest::default()
                        .with_objective(objective)
                        .validate(capability)
                        .expect("coverage portfolio objective")
                        .objective(),
                    objective
                );
            }
        }
        assert!(matches!(
            BuildV2OptionRequest::default()
                .with_objective(BuildObjective::MinCover)
                .validate(BuildV2Capability::Target(BuildTargetSearchContract::Setup)),
            Err(BuildV2OptionError::ObjectiveUnavailable { .. })
        ));
    }

    #[test]
    fn score_options_are_closed_to_score_forms_and_disclose_approximation() {
        let score = BuildV2OptionRequest::default()
            .with_score_profile(BuildScoreProfile::Guideline)
            .with_initial_b2b(u16::MAX)
            .validate(BuildV2Capability::Target(
                BuildTargetSearchContract::SetupCoverScore,
            ))
            .expect("score form");
        assert_eq!(score.objective(), BuildObjective::MaxScoreCover);
        assert_eq!(score.score_profile(), Some(BuildScoreProfile::Guideline));
        assert_eq!(score.initial_b2b(), Some(u16::MAX));
        assert_eq!(score.score_accuracy(), Some("basic-approximation"));
        assert_eq!(score.profile_specific_exact(), Some(false));

        assert!(matches!(
            BuildV2OptionRequest::default()
                .with_initial_b2b(1)
                .validate(BuildV2Capability::Target(BuildTargetSearchContract::Cover)),
            Err(BuildV2OptionError::ScoreOptionUnavailable { .. })
        ));
    }

    #[test]
    fn visible_seven_is_exactly_the_tiling_only_incompatibility() {
        assert_eq!(
            BuildV2OptionRequest::default()
                .with_queue_knowledge(BuildQueueKnowledge::VisibleSeven)
                .with_execution_semantics(BuildExecutionSemantics::TilingOnly)
                .validate(BuildV2Capability::Target(BuildTargetSearchContract::Cover)),
            Err(BuildV2OptionError::VisibleSevenUnavailableWithTilingOnly)
        );
        assert_eq!(
            BuildV2OptionRequest::default()
                .with_queue_knowledge(BuildQueueKnowledge::VisibleSeven)
                .validate(BuildV2Capability::Target(BuildTargetSearchContract::Cover))
                .expect("reachable visible-seven")
                .queue_knowledge(),
            BuildQueueKnowledge::VisibleSeven
        );
    }
}
