use clearra_output::{text::TextOutputProfile, RenderFormat};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputVerbosity {
    #[default]
    Default,
    Verbose,
    Diagnostics,
}

impl OutputVerbosity {
    pub fn apply_to_format(self, format: RenderFormat) -> RenderFormat {
        format.with_text_profile(self.text_output_profile())
    }
}
impl OutputVerbosity {
    fn text_output_profile(self) -> TextOutputProfile {
        match self {
            Self::Default => TextOutputProfile::HumanSummary,
            Self::Verbose => TextOutputProfile::Verbose,
            Self::Diagnostics => TextOutputProfile::Diagnostics,
        }
    }
}
