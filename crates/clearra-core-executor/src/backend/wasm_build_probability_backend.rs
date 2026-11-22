use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    SearchProblem,
};

use crate::CoreExecutionResult;

use super::{
    wasm_cpu::{BuildProbabilityAdvance, WasmBuildProbabilitySession as InnerSession},
    WasmCpuSearchError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
// The completed result stays inline so the public advance API preserves zero-copy ownership.
#[allow(clippy::large_enum_variant)]
pub enum WasmBuildProbabilityAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub struct WasmBuildProbabilitySession {
    inner: InnerSession,
}

impl WasmBuildProbabilitySession {
    pub fn new(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
    ) -> Result<Self, WasmCpuSearchError> {
        if matches!(
            problem.backend_policy().requested_backend(),
            RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
        ) {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "webgpu_backend_retired",
            });
        }
        Ok(Self {
            inner: InnerSession::new(problem, field, aggregation, finesse).map_err(map_error)?,
        })
    }

    /// Starts the cooperative Build session with an explicit accounting
    /// authority for every caller-owned byte that remains live beside Core and
    /// for the fixed-size carrier returned by each finite advance.
    pub fn new_finite(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, WasmCpuSearchError> {
        if matches!(
            problem.backend_policy().requested_backend(),
            RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
        ) {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "webgpu_backend_retired",
            });
        }
        Ok(Self {
            inner: InnerSession::new_finite(
                problem,
                field,
                aggregation,
                finesse,
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .map_err(map_error)?,
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmBuildProbabilityAdvance, WasmCpuSearchError> {
        let advance = self
            .inner
            .advance(work_budget, control)
            .map_err(map_error)?;
        self.map_advance(advance)
    }

    /// Advances a finite cooperative session after replacing the caller's
    /// retained-owner and returned-carrier byte authority for this generation.
    pub fn advance_finite(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<WasmBuildProbabilityAdvance, WasmCpuSearchError> {
        let advance = self
            .inner
            .advance_finite(
                work_budget,
                control,
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .map_err(map_error)?;
        let mapped = self.map_advance(advance);
        if mapped.is_err() {
            self.inner
                .validate_finite_noncompleted_return_memory()
                .map_err(map_error)?;
        }
        mapped
    }

    /// Checks the complete non-completed advance carrier against a proposed
    /// replacement caller-memory tranche without changing the active tranche
    /// or advancing the search session.
    pub fn validate_finite_advance_memory(
        &self,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.inner
            .validate_finite_noncompleted_return_memory_with_replacement(
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .map_err(map_error)
    }

    fn map_advance(
        &mut self,
        advance: BuildProbabilityAdvance,
    ) -> Result<WasmBuildProbabilityAdvance, WasmCpuSearchError> {
        match advance {
            BuildProbabilityAdvance::Pending => Ok(WasmBuildProbabilityAdvance::Pending),
            BuildProbabilityAdvance::Completed(result) => {
                Ok(WasmBuildProbabilityAdvance::Completed(result))
            }
            BuildProbabilityAdvance::Cancelled => Ok(WasmBuildProbabilityAdvance::Cancelled),
        }
    }

    pub fn validate_public_result_memory(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmCpuSearchError> {
        self.validate_public_result_memory_with_future(result, 0)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.inner
            .validate_public_result_memory_with_future(result, checked_future_bytes)
            .map_err(map_error)
    }

    /// Validates a completed public result against a replacement finite
    /// caller-memory tranche without treating caller-owned retained bytes as a
    /// future allocation.
    pub fn validate_public_result_memory_with_finite_caller_memory(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.inner
            .validate_public_result_memory_with_finite_caller_memory(
                result,
                checked_future_bytes,
                external_retained_owner_bytes,
                returned_carrier_bytes,
            )
            .map_err(map_error)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmBuildProbabilityBackend;

impl WasmBuildProbabilityBackend {
    pub fn execute_with_control(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        Self::execute_with_control_and_terminal(
            problem,
            field,
            aggregation,
            finesse,
            control,
            validated_terminal_result,
        )
    }

    pub fn execute_with_control_and_terminal<R>(
        problem: &SearchProblem,
        field: BuildProbabilityField,
        aggregation: BuildProbabilityAggregation,
        finesse: BuildProbabilityFinesseRequest,
        control: &ExecutionControl,
        terminal: impl FnOnce(
            Result<CoreExecutionResult, WasmCpuSearchError>,
            Option<&WasmBuildProbabilitySession>,
        ) -> R,
    ) -> R {
        let mut session =
            match WasmBuildProbabilitySession::new(problem, field, aggregation, finesse) {
                Ok(session) => session,
                Err(error) => return terminal(Err(error), None),
            };
        let result = loop {
            match session.advance(4096, control) {
                Ok(WasmBuildProbabilityAdvance::Pending) => {}
                Ok(WasmBuildProbabilityAdvance::Completed(result)) => break Ok(result),
                Ok(WasmBuildProbabilityAdvance::Cancelled) => {
                    break Err(WasmCpuSearchError::Cancelled)
                }
                Err(error) => break Err(error),
            }
        };
        terminal(result, Some(&session))
    }
}

fn validated_terminal_result(
    result: Result<CoreExecutionResult, WasmCpuSearchError>,
    authority: Option<&WasmBuildProbabilitySession>,
) -> Result<CoreExecutionResult, WasmCpuSearchError> {
    let result = result?;
    let authority = authority.ok_or(WasmCpuSearchError::InvalidProblem {
        reason: "wasm_build_probability_terminal_authority_missing",
    })?;
    authority.validate_public_result_memory(&result)?;
    Ok(result)
}

fn map_error(error: super::wasm_cpu::WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        super::wasm_cpu::WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        super::wasm_cpu::WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::resource_admission(*resource_report)
        }
        super::wasm_cpu::WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow,
    };
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
        ProblemCompiler,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        validated_terminal_result, WasmBuildProbabilityBackend, WasmBuildProbabilitySession,
    };
    use crate::{solution_probability_pattern_weights, CoreExecutionResult, WasmCpuSearchError};

    fn one_i_problem(max_memory_mib: Option<u64>) -> clearra_problem::SearchProblem {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default().with_max_memory_mib(max_memory_mib),
        );
        ProblemCompiler::compile_scenario_pc(&query).expect("one-I probability problem")
    }

    fn one_i_field() -> BuildProbabilityField {
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-I field")
    }

    #[test]
    fn public_convenience_backend_returns_a_terminally_validated_weighted_result() {
        let result = WasmBuildProbabilityBackend::execute_with_control(
            &one_i_problem(None),
            one_i_field(),
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
            &ExecutionControl::default(),
        )
        .expect("terminally validated Include result");

        assert_eq!(result.solution_probabilities().len(), 1);
        assert_eq!(
            solution_probability_pattern_weights(&result)
                .expect("typed weight authority")
                .len(),
            1
        );
    }

    #[test]
    fn public_terminal_validator_rejects_oversized_weight_storage() {
        let problem = one_i_problem(Some(8));
        let session = WasmBuildProbabilitySession::new(
            &problem,
            one_i_field(),
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        )
        .expect("bounded probability session");
        let oversized = CoreExecutionResult::default().with_postprocess_execution_batch(
            Vec::new(),
            true,
            vec!["x".repeat(9 * 1024 * 1024)],
        );

        let error = validated_terminal_result(Ok(oversized), Some(&session))
            .expect_err("oversized final weight storage must fail closed");
        assert!(matches!(
            error,
            WasmCpuSearchError::ResourceAdmission { .. }
        ));
    }

    #[test]
    fn public_finite_and_compatibility_advance_paths_are_not_interchangeable() {
        let problem = one_i_problem(None);
        let mut finite = WasmBuildProbabilitySession::new_finite(
            &problem,
            one_i_field(),
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
            0,
            core::mem::size_of::<super::WasmBuildProbabilityAdvance>() as u128,
        )
        .expect("finite probability session");
        assert!(matches!(
            finite.advance(1, &ExecutionControl::default()),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_finite_build_probability_advance_requires_caller_memory"
            })
        ));
        drop(finite);

        let mut compatibility = WasmBuildProbabilitySession::new(
            &problem,
            one_i_field(),
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        )
        .expect("compatibility probability session");
        assert!(matches!(
            compatibility.advance_finite(
                1,
                &ExecutionControl::default(),
                0,
                core::mem::size_of::<super::WasmBuildProbabilityAdvance>() as u128,
            ),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_build_probability_compatibility_session_rejects_finite_advance"
            })
        ));
    }
}
