#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiFormState {
    selected_language: String,
    selected_backend: String,
    selected_problem_preset: String,
    selected_lines: u8,
    selected_rule: String,
}

impl GuiFormState {
    pub fn new(
        selected_language: impl Into<String>,
        selected_backend: impl Into<String>,
        selected_problem_preset: impl Into<String>,
        selected_lines: u8,
        selected_rule: impl Into<String>,
    ) -> Self {
        Self {
            selected_language: selected_language.into(),
            selected_backend: selected_backend.into(),
            selected_problem_preset: selected_problem_preset.into(),
            selected_lines,
            selected_rule: selected_rule.into(),
        }
    }
}
impl GuiFormState {
    pub fn selected_language(&self) -> &str {
        &self.selected_language
    }
}
impl GuiFormState {
    pub fn selected_backend(&self) -> &str {
        &self.selected_backend
    }
}
impl GuiFormState {
    pub fn selected_problem_preset(&self) -> &str {
        &self.selected_problem_preset
    }
}
impl GuiFormState {
    pub fn selected_lines(&self) -> u8 {
        self.selected_lines
    }
}
impl GuiFormState {
    pub fn selected_rule(&self) -> &str {
        &self.selected_rule
    }
}

impl Default for GuiFormState {
    fn default() -> Self {
        Self::new("en", "auto", "opening-pc", 2, "srs-plus")
    }
}
