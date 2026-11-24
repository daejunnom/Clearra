use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::spin_structure_app_command::{
        promote_guaranteed_family_result, promote_result, promote_search_result,
        SpinStructureProductMode,
    },
    cooperative_execution::CooperativeAppExecution,
    product_capability_contract::ValidatedProductCapabilityContract,
    spin_structure_coverage_result::prepare_spin_structure_coverage,
    spin_structure_search_result::SpinStructureSearchResult,
    AppCommand,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_host_contract::{AppCommandKind, ResourceBudget};
use clearra_spin_structure_search::{
    SpinStructureParallelMode, SpinStructureParallelResult, SpinStructureQuery,
    SpinStructureReport, SpinStructureSearcher,
};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

#[allow(clippy::large_enum_variant)]
pub enum DistributedSpinStructurePreparation {
    Ready(AppResponse),
    Search(PreparedDistributedSpinStructureSearch),
}

pub struct PreparedDistributedSpinStructureSearch {
    context: AppContext,
    query: SpinStructureQuery,
    mode: SpinStructureProductMode,
    workers: usize,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation: DiagnosticReport,
    capability: Option<ValidatedProductCapabilityContract>,
    resource_budget: ResourceBudget,
}

impl AppContext {
    pub fn prepare_distributed_spin_structure_search(
        &self,
        request: AppRequest,
    ) -> DistributedSpinStructurePreparation {
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, resource_budget, _, _, capability) =
            match request.into_execution_parts() {
                Ok(parts) => parts,
                Err(rejection) => {
                    return DistributedSpinStructurePreparation::Ready(
                        self.finalize_execution_parts_rejection(rejection),
                    )
                }
            };
        let command_kind = command.kind();
        let validation = command.validate();
        if validation.has_errors() {
            let response = command
                .validation_failed_response(validation.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation));
            return DistributedSpinStructurePreparation::Ready(
                self.finalize_response_with_product_capability(
                    response,
                    command_kind,
                    &output_policy,
                    capability,
                ),
            );
        }
        let AppCommand::SpinStructure(command) = command else {
            return DistributedSpinStructurePreparation::Ready(
                self.finalize_response_with_product_capability(
                    AppResponse::validation_failed(DiagnosticReport::new()),
                    command_kind,
                    &output_policy,
                    capability,
                ),
            );
        };
        let mode = command.product_mode();
        DistributedSpinStructurePreparation::Search(PreparedDistributedSpinStructureSearch {
            context: self.clone(),
            query: command.into_query(),
            mode,
            workers,
            command_kind,
            output_policy,
            validation,
            capability,
            resource_budget,
        })
    }
}

impl PreparedDistributedSpinStructureSearch {
    pub fn query(&self) -> &SpinStructureQuery {
        &self.query
    }
    pub fn mode(&self) -> SpinStructureProductMode {
        self.mode
    }
    pub fn workers(&self) -> usize {
        self.workers
    }
    pub fn parallel_mode(&self) -> SpinStructureParallelMode {
        match self.mode {
            SpinStructureProductMode::Search => SpinStructureParallelMode::Search,
            SpinStructureProductMode::Cover { max_patterns } => {
                SpinStructureParallelMode::Cover { max_patterns }
            }
            SpinStructureProductMode::Guaranteed {
                final_piece,
                max_patterns,
                ..
            } => SpinStructureParallelMode::Guaranteed {
                final_piece,
                max_patterns,
            },
        }
    }
    pub fn requires_cooperative_completion(&self) -> bool {
        matches!(self.mode, SpinStructureProductMode::Cover { .. })
    }
    fn validate_parallel_result(
        &self,
        result: &SpinStructureParallelResult,
    ) -> Result<SpinStructureQuery, &'static str> {
        if result.mode() != self.parallel_mode() {
            return Err("spin_structure_parallel_product_mode_mismatch");
        }
        SpinStructureSearcher::normalize_query(self.query.clone())
            .map_err(|_| "spin_structure_parallel_query_invalid")
    }
    pub fn complete_parallel(
        self,
        result: SpinStructureParallelResult,
    ) -> Result<AppResponse, &'static str> {
        let query = self.validate_parallel_result(&result)?;
        let (report, coverage) = result.into_parts();
        if report.query.as_ref() != Some(&query) || coverage.is_some() {
            return Err("spin_structure_parallel_source_mismatch");
        }
        let response = match self.mode {
            SpinStructureProductMode::Search => promote_search_result(&query, report),
            SpinStructureProductMode::Guaranteed {
                final_piece,
                dependency_report,
                ..
            } => promote_guaranteed_family_result(&query, report, final_piece, dependency_report),
            SpinStructureProductMode::Cover { .. } => {
                return Err("spin_structure_cover_requires_cooperative_completion")
            }
        }
        .with_validation_diagnostics(self.validation);
        Ok(self.context.finalize_response_with_product_capability(
            response,
            self.command_kind,
            &self.output_policy,
            self.capability,
        ))
    }
    pub fn into_cooperative_completion(
        self,
        result: SpinStructureParallelResult,
    ) -> Result<CooperativeAppExecution, &'static str> {
        let query = self.validate_parallel_result(&result)?;
        if !self.requires_cooperative_completion() || self.capability.is_some() {
            return Err("spin_structure_cover_preparation_kind_mismatch");
        }
        let (report, coverage) = result.into_parts();
        if report.query.as_ref() != Some(&query) {
            return Err("spin_structure_parallel_source_mismatch");
        }
        let coverage = coverage.ok_or("spin_structure_cover_analysis_missing")?;
        let source = SpinStructureSearchResult::promote(&query, report)
            .map_err(|_| "spin_structure_cover_source_rejected")?;
        let preparation = prepare_spin_structure_coverage(&source, &coverage)?;
        Ok(CooperativeAppExecution::from_prepared_spin_cover_minimum(
            self.context,
            source.into_report(),
            self.command_kind,
            self.output_policy,
            self.validation,
            self.resource_budget,
            preparation,
        ))
    }
    pub fn complete(self, report: SpinStructureReport, control: &ExecutionControl) -> AppResponse {
        let query = report.query.as_ref().unwrap_or(&self.query).clone();
        let mut response = promote_result(&query, self.mode, report, control);
        if !self.validation.is_empty() {
            response = response.with_validation_diagnostics(self.validation);
        }
        self.context.finalize_response_with_product_capability(
            response,
            self.command_kind,
            &self.output_policy,
            self.capability,
        )
    }
}
