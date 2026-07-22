#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiRenderForm {
    render_enabled: bool,
    skin_id: String,
    exact_render_required: bool,
    unsupported_reason: Option<String>,
}

impl GuiRenderForm {
    pub fn new(
        render_enabled: bool,
        skin_id: impl Into<String>,
        exact_render_required: bool,
        unsupported_reason: Option<impl Into<String>>,
    ) -> Self {
        Self {
            render_enabled,
            skin_id: skin_id.into(),
            exact_render_required,
            unsupported_reason: unsupported_reason.map(Into::into),
        }
    }
}
impl GuiRenderForm {
    pub const fn render_enabled(&self) -> bool {
        self.render_enabled
    }
}
impl GuiRenderForm {
    pub fn skin_id(&self) -> &str {
        &self.skin_id
    }
}
impl GuiRenderForm {
    pub const fn exact_render_required(&self) -> bool {
        self.exact_render_required
    }
}
impl GuiRenderForm {
    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }
}

impl Default for GuiRenderForm {
    fn default() -> Self {
        Self::new(false, "default", false, None::<String>)
    }
}
