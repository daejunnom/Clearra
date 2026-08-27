use std::fmt;

use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy,
    score_objective_policy::{ScoreProfileSelection, SpinProfileSelection},
};
use clearra_profiles::{
    bag::bag_profile::BagProfileId, board::board_profile::BoardProfileId,
    pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_rules::profile::{
    rule_capability::RuleCapability,
    rule_profile::{RuleProfile, RuleProfileId},
};
use clearra_scoring::profile::{SpinProfileId, SpinProfileRegistry};

use crate::{app_command::AppCommand, pc_result_projection::PcResultProjection};

/// The structural profiles accepted by the current exact standard-board
/// engines. Keeping this value typed and request-owned prevents a frontend
/// preference or process-global setting from silently changing a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestStructuralProfiles {
    board: BoardProfileId,
    piece_set: PieceSetProfileId,
    bag: BagProfileId,
}

impl RequestStructuralProfiles {
    pub const STANDARD: Self = Self {
        board: BoardProfileId::Standard10,
        piece_set: PieceSetProfileId::StandardTetrominoes,
        bag: BagProfileId::Standard7Bag,
    };

    pub fn parse_canonical(
        board: &str,
        piece_set: &str,
        bag: &str,
    ) -> Result<Self, RequestProfileSelectionError> {
        let board = match board {
            "standard-10" => BoardProfileId::Standard10,
            value => {
                return Err(RequestProfileSelectionError::UnsupportedProfile {
                    kind: "board",
                    value: value.to_owned(),
                })
            }
        };
        let piece_set = match piece_set {
            "standard-tetrominoes" => PieceSetProfileId::StandardTetrominoes,
            value => {
                return Err(RequestProfileSelectionError::UnsupportedProfile {
                    kind: "piece",
                    value: value.to_owned(),
                })
            }
        };
        let bag = match bag {
            "standard-7-bag" => BagProfileId::Standard7Bag,
            value => {
                return Err(RequestProfileSelectionError::UnsupportedProfile {
                    kind: "bag",
                    value: value.to_owned(),
                })
            }
        };
        Ok(Self {
            board,
            piece_set,
            bag,
        })
    }

    pub const fn board(self) -> BoardProfileId {
        self.board
    }

    pub const fn piece_set(self) -> PieceSetProfileId {
        self.piece_set
    }

    pub const fn bag(self) -> BagProfileId {
        self.bag
    }
}

impl Default for RequestStructuralProfiles {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Six-profile request-local authority. Rule, spin, and score are copied from
/// the typed command that is actually executed; the structural profiles are
/// selected independently at ingress and remain closed to verified built-ins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestProfileSelection {
    structural: RequestStructuralProfiles,
    rule: RuleProfileId,
    spin: SpinProfileId,
    score: ScoreProfileSelection,
}

impl RequestProfileSelection {
    pub const STANDARD: Self = Self {
        structural: RequestStructuralProfiles::STANDARD,
        rule: RuleProfileId::SrsPlus,
        spin: SpinProfileId::TSpins,
        score: ScoreProfileSelection::Tetrio,
    };

    pub(crate) fn for_command(command: &AppCommand) -> Self {
        let expected = command_profile_expectations(command);
        Self {
            structural: RequestStructuralProfiles::STANDARD,
            rule: expected.rule.unwrap_or(RuleProfileId::SrsPlus),
            spin: expected.spin.unwrap_or(SpinProfileId::TSpins),
            score: expected.score.unwrap_or(ScoreProfileSelection::Tetrio),
        }
    }

    pub fn with_structural_profiles(mut self, structural: RequestStructuralProfiles) -> Self {
        self.structural = structural;
        self
    }

    pub const fn structural(self) -> RequestStructuralProfiles {
        self.structural
    }

    pub const fn board(self) -> BoardProfileId {
        self.structural.board()
    }

    pub const fn piece_set(self) -> PieceSetProfileId {
        self.structural.piece_set()
    }

    pub const fn bag(self) -> BagProfileId {
        self.structural.bag()
    }

    pub const fn rule(self) -> RuleProfileId {
        self.rule
    }

    pub const fn spin(self) -> SpinProfileId {
        self.spin
    }

    pub const fn score(self) -> ScoreProfileSelection {
        self.score
    }

    pub(crate) fn validate_for_command(
        self,
        command: &AppCommand,
    ) -> Result<(), RequestProfileSelectionError> {
        self.validate_supported()?;
        let expected = command_profile_expectations(command);
        if let Some(expected) = expected.rule {
            if self.rule != expected {
                return Err(RequestProfileSelectionError::CommandProfileMismatch {
                    kind: "rule",
                    requested: self.rule.as_str(),
                    command: expected.as_str(),
                });
            }
        }
        if let Some(expected) = expected.spin {
            if self.spin != expected {
                return Err(RequestProfileSelectionError::CommandProfileMismatch {
                    kind: "spin",
                    requested: self.spin.as_str(),
                    command: expected.as_str(),
                });
            }
        }
        if let Some(expected) = expected.score {
            if self.score != expected {
                return Err(RequestProfileSelectionError::CommandProfileMismatch {
                    kind: "score",
                    requested: self.score.as_str(),
                    command: expected.as_str(),
                });
            }
        }
        Ok(())
    }

    fn validate_supported(self) -> Result<(), RequestProfileSelectionError> {
        if self.structural != RequestStructuralProfiles::STANDARD {
            return Err(RequestProfileSelectionError::IncompatibleStructuralProfiles);
        }
        let rule = RuleProfile::new(self.rule);
        if !RuleCapability::from_rule(rule).search_backend_supported() {
            return Err(RequestProfileSelectionError::UnverifiedOrUnsupportedRule {
                value: self.rule.as_str(),
            });
        }
        if self.spin != SpinProfileId::Disabled
            && SpinProfileRegistry::builtins().get(self.spin).is_none()
        {
            return Err(RequestProfileSelectionError::UnsupportedSpin {
                value: self.spin.as_str(),
            });
        }
        Ok(())
    }
}

impl Default for RequestProfileSelection {
    fn default() -> Self {
        Self::STANDARD
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestProfileSelectionError {
    UnsupportedProfile {
        kind: &'static str,
        value: String,
    },
    IncompatibleStructuralProfiles,
    UnverifiedOrUnsupportedRule {
        value: &'static str,
    },
    UnsupportedSpin {
        value: &'static str,
    },
    CommandProfileMismatch {
        kind: &'static str,
        requested: &'static str,
        command: &'static str,
    },
}

impl fmt::Display for RequestProfileSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProfile { kind, value } => {
                write!(formatter, "unsupported request {kind} profile '{value}'")
            }
            Self::IncompatibleStructuralProfiles => formatter.write_str(
                "request board, piece, and bag profiles are not a verified compatible bundle",
            ),
            Self::UnverifiedOrUnsupportedRule { value } => write!(
                formatter,
                "request rule profile '{value}' is unverified or unsupported by search"
            ),
            Self::UnsupportedSpin { value } => {
                write!(formatter, "request spin profile '{value}' is unsupported")
            }
            Self::CommandProfileMismatch {
                kind,
                requested,
                command,
            } => write!(
                formatter,
                "request {kind} profile '{requested}' does not match command profile '{command}'"
            ),
        }
    }
}

impl std::error::Error for RequestProfileSelectionError {}

#[derive(Clone, Copy, Debug, Default)]
struct CommandProfileExpectations {
    rule: Option<RuleProfileId>,
    spin: Option<SpinProfileId>,
    score: Option<ScoreProfileSelection>,
}

fn command_profile_expectations(command: &AppCommand) -> CommandProfileExpectations {
    match command {
        AppCommand::Pc(command) => profiles_for_pc(
            command.query().rule().id(),
            command.query().objective(),
            command.result_projection(),
        ),
        AppCommand::Scenario(command) => profiles_for_pc(
            command.query().rule().id(),
            command.query().objective(),
            command.result_projection(),
        ),
        AppCommand::Path(command) => {
            profiles_for_objective(command.query().rule().id(), command.query().objective())
        }
        AppCommand::Percent(command) => command
            .query()
            .map(|query| profiles_for_objective(query.rule().id(), query.objective()))
            .or_else(|| {
                command
                    .opening_query()
                    .map(|query| profiles_for_objective(query.rule().id(), query.objective()))
            })
            .unwrap_or_default(),
        AppCommand::Setup(command) => CommandProfileExpectations {
            rule: Some(command.query().rule().id()),
            ..CommandProfileExpectations::default()
        },
        AppCommand::SetupScore(command) => CommandProfileExpectations {
            rule: Some(command.rule().id()),
            score: Some(command.score_profile()),
            ..CommandProfileExpectations::default()
        },
        AppCommand::BuildProbability(command) => profiles_for_build(command.query()),
        AppCommand::BuildV2(command) => profiles_for_build(command.request_profile_query()),
        AppCommand::Damage(command) => profiles_for_forward(command.query()),
        AppCommand::SpinFinder(command) => profiles_for_forward(command.query()),
        AppCommand::Ren(command) => profiles_for_forward(command.query()),
        AppCommand::SpinStructure(command) => CommandProfileExpectations {
            rule: Some(command.query().rule_profile),
            spin: Some(command.query().mode.profile().id()),
            score: None,
        },
        AppCommand::UtilitySequence(command) => CommandProfileExpectations {
            rule: Some(command.problem().rule_profile),
            ..CommandProfileExpectations::default()
        },
        AppCommand::UtilitySequenceDependencies(command) => CommandProfileExpectations {
            rule: Some(command.problem().rule_profile),
            ..CommandProfileExpectations::default()
        },
        AppCommand::Cover(_)
        | AppCommand::Rules(_)
        | AppCommand::Scoring(_)
        | AppCommand::Convert(_)
        | AppCommand::Continue(_)
        | AppCommand::InspectUnsupported(_)
        | AppCommand::Verify(_)
        | AppCommand::VerifyKicks(_)
        | AppCommand::UtilityParity(_)
        | AppCommand::UtilityFumen(_)
        | AppCommand::UtilityRender(_)
        | AppCommand::UtilityToGray(_)
        | AppCommand::UtilityMirror(_) => CommandProfileExpectations::default(),
    }
}

fn profiles_for_build(
    query: &clearra_problem::BuildProbabilityQuery,
) -> CommandProfileExpectations {
    let mut profiles = profiles_for_objective(
        query.core_query().rule().id(),
        query.core_query().objective(),
    );
    if let Some(profile) = query.aggregation().spin_profile() {
        profiles.spin = Some(spin_profile_id(profile));
    }
    profiles
}

fn profiles_for_pc(
    rule: RuleProfileId,
    objective: ObjectivePolicy,
    projection: PcResultProjection,
) -> CommandProfileExpectations {
    let mut profiles = profiles_for_objective(rule, objective);
    if let Some(profile) = projection.spin_profile() {
        profiles.spin = Some(spin_profile_id(profile));
    }
    profiles
}

fn profiles_for_objective(
    rule: RuleProfileId,
    objective: ObjectivePolicy,
) -> CommandProfileExpectations {
    let score = objective.score();
    let constraints = objective.execution_constraints();
    CommandProfileExpectations {
        rule: Some(rule),
        spin: if score.requested() {
            Some(spin_profile_id(score.spin_profile()))
        } else if constraints.requested() {
            Some(spin_profile_id(constraints.spin_profile()))
        } else {
            None
        },
        score: score.requested().then_some(score.profile()),
    }
}

fn profiles_for_forward(
    query: &clearra_forward_search::ForwardSearchQuery,
) -> CommandProfileExpectations {
    CommandProfileExpectations {
        rule: Some(query.rule_profile()),
        spin: Some(query.spin_profile()),
        score: None,
    }
}

const fn spin_profile_id(profile: SpinProfileSelection) -> SpinProfileId {
    match profile {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy,
        score_objective_policy::{ScoreProfileSelection, SpinProfileSelection},
    };
    use clearra_pc_graph::request::OpeningPcSearchQuery;
    use clearra_rules::profile::rule_profile::{RuleProfile, RuleProfileId};

    use super::*;
    use crate::{commands::PcAppCommand, AppCommand, AppContext, AppRequest, AppStatus};

    #[test]
    fn structural_profiles_accept_only_the_verified_canonical_bundle() {
        assert_eq!(
            RequestStructuralProfiles::parse_canonical(
                "standard-10",
                "standard-tetrominoes",
                "standard-7-bag",
            ),
            Ok(RequestStructuralProfiles::STANDARD)
        );
        for (board, piece, bag) in [
            ("wide-10", "standard-tetrominoes", "standard-7-bag"),
            ("standard-10", "pentominoes", "standard-7-bag"),
            ("standard-10", "standard-tetrominoes", "history-6-rolls"),
        ] {
            assert!(RequestStructuralProfiles::parse_canonical(board, piece, bag).is_err());
        }
    }

    #[test]
    fn command_profiles_are_request_local_and_match_actual_semantics() {
        let objective = ObjectivePolicy::all()
            .with_score_profile(ScoreProfileSelection::Guideline)
            .with_spin_profile(SpinProfileSelection::AllMiniPlus);
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_rule(RuleProfile::new(RuleProfileId::SrsX))
            .with_objective(objective);
        let command = AppCommand::Pc(PcAppCommand::new(query));
        let profiles = RequestProfileSelection::for_command(&command);

        assert_eq!(profiles.rule(), RuleProfileId::SrsX);
        assert_eq!(profiles.spin(), SpinProfileId::AllMiniPlus);
        assert_eq!(profiles.score(), ScoreProfileSelection::Guideline);
        assert_eq!(profiles.validate_for_command(&command), Ok(()));
    }

    #[test]
    fn unverified_and_search_unsupported_rules_fail_closed() {
        for rule in [
            RuleProfileId::Asc,
            RuleProfileId::Ars,
            RuleProfileId::Custom,
        ] {
            let command = AppCommand::Pc(PcAppCommand::new(
                OpeningPcSearchQuery::new(PcTarget::two_lines()).with_rule(RuleProfile::new(rule)),
            ));
            let profiles = RequestProfileSelection::for_command(&command);
            assert!(matches!(
                profiles.validate_for_command(&command),
                Err(RequestProfileSelectionError::UnverifiedOrUnsupportedRule { .. })
            ));
            let response = AppContext::default().run(AppRequest::new(command));
            assert_eq!(response.status(), AppStatus::ValidationFailed);
            assert!(response.error().is_some_and(|error| error
                .message()
                .contains("unverified or unsupported by search")));
        }
    }
}
