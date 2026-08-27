use clearra_core_domain::pc::pc_target::{PcTarget, PcTargetError};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_objectives::policy::score_objective_policy::{
    ScoreProfileSelection, SpinProfileSelection,
};
use clearra_pc_graph::request::{
    opening_pc_search_query::OpeningPcSearchQuery, pc_hold_policy::PcHoldPolicy,
    pc_queue_input::PcQueueInput, validate_pc_observation_objective, PcSearchContractError,
    PcSolutionProbabilityPolicy,
};
use clearra_supply::queue::queue_parser::{parse_observed_queue, QueueParseError};

use crate::{
    args::pc_args::PcArgs,
    assemble::{
        execution_policy_assembler::{ExecutionPolicyAssembler, ExecutionPolicyAssemblyError},
        piece_sequence_assembler::PieceSequenceAssembler,
        rule_profile_assembler::{RuleProfileAssembler, RuleProfileAssemblyError},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcQueryAssemblyError {
    SearchContract(PcSearchContractError),
    InvalidTarget(PcTargetError),
    UnsupportedMvpTarget {
        lines: u8,
    },
    UnknownPiece {
        index: usize,
        value: char,
    },
    UnsupportedObjective {
        value: String,
    },
    UnsupportedScoreProfile {
        value: String,
    },
    UnsupportedSpinProfile {
        value: String,
    },
    IncompatibleTilingOnlyOption {
        option: &'static str,
    },
    UnknownRuleProfile {
        value: String,
    },
    InvalidKickProfileJson {
        code: &'static str,
    },
    InvalidExecutionPolicy {
        message: String,
    },
    UnverifiedKickProfile {
        issue_count: usize,
        missing_transition_count: usize,
        duplicate_transition_count: usize,
        unsupported_annotation_count: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcQueryAssembler;

impl PcQueryAssembler {
    pub fn assemble(args: &PcArgs) -> Result<OpeningPcSearchQuery, PcQueryAssemblyError> {
        let target = PcTarget::new(args.lines()).map_err(PcQueryAssemblyError::InvalidTarget)?;
        if !matches!(target.lines(), 2 | 4 | 6) {
            return Err(PcQueryAssemblyError::UnsupportedMvpTarget {
                lines: target.lines(),
            });
        }
        let mut objective = parse_objective(args.objective())?;
        validate_pc_observation_objective(args.queue_observation_policy(), objective.kind())
            .map_err(PcQueryAssemblyError::SearchContract)?;
        let tiling_only = objective.kind()
            == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling;
        if tiling_only {
            validate_tiling_only_options(args)?;
        }
        let query = OpeningPcSearchQuery::new(target);
        let queue = parse_queue(args)?;
        let hold_policy = if args.hold_enabled() {
            PcHoldPolicy::EnabledEmpty
        } else {
            PcHoldPolicy::Disabled
        };
        let spin_profile = args
            .spin_profile()
            .map(|value| {
                SpinProfileSelection::parse(value).ok_or_else(|| {
                    PcQueryAssemblyError::UnsupportedSpinProfile {
                        value: value.to_owned(),
                    }
                })
            })
            .transpose()?;
        if args.score_requested() && !objective.score().requested() {
            objective = objective.with_score_summary();
        }
        if let Some(initial_b2b) = args.initial_b2b() {
            objective = objective.with_initial_b2b(initial_b2b);
        }
        if let Some(value) = args.score_profile() {
            let profile = ScoreProfileSelection::parse(value).ok_or_else(|| {
                PcQueryAssemblyError::UnsupportedScoreProfile {
                    value: value.to_owned(),
                }
            })?;
            objective = objective.with_score_profile(profile);
        }
        if args.score_requested() {
            if let Some(profile) = spin_profile {
                objective = objective.with_spin_profile(profile);
            }
        }
        if args.preserves_back_to_back() {
            let profile = spin_profile.unwrap_or(SpinProfileSelection::TSpins);
            objective = objective.with_back_to_back_preservation(profile);
        }
        let rule = parse_rule(args.rule())?;
        let verified_profile = parse_verified_kick_profile(args.kick_profile_json())?;
        let execution_policy = ExecutionPolicyAssembler::from_pc_args(args)
            .map_err(pc_query_error_from_execution_policy_error)?;

        let mut query = query
            .with_queue(queue)
            .with_hold_policy(hold_policy)
            .with_queue_observation_policy(args.queue_observation_policy())
            .with_rule(rule)
            .with_objective(objective)
            .with_execution_policy(execution_policy);
        if let Some(profile) = verified_profile {
            query = query.with_verified_kick_table_profile(profile);
        }
        if args.solution_probabilities() {
            query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        }
        Ok(query)
    }
}

fn parse_queue(args: &PcArgs) -> Result<PcQueueInput, PcQueryAssemblyError> {
    let map_error = |error| match error {
        QueueParseError::UnknownPiece { index, value } => {
            PcQueryAssemblyError::UnknownPiece { index, value }
        }
    };

    if !args.fixed_queue() && args.queue().trim().is_empty() {
        Ok(PcQueueInput::standard_7_bag())
    } else if args.fixed_queue() {
        PieceSequenceAssembler::parse_fixed_sequence(args.queue())
            .map(PcQueueInput::fixed_sequence)
            .map_err(|error| match error {
                crate::assemble::piece_sequence_assembler::PieceSequenceAssemblyError::UnknownPiece {
                    index,
                    value,
                } => PcQueryAssemblyError::UnknownPiece { index, value },
            })
    } else {
        parse_observed_queue(args.queue())
            .map(PcQueueInput::observed)
            .map_err(map_error)
    }
}

fn parse_rule(
    value: Option<&str>,
) -> Result<clearra_rules::profile::rule_profile::RuleProfile, PcQueryAssemblyError> {
    RuleProfileAssembler::parse_optional_rule(
        value,
        clearra_rules::profile::builtin_rules::srs_plus(),
    )
    .map_err(pc_query_error_from_rule_profile_error)
}

fn parse_verified_kick_profile(
    value: Option<&str>,
) -> Result<Option<clearra_rules::kicks::VerifiedKickTableProfile>, PcQueryAssemblyError> {
    RuleProfileAssembler::parse_verified_kick_profile(value)
        .map_err(pc_query_error_from_rule_profile_error)
}

fn pc_query_error_from_rule_profile_error(error: RuleProfileAssemblyError) -> PcQueryAssemblyError {
    match error {
        RuleProfileAssemblyError::UnknownRuleProfile { value } => {
            PcQueryAssemblyError::UnknownRuleProfile { value }
        }
        RuleProfileAssemblyError::InvalidKickProfileJson { code } => {
            PcQueryAssemblyError::InvalidKickProfileJson { code }
        }
        RuleProfileAssemblyError::UnverifiedKickProfile {
            issue_count,
            missing_transition_count,
            duplicate_transition_count,
            unsupported_annotation_count,
        } => PcQueryAssemblyError::UnverifiedKickProfile {
            issue_count,
            missing_transition_count,
            duplicate_transition_count,
            unsupported_annotation_count,
        },
    }
}

fn pc_query_error_from_execution_policy_error(
    error: ExecutionPolicyAssemblyError,
) -> PcQueryAssemblyError {
    PcQueryAssemblyError::InvalidExecutionPolicy {
        message: error.message(),
    }
}

fn parse_objective(value: &str) -> Result<ObjectivePolicy, PcQueryAssemblyError> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "" | "all" => Ok(ObjectivePolicy::all()),
        "unique" => Ok(ObjectivePolicy::unique()),
        "minimum-cover" | "min-cover" => Ok(ObjectivePolicy::minimum_cover()),
        "tiling" | "tiling-only" => Ok(ObjectivePolicy::tiling()),
        _ => Err(PcQueryAssemblyError::UnsupportedObjective {
            value: value.to_owned(),
        }),
    }
}

fn validate_tiling_only_options(args: &PcArgs) -> Result<(), PcQueryAssemblyError> {
    let incompatible = [
        (args.score_requested(), "--score"),
        (args.score_profile().is_some(), "--score-profile"),
        (args.spin_profile().is_some(), "--spin-profile"),
        (args.preserves_back_to_back(), "--preserve-b2b"),
        (args.initial_b2b().is_some(), "--initial-b2b"),
        (args.rule().is_some(), "--rule"),
        (args.kick_profile_json().is_some(), "--kick-profile-json"),
        (args.tablebase_requested() == Some(true), "--tablebase"),
        (
            args.precompute_build_dependencies() == Some(true),
            "--build-dependency-dag",
        ),
        (args.solution_probabilities(), "--solution-probabilities"),
        (
            args.queue_observation_policy()
                .requires_observation_policy(),
            "--queue-knowledge",
        ),
    ];
    if let Some((_, option)) = incompatible.into_iter().find(|(enabled, _)| *enabled) {
        return Err(PcQueryAssemblyError::IncompatibleTilingOnlyOption { option });
    }
    Ok(())
}

#[cfg(test)]
#[path = "pc_query_assembler_tests.rs"]
mod tests;
