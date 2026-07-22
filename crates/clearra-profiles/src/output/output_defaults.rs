#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Csv,
    FumenLike,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputDefaults {
    format: OutputFormat,
    include_diagnostics: bool,
}

impl OutputDefaults {
    pub const DEFAULT: Self = Self {
        format: OutputFormat::Text,
        include_diagnostics: true,
    };
}
impl OutputDefaults {
    pub fn format(self) -> OutputFormat {
        self.format
    }
}
impl OutputDefaults {
    pub fn include_diagnostics(self) -> bool {
        self.include_diagnostics
    }
}

impl Default for OutputDefaults {
    fn default() -> Self {
        Self::DEFAULT
    }
}
