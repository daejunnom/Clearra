#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiOutputFormat {
    #[default]
    Text,
    Json,
    FumenLike,
    Render,
}

impl GuiOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::FumenLike => "fumen-like",
            Self::Render => "render",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiCopyPolicy {
    #[default]
    SummaryOnly,
    FullContract,
}

impl GuiCopyPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SummaryOnly => "summary-only",
            Self::FullContract => "full-contract",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GuiExportPolicy {
    #[default]
    Disabled,
    PromptForPath,
    UseConfiguredFolder,
}

impl GuiExportPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::PromptForPath => "prompt-for-path",
            Self::UseConfiguredFolder => "use-configured-folder",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiOutputForm {
    format: GuiOutputFormat,
    verbose: bool,
    diagnostics: bool,
    copy_policy: GuiCopyPolicy,
    export_policy: GuiExportPolicy,
}

impl GuiOutputForm {
    pub const fn new(format: GuiOutputFormat) -> Self {
        Self {
            format,
            verbose: false,
            diagnostics: false,
            copy_policy: GuiCopyPolicy::SummaryOnly,
            export_policy: GuiExportPolicy::Disabled,
        }
    }
}
impl GuiOutputForm {
    pub const fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}
impl GuiOutputForm {
    pub const fn with_diagnostics(mut self, diagnostics: bool) -> Self {
        self.diagnostics = diagnostics;
        self
    }
}
impl GuiOutputForm {
    pub const fn with_copy_policy(mut self, copy_policy: GuiCopyPolicy) -> Self {
        self.copy_policy = copy_policy;
        self
    }
}
impl GuiOutputForm {
    pub const fn with_export_policy(mut self, export_policy: GuiExportPolicy) -> Self {
        self.export_policy = export_policy;
        self
    }
}
impl GuiOutputForm {
    pub const fn format(&self) -> GuiOutputFormat {
        self.format
    }
}
impl GuiOutputForm {
    pub const fn verbose(&self) -> bool {
        self.verbose
    }
}
impl GuiOutputForm {
    pub const fn diagnostics(&self) -> bool {
        self.diagnostics
    }
}
impl GuiOutputForm {
    pub const fn copy_policy(&self) -> GuiCopyPolicy {
        self.copy_policy
    }
}
impl GuiOutputForm {
    pub const fn export_policy(&self) -> GuiExportPolicy {
        self.export_policy
    }
}

impl Default for GuiOutputForm {
    fn default() -> Self {
        Self::new(GuiOutputFormat::Text)
    }
}
