#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RulesArgs {
    action: RulesAction,
    profile: Option<String>,
    input: Option<String>,
}

impl RulesArgs {
    pub fn new(action: RulesAction) -> Self {
        Self {
            action,
            profile: None,
            input: None,
        }
    }
}
impl RulesArgs {
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }
}
impl RulesArgs {
    pub fn with_input(mut self, input: Option<String>) -> Self {
        self.input = input;
        self
    }
}
impl RulesArgs {
    pub fn action(&self) -> RulesAction {
        self.action
    }
}
impl RulesArgs {
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
impl RulesArgs {
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }
}

impl Default for RulesArgs {
    fn default() -> Self {
        Self::new(RulesAction::List)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RulesAction {
    #[default]
    List,
    Inspect,
    Verify,
    Import,
    Export,
}

impl RulesAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "list" => Some(Self::List),
            "inspect" => Some(Self::Inspect),
            "verify" => Some(Self::Verify),
            "import" => Some(Self::Import),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}
impl RulesAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Inspect => "inspect",
            Self::Verify => "verify",
            Self::Import => "import",
            Self::Export => "export",
        }
    }
}
