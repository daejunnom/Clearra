use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_response::AppResponse,
    commands::rules_app_handlers::{inspect_rules, list_fields, verify_rules},
    commands::rules_app_import_export::{export_rules, import_rules},
    commands::rules_app_output::rules_success,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RulesAppCommand {
    action: RulesAppAction,
    profile: Option<String>,
    input: Option<String>,
}

impl RulesAppCommand {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: RulesAppAction::parse(&action.into()),
            profile: None,
            input: None,
        }
    }
}
impl RulesAppCommand {
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }
}
impl RulesAppCommand {
    pub fn with_input(mut self, input: Option<String>) -> Self {
        self.input = input;
        self
    }
}

impl RunnableAppCommand for RulesAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let _file_resolver = context.file_resolver();
        match self.action {
            RulesAppAction::List => rules_success(list_fields()),
            RulesAppAction::Inspect => inspect_rules(self.profile.as_deref()),
            RulesAppAction::Verify => verify_rules(self.input.as_deref()),
            RulesAppAction::Import => import_rules(self.input.as_deref()),
            RulesAppAction::Export => export_rules(self.profile.as_deref()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RulesAppAction {
    #[default]
    List,
    Inspect,
    Verify,
    Import,
    Export,
}

impl RulesAppAction {
    fn parse(value: &str) -> Self {
        match value {
            "inspect" => Self::Inspect,
            "verify" => Self::Verify,
            "import" => Self::Import,
            "export" => Self::Export,
            _ => Self::List,
        }
    }
}
