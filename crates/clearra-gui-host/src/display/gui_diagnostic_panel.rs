use clearra_app::AppResponse;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiDiagnosticEvidence {
    key: String,
    value: String,
}

impl GuiDiagnosticEvidence {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl GuiDiagnosticEvidence {
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiDiagnosticEntry {
    severity: String,
    code: String,
    message: String,
    evidence: Vec<GuiDiagnosticEvidence>,
    suggested_next_step: Option<String>,
}

impl GuiDiagnosticEntry {
    fn from_diagnostic(
        diagnostic: &clearra_validation::diagnostic::diagnostic::Diagnostic,
    ) -> Self {
        Self {
            severity: format!("{:?}", diagnostic.severity()).to_ascii_lowercase(),
            code: diagnostic.code().as_str().to_owned(),
            message: diagnostic.message().to_owned(),
            evidence: diagnostic
                .evidence()
                .iter()
                .map(|evidence| GuiDiagnosticEvidence {
                    key: evidence.key().to_owned(),
                    value: evidence.value().to_owned(),
                })
                .collect(),
            suggested_next_step: diagnostic
                .suggested_next_step()
                .map(|step| step.text().to_owned()),
        }
    }
}
impl GuiDiagnosticEntry {
    pub fn severity(&self) -> &str {
        &self.severity
    }
}
impl GuiDiagnosticEntry {
    pub fn code(&self) -> &str {
        &self.code
    }
}
impl GuiDiagnosticEntry {
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl GuiDiagnosticEntry {
    pub fn evidence(&self) -> &[GuiDiagnosticEvidence] {
        &self.evidence
    }
}
impl GuiDiagnosticEntry {
    pub fn suggested_next_step(&self) -> Option<&str> {
        self.suggested_next_step.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiDiagnosticPanel {
    label_i18n_key: &'static str,
    diagnostics: Vec<GuiDiagnosticEntry>,
}

impl GuiDiagnosticPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.diagnostics",
            diagnostics: response
                .diagnostics()
                .validation()
                .diagnostics()
                .iter()
                .map(GuiDiagnosticEntry::from_diagnostic)
                .collect(),
        }
    }
}
impl GuiDiagnosticPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiDiagnosticPanel {
    pub fn diagnostics(&self) -> &[GuiDiagnosticEntry] {
        &self.diagnostics
    }
}
