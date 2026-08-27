use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult};
use clearra_host_contract::{AppCommandKind, ResourceBudget};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildSolutionProbabilityPolicy,
    FinesseMetric, FinessePatternKnowledge, SearchProblem,
};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_error::{AppError, AppErrorCode},
    app_request::{AppOutputPolicy, AppRequest},
    app_response::{
        try_finite_build_success_response, AppResponse, AppStatus, FiniteBuildMemoryPhase,
        GovernedAppResponse,
    },
    build_solution_probability_result::build_probability_response_is_authorized,
    commands::core_execution_error_response,
    cooperative_execution::{
        compile_search_command, response_from_search, CooperativeSearchResponseKind,
    },
    product_capability_contract::ValidatedProductCapabilityContract,
};

pub enum DistributedSearchPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedSearch),
}

pub struct PreparedDistributedSearch {
    context: AppContext,
    problem: Arc<SearchProblem>,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    resource_budget: ResourceBudget,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

/// Owns a terminal Core result after its producer/merger memory authority has
/// finished. Creating an `AppResponse` is deliberately a separate operation:
/// Host report types allocate private strings whose actual capacities are not
/// observable at the App boundary, so they must never be materialized while a
/// producer authority is the only live-byte authority.
pub struct PreparedDistributedSearchCompletion {
    context: AppContext,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    resource_budget: ResourceBudget,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
    result: Result<CoreExecutionResult, CoreExecutionError>,
}

enum CompletedDistributedResponse {
    Compatibility(AppResponse),
    Governed(GovernedAppResponse),
}

impl AppContext {
    pub fn prepare_distributed_search(&self, request: AppRequest) -> DistributedSearchPreparation {
        let (command, output_policy, resource_budget, _, _, product_capability_contract) =
            match request.into_execution_parts() {
                Ok(execution_parts) => execution_parts,
                Err(rejection) => {
                    return DistributedSearchPreparation::Ready(
                        self.finalize_execution_parts_rejection(rejection),
                    )
                }
            };
        let command_kind = command.kind();
        if product_capability_contract.is_some() {
            return DistributedSearchPreparation::Ready(self.finalize_response(
                AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(
                        AppErrorCode::InvalidInput,
                        "distributed product capability result binding is unavailable",
                    ),
                ),
                command_kind,
                &output_policy,
            ));
        }
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return DistributedSearchPreparation::Ready(
                self.finalize_response_with_product_capability(
                    response,
                    command_kind,
                    &output_policy,
                    product_capability_contract,
                ),
            );
        }

        let (problem, response_kind) = match compile_search_command(command) {
            Ok(compiled) => compiled,
            Err(response) => {
                return DistributedSearchPreparation::Ready(
                    self.finalize_response_with_product_capability(
                        response,
                        command_kind,
                        &output_policy,
                        product_capability_contract,
                    ),
                );
            }
        };
        DistributedSearchPreparation::Search(PreparedDistributedSearch {
            context: self.clone(),
            problem,
            response_kind,
            command_kind,
            output_policy,
            resource_budget,
            validation_report,
            product_capability_contract,
        })
    }
}

impl PreparedDistributedSearch {
    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub fn build_probability_request(
        &self,
    ) -> Option<(BuildProbabilityField, BuildProbabilityAggregation)> {
        match &self.response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                field,
                aggregation,
                finesse,
                ..
            } if finesse.score().is_none() => Some((*field, *aggregation)),
            _ => None,
        }
    }

    pub fn build_probability_finesse_request(
        &self,
    ) -> Option<(FinesseMetric, FinessePatternKnowledge)> {
        match &self.response_kind {
            CooperativeSearchResponseKind::BuildProbability { finesse, .. }
                if finesse.score().is_none() =>
            {
                Some((finesse.metric(), finesse.pattern_knowledge()))
            }
            _ => None,
        }
    }

    pub fn build_probability_solution_probability_policy(
        &self,
    ) -> Option<BuildSolutionProbabilityPolicy> {
        match &self.response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                finesse,
                solution_probability_policy,
                ..
            } if finesse.score().is_none() => Some(*solution_probability_policy),
            _ => None,
        }
    }

    pub fn complete(self, result: CoreExecutionResult, control: &ExecutionControl) -> AppResponse {
        let result =
            decorate_distributed_build_probability_tiling_result(&self.response_kind, result);
        let core_executor = self.context.services().core_executor();
        let result = match &self.response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                solution_probability_policy,
                ..
            } => core_executor.materialize_build_probability_public_result(
                result,
                *solution_probability_policy,
                control,
            ),
            _ => core_executor.postprocess_search_result(result, control),
        };
        let response = match result {
            Ok(result) => response_from_search(self.response_kind, result),
            Err(error) => core_execution_error_response(error),
        };
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context.finalize_response_with_product_capability(
            response,
            self.command_kind,
            &self.output_policy,
            self.product_capability_contract,
        )
    }

    /// Completes only the terminal Core materialization while retaining the
    /// caller's producer/merger authority. The returned stage must be held
    /// until that authority owner has been dropped, and only then converted to
    /// a governed response with
    /// [`PreparedDistributedSearchCompletion::complete_governed`].
    ///
    /// This entry point is intentionally fail-closed for response families,
    /// product-capability bindings, and finesse-score requests that do not
    /// share the distributed Build terminal authority.
    pub fn complete_with_memory_guard(
        self,
        result: CoreExecutionResult,
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
    ) -> PreparedDistributedSearchCompletion {
        let PreparedDistributedSearch {
            context,
            problem,
            response_kind,
            command_kind,
            output_policy,
            resource_budget,
            validation_report,
            product_capability_contract,
        } = self;

        // Search workers have finished. Drop the compiled problem before any
        // terminal projection so its request-owned pointee cannot silently
        // coexist with the final Core result.
        drop(problem);

        if !matches!(
            &response_kind,
            CooperativeSearchResponseKind::BuildProbability { .. }
        ) {
            return prepared_distributed_completion(
                context,
                response_kind,
                command_kind,
                output_policy,
                resource_budget,
                validation_report,
                product_capability_contract,
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_memory_authority_kind_mismatch",
                }),
            );
        }
        if product_capability_contract.is_some() {
            return prepared_distributed_completion(
                context,
                response_kind,
                command_kind,
                output_policy,
                resource_budget,
                validation_report,
                product_capability_contract,
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_product_capability_authority_unavailable",
                }),
            );
        }
        if matches!(
            &response_kind,
            CooperativeSearchResponseKind::BuildProbability { finesse, .. }
                if finesse.score().is_some()
        ) {
            return prepared_distributed_completion(
                context,
                response_kind,
                command_kind,
                output_policy,
                resource_budget,
                validation_report,
                product_capability_contract,
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_finesse_score_memory_authority_unavailable",
                }),
            );
        }
        let external_retained_bytes = match checked_distributed_terminal_external_retained_bytes(
            &response_kind,
            &output_policy,
            &validation_report,
        ) {
            Some(bytes) => bytes,
            None => {
                return prepared_distributed_completion(
                    context,
                    response_kind,
                    command_kind,
                    output_policy,
                    resource_budget,
                    validation_report,
                    product_capability_contract,
                    Err(CoreExecutionError::RuntimeUnavailable {
                        component: "distributed_terminal_external_memory_projection_overflow",
                    }),
                )
            }
        };
        let request_memory_limit_bytes = match checked_request_memory_limit_bytes(resource_budget) {
            Ok(limit) => limit,
            Err(error) => {
                return prepared_distributed_completion(
                    context,
                    response_kind,
                    command_kind,
                    output_policy,
                    resource_budget,
                    validation_report,
                    product_capability_contract,
                    Err(error),
                )
            }
        };
        let mut terminal_guard = |stage_result: &CoreExecutionResult,
                                  checked_future_bytes: u128| {
            let checked_future_bytes = checked_future_bytes
                .checked_add(external_retained_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_external_memory_projection_overflow",
                })?;
            memory_guard(stage_result, checked_future_bytes)?;
            validate_request_terminal_memory(
                stage_result,
                checked_future_bytes,
                request_memory_limit_bytes,
            )
        };
        if let Err(error) = terminal_guard(&result, 0) {
            return prepared_distributed_completion(
                context,
                response_kind,
                command_kind,
                output_policy,
                resource_budget,
                validation_report,
                product_capability_contract,
                Err(error),
            );
        }
        let requires_tiling_decoration = matches!(
            &response_kind,
            CooperativeSearchResponseKind::BuildProbability { aggregation, .. }
                if aggregation.is_tiling_only()
                    && result.field("search_kind") != Some("build-probability")
        );
        if requires_tiling_decoration {
            return prepared_distributed_completion(
                context,
                response_kind,
                command_kind,
                output_policy,
                resource_budget,
                validation_report,
                product_capability_contract,
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_tiling_terminal_memory_authority_unavailable",
                }),
            );
        }

        let result = decorate_distributed_build_probability_tiling_result(&response_kind, result);
        let core_executor = context.services().core_executor();
        let result = match &response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                solution_probability_policy,
                ..
            } => core_executor.materialize_build_probability_public_result_with_memory_guard(
                result,
                *solution_probability_policy,
                control,
                &mut terminal_guard,
            ),
            _ => unreachable!("response kind was checked above"),
        };
        let result = result.and_then(|result| {
            terminal_guard(&result, 0)?;
            Ok(result)
        });
        prepared_distributed_completion(
            context,
            response_kind,
            command_kind,
            output_policy,
            resource_budget,
            validation_report,
            product_capability_contract,
            result,
        )
    }

    pub fn fail(self, error: CoreExecutionError) -> AppResponse {
        self.context.finalize_response_with_product_capability(
            core_execution_error_response(error),
            self.command_kind,
            &self.output_policy,
            self.product_capability_contract,
        )
    }
}

fn prepared_distributed_completion(
    context: AppContext,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    resource_budget: ResourceBudget,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
    result: Result<CoreExecutionResult, CoreExecutionError>,
) -> PreparedDistributedSearchCompletion {
    PreparedDistributedSearchCompletion {
        context,
        response_kind,
        command_kind,
        output_policy,
        resource_budget,
        validation_report,
        product_capability_contract,
        result,
    }
}

impl PreparedDistributedSearchCompletion {
    /// Compatibility completion for unlimited requests. A finite Build must
    /// use [`PreparedDistributedSearchCompletion::complete_governed`]; this
    /// method fails before response materialization instead of discarding its
    /// memory authority.
    pub fn complete(self) -> Result<AppResponse, CoreExecutionError> {
        if self.resource_budget.max_memory_mib().is_some() {
            drop(self);
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "distributed_finite_response_requires_governed_completion",
            });
        }
        match self.complete_terminal()? {
            CompletedDistributedResponse::Compatibility(response) => Ok(response),
            CompletedDistributedResponse::Governed(response) => {
                drop(response);
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_finite_response_requires_governed_completion",
                })
            }
        }
    }

    /// Completes a finite distributed Build while retaining its request limit
    /// and exact final App-response capacity in a non-clone authority owner.
    /// Unlimited compatibility responses have no such finite authority and
    /// fail closed through this entry point.
    pub fn complete_governed(self) -> Result<GovernedAppResponse, CoreExecutionError> {
        match self.complete_terminal()? {
            CompletedDistributedResponse::Governed(response) => Ok(response),
            CompletedDistributedResponse::Compatibility(response) => {
                drop(response);
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_final_app_response_memory_limit_unavailable",
                })
            }
        }
    }

    /// Materializes the final response after the producer/merger authority has
    /// been destroyed. Finite Build requests first authorize the exact
    /// construction peak from the still-owned completion, Core result,
    /// pending validation report, and output policy. They then reauthorize the
    /// actual response capacities immediately after construction.
    fn complete_terminal(self) -> Result<CompletedDistributedResponse, CoreExecutionError> {
        let PreparedDistributedSearchCompletion {
            context,
            response_kind,
            command_kind,
            output_policy,
            resource_budget,
            validation_report,
            product_capability_contract,
            result,
        } = self;

        if product_capability_contract.is_some() {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "distributed_terminal_product_capability_authority_unavailable",
            });
        }
        drop(product_capability_contract);

        match &response_kind {
            CooperativeSearchResponseKind::BuildProbability { finesse, .. }
                if finesse.score().is_none() => {}
            CooperativeSearchResponseKind::BuildProbability { .. } => {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_finesse_score_memory_authority_unavailable",
                })
            }
            _ => {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_terminal_memory_authority_kind_mismatch",
                })
            }
        }

        let request_memory_limit_bytes = checked_request_memory_limit_bytes(resource_budget)?;
        let Some(request_memory_limit_bytes) = request_memory_limit_bytes else {
            let response = match result {
                Ok(result) => response_from_search(response_kind, result),
                Err(error) => core_execution_error_response(error),
            };
            let response = if validation_report.is_empty() {
                response
            } else {
                response.with_validation_diagnostics(validation_report)
            };
            return Ok(CompletedDistributedResponse::Compatibility(
                context.finalize_response(response, command_kind, &output_policy),
            ));
        };

        // A finite request has no projected rich error-response construction
        // path. Preserve the static Core error instead of formatting or
        // allocating an App error shape outside the admitted authority.
        let result = result.map_err(finite_distributed_completion_error)?;
        let response_authorized = match &response_kind {
            CooperativeSearchResponseKind::BuildProbability {
                field,
                aggregation,
                finesse,
                solution_probability_policy,
            } => build_probability_response_is_authorized(
                finesse,
                *field,
                *aggregation,
                *solution_probability_policy,
                &result,
            ),
            _ => unreachable!("the distributed Build response kind was checked above"),
        };
        if !response_authorized {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "distributed_final_app_response_shape_unauthorized",
            });
        }

        let completion_actual_bytes = checked_distributed_final_completion_actual_bytes(
            &result,
            &output_policy,
            &validation_report,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "distributed_final_app_response_memory_projection_overflow",
        })?;
        validate_distributed_final_response_memory_requirement(
            completion_actual_bytes,
            request_memory_limit_bytes,
            "distributed_final_app_response_memory_budget_exceeded",
        )?;

        // Query-owned shape authority has finished; do not retain a second
        // response-kind owner while constructing the governed App response.
        drop(response_kind);
        let mut memory_guard = |phase: FiniteBuildMemoryPhase, required_bytes: u128| {
            let component = if phase == FiniteBuildMemoryPhase::FinalizedResponse {
                "distributed_final_app_response_actual_memory_budget_exceeded"
            } else {
                "distributed_final_app_response_memory_budget_exceeded"
            };
            validate_distributed_final_response_memory_requirement(
                required_bytes,
                request_memory_limit_bytes,
                component,
            )
        };
        let governed = try_finite_build_success_response(
            result,
            validation_report,
            command_kind,
            output_policy,
            context,
            Some(request_memory_limit_bytes),
            0,
            core::mem::size_of::<CompletedDistributedResponse>() as u128,
            &mut memory_guard,
        )?;
        validate_distributed_final_response_memory_requirement(
            governed.actual_retained_bytes(),
            request_memory_limit_bytes,
            "distributed_final_app_response_actual_memory_budget_exceeded",
        )?;
        Ok(CompletedDistributedResponse::Governed(governed))
    }
}

fn checked_distributed_final_completion_actual_bytes(
    result: &CoreExecutionResult,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
) -> Option<u128> {
    let source_result_heap_bytes = result
        .checked_resource_retained_bytes()?
        .checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)?;
    (core::mem::size_of::<PreparedDistributedSearchCompletion>() as u128)
        .checked_add(source_result_heap_bytes)?
        .checked_add(validation_report.checked_retained_capacity_bytes()?)?
        .checked_add(output_policy.checked_retained_capacity_bytes()?)
}

fn validate_distributed_final_response_memory_requirement(
    required_bytes: u128,
    request_memory_limit_bytes: u128,
    failure_component: &'static str,
) -> Result<(), CoreExecutionError> {
    if required_bytes > request_memory_limit_bytes {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: failure_component,
        });
    }
    Ok(())
}

/// Keeps allocation-free Core errors intact and consumes any heap-bearing
/// error payload for which the finite response boundary has no construction
/// projection.
fn finite_distributed_completion_error(error: CoreExecutionError) -> CoreExecutionError {
    drop(error);
    CoreExecutionError::RuntimeUnavailable {
        component: "distributed_final_core_result_unavailable",
    }
}

fn checked_distributed_terminal_external_retained_bytes(
    response_kind: &CooperativeSearchResponseKind,
    output_policy: &AppOutputPolicy,
    validation_report: &DiagnosticReport,
) -> Option<u128> {
    if matches!(
        response_kind,
        CooperativeSearchResponseKind::BuildProbability { finesse, .. }
            if finesse.score().is_some()
    ) {
        return None;
    }
    (core::mem::size_of::<PreparedDistributedSearch>() as u128)
        .checked_add(output_policy.checked_retained_capacity_bytes()?)?
        .checked_add(validation_report.checked_retained_capacity_bytes()?)
}

fn checked_request_memory_limit_bytes(
    resource_budget: ResourceBudget,
) -> Result<Option<u128>, CoreExecutionError> {
    resource_budget
        .max_memory_mib()
        .map(|mib| {
            (mib as u128)
                .checked_mul(1024 * 1024)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "distributed_request_memory_limit_overflow",
                })
        })
        .transpose()
}

fn validate_request_terminal_memory(
    result: &CoreExecutionResult,
    checked_future_bytes: u128,
    request_memory_limit_bytes: Option<u128>,
) -> Result<(), CoreExecutionError> {
    let Some(limit) = request_memory_limit_bytes else {
        return Ok(());
    };
    let retained = result
        .checked_resource_retained_bytes()
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "distributed_request_terminal_memory_projection_overflow",
        })?
        .max(result.usize_field("resource_peak_cpu_bytes").unwrap_or(0) as u128);
    let required = retained.checked_add(checked_future_bytes).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "distributed_request_terminal_memory_projection_overflow",
        },
    )?;
    if required > limit {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "distributed_request_terminal_memory_budget_exceeded",
        });
    }
    Ok(())
}

fn decorate_distributed_build_probability_tiling_result(
    response_kind: &CooperativeSearchResponseKind,
    result: CoreExecutionResult,
) -> CoreExecutionResult {
    let CooperativeSearchResponseKind::BuildProbability {
        field, aggregation, ..
    } = response_kind
    else {
        return result;
    };
    if !aggregation.is_tiling_only() || result.field("search_kind") == Some("build-probability") {
        return result;
    }
    let Some(base_mask) = field.compact_base_mask() else {
        return result;
    };
    let Some(target_cells) = field.compact_target_mask() else {
        return result;
    };
    let Some(final_board) = field.compact_final_board_mask() else {
        return result;
    };
    let mirror_included = field.includes_applicable_horizontal_mirror();
    let solution_count = result.usize_field("unique_solution_count").unwrap_or(0);
    let mirror_distinct = result
        .bool_field("build_mirror_distinct_target")
        .unwrap_or(false);
    let mirror_search_executed = result
        .bool_field("build_mirror_search_executed")
        .unwrap_or(mirror_distinct);
    let mirror_solution_count = result
        .usize_field("mirror_unique_solution_count")
        .unwrap_or(if mirror_included { solution_count } else { 0 });
    let mirror_candidate_count = result
        .usize_field("mirror_packing_candidate_count")
        .unwrap_or(0);
    let solution_hash = result
        .field("normalized_solution_set_hash")
        .unwrap_or("not-calculated")
        .to_owned();
    let mirror_solution_hash = result
        .field("mirror_normalized_solution_set_hash")
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mirror_included {
                solution_hash.clone()
            } else {
                "not-calculated".to_owned()
            }
        });
    result.with_replaced_fields(vec![
        text_field("search_kind", "build-probability"),
        text_field(
            "build_probability_completion",
            "exact-board-with-inverse-lock-clear",
        ),
        text_field("build_base_mask", base_mask),
        text_field("build_target_cells_mask", target_cells),
        text_field("build_target_board_mask", base_mask | target_cells),
        text_field("build_final_board_mask", final_board),
        text_field("target_piece_count", field.target_piece_count()),
        text_field("objective", "build-probability"),
        text_field("build_probability_aggregation", aggregation.as_str()),
        text_field("build_probability_evaluation_basis", "geometry-only"),
        text_field("build_path_multiplicity_counted", false),
        text_field("buildability_verified", false),
        text_field("coverage_calculated", false),
        text_field("probability_calculated", false),
        text_field(
            "build_symmetry_policy",
            if mirror_included {
                "original-or-horizontal-mirror"
            } else {
                "original-only"
            },
        ),
        text_field("build_mirror_included", mirror_included),
        text_field("build_mirror_distinct_target", mirror_distinct),
        text_field("build_mirror_search_executed", mirror_search_executed),
        text_field(
            "solution_count_basis",
            if mirror_included {
                "original-or-horizontal-mirror-union"
            } else {
                "original-field"
            },
        ),
        text_field("coverage_basis", "not-evaluated-tiling-only"),
        text_field("original_covered_pattern_count", 0),
        text_field("original_coverage_probability", "not-calculated"),
        text_field("mirror_covered_pattern_count", 0),
        text_field("mirror_coverage_probability", "not-calculated"),
        text_field("mirror_union_added_pattern_count", 0),
        text_field("mirror_unique_solution_count", mirror_solution_count),
        text_field("mirror_packing_candidate_count", mirror_candidate_count),
        text_field("mirror_normalized_solution_set_hash", mirror_solution_hash),
    ])
}

fn text_field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod tests {
    use clearra_core_executor::CoreExecutionError;

    use super::{
        finite_distributed_completion_error, validate_distributed_final_response_memory_requirement,
    };

    #[test]
    fn final_response_memory_boundary_accepts_exact_peak_and_rejects_peak_minus_one() {
        const PEAK: u128 = 4_096;
        const COMPONENT: &str = "test_distributed_final_response_memory_budget_exceeded";

        assert_eq!(
            validate_distributed_final_response_memory_requirement(PEAK, PEAK, COMPONENT),
            Ok(())
        );
        assert_eq!(
            validate_distributed_final_response_memory_requirement(PEAK, PEAK - 1, COMPONENT),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: COMPONENT,
            })
        );
    }

    #[test]
    fn finite_completion_consumes_heap_bearing_core_error_payloads() {
        assert_eq!(
            finite_distributed_completion_error(CoreExecutionError::Pc(
                "unprojected rich failure".to_owned(),
            )),
            CoreExecutionError::RuntimeUnavailable {
                component: "distributed_final_core_result_unavailable",
            }
        );
        assert_eq!(
            finite_distributed_completion_error(CoreExecutionError::Cancelled),
            CoreExecutionError::RuntimeUnavailable {
                component: "distributed_final_core_result_unavailable",
            }
        );
    }
}
