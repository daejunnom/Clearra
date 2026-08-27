use core::mem::size_of;

use clearra_pc_graph::request::{validate_pc_observation_objective, PcQueueInput, PcScenarioQuery};
use clearra_supply::{
    finite_build_piece_source_returned_carrier_delta_bytes,
    mixed::supply_provenance::BagBoundaryEvidence, FiniteBuildPieceSourceRequest,
    FiniteBuildQueueRef, FiniteBuildSupplyQueue, FinitePieceSourceMaterializationError,
    FiniteSupplyAllocationError, FiniteSupplyAllocationLedger, FiniteSupplyProvenanceRef,
};

use crate::{
    compile::{ProblemCompileError, ProblemCompiler},
    query::{ScenarioQuery, ScenarioQuerySource},
    search_problem::{PieceSource, SearchProblem, SearchProblemId, SearchProblemKind},
};

/// The caller-owned portion of the finite scenario-PC compile envelope.
///
/// `external_retained_owner_bytes` excludes the consumed `PcScenarioQuery`:
/// the compiler owns and measures that graph from entry through return.
/// `returned_carrier_bytes` is the caller's complete outer return carrier,
/// excluding the compiled `SearchProblem` itself. The compiler combines it by
/// `max` with its own wrapper delta because those carriers overlap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteScenarioPcCompileBudget {
    max_memory_bytes: u128,
    external_retained_owner_bytes: u128,
    returned_carrier_bytes: u128,
}

impl FiniteScenarioPcCompileBudget {
    pub fn try_new(
        max_memory_bytes: u128,
        external_retained_owner_bytes: u128,
        returned_carrier_bytes: u128,
    ) -> Result<Self, FiniteScenarioPcCompileError> {
        external_retained_owner_bytes
            .checked_add(returned_carrier_bytes)
            .ok_or(FiniteScenarioPcCompileError::ProjectionOverflow)?;
        Ok(Self {
            max_memory_bytes,
            external_retained_owner_bytes,
            returned_carrier_bytes,
        })
    }

    pub const fn max_memory_bytes(self) -> u128 {
        self.max_memory_bytes
    }

    pub const fn external_retained_owner_bytes(self) -> u128 {
        self.external_retained_owner_bytes
    }

    pub const fn returned_carrier_bytes(self) -> u128 {
        self.returned_carrier_bytes
    }
}

/// Allocation-free projection produced before the finite compiler creates its
/// first owned buffer.
///
/// The byte model intentionally matches the rest of the repository: inline
/// owners, `Vec`/`String` logical capacities, and explicit `Arc<[T]>` payloads
/// are included; allocator metadata, rounding, and `Arc` control blocks are
/// excluded. `requested_peak_bytes` includes all requested transient buffers
/// that can coexist in the current constructors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteScenarioPcCompileProjection {
    requested_peak_bytes: u128,
    projected_problem_retained_bytes: u128,
    effective_returned_carrier_bytes: u128,
}

impl FiniteScenarioPcCompileProjection {
    pub const fn requested_peak_bytes(self) -> u128 {
        self.requested_peak_bytes
    }

    pub const fn projected_problem_retained_bytes(self) -> u128 {
        self.projected_problem_retained_bytes
    }

    pub const fn effective_returned_carrier_bytes(self) -> u128 {
        self.effective_returned_carrier_bytes
    }
}

/// Move-only finite compiler output. There is deliberately no public
/// constructor and no `Clone` implementation: only the checked compiler can
/// attach the peak and retained-byte evidence to a `SearchProblem`.
#[derive(Debug, PartialEq)]
pub struct FiniteScenarioPcCompilation {
    problem: SearchProblem,
    peak_required_memory_bytes: u128,
    problem_retained_bytes: u128,
}

impl FiniteScenarioPcCompilation {
    pub const fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub const fn peak_required_memory_bytes(&self) -> u128 {
        self.peak_required_memory_bytes
    }

    pub const fn problem_retained_bytes(&self) -> u128 {
        self.problem_retained_bytes
    }

    pub fn into_parts(self) -> (SearchProblem, u128, u128) {
        (
            self.problem,
            self.peak_required_memory_bytes,
            self.problem_retained_bytes,
        )
    }
}

/// Allocation-free rejection for the finite compiler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteScenarioPcCompileError {
    UnsupportedBuildProbabilityShape,
    ProjectionOverflow,
    FiniteSupplyAllocation(FiniteSupplyAllocationError),
    PieceSourceMaterialization(FinitePieceSourceMaterializationError),
    RetainedMemoryMeasurementUnavailable,
    RetainedMemoryAccountingMismatch {
        ledger_live_memory_bytes: u128,
        expected_live_memory_bytes: u128,
    },
    ProblemIdLengthMismatch {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    ProblemIdAuthorizedLengthExceeded {
        authorized_bytes: usize,
        attempted_bytes: usize,
    },
    ProblemIdAllocatedCapacityExceeded {
        allocated_capacity_bytes: usize,
        attempted_bytes: usize,
    },
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    ProblemCompile(ProblemCompileError),
}

impl From<ProblemCompileError> for FiniteScenarioPcCompileError {
    fn from(error: ProblemCompileError) -> Self {
        Self::ProblemCompile(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SupplyWindowProjection {
    source_sequence_length: usize,
    projects_unplaced_lookahead: bool,
}

impl ProblemCompiler {
    /// Computes the complete requested-capacity peak for the active finite
    /// Build compiler without allocating. Actual allocator capacities can be
    /// larger and remain authoritative in `compile_scenario_pc_finite_build`.
    /// Unsupported queue/profile/identity owners fail closed before semantic
    /// compilation begins.
    pub fn checked_finite_build_scenario_pc_compile_projection(
        query: &PcScenarioQuery,
        budget: FiniteScenarioPcCompileBudget,
    ) -> Result<FiniteScenarioPcCompileProjection, FiniteScenarioPcCompileError> {
        let query_retained_bytes = query
            .checked_build_probability_retained_capacity_bytes()
            .ok_or(FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape)?;
        validate_compile_semantics(query)?;
        let supply_window = resolve_supply_window(query)?;
        let supply_projection = PieceSource::checked_finite_build_allocation_projection(
            finite_build_piece_source_request(query, supply_window),
        )
        .map_err(map_piece_source_materialization_error)?;

        let problem_id_bytes = checked_problem_id_requested_bytes(query, supply_window)?;
        let supply_returned_carrier_delta_bytes =
            finite_build_piece_source_returned_carrier_delta_bytes();
        let compiler_peak_bytes = checked_sum([
            size_of::<SearchProblem>() as u128,
            query_retained_bytes,
            problem_id_bytes,
            supply_projection.requested_additional_bytes(),
            supply_returned_carrier_delta_bytes,
        ])?;
        let projected_problem_retained_bytes = checked_sum([
            size_of::<SearchProblem>() as u128,
            query_retained_bytes,
            problem_id_bytes,
            supply_projection.projected_piece_source_retained_bytes(),
            supply_projection.projected_retained_queue_bytes(),
        ])?;
        let compiler_carrier_delta = checked_compiler_returned_carrier_delta_bytes()?;
        let effective_returned_carrier_bytes =
            budget.returned_carrier_bytes.max(compiler_carrier_delta);
        let requested_peak_bytes = checked_sum([
            budget.external_retained_owner_bytes,
            effective_returned_carrier_bytes,
            compiler_peak_bytes,
        ])?;

        Ok(FiniteScenarioPcCompileProjection {
            requested_peak_bytes,
            projected_problem_retained_bytes,
            effective_returned_carrier_bytes,
        })
    }

    /// Consumes a canonical Build `PcScenarioQuery` and materializes its
    /// complete `SearchProblem` under one move-only allocation authority.
    ///
    /// Unsupported shapes and semantic failures are rejected before the first
    /// compile allocation. Every Problem-owned `String`/`Vec` is authorized at
    /// its requested capacity and rechecked at its actual capacity by the same
    /// ledger. The borrowed compatibility compiler remains unchanged.
    pub fn compile_scenario_pc_finite_build(
        mut query: PcScenarioQuery,
        budget: FiniteScenarioPcCompileBudget,
    ) -> Result<FiniteScenarioPcCompilation, FiniteScenarioPcCompileError> {
        let query_retained_bytes = query
            .checked_build_probability_retained_capacity_bytes()
            .ok_or(FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape)?;
        validate_compile_semantics(&query)?;
        let supply_window = resolve_supply_window(&query)?;
        let normalized_board = query.initial_board().after_initial_line_clear();
        query = query.with_initial_board(normalized_board);
        let supply_request = finite_build_piece_source_request(&query, supply_window);
        PieceSource::validate_finite_build(supply_request)
            .map_err(map_piece_source_materialization_error)?;
        let effective_returned_carrier_bytes = budget
            .returned_carrier_bytes
            .max(checked_compiler_returned_carrier_delta_bytes()?);
        let supply_returned_carrier_delta_bytes =
            finite_build_piece_source_returned_carrier_delta_bytes();
        let initial_live_memory_bytes = checked_sum([
            budget.external_retained_owner_bytes,
            effective_returned_carrier_bytes,
            size_of::<SearchProblem>() as u128,
            query_retained_bytes,
            supply_returned_carrier_delta_bytes,
        ])?;
        let mut ledger = FiniteSupplyAllocationLedger::try_new(
            budget.max_memory_bytes,
            initial_live_memory_bytes,
        )
        .map_err(map_finite_allocation_error)?;

        let mut transaction = ledger.transaction();
        let problem_id_requested_bytes =
            usize::try_from(checked_problem_id_requested_bytes(&query, supply_window)?)
                .map_err(|_| FiniteScenarioPcCompileError::ProjectionOverflow)?;
        let mut problem_id_value = transaction
            .try_string_with_capacity(problem_id_requested_bytes)
            .map_err(map_finite_allocation_error)?;
        write_finite_problem_id(
            &query,
            supply_window,
            &mut problem_id_value,
            problem_id_requested_bytes,
        )?;
        let problem_id = SearchProblemId::new(problem_id_value);
        let supply_materialization =
            PieceSource::materialize_finite_build_in_transaction(supply_request, &mut transaction)
                .map_err(map_piece_source_materialization_error)?;
        let (piece_source, retained_queue) = supply_materialization.into_parts();
        let retained_queue = into_pc_queue_input(retained_queue);
        transaction
            .release_retained_bytes(supply_returned_carrier_delta_bytes)
            .map_err(map_finite_allocation_error)?;

        let allowed_colored_solution_identities = query.take_allowed_colored_solution_identities();
        let scenario = ScenarioQuery::scenario_preset(query);
        let problem = SearchProblem::from_validated_finite_scenario_parts(
            scenario,
            problem_id,
            piece_source,
            retained_queue,
            supply_window.source_sequence_length,
            supply_window.projects_unplaced_lookahead,
            allowed_colored_solution_identities,
        );
        let problem_retained_bytes = problem
            .checked_build_probability_pointee_retained_bytes()
            .ok_or(FiniteScenarioPcCompileError::RetainedMemoryMeasurementUnavailable)?;
        let expected_live_memory_bytes = checked_sum([
            budget.external_retained_owner_bytes,
            effective_returned_carrier_bytes,
            problem_retained_bytes,
        ])?;
        if transaction.live_memory_bytes() != expected_live_memory_bytes {
            return Err(
                FiniteScenarioPcCompileError::RetainedMemoryAccountingMismatch {
                    ledger_live_memory_bytes: transaction.live_memory_bytes(),
                    expected_live_memory_bytes,
                },
            );
        }
        let peak_required_memory_bytes = transaction.peak_memory_bytes();
        transaction.commit();

        Ok(FiniteScenarioPcCompilation {
            problem,
            peak_required_memory_bytes,
            problem_retained_bytes,
        })
    }
}

fn checked_compiler_returned_carrier_delta_bytes() -> Result<u128, FiniteScenarioPcCompileError> {
    (size_of::<Result<FiniteScenarioPcCompilation, FiniteScenarioPcCompileError>>() as u128)
        .checked_sub(size_of::<SearchProblem>() as u128)
        .ok_or(FiniteScenarioPcCompileError::ProjectionOverflow)
}

fn write_finite_problem_id(
    query: &PcScenarioQuery,
    supply_window: SupplyWindowProjection,
    value: &mut String,
    authorized_bytes: usize,
) -> Result<(), FiniteScenarioPcCompileError> {
    let mut writer = FiniteProblemIdWriter::new(value, authorized_bytes);

    writer.try_push_str(SearchProblemKind::ScenarioPc.as_str())?;
    writer.try_push_ascii(':')?;
    writer.try_push_str(ScenarioQuerySource::ScenarioPreset.as_str())?;
    writer.try_push_ascii(':')?;
    writer.try_push_decimal(u128::from(query.initial_board().width()))?;
    writer.try_push_ascii('x')?;
    writer.try_push_decimal(u128::from(query.initial_board().visible_height()))?;
    writer.try_push_ascii(':')?;
    writer.try_push_fixed_hex_u64(query.initial_board().occupied_mask())?;
    writer.try_push_ascii(':')?;
    writer.try_push_str(query.remaining_queue().mode())?;
    writer.try_push_ascii(':')?;
    writer.try_push_decimal(query.piece_window().max_pieces() as u128)?;
    writer.try_push_ascii(':')?;
    writer.try_push_decimal(query.exact_pieces().unwrap_or(0) as u128)?;
    writer.try_push_ascii(':')?;
    writer.try_push_decimal(supply_window.source_sequence_length as u128)?;
    writer.try_push_ascii(':')?;
    writer.try_push_str(if supply_window.projects_unplaced_lookahead {
        "true"
    } else {
        "false"
    })?;
    writer.finish()
}

struct FiniteProblemIdWriter<'a> {
    value: &'a mut String,
    authorized_bytes: usize,
}

impl<'a> FiniteProblemIdWriter<'a> {
    fn new(value: &'a mut String, authorized_bytes: usize) -> Self {
        Self {
            value,
            authorized_bytes,
        }
    }

    fn try_push_str(&mut self, fragment: &str) -> Result<(), FiniteScenarioPcCompileError> {
        self.ensure_append(fragment.len())?;
        self.value.push_str(fragment);
        Ok(())
    }

    fn try_push_ascii(&mut self, value: char) -> Result<(), FiniteScenarioPcCompileError> {
        debug_assert!(value.is_ascii());
        self.ensure_append(1)?;
        self.value.push(value);
        Ok(())
    }

    fn try_push_decimal(&mut self, mut value: u128) -> Result<(), FiniteScenarioPcCompileError> {
        let mut digits = [0_u8; 39];
        let mut first = digits.len();
        loop {
            first -= 1;
            digits[first] = (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.ensure_append(digits.len() - first)?;
        for digit in &digits[first..] {
            self.value.push(char::from(b'0' + *digit));
        }
        Ok(())
    }

    fn try_push_fixed_hex_u64(&mut self, value: u64) -> Result<(), FiniteScenarioPcCompileError> {
        const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
        self.ensure_append(16)?;
        for digit_index in (0..16).rev() {
            let shift = digit_index * 4;
            let digit = ((value >> shift) & 0x0f) as usize;
            self.value.push(char::from(HEX_DIGITS[digit]));
        }
        Ok(())
    }

    fn finish(self) -> Result<(), FiniteScenarioPcCompileError> {
        if self.value.len() != self.authorized_bytes {
            return Err(FiniteScenarioPcCompileError::ProblemIdLengthMismatch {
                expected_bytes: self.authorized_bytes,
                actual_bytes: self.value.len(),
            });
        }
        Ok(())
    }

    fn ensure_append(&self, additional_bytes: usize) -> Result<(), FiniteScenarioPcCompileError> {
        let attempted_bytes = self
            .value
            .len()
            .checked_add(additional_bytes)
            .ok_or(FiniteScenarioPcCompileError::ProjectionOverflow)?;
        if attempted_bytes > self.authorized_bytes {
            return Err(
                FiniteScenarioPcCompileError::ProblemIdAuthorizedLengthExceeded {
                    authorized_bytes: self.authorized_bytes,
                    attempted_bytes,
                },
            );
        }
        if attempted_bytes > self.value.capacity() {
            return Err(
                FiniteScenarioPcCompileError::ProblemIdAllocatedCapacityExceeded {
                    allocated_capacity_bytes: self.value.capacity(),
                    attempted_bytes,
                },
            );
        }
        Ok(())
    }
}

fn finite_build_piece_source_request<'a>(
    query: &'a PcScenarioQuery,
    supply_window: SupplyWindowProjection,
) -> FiniteBuildPieceSourceRequest<'a> {
    FiniteBuildPieceSourceRequest::new(
        finite_build_queue_ref(query.remaining_queue()),
        supply_window.source_sequence_length,
        query.execution_policy().max_patterns(),
        FiniteSupplyProvenanceRef::new(
            query.bag().id().as_str(),
            query.piece_set().id().as_str(),
            None,
            BagBoundaryEvidence::FixedBoundary,
            false,
            false,
        ),
    )
}

fn finite_build_queue_ref(queue: &PcQueueInput) -> FiniteBuildQueueRef<'_> {
    match queue {
        PcQueueInput::FixedSequence(sequence) => FiniteBuildQueueRef::FixedSequence(sequence),
        PcQueueInput::PatternExpression(expression) => {
            FiniteBuildQueueRef::PatternExpression(expression)
        }
        PcQueueInput::Standard7Bag => FiniteBuildQueueRef::Standard7Bag,
        PcQueueInput::BagAlignedPattern(_) | PcQueueInput::Observed(_) => {
            FiniteBuildQueueRef::Unsupported
        }
    }
}

fn into_pc_queue_input(queue: FiniteBuildSupplyQueue) -> PcQueueInput {
    match queue {
        FiniteBuildSupplyQueue::FixedSequence(sequence) => PcQueueInput::FixedSequence(sequence),
        FiniteBuildSupplyQueue::PatternExpression(expression) => {
            PcQueueInput::PatternExpression(expression)
        }
        FiniteBuildSupplyQueue::Standard7Bag => PcQueueInput::Standard7Bag,
    }
}

fn map_piece_source_materialization_error(
    error: FinitePieceSourceMaterializationError,
) -> FiniteScenarioPcCompileError {
    match error {
        FinitePieceSourceMaterializationError::UnsupportedQueueShape => {
            FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape
        }
        FinitePieceSourceMaterializationError::PatternUniverse(error) => {
            FiniteScenarioPcCompileError::ProblemCompile(
                ProblemCompileError::PatternUniverseMaterialization(error),
            )
        }
        FinitePieceSourceMaterializationError::Allocation(error) => {
            map_finite_allocation_error(error)
        }
        error => FiniteScenarioPcCompileError::PieceSourceMaterialization(error),
    }
}

fn map_finite_allocation_error(error: FiniteSupplyAllocationError) -> FiniteScenarioPcCompileError {
    match error {
        FiniteSupplyAllocationError::ProjectionOverflow => {
            FiniteScenarioPcCompileError::ProjectionOverflow
        }
        FiniteSupplyAllocationError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        } => FiniteScenarioPcCompileError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        },
        error @ (FiniteSupplyAllocationError::AccountingUnderflow
        | FiniteSupplyAllocationError::AllocationFailed { .. }) => {
            FiniteScenarioPcCompileError::FiniteSupplyAllocation(error)
        }
    }
}

fn validate_compile_semantics(query: &PcScenarioQuery) -> Result<(), FiniteScenarioPcCompileError> {
    validate_pc_observation_objective(query.queue_observation_policy(), query.objective().kind())
        .map_err(ProblemCompileError::PcSearchContract)?;

    let max_pieces = query.piece_window().max_pieces();
    if u16::try_from(max_pieces).is_err() {
        return Err(ProblemCompileError::PackingPieceWindowTooLarge { max_pieces }.into());
    }
    Ok(())
}

fn resolve_supply_window(
    query: &PcScenarioQuery,
) -> Result<SupplyWindowProjection, FiniteScenarioPcCompileError> {
    let geometry_piece_count = query
        .exact_pieces()
        .unwrap_or_else(|| query.piece_window().max_pieces());
    let initial_hold_piece_count =
        usize::from(query.allow_hold() && query.hold_state().piece().is_some());
    let required_source_pieces = geometry_piece_count.saturating_sub(initial_hold_piece_count);
    let automatic_source_pieces = geometry_piece_count
        .saturating_add(usize::from(query.allow_hold()))
        .saturating_sub(initial_hold_piece_count);
    let requested_source_pieces = query
        .supply_window_size()
        .map(|window| window.source_pieces());

    let source_sequence_length = match query.remaining_queue() {
        PcQueueInput::FixedSequence(sequence) => {
            let queue_pieces = sequence.len();
            if requested_source_pieces.is_some_and(|requested| requested > queue_pieces) {
                return Err(
                    ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                        source_pieces: requested_source_pieces.unwrap_or_default(),
                        queue_pieces,
                    }
                    .into(),
                );
            }
            requested_source_pieces
                .unwrap_or(automatic_source_pieces)
                .min(queue_pieces)
        }
        PcQueueInput::PatternExpression(expression) => {
            let queue_pieces = expression.sequence_len();
            if requested_source_pieces.is_some_and(|requested| requested > queue_pieces) {
                return Err(
                    ProblemCompileError::SupplyWindowConflictsWithConcreteQueue {
                        source_pieces: requested_source_pieces.unwrap_or_default(),
                        queue_pieces,
                    }
                    .into(),
                );
            }
            requested_source_pieces
                .unwrap_or(automatic_source_pieces)
                .min(queue_pieces)
        }
        PcQueueInput::Standard7Bag => requested_source_pieces.unwrap_or(automatic_source_pieces),
        PcQueueInput::BagAlignedPattern(_) | PcQueueInput::Observed(_) => {
            return Err(FiniteScenarioPcCompileError::UnsupportedBuildProbabilityShape)
        }
    };

    if source_sequence_length < required_source_pieces {
        return Err(ProblemCompileError::SupplyWindowTooShort {
            source_pieces: source_sequence_length,
            required_source_pieces,
        }
        .into());
    }
    let projects_unplaced_lookahead = query.allow_hold()
        && query.exact_pieces() == Some(geometry_piece_count)
        && source_sequence_length == required_source_pieces
        && automatic_source_pieces > source_sequence_length;
    Ok(SupplyWindowProjection {
        source_sequence_length,
        projects_unplaced_lookahead,
    })
}

fn checked_problem_id_requested_bytes(
    query: &PcScenarioQuery,
    supply_window: SupplyWindowProjection,
) -> Result<u128, FiniteScenarioPcCompileError> {
    checked_sum([
        SearchProblemKind::ScenarioPc.as_str().len() as u128,
        ScenarioQuerySource::ScenarioPreset.as_str().len() as u128,
        query.remaining_queue().mode().len() as u128,
        decimal_digits(u128::from(query.initial_board().width())),
        decimal_digits(u128::from(query.initial_board().visible_height())),
        16,
        decimal_digits(query.piece_window().max_pieces() as u128),
        decimal_digits(query.exact_pieces().unwrap_or(0) as u128),
        decimal_digits(supply_window.source_sequence_length as u128),
        if supply_window.projects_unplaced_lookahead {
            4
        } else {
            5
        },
        9,
    ])
}

fn decimal_digits(mut value: u128) -> u128 {
    let mut digits = 1_u128;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn checked_sum<const N: usize>(values: [u128; N]) -> Result<u128, FiniteScenarioPcCompileError> {
    values.into_iter().try_fold(0_u128, |total, value| {
        total
            .checked_add(value)
            .ok_or(FiniteScenarioPcCompileError::ProjectionOverflow)
    })
}

#[cfg(test)]
#[path = "finite_scenario_pc_compiler_tests.rs"]
mod tests;
