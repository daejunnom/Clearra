use clearra_scoring::{
    export::ScoreProfileExport,
    import::ScoreProfileImport,
    profile::{ScoreProfile, ScoreProfileRegistry},
};
use clearra_validation::validators::score_profile_validator::validate_score_profile;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::string_field,
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScoringAppCommand {
    action: ScoringAppAction,
    profile: Option<String>,
    input: Option<String>,
}

impl ScoringAppCommand {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: ScoringAppAction::parse(&action.into()),
            profile: None,
            input: None,
        }
    }
}
impl ScoringAppCommand {
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }
}
impl ScoringAppCommand {
    pub fn with_input(mut self, input: Option<String>) -> Self {
        self.input = input;
        self
    }
}

impl RunnableAppCommand for ScoringAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        match self.action {
            ScoringAppAction::List => scoring_success(list_fields()),
            ScoringAppAction::Inspect => inspect_scoring(self.profile.as_deref()),
            ScoringAppAction::Import => import_scoring(self.input.as_deref()),
            ScoringAppAction::Export => export_scoring(self.profile.as_deref()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoringAppAction {
    #[default]
    List,
    Inspect,
    Import,
    Export,
}

impl ScoringAppAction {
    fn parse(value: &str) -> Self {
        match value {
            "inspect" => Self::Inspect,
            "import" => Self::Import,
            "export" => Self::Export,
            _ => Self::List,
        }
    }
}

fn scoring_success(fields: Vec<(String, String)>) -> AppResponse {
    AppResponse::success(AppRenderModel::Scoring(AppMessage::new(
        AppResultKind::Scoring,
        fields
            .into_iter()
            .map(|(key, value)| string_field(key, value))
            .collect(),
    )))
}

fn scoring_error(code: AppErrorCode, message: impl Into<String>) -> AppResponse {
    AppResponse::failed(AppStatus::ExecutionFailed, AppError::new(code, message))
}

fn list_fields() -> Vec<(String, String)> {
    let registry = ScoreProfileRegistry::builtins();
    let mut fields = vec![
        ("action".to_owned(), "list".to_owned()),
        (
            "profile_count".to_owned(),
            registry.profiles().len().to_string(),
        ),
    ];
    for (index, profile) in registry.profiles().iter().enumerate() {
        fields.extend(profile_fields(profile, Some(index)));
    }
    fields
}

fn inspect_scoring(profile_id: Option<&str>) -> AppResponse {
    let Some(profile_id) = profile_id else {
        return scoring_error(
            AppErrorCode::ScoringProfileUnknown,
            "scoring inspect requires --profile <id>",
        );
    };
    let registry = ScoreProfileRegistry::builtins();
    let Some(profile) = registry.get(profile_id) else {
        return scoring_error(
            AppErrorCode::ScoringProfileUnknown,
            format!("unknown scoring profile '{profile_id}'"),
        );
    };
    let mut fields = vec![("action".to_owned(), "inspect".to_owned())];
    fields.extend(profile_fields(profile, None));
    scoring_success(fields)
}

fn import_scoring(input: Option<&str>) -> AppResponse {
    let Some(input) = input else {
        return scoring_error(
            AppErrorCode::ScoringInputRequired,
            "scoring import requires --input <json>",
        );
    };
    let profile = match ScoreProfileImport::from_json(input) {
        Ok(profile) => profile,
        Err(error) => {
            return scoring_error(
                AppErrorCode::ScoringInputInvalid,
                format!("invalid score profile JSON: {}", error.code()),
            )
        }
    };
    let report = validate_score_profile(&profile);
    if report.has_errors() {
        return AppResponse::validation_failed(report);
    }

    let mut fields = vec![
        ("action".to_owned(), "import".to_owned()),
        (
            "diagnostic_count".to_owned(),
            report.diagnostics().len().to_string(),
        ),
    ];
    fields.extend(profile_fields(&profile, None));
    scoring_success(fields)
}

fn export_scoring(profile_id: Option<&str>) -> AppResponse {
    let Some(profile_id) = profile_id else {
        return scoring_error(
            AppErrorCode::ScoringProfileUnknown,
            "scoring export requires --profile <id>",
        );
    };
    let registry = ScoreProfileRegistry::builtins();
    let Some(profile) = registry.get(profile_id) else {
        return scoring_error(
            AppErrorCode::ScoringProfileUnknown,
            format!("unknown scoring profile '{profile_id}'"),
        );
    };
    let json = match ScoreProfileExport::to_json(profile) {
        Ok(json) => json,
        Err(error) => {
            return scoring_error(
                AppErrorCode::ScoringInputInvalid,
                format!("failed to export score profile: {error:?}"),
            )
        }
    };
    scoring_success(vec![
        ("action".to_owned(), "export".to_owned()),
        ("profile".to_owned(), profile.id().to_owned()),
        ("json".to_owned(), json),
    ])
}

fn profile_fields(profile: &ScoreProfile, indexed: Option<usize>) -> Vec<(String, String)> {
    let prefix = indexed
        .map(|index| format!("profile_{index}_"))
        .unwrap_or_default();
    let combo = profile.combo_policy();
    let b2b = profile.b2b_policy();
    vec![
        (format!("{prefix}id"), profile.id().to_owned()),
        (
            format!("{prefix}display_name"),
            profile.display_name().to_owned(),
        ),
        (
            format!("{prefix}score_model"),
            profile.score_model().as_str().to_owned(),
        ),
        (
            format!("{prefix}attack_model"),
            profile.attack_model().as_str().to_owned(),
        ),
        (
            format!("{prefix}spin_rule"),
            profile.spin_rule().as_str().to_owned(),
        ),
        (
            format!("{prefix}accuracy_level"),
            profile.accuracy_level().as_str().to_owned(),
        ),
        (
            format!("{prefix}profile_specific_exact"),
            profile.profile_specific_exact().to_string(),
        ),
        (
            format!("{prefix}accuracy_reason"),
            profile.accuracy_reason().to_owned(),
        ),
        (
            format!("{prefix}combo_enabled"),
            combo.enabled().to_string(),
        ),
        (
            format!("{prefix}combo_score_bonus_per_combo"),
            combo.score_bonus_per_combo().to_string(),
        ),
        (
            format!("{prefix}combo_attack_bonus_per_combo"),
            combo.attack_bonus_per_combo().to_string(),
        ),
        (format!("{prefix}b2b_enabled"), b2b.enabled().to_string()),
        (
            format!("{prefix}b2b_score_bonus"),
            b2b.score_bonus().to_string(),
        ),
        (
            format!("{prefix}b2b_attack_bonus"),
            b2b.attack_bonus().to_string(),
        ),
    ]
}
