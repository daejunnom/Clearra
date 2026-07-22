use crate::gui_bridge::gui_form_validation::GuiValidatedForm;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiCommandPreview {
    command: String,
    execution_policy: &'static str,
}

impl GuiCommandPreview {
    pub fn pc_opening(validated: &GuiValidatedForm) -> Self {
        Self {
            command: format!(
                "clearra pc --lines {} --backend {}",
                validated.selected_lines(),
                validated.backend().as_str()
            ),
            execution_policy: "display_only_no_subprocess",
        }
    }
}
impl GuiCommandPreview {
    pub fn command(&self) -> &str {
        &self.command
    }
}
impl GuiCommandPreview {
    pub fn execution_policy(&self) -> &str {
        self.execution_policy
    }
}
