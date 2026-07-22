use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_i18n::LanguageId;

use crate::{
    app_command::RunnableAppCommand,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    app_services::AppServices,
    io::{AppFilePolicy, AppFileResolver},
};

#[derive(Clone, Debug, PartialEq)]
pub struct AppContext {
    services: AppServices,
    language: LanguageId,
    file_policy: AppFilePolicy,
}

impl AppContext {
    pub fn new(services: AppServices) -> Self {
        Self {
            services,
            language: LanguageId::default(),
            file_policy: AppFilePolicy::default(),
        }
    }
}
impl AppContext {
    pub fn with_language(mut self, language: LanguageId) -> Self {
        self.language = language;
        self
    }
}
impl AppContext {
    pub fn with_file_policy(mut self, file_policy: AppFilePolicy) -> Self {
        self.file_policy = file_policy;
        self
    }
}
impl AppContext {
    pub fn run(&self, request: AppRequest) -> AppResponse {
        self.run_with_cancellation(request, &ExecutionCancellationToken::new())
    }

    pub fn run_with_cancellation(
        &self,
        request: AppRequest,
        cancellation: &ExecutionCancellationToken,
    ) -> AppResponse {
        self.run_with_execution_control(request, &ExecutionControl::new(cancellation.clone()))
    }

    pub fn run_with_execution_control(
        &self,
        request: AppRequest,
        execution_control: &ExecutionControl,
    ) -> AppResponse {
        let command_kind = request.command_kind();
        let (command, output_policy, language, file_policy) = request.into_parts();
        let file_policy = file_policy.as_ref().unwrap_or(&self.file_policy);
        let language = self
            .services
            .language_resolver()
            .resolve_from_selected(Some(language.unwrap_or(self.language)));
        let execution_context = AppExecutionContext {
            services: &self.services,
            language,
            file_policy,
            output_policy: &output_policy,
            execution_control,
        };
        execution_control.report_progress("validation", 0, Some(1));
        let validation_report = command.validate();
        execution_control.report_progress("validation", 1, Some(1));
        let response = if validation_report.has_errors() {
            command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report))
        } else {
            let validation_report_is_empty = validation_report.is_empty();
            let response = command.run(&execution_context);
            if validation_report_is_empty {
                response
            } else {
                response.with_validation_diagnostics(validation_report)
            }
        };
        self.finalize_response(response, command_kind, &output_policy)
    }
}
impl AppContext {
    pub fn validate_request(
        &self,
        request: &AppRequest,
    ) -> crate::diagnostics::AppDiagnosticReport {
        crate::diagnostics::AppDiagnosticReport::new(request.command().validate())
    }
}
impl AppContext {
    pub fn services(&self) -> &AppServices {
        &self.services
    }
}
impl AppContext {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl AppContext {
    pub fn file_policy(&self) -> &AppFilePolicy {
        &self.file_policy
    }

    pub(crate) fn finalize_response(
        &self,
        response: AppResponse,
        command_kind: clearra_host_contract::AppCommandKind,
        output_policy: &AppOutputPolicy,
    ) -> AppResponse {
        let response = response.with_contract_context(command_kind);
        self.services
            .diagnostic_sink()
            .observe(response.diagnostics());
        if output_policy.include_render_model() {
            response
        } else {
            response.without_render_model()
        }
    }
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new(AppServices::default())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AppExecutionContext<'a> {
    pub services: &'a AppServices,
    pub language: LanguageId,
    pub file_policy: &'a AppFilePolicy,
    pub output_policy: &'a AppOutputPolicy,
    pub execution_control: &'a ExecutionControl,
}

impl<'a> AppExecutionContext<'a> {
    pub fn services(&self) -> &'a AppServices {
        self.services
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn file_policy(&self) -> &'a AppFilePolicy {
        self.file_policy
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn file_resolver(&self) -> AppFileResolver {
        self.services.file_resolver_for(self.file_policy)
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn output_policy(&self) -> &'a AppOutputPolicy {
        self.output_policy
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn cancellation(&self) -> &'a ExecutionCancellationToken {
        &self.execution_control.cancellation
    }
}
impl<'a> AppExecutionContext<'a> {
    pub fn execution_control(&self) -> &'a ExecutionControl {
        self.execution_control
    }
}

#[cfg(test)]
#[path = "app_context_tests.rs"]
mod tests;
