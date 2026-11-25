//! Bounded Legacy Cover candidate providers, not a product routing decision.
//!
//! The Legacy reducer's denominator is the requested materialized prefix, not
//! the queue language of a new scenario. The complete-authority entry point
//! requires complete expansion. The full-Observed provider additionally
//! requires the whole expansion to be the requested denominator. The
//! separate 10×4 probe never completes a truncated source.

use clearra_build_coverage::{
    coverage::{
        BuildCoverageExecution, BuildCoverageExecutionError, ConcreteAssignmentCoverageRow,
        ConcreteCoverageCompleteness, ConcreteCoverageScope,
    },
    query::build_coverage_query::BuildCoverageQuery,
    template::{
        SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint, SlotSymmetry,
        TemplateCanonicalization, TemplateSymmetry,
    },
};
use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionControl,
    solution::{
        NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64ColoredTilingIdentity,
        StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend, WasmCpuSearchError};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::{
    piece_source::{PieceSourceKind, SupplyTruncationReason},
    queue::fixed_sequence::FixedSequence,
};
use sha2::{Digest, Sha256};

const ORIGINAL_ORDINAL: usize = 0;
const REQUESTED_PATTERN_COUNT: usize = 1;
const COMPLETE_AUTHORITY_MAX_SLOTS: usize = 5;
const FOUR_LINE_PREFIX_SLOTS: usize = 10;

#[derive(Clone, Copy)]
enum OriginalSupplyAuthority {
    CompleteAuthority,
    TruncatedSourceExactPrefix,
    CompleteObserved,
}

#[derive(Debug, PartialEq)]
pub(super) enum RustLegacyCoverCandidateError {
    Cancelled,
    UnsupportedAuthority,
    UnsupportedAssignment,
    IncompleteOriginalSupply,
    InvalidPhysicalProblem,
    Kernel(WasmCpuSearchError),
    IncompletePhysicalEvidence,
    MissingExactPositiveRow,
    ConflictingExactPositiveRows,
    Reduction(BuildCoverageExecutionError),
}

/// Positive coverage of the one original ordinal only. Deliberately carries
/// no Cover result, execution count, or probability of the full source.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct TruncatedLegacyCoverPrefixEvidence {
    original_source_id: u64,
    original_ordinal: usize,
    materialized_pattern_count: usize,
    total_possible_pattern_count: u128,
    truncation_reason: SupplyTruncationReason,
    exact_assignment: StandardBoard64TilingIdentity,
    prefix_coverage: PatternBitSet,
}

struct VerifiedOrdinalCandidate {
    exact_assignment: StandardBoard64TilingIdentity,
    materialized_pattern_count: usize,
    total_possible_pattern_count: u128,
    reduction: BuildCoverageExecution,
}

/// Runs the already-supported Rust physical PC path on the original Legacy
/// ordinal and submits a typed, exact positive row to the Legacy reducer.
/// An inapplicable request or unproven absence is not a zero coverage result.
pub(super) fn execute_single_ordinal_candidate(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    control: &ExecutionControl,
) -> Result<BuildCoverageExecution, RustLegacyCoverCandidateError> {
    execute_scoped_ordinal(
        legacy,
        query,
        OriginalSupplyAuthority::CompleteAuthority,
        control,
    )
    .map(|verified| verified.reduction)
}

/// Uses one existing Rust PC verification run for the whole, unchanged
/// Observed universe. The colored allow-list may keep several physical
/// partitions; only the exact placement identity may publish this row.
/// This candidate provider does not own the public Legacy result fields.
pub(super) fn execute_complete_observed_candidate(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    control: &ExecutionControl,
) -> Result<BuildCoverageExecution, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;
    let authority = OriginalSupplyAuthority::CompleteObserved;
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    validate_canonical_authority(legacy, query, authority)?;
    let expected = concrete_assignment_identity(query)?;
    let original = legacy.piece_source();
    let original_universe = original
        .materialized_universe()
        .filter(|universe| {
            original.kind() == PieceSourceKind::ObservedWindow
                && original.complete()
                && complete_requested_universe(universe, query.pattern_count())
        })
        .ok_or(Error::IncompleteOriginalSupply)?;
    let board = query.template().board_size();
    let projected_query = physical_scenario_query(
        board,
        legacy,
        legacy.core_query().remaining_queue().clone(),
        expected,
    );
    let projected = ProblemCompiler::compile_scenario_pc(&projected_query)
        .map_err(|_| Error::InvalidPhysicalProblem)?;
    let projected_universe = projected
        .piece_source()
        .materialized_universe()
        .filter(|universe| {
            projected.piece_source().kind() == PieceSourceKind::ObservedWindow
                && projected.piece_source().complete()
                && complete_requested_universe(universe, query.pattern_count())
        })
        .ok_or(Error::IncompleteOriginalSupply)?;
    // A universe with the same size or same set of queues may have different
    // ordinals. Do not publish coverage until every original ordinal and its
    // queue length are bound to the PC execution's own materialization.
    for ordinal in 0..query.pattern_count() {
        if control.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let original_queue = original_universe
            .try_sequence_at(ordinal)
            .ok_or(Error::IncompleteOriginalSupply)?;
        if original_queue.len() != expected.placement_count()
            || projected_universe
                .try_sequence_at(ordinal)
                .is_none_or(|queue| queue != original_queue)
        {
            return Err(Error::IncompleteOriginalSupply);
        }
    }
    let execution = execute_physical(&projected, control)?;
    let covered = exact_complete_bits(&execution, expected, query.pattern_count())?;
    reduce_verified_row(legacy, query, control, authority, expected, covered)
}

fn complete_requested_universe(
    universe: &clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse,
    requested: usize,
) -> bool {
    universe.complete()
        && universe.truncation_reason().is_none()
        && universe.pattern_count() == requested
        && universe.total_possible_pattern_count() == requested as u128
}

/// Qualify a bounded positive Legacy 10×4 row on the materialized ordinal
/// zero even though the source budget truncates the wider observed expansion.
/// Never use this evidence to replace the existing Cover completeness result.
pub(super) fn execute_truncated_source_exact_prefix(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    control: &ExecutionControl,
) -> Result<TruncatedLegacyCoverPrefixEvidence, RustLegacyCoverCandidateError> {
    let verified = execute_scoped_ordinal(
        legacy,
        query,
        OriginalSupplyAuthority::TruncatedSourceExactPrefix,
        control,
    )?;
    Ok(TruncatedLegacyCoverPrefixEvidence {
        original_source_id: legacy.piece_source().id().get(),
        original_ordinal: ORIGINAL_ORDINAL,
        materialized_pattern_count: verified.materialized_pattern_count,
        total_possible_pattern_count: verified.total_possible_pattern_count,
        truncation_reason: SupplyTruncationReason::ObservedWindowBudgetExceeded,
        exact_assignment: verified.exact_assignment,
        prefix_coverage: verified.reduction.union().covered_patterns().clone(),
    })
}

fn execute_scoped_ordinal(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    authority: OriginalSupplyAuthority,
    control: &ExecutionControl,
) -> Result<VerifiedOrdinalCandidate, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;

    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    validate_canonical_authority(legacy, query, authority)?;
    let expected = concrete_assignment_identity(query)?;
    let source = legacy.piece_source();
    let materialized = source
        .materialized_universe()
        .filter(|universe| match authority {
            OriginalSupplyAuthority::CompleteAuthority => {
                source.complete()
                    && universe.complete()
                    && universe.total_possible_pattern_count() == universe.pattern_count() as u128
                    && universe.truncation_reason().is_none()
            }
            // Whole expansion evidence has its own execution entry point.
            OriginalSupplyAuthority::CompleteObserved => false,
            OriginalSupplyAuthority::TruncatedSourceExactPrefix => {
                source.kind() == PieceSourceKind::ObservedWindow
                    && !source.complete()
                    && !universe.complete()
                    && universe.pattern_count() == query.limits().max_patterns()
                    && universe.pattern_count() >= REQUESTED_PATTERN_COUNT
                    && universe.total_possible_pattern_count() > universe.pattern_count() as u128
                    && universe.truncation_reason()
                        == Some(SupplyTruncationReason::ObservedWindowBudgetExceeded)
            }
        })
        .ok_or(Error::IncompleteOriginalSupply)?;
    let ordinal = materialized
        .try_sequence_at(ORIGINAL_ORDINAL)
        .filter(|sequence| sequence.len() == expected.placement_count())
        .ok_or(Error::IncompleteOriginalSupply)?;
    let board = query.template().board_size();
    let projected_query = physical_scenario_query(
        board,
        legacy,
        PcQueueInput::fixed_sequence(FixedSequence::new(ordinal.into_owned())),
        expected,
    );
    let problem = ProblemCompiler::compile_scenario_pc(&projected_query)
        .map_err(|_| Error::InvalidPhysicalProblem)?;
    let execution = execute_physical(&problem, control)?;
    let covered = exact_positive_bits(&execution, expected)?;
    Ok(VerifiedOrdinalCandidate {
        exact_assignment: expected,
        materialized_pattern_count: materialized.pattern_count(),
        total_possible_pattern_count: materialized.total_possible_pattern_count(),
        reduction: reduce_verified_row(legacy, query, control, authority, expected, covered)?,
    })
}

fn physical_scenario_query(
    board: BoardSize,
    legacy: &SearchProblem,
    source: PcQueueInput,
    expected: StandardBoard64TilingIdentity,
) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::new(board.width(), board.height(), 0),
        source,
        PieceWindow::new(expected.placement_count()),
    )
    .with_rule(legacy.rule_profile_value())
    .with_allow_hold(false)
    .with_exact_pieces(Some(expected.placement_count()))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    // Only a coarse pruning hint. Same-color unions can hide distinct
    // placements, so they are never accepted as evidence below.
    .with_allowed_colored_solution_identities([
        StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(expected),
    ])
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_workers(1)
            .with_max_patterns(legacy.budget().max_patterns()),
    )
}

fn execute_physical(
    problem: &SearchProblem,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;
    let execution = match WasmCpuSearchBackend::execute_with_control(problem, control) {
        Err(WasmCpuSearchError::Cancelled) => return Err(Error::Cancelled),
        other => other.map_err(Error::Kernel)?,
    };
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(execution)
}

fn reduce_verified_row(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    control: &ExecutionControl,
    authority: OriginalSupplyAuthority,
    expected: StandardBoard64TilingIdentity,
    covered: PatternBitSet,
) -> Result<BuildCoverageExecution, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;
    let scope = ConcreteCoverageScope::new_uniform(
        query.template().board_size(),
        legacy.piece_source().id().get(),
        PatternUniverseId::new(scope_id(
            legacy,
            authority,
            query.pattern_count(),
            b"ordinal-universe",
        )),
        PatternWeightModelId::new(scope_id(
            legacy,
            authority,
            query.pattern_count(),
            b"uniform-weights",
        )),
        scope_id(
            legacy,
            authority,
            query.pattern_count(),
            b"physical-execution",
        ),
    );
    let row = ConcreteAssignmentCoverageRow::new(
        expected,
        scope,
        covered,
        ConcreteCoverageCompleteness::Complete,
    );
    // The truncated probe can only reach this function with its single
    // positive row; its caller returns prefix evidence, not the reduction.
    let reduction =
        BuildCoverageExecution::from_verified_concrete_assignment_rows(query, scope, vec![row]);
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    reduction.map_err(Error::Reduction)
}

fn validate_canonical_authority(
    legacy: &SearchProblem,
    query: &BuildCoverageQuery,
    authority: OriginalSupplyAuthority,
) -> Result<(), RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError::UnsupportedAuthority;

    let bridge = legacy.build_query().ok_or(UnsupportedAuthority)?;
    let template = query.template();
    let board = template.board_size();
    let slot_count = template.slots().len();
    let limits = query.limits();
    let admitted_shape = match authority {
        OriginalSupplyAuthority::CompleteAuthority | OriginalSupplyAuthority::CompleteObserved => {
            slot_count <= COMPLETE_AUTHORITY_MAX_SLOTS
        }
        OriginalSupplyAuthority::TruncatedSourceExactPrefix => {
            board.width() == 10 && board.height() == 4 && slot_count == FOUR_LINE_PREFIX_SLOTS
        }
    };
    let admitted_pattern_count = match authority {
        OriginalSupplyAuthority::CompleteObserved => {
            query.pattern_count() != 0
                && (limits.max_patterns() == 0 || query.pattern_count() <= limits.max_patterns())
        }
        _ => query.pattern_count() == REQUESTED_PATTERN_COUNT,
    };
    if !admitted_pattern_count
        || bridge.pattern_count() != query.pattern_count()
        || bridge.selected_pattern_id().is_some()
        || bridge.template().id() != template.id()
        || bridge.template().label() != template.label()
        || bridge.template().board_size() != template.board_size()
        || bridge.template().slot_count() != template.slots().len()
        || bridge.limits().max_assignments() != limits.max_assignments()
        || bridge.limits().max_patterns() != limits.max_patterns()
        || !admitted_shape
        || legacy.initial_board().occupied_mask() != 0
        || legacy.supply().hold_enabled()
    {
        return Err(UnsupportedAuthority);
    }
    let canonical = ProblemCompiler::compile_build(bridge).map_err(|_| UnsupportedAuthority)?;
    if &canonical != legacy {
        return Err(UnsupportedAuthority);
    }
    Ok(())
}

fn concrete_assignment_identity(
    query: &BuildCoverageQuery,
) -> Result<StandardBoard64TilingIdentity, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError::UnsupportedAssignment;

    let template = query.template();
    let board = template.board_size();
    let slots = template.slots();
    let domains = query.domains();
    if !(1..=FOUR_LINE_PREFIX_SLOTS).contains(&slots.len())
        || !(4..=10).contains(&board.width())
        || board.area() as usize != slots.len() * 4
        || domains.len() != slots.len()
        || !query.constraints().is_empty()
        || template.symmetry() != TemplateSymmetry::None
        || template.canonicalization() != TemplateCanonicalization::None
    {
        return Err(UnsupportedAssignment);
    }

    let mut occupied = 0_u64;
    let mut placements = Vec::with_capacity(slots.len());
    for (index, slot) in slots.iter().enumerate() {
        if slots[..index].iter().any(|prior| prior.id() == slot.id())
            || slot.cells().len() != 4
            || slot.hold_constraint() != SlotHoldConstraint::Any
            || slot.order_constraint() != SlotOrderConstraint::Any
            || slot.symmetry() != SlotSymmetry::None
            || slot.canonicalization() != SlotCanonicalization::None
        {
            return Err(UnsupportedAssignment);
        }
        let mut matching_domains = domains
            .iter()
            .filter(|domain| domain.slot_id() == slot.id());
        let Some([piece]) = matching_domains.next().map(|domain| domain.pieces()) else {
            return Err(UnsupportedAssignment);
        };
        if matching_domains.next().is_some()
            || !slot.allowed_pieces().contains(piece)
            || slot
                .required_piece()
                .is_some_and(|required| required != *piece)
        {
            return Err(UnsupportedAssignment);
        }
        let mut mask = 0;
        for cell in slot.cells() {
            if cell.x() >= board.width() || cell.y() >= board.height() {
                return Err(UnsupportedAssignment);
            }
            let cell_mask =
                1_u64 << (u32::from(board.width()) * u32::from(cell.y()) + u32::from(cell.x()));
            if occupied & cell_mask != 0 {
                return Err(UnsupportedAssignment);
            }
            mask |= cell_mask;
            occupied |= cell_mask;
        }
        placements.push(PiecePlacementMask::new(*piece, mask));
    }
    StandardBoard64TilingIdentity::from_placements(0, placements).map_err(|_| UnsupportedAssignment)
}

fn exact_positive_bits(
    execution: &CoreExecutionResult,
    expected: StandardBoard64TilingIdentity,
) -> Result<PatternBitSet, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;

    require_complete_coverage(execution, REQUESTED_PATTERN_COUNT)?;
    let mut exact = execution
        .solution_coverages()
        .iter()
        .filter(|row| row.identity() == expected);
    let row = exact.next().ok_or(Error::MissingExactPositiveRow)?;
    if exact.next().is_some() {
        return Err(Error::ConflictingExactPositiveRows);
    }
    let covered = row.covered_patterns();
    if covered.pattern_count() != REQUESTED_PATTERN_COUNT || covered.word_at(0) != 1 {
        return Err(Error::MissingExactPositiveRow);
    }
    Ok(covered.clone())
}

fn require_complete_coverage(
    execution: &CoreExecutionResult,
    pattern_count: usize,
) -> Result<(), RustLegacyCoverCandidateError> {
    if execution.usize_field("coverage_pattern_count") != Some(pattern_count)
        || [
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
            "build_variant_count_exact",
        ]
        .iter()
        .any(|key| execution.bool_field(key) != Some(true))
        || execution.bool_field("resource_truncated") != Some(false)
    {
        return Err(RustLegacyCoverCandidateError::IncompletePhysicalEvidence);
    }
    Ok(())
}

fn exact_complete_bits(
    execution: &CoreExecutionResult,
    expected: StandardBoard64TilingIdentity,
    pattern_count: usize,
) -> Result<PatternBitSet, RustLegacyCoverCandidateError> {
    use RustLegacyCoverCandidateError as Error;
    require_complete_coverage(execution, pattern_count)?;
    if execution.bool_field("solution_keys_complete") != Some(true) {
        return Err(Error::IncompletePhysicalEvidence);
    }
    let key = NormalizedTilingSolutionKey::from_standard_board64_identity(expected);
    let mut keys = execution
        .normalized_solution_keys()
        .iter()
        .filter(|candidate| candidate.as_str() == key.as_str());
    let mut rows = execution
        .solution_coverages()
        .iter()
        .filter(|row| row.identity() == expected);
    match (keys.next(), rows.next()) {
        (None, None) => Ok(PatternBitSet::new(pattern_count)),
        (Some(_), Some(row))
            if keys.next().is_none()
                && rows.next().is_none()
                && row.covered_patterns().pattern_count() == pattern_count
                && row.covered_patterns().count_ones() != 0 =>
        {
            Ok(row.covered_patterns().clone())
        }
        _ => Err(Error::IncompletePhysicalEvidence),
    }
}

fn scope_id(
    legacy: &SearchProblem,
    authority: OriginalSupplyAuthority,
    pattern_count: usize,
    purpose: &[u8],
) -> u64 {
    // Bind the original source and physical rules, never a producer candidate
    // ID or display key. This labels in-process evidence; it is not a proof.
    let mut hash = Sha256::new();
    hash.update(b"clearra-legacy-cover-rust-single-ordinal-v1\0");
    hash.update([match authority {
        OriginalSupplyAuthority::CompleteAuthority => 0,
        OriginalSupplyAuthority::TruncatedSourceExactPrefix => 1,
        OriginalSupplyAuthority::CompleteObserved => 2,
    }]);
    hash.update(purpose);
    hash.update([0]);
    hash.update(legacy.piece_source().id().get().to_le_bytes());
    hash.update((ORIGINAL_ORDINAL as u64).to_le_bytes());
    hash.update((pattern_count as u64).to_le_bytes());
    hash.update(legacy.rule_profile_value().id().as_str());
    hash.update([0]);
    hash.update(legacy.problem_id().as_str());
    let digest = hash.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix")).max(1)
}

#[cfg(test)]
#[path = "app_services_legacy_cover_rust_tests.rs"]
mod tests;
