#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CoverArgs {
    template: Option<String>,
    template_json: Option<String>,
    template_file: Option<String>,
    export_template_json: bool,
}

impl CoverArgs {
    pub fn new(template: Option<String>) -> Self {
        Self {
            template,
            template_json: None,
            template_file: None,
            export_template_json: false,
        }
    }
}
impl CoverArgs {
    pub fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }
}
impl CoverArgs {
    pub fn template_json(&self) -> Option<&str> {
        self.template_json.as_deref()
    }
}
impl CoverArgs {
    pub fn template_file(&self) -> Option<&str> {
        self.template_file.as_deref()
    }
}
impl CoverArgs {
    pub fn export_template_json(&self) -> bool {
        self.export_template_json
    }
}
impl CoverArgs {
    pub fn with_template_json(mut self, template_json: Option<String>) -> Self {
        self.template_json = template_json;
        self
    }
}
impl CoverArgs {
    pub fn with_template_file(mut self, template_file: Option<String>) -> Self {
        self.template_file = template_file;
        self
    }
}
impl CoverArgs {
    pub fn with_export_template_json(mut self, export_template_json: bool) -> Self {
        self.export_template_json = export_template_json;
        self
    }
}
