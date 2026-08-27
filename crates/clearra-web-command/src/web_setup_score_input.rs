use clearra_app::{
    FieldDocumentFormat, SetupScoreAppCommand, SetupScoreDocumentV1, PC_SCORE_MAX_SOURCE_PIECES,
};
use clearra_objectives::policy::score_objective_policy::ScoreProfileSelection;
use clearra_pc_graph::request::{PcExecutionPolicy, PcQueueInput};
use clearra_rules::profile::rule_profile::RuleProfile;
use clearra_supply::queue::{queue_parser, queue_pattern_expression::QueuePatternExpression};

use crate::{WebCommandError, WebCommandErrorCode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebSetupScoreQueueInput {
    Queue(String),
    Patterns(String),
}

impl WebSetupScoreQueueInput {
    pub fn queue(value: impl Into<String>) -> Self {
        Self::Queue(value.into())
    }

    pub fn patterns(value: impl Into<String>) -> Self {
        Self::Patterns(value.into())
    }

    fn lower(
        &self,
        max_patterns: usize,
        score_queue: bool,
    ) -> Result<(PcQueueInput, Option<usize>), WebCommandError> {
        match self {
            Self::Queue(value) => {
                let queue = queue_parser::parse_fixed_sequence(value).map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid Setup score queue: {error:?}"),
                    )
                })?;
                if score_queue && queue.len() > PC_SCORE_MAX_SOURCE_PIECES {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "Setup score continuation accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"
                        ),
                    ));
                }
                Ok((PcQueueInput::fixed_sequence(queue), None))
            }
            Self::Patterns(value) => {
                if let Some((leading, length)) =
                    QueuePatternExpression::standard_7_bag_with_optional_leading_piece(value)
                {
                    if leading.is_some() {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "Setup score does not accept an occupied-hold prefix",
                        ));
                    }
                    if score_queue && length > PC_SCORE_MAX_SOURCE_PIECES {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "Setup score continuation accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"
                            ),
                        ));
                    }
                    return Ok((PcQueueInput::standard_7_bag(), Some(length)));
                }
                let expression =
                    QueuePatternExpression::parse(value, max_patterns).map_err(|error| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid Setup score pattern: {error}"),
                        )
                    })?;
                if score_queue && expression.sequence_len() > PC_SCORE_MAX_SOURCE_PIECES {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "Setup score continuation accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"
                        ),
                    ));
                }
                Ok((PcQueueInput::pattern_expression(expression), None))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSetupScoreInput {
    document_format: FieldDocumentFormat,
    document: String,
    setup_source: WebSetupScoreQueueInput,
    solution_source: WebSetupScoreQueueInput,
    clear_height: u8,
    setup_hold_enabled: bool,
    score_profile: ScoreProfileSelection,
    initial_b2b: u32,
}

impl WebSetupScoreInput {
    pub fn new(
        document_format: FieldDocumentFormat,
        document: impl Into<String>,
        setup_source: WebSetupScoreQueueInput,
        solution_source: WebSetupScoreQueueInput,
    ) -> Self {
        Self {
            document_format,
            document: document.into(),
            setup_source,
            solution_source,
            clear_height: 4,
            setup_hold_enabled: true,
            score_profile: ScoreProfileSelection::Tetrio,
            initial_b2b: 0,
        }
    }

    pub const fn with_clear_height(mut self, clear_height: u8) -> Self {
        self.clear_height = clear_height;
        self
    }

    pub const fn with_setup_hold_enabled(mut self, enabled: bool) -> Self {
        self.setup_hold_enabled = enabled;
        self
    }

    pub const fn with_score_profile(mut self, profile: ScoreProfileSelection) -> Self {
        self.score_profile = profile;
        self
    }

    pub const fn with_initial_b2b(mut self, initial_b2b: u32) -> Self {
        self.initial_b2b = initial_b2b;
        self
    }

    pub const fn document_format(&self) -> FieldDocumentFormat {
        self.document_format
    }

    pub fn document(&self) -> &str {
        &self.document
    }

    pub const fn setup_hold_enabled(&self) -> bool {
        self.setup_hold_enabled
    }

    pub(crate) fn to_app_command(
        &self,
        execution_policy: PcExecutionPolicy,
        rule: RuleProfile,
    ) -> Result<SetupScoreAppCommand, WebCommandError> {
        let document = SetupScoreDocumentV1::decode(self.document_format, &self.document).map_err(
            |error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid Setup score document: {error:?}"),
                )
            },
        )?;
        let (setup_queue, setup_standard_bag_len) = self
            .setup_source
            .lower(execution_policy.max_patterns(), false)?;
        let (solution_queue, solution_standard_bag_len) = self
            .solution_source
            .lower(execution_policy.max_patterns(), true)?;
        SetupScoreAppCommand::new(
            document,
            setup_queue,
            setup_standard_bag_len,
            solution_queue,
            solution_standard_bag_len,
            self.clear_height,
            self.setup_hold_enabled,
            self.score_profile,
            self.initial_b2b,
            rule,
            execution_policy,
        )
        .map_err(|error| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("invalid Setup score request: {error:?}"),
            )
        })
    }
}
