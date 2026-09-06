use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_host_contract::ResourceBudget;
use clearra_i18n::LanguageId;
use clearra_validation::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport,
};

use crate::{
    app_command::AppCommand,
    app_command::RunnableAppCommand,
    app_error::{AppError, AppErrorCode},
    app_request::{AppExecutionParts, AppExecutionPartsRejection, AppOutputPolicy, AppRequest},
    app_response::{AppResponse, AppStatus},
    app_services::AppServices,
    io::{AppFilePolicy, AppFileResolver},
    pc_score_minimum_cover_result::ValidatedPcScorePortfolioExecutionEvidence,
    pc_score_postprocess::PcScoreDerivation,
    pc_score_summary_result::ValidatedPcScoreExecutionEvidence,
    pc_tiling_family_result::ValidatedPcTilingExecutionEvidence,
    product_capability_contract::{ProductCapabilityContract, ValidatedProductCapabilityContract},
    product_capability_result::ProductCapabilityResult,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AppContext {
    services: AppServices,
    language: LanguageId,
    file_policy: AppFilePolicy,
}

impl AppContext {
    /// App services, language and file policy currently own only inline values.
    /// Fail closed if this context acquires a dropping (heap-owning) field in a
    /// later refactor, rather than silently omitting it from local-shard guards.
    pub const fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        if core::mem::needs_drop::<Self>() {
            None
        } else {
            Some(0)
        }
    }

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
        let execution_parts = match request.into_execution_parts() {
            Ok(execution_parts) => execution_parts,
            Err(rejection) => return self.finalize_execution_parts_rejection(rejection),
        };
        self.run_execution_parts(execution_parts, execution_control)
    }

    pub(crate) fn run_execution_parts(
        &self,
        execution_parts: AppExecutionParts,
        execution_control: &ExecutionControl,
    ) -> AppResponse {
        let (
            command,
            output_policy,
            resource_budget,
            language,
            file_policy,
            product_capability_contract,
        ) = execution_parts;
        let command_kind = command.kind();
        let file_policy = file_policy.as_ref().unwrap_or(&self.file_policy);
        let language = self
            .services
            .language_resolver()
            .resolve_from_selected(Some(language.unwrap_or(self.language)));
        execution_control.report_progress("validation", 0, Some(1));
        let validation_report = command.validate();
        execution_control.report_progress("validation", 1, Some(1));
        let response = if validation_report.has_errors() {
            command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report))
        } else {
            let validation_report_is_empty = validation_report.is_empty();
            let response = match checked_score_direct_context_retained_bytes(
                &command,
                &validation_report,
                &output_policy,
                product_capability_contract.as_ref(),
            ) {
                Ok(pc_score_external_retained_context_bytes) => {
                    let execution_context = AppExecutionContext {
                        services: &self.services,
                        language,
                        file_policy,
                        output_policy: &output_policy,
                        resource_budget: &resource_budget,
                        execution_control,
                        pc_score_external_retained_context_bytes,
                    };
                    command.run(&execution_context)
                }
                Err(component) => AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, component),
                ),
            };
            if validation_report_is_empty {
                response
            } else {
                response.with_validation_diagnostics(validation_report)
            }
        };
        self.finalize_response_with_product_capability(
            response,
            command_kind,
            &output_policy,
            product_capability_contract,
        )
    }

    pub(crate) fn finalize_execution_parts_rejection(
        &self,
        rejection: AppExecutionPartsRejection,
    ) -> AppResponse {
        let (command_kind, output_policy, error) = rejection.into_parts();
        self.finalize_response(
            AppResponse::failed(
                AppStatus::ValidationFailed,
                AppError::new(AppErrorCode::InvalidInput, error.to_string()),
            ),
            command_kind,
            &output_policy,
        )
    }
}
impl AppContext {
    pub fn validate_request(
        &self,
        request: &AppRequest,
    ) -> crate::diagnostics::AppDiagnosticReport {
        let mut validation = request.command().validate();
        if let Err(error) = request.validate_request_profile_binding() {
            validation.push(Diagnostic::new(
                DiagnosticCode::EFrontendTypedRequestRequired,
                format!("request profile selection rejected: {error}"),
            ));
        }
        if let Err(error) = request.validate_product_capability_binding() {
            validation.push(Diagnostic::new(
                DiagnosticCode::EFrontendTypedRequestRequired,
                format!("product capability request rejected: {error}"),
            ));
        }
        crate::diagnostics::AppDiagnosticReport::new(validation)
    }
}
impl AppContext {
    pub fn services(&self) -> &AppServices {
        &self.services
    }

    pub fn set_product_retention_budget(&mut self, budget: Option<crate::ProductRetentionBudget>) {
        let executor = self
            .services
            .core_executor()
            .with_product_retention_budget(budget);
        self.services = self.services.clone().with_core_executor(executor);
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
        self.finalize_response_with_product_capability(response, command_kind, output_policy, None)
    }

    pub(crate) fn finalize_response_with_product_capability(
        &self,
        response: AppResponse,
        command_kind: clearra_host_contract::AppCommandKind,
        output_policy: &AppOutputPolicy,
        product_capability_contract: Option<ValidatedProductCapabilityContract>,
    ) -> AppResponse {
        let response = response.with_contract_context(command_kind);
        let response = match product_capability_contract {
            Some(contract) if response.status() == AppStatus::Success => {
                match ProductCapabilityResult::validate(contract, &response) {
                    Ok(result) => response.with_product_capability_result(result),
                    Err(error) => AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            format!("product capability result rejected: {error}"),
                        ),
                    )
                    .with_contract_context(command_kind),
                }
            }
            _ => response,
        };
        self.finalize_response_surface(response, output_policy)
    }

    /// Finalizes a response whose product proof was already completed by the
    /// cooperative product coordinator. The caller must obtain `result` from
    /// `ProductCapabilityResult`'s checked preparation API; this seam only
    /// avoids replaying that proof through the blocking validator.
    pub(crate) fn finalize_response_with_prevalidated_product_capability(
        &self,
        response: AppResponse,
        command_kind: clearra_host_contract::AppCommandKind,
        output_policy: &AppOutputPolicy,
        result: ProductCapabilityResult,
    ) -> AppResponse {
        let response = response
            .with_contract_context(command_kind)
            .with_product_capability_result(result);
        self.finalize_response_surface(response, output_policy)
    }

    fn finalize_response_surface(
        &self,
        response: AppResponse,
        output_policy: &AppOutputPolicy,
    ) -> AppResponse {
        let response = response.without_product_capability_transients();
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
    pub resource_budget: &'a ResourceBudget,
    pub execution_control: &'a ExecutionControl,
    pub(crate) pc_score_external_retained_context_bytes: Option<u128>,
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
    pub fn resource_budget(&self) -> ResourceBudget {
        *self.resource_budget
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

impl AppExecutionContext<'_> {
    pub(crate) const fn pc_score_external_retained_context_bytes(&self) -> Option<u128> {
        self.pc_score_external_retained_context_bytes
    }

    pub(crate) const fn pc_tiling_external_retained_context_bytes(&self) -> Option<u128> {
        self.pc_score_external_retained_context_bytes
    }
}

/// Counts every direct-App owner that remains alive beside the score authority.
/// Cardinality-dependent Core result storage is accounted by the terminal Core
/// guard; this projection covers the request tuple, product proof/query `Arc`
/// handle, validation/output backing, execution context, direct problem handle,
/// and derivation/evidence transition inlines. `AppResponse` construction starts
/// only after the score authority has been explicitly dropped.
pub(crate) fn checked_pc_score_direct_context_retained_bytes(
    validation_report: &DiagnosticReport,
    output_policy: &AppOutputPolicy,
    product_capability_contract: Option<&ValidatedProductCapabilityContract>,
) -> Result<Option<u128>, &'static str> {
    let Some(contract) = product_capability_contract else {
        return Ok(None);
    };
    if !matches!(
        contract.contract(),
        ProductCapabilityContract::PcScore
            | ProductCapabilityContract::PcScoreFinder
            | ProductCapabilityContract::PcScoreMinimals
            | ProductCapabilityContract::PcTiling
    ) {
        return Ok(None);
    }

    checked_direct_score_context_base_bytes(
        validation_report,
        output_policy,
        contract.contract() == ProductCapabilityContract::PcTiling,
    )
    .map(Some)
}

fn checked_score_direct_context_retained_bytes(
    command: &AppCommand,
    validation_report: &DiagnosticReport,
    output_policy: &AppOutputPolicy,
    product_capability_contract: Option<&ValidatedProductCapabilityContract>,
) -> Result<Option<u128>, &'static str> {
    if let Some(bytes) = checked_pc_score_direct_context_retained_bytes(
        validation_report,
        output_policy,
        product_capability_contract,
    )? {
        return Ok(Some(bytes));
    }
    let AppCommand::SetupScore(command) = command else {
        return Ok(None);
    };
    let bytes = checked_direct_score_context_base_bytes(validation_report, output_policy, false)?
        .checked_add(
            command
                .checked_direct_external_retained_upper_bound_bytes()
                .ok_or("setup_score_direct_external_retained_projection_unavailable")?,
        )
        .ok_or("setup_score_direct_external_retained_projection_unavailable")?;
    Ok(Some(bytes))
}

fn checked_direct_score_context_base_bytes(
    validation_report: &DiagnosticReport,
    output_policy: &AppOutputPolicy,
    include_pc_tiling_authority: bool,
) -> Result<u128, &'static str> {
    (core::mem::size_of::<AppExecutionParts>() as u128)
        .checked_add(core::mem::size_of::<AppExecutionContext<'static>>() as u128)
        .and_then(|bytes| {
            bytes.checked_add(
                core::mem::size_of::<std::sync::Arc<clearra_problem::SearchProblem>>() as u128,
            )
        })
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<PcScoreDerivation>() as u128))
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<ValidatedPcScoreExecutionEvidence>() as u128)
        })
        .and_then(|bytes| {
            bytes.checked_add(
                core::mem::size_of::<ValidatedPcScorePortfolioExecutionEvidence>() as u128,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<ValidatedPcTilingExecutionEvidence>() as u128)
        })
        .and_then(|bytes| {
            bytes.checked_add(
                if include_pc_tiling_authority {
                    crate::pc_tiling_family_result::PcTilingCompiledAuthority::execution_evidence_retained_upper_bound_bytes()
                } else {
                    Default::default()
                },
            )
        })
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<DiagnosticReport>() as u128))
        .and_then(|bytes| {
            validation_report
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .and_then(|bytes| {
            output_policy
                .checked_retained_capacity_bytes()
                .and_then(|heap| bytes.checked_add(heap))
        })
        .ok_or("pc_score_direct_external_retained_projection_unavailable")
}

#[cfg(test)]
#[path = "app_context_tests.rs"]
mod tests;
