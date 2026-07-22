use clearra_i18n::LanguageId;

use crate::{app_command::AppCommand, io::AppFilePolicy};
use clearra_host_contract::{
    BackendPolicy, DiagnosticsPolicy, LocalePolicy, OutputPolicy, QueryEnvelope, ResourceBudget,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppOutputPolicy {
    contract: OutputPolicy,
}

impl AppOutputPolicy {
    pub fn new(include_render_model: bool) -> Self {
        Self {
            contract: OutputPolicy::new("text", include_render_model),
        }
    }
}
impl AppOutputPolicy {
    pub fn include_render_model(&self) -> bool {
        self.contract.include_render_model()
    }
}
impl AppOutputPolicy {
    pub fn contract(&self) -> &OutputPolicy {
        &self.contract
    }
}

impl Default for AppOutputPolicy {
    fn default() -> Self {
        Self::new(true)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppRequest {
    command: AppCommand,
    query: QueryEnvelope,
    backend_policy: BackendPolicy,
    output_policy: AppOutputPolicy,
    diagnostics_policy: DiagnosticsPolicy,
    locale_policy: LocalePolicy,
    resource_budget: ResourceBudget,
    language: Option<LanguageId>,
    file_policy: Option<AppFilePolicy>,
}

impl AppRequest {
    pub fn new(command: AppCommand) -> Self {
        let query = command.query_envelope();
        let backend_policy = command.backend_policy();
        Self {
            command,
            query,
            backend_policy,
            output_policy: AppOutputPolicy::default(),
            diagnostics_policy: DiagnosticsPolicy::default(),
            locale_policy: LocalePolicy::default(),
            resource_budget: ResourceBudget::default(),
            language: None,
            file_policy: None,
        }
    }
}
impl AppRequest {
    pub fn with_output_policy(mut self, output_policy: AppOutputPolicy) -> Self {
        self.output_policy = output_policy;
        self
    }
}
impl AppRequest {
    pub fn with_language(mut self, language: LanguageId) -> Self {
        self.language = Some(language);
        self
    }
}
impl AppRequest {
    pub fn with_file_policy(mut self, file_policy: AppFilePolicy) -> Self {
        self.file_policy = Some(file_policy);
        self
    }
}
impl AppRequest {
    pub fn with_backend_policy(mut self, backend_policy: BackendPolicy) -> Self {
        self.backend_policy = backend_policy;
        self
    }
}
impl AppRequest {
    pub fn with_diagnostics_policy(mut self, diagnostics_policy: DiagnosticsPolicy) -> Self {
        self.diagnostics_policy = diagnostics_policy;
        self
    }
}
impl AppRequest {
    pub fn with_locale_policy(mut self, locale_policy: LocalePolicy) -> Self {
        self.locale_policy = locale_policy;
        self
    }
}
impl AppRequest {
    pub fn with_resource_budget(mut self, resource_budget: ResourceBudget) -> Self {
        self.resource_budget = resource_budget;
        self
    }
}
impl AppRequest {
    pub fn command(&self) -> &AppCommand {
        &self.command
    }
}
impl AppRequest {
    pub fn command_kind(&self) -> clearra_host_contract::AppCommandKind {
        self.command.kind()
    }
}
impl AppRequest {
    pub fn query(&self) -> &QueryEnvelope {
        &self.query
    }
}
impl AppRequest {
    pub fn backend_policy(&self) -> &BackendPolicy {
        &self.backend_policy
    }
}
impl AppRequest {
    pub fn diagnostics_policy(&self) -> DiagnosticsPolicy {
        self.diagnostics_policy
    }
}
impl AppRequest {
    pub fn locale_policy(&self) -> &LocalePolicy {
        &self.locale_policy
    }
}
impl AppRequest {
    pub fn resource_budget(&self) -> ResourceBudget {
        self.resource_budget
    }
}
impl AppRequest {
    pub fn into_command(self) -> AppCommand {
        self.command
    }
}
impl AppRequest {
    pub fn into_parts(
        self,
    ) -> (
        AppCommand,
        AppOutputPolicy,
        Option<LanguageId>,
        Option<AppFilePolicy>,
    ) {
        (
            self.command,
            self.output_policy,
            self.language,
            self.file_policy,
        )
    }
}
impl AppRequest {
    pub fn output_policy(&self) -> &AppOutputPolicy {
        &self.output_policy
    }
}
impl AppRequest {
    pub fn language(&self) -> Option<LanguageId> {
        self.language
    }
}
impl AppRequest {
    pub fn file_policy(&self) -> Option<&AppFilePolicy> {
        self.file_policy.as_ref()
    }
}
