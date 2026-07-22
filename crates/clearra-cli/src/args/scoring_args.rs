#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoringArgs {
    action: ScoringAction,
    profile: Option<String>,
    input: Option<String>,
}

impl ScoringArgs {
    pub fn new(action: ScoringAction) -> Self {
        Self {
            action,
            profile: None,
            input: None,
        }
    }
}
impl ScoringArgs {
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }
}
impl ScoringArgs {
    pub fn with_input(mut self, input: Option<String>) -> Self {
        self.input = input;
        self
    }
}
impl ScoringArgs {
    pub fn action(&self) -> ScoringAction {
        self.action
    }
}
impl ScoringArgs {
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
impl ScoringArgs {
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }
}

impl Default for ScoringArgs {
    fn default() -> Self {
        Self::new(ScoringAction::List)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoringAction {
    #[default]
    List,
    Inspect,
    Import,
    Export,
}

impl ScoringAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "list" => Some(Self::List),
            "inspect" => Some(Self::Inspect),
            "import" => Some(Self::Import),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}
impl ScoringAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Inspect => "inspect",
            Self::Import => "import",
            Self::Export => "export",
        }
    }
}
