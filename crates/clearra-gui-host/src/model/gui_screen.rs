#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiScreen {
    #[default]
    Home,
    PcSearch,
    ScenarioPc,
    SetupSearch,
    BuildCoverage,
    Rules,
    Scoring,
    Render,
    Settings,
    Diagnostics,
}

impl GuiScreen {
    pub const ALL: [Self; 10] = [
        Self::Home,
        Self::PcSearch,
        Self::ScenarioPc,
        Self::SetupSearch,
        Self::BuildCoverage,
        Self::Rules,
        Self::Scoring,
        Self::Render,
        Self::Settings,
        Self::Diagnostics,
    ];
}
impl GuiScreen {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::PcSearch => "pc-search",
            Self::ScenarioPc => "scenario-pc",
            Self::SetupSearch => "setup-search",
            Self::BuildCoverage => "build-coverage",
            Self::Rules => "rules",
            Self::Scoring => "scoring",
            Self::Render => "render",
            Self::Settings => "settings",
            Self::Diagnostics => "diagnostics",
        }
    }
}
