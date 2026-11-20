//! The Legacy Cover bridge reduces exact physical placements in the original
//! request's ordinal space. Modern Build Probability has a different target
//! and probability contract and is not a fallback for this command.

use std::collections::{BTreeMap, BTreeSet};

use clearra_build_coverage::{
    coverage::{
        BuildCoverageExecution, BuildCoverageExecutionError, ConcreteAssignmentCoverageRow,
        ConcreteCoverageCompleteness, ConcreteCoverageScope, PreparedConcreteAssignmentCoverage,
    },
    query::build_coverage_query::BuildCoverageQuery,
};
use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
    solution::{
        StandardBoard64ColoredTilingIdentity, StandardBoard64TilingIdentity,
        STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
    },
};
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem, SearchProblemPreset};
use clearra_supply::{
    pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse,
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    PieceMultisetKey,
};
use sha2::{Digest, Sha256};

use super::{
    cover_service::{
        probability_formatter::format_probability, query_bridge::build_coverage_query_from_bridge,
    },
    field,
};
use crate::{CoreExecutionError, CoreExecutionResult, WasmCpuSearchBackend};

type QueueOrdinals = BTreeMap<Vec<PieceKind>, Vec<usize>>;
type CoverResult = Result<CoreExecutionResult, CoreExecutionError>;

struct OriginalScope {
    concrete: ConcreteCoverageScope,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
}

struct AssignmentPlan<'a> {
    scope: OriginalScope,
    shape: AssignmentShape<'a>,
}

enum AssignmentShape<'a> {
    Positional,
    Concrete {
        prepared: PreparedConcreteAssignmentCoverage<'a>,
        keys: BTreeSet<StandardBoard64TilingIdentity>,
    },
}

impl<'a> AssignmentPlan<'a> {
    fn new(
        problem: &SearchProblem,
        query: &'a BuildCoverageQuery,
        initial: u64,
        goal_mask: u64,
    ) -> Result<Self, CoreExecutionError> {
        let board = query.template().board_size();
        let piece_source_id = problem.piece_source().id().get();
        let pattern_universe_id = PatternUniverseId::new(scope_id(problem, b"ordinal-universe"));
        let pattern_weight_model_id =
            PatternWeightModelId::new(scope_id(problem, b"uniform-weights"));
        let scope = OriginalScope {
            concrete: ConcreteCoverageScope::new_uniform(
                board,
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                scope_id(problem, b"physical-evidence"),
            )
            .with_initial_occupancy(initial),
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count: query.pattern_count(),
        };
        // The original bridge uses one-cell placeholder slots when it has
        // no actual placement coordinates. The public legacy projection
        // exposes only the number of rows and their symmetric union, not
        // a positional binding of candidate IDs to those slots.
        if query
            .template()
            .slots()
            .iter()
            .any(|slot| slot.cells().len() != 4)
        {
            return Ok(Self {
                scope,
                shape: AssignmentShape::Positional,
            });
        }
        let prepared = PreparedConcreteAssignmentCoverage::new(query, scope.concrete)
            .map_err(reduction_error)?;
        let keys = prepared
            .identities()
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(reduction_error)?;
        if keys.len() != prepared.identities().len() {
            return Err(reduction_error(
                BuildCoverageExecutionError::AmbiguousConcreteAssignment,
            ));
        }
        if keys.iter().any(|identity| {
            identity
                .placement_masks()
                .iter()
                .fold(initial, |all, &mask| all | mask)
                != goal_mask
        }) {
            return Err(unavailable(
                "rust_legacy_cover_template_outside_packing_frame",
            ));
        }
        Ok(Self {
            scope,
            shape: AssignmentShape::Concrete { prepared, keys },
        })
    }

    fn precise_keys(&self) -> Option<&BTreeSet<StandardBoard64TilingIdentity>> {
        match &self.shape {
            AssignmentShape::Positional => None,
            AssignmentShape::Concrete { keys, .. } => Some(keys),
        }
    }

    fn compatible_multisets(&self) -> Option<BTreeSet<PieceMultisetKey>> {
        Some(
            self.precise_keys()?
                .iter()
                .map(|identity| {
                    PieceMultisetKey::from_pieces((0..identity.placement_count()).filter_map(
                        |index| identity.placement(index).map(|placement| placement.piece()),
                    ))
                })
                .collect(),
        )
    }

    fn reduce(
        self,
        query: &BuildCoverageQuery,
        ordinals: &[Vec<usize>],
        execution: Option<&CoreExecutionResult>,
    ) -> Result<BuildCoverageExecution, CoreExecutionError> {
        let positive = physical_rows(self.precise_keys(), ordinals.len(), execution)?;
        match self.shape {
            AssignmentShape::Concrete { prepared, keys } => {
                let rows = keys
                    .into_iter()
                    .map(|identity| {
                        let covered = remap_bits(
                            positive.get(&identity).copied(),
                            self.scope.pattern_count,
                            ordinals,
                        )?;
                        Ok(ConcreteAssignmentCoverageRow::new(
                            identity,
                            self.scope.concrete,
                            covered,
                            ConcreteCoverageCompleteness::Complete,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                prepared.reduce_rows(rows).map_err(reduction_error)
            }
            AssignmentShape::Positional => {
                let rows = positive
                    .into_values()
                    .enumerate()
                    .map(|(index, physical)| {
                        let covered =
                            remap_bits(Some(physical), self.scope.pattern_count, ordinals)?;
                        Ok(CoverageRow::new_with_piece_source(
                            index as u64,
                            CoverageRowKind::Build,
                            self.scope.piece_source_id,
                            self.scope.pattern_universe_id,
                            self.scope.pattern_weight_model_id,
                            covered,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                BuildCoverageExecution::from_legacy_positional_rows(query, &rows)
                    .map_err(reduction_error)
            }
        }
    }
}

pub(crate) fn execute_bridge(problem: &SearchProblem, control: &ExecutionControl) -> CoverResult {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    let bridge = problem
        .build_query()
        .ok_or(CoreExecutionError::UnsupportedProblem)?;
    execute(problem, &build_coverage_query_from_bridge(bridge), control)
}

pub(crate) fn execute(
    problem: &SearchProblem,
    query: &BuildCoverageQuery,
    control: &ExecutionControl,
) -> CoverResult {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if problem.preset() != SearchProblemPreset::Build {
        return Err(CoreExecutionError::UnsupportedProblem);
    }
    let bridge = problem
        .build_query()
        .ok_or(CoreExecutionError::UnsupportedProblem)?;
    let template = query.template();
    let board = template.board_size();
    let limits = query.limits();
    let pattern_count = query.pattern_count();
    let selected = bridge.selected_pattern_id();
    if bridge.template().id() != template.id()
        || bridge.template().label() != template.label()
        || bridge.template().board_size() != board
        || bridge.template().slot_count() != template.slots().len()
        || bridge.pattern_count() != pattern_count
        || bridge.limits().max_assignments() != limits.max_assignments()
        || bridge.limits().max_patterns() != limits.max_patterns()
        || problem.initial_board().width() != board.width()
        || problem.visible_height() != board.height()
        || problem.initial_board().has_extended_occupancy()
        || problem
            .core_query()
            .allowed_colored_solution_identities()
            .is_some()
    {
        return Err(unavailable("rust_legacy_cover_authority_mismatch"));
    }
    if pattern_count == 0
        || pattern_count > limits.max_patterns()
        || limits.max_assignments() == 0
        || selected.is_some_and(|ordinal| ordinal >= pattern_count)
    {
        return Err(CoreExecutionError::Cover(
            "invalid_legacy_cover_scope".to_owned(),
        ));
    }
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or_else(|| unavailable("rust_legacy_cover_missing_original_supply"))?;
    let index_range = selected.map_or(0..pattern_count, |ordinal| ordinal..ordinal + 1);
    if index_range.end > universe.pattern_count() {
        return Err(unavailable("rust_legacy_cover_missing_original_ordinals"));
    }

    // As on the legacy C bridge, the active packing frame is the smaller
    // of visible rows and ceil(4 * maximum pieces / width). Divisibility
    // and area are necessary whether or not the template is concrete.
    let width = usize::from(board.width());
    let max_pieces = problem
        .exact_pieces()
        .unwrap_or_else(|| problem.piece_window().max_pieces());
    let max_cells = max_pieces
        .checked_mul(4)
        .ok_or_else(|| unavailable("rust_legacy_cover_piece_window_overflow"))?;
    let initial = problem.initial_board().occupied_mask();
    let occupied = initial.count_ones() as usize;
    let top = (u64::BITS - initial.leading_zeros()) as usize;
    let active_rows = max_cells
        .checked_add(occupied)
        .ok_or_else(|| unavailable("rust_legacy_cover_piece_window_overflow"))?
        .div_ceil(width)
        .max(top.div_ceil(width))
        .clamp(1, usize::from(board.height()));
    let active_area = active_rows * width;
    let empty_area = active_area.saturating_sub(occupied);
    if empty_area > max_cells
        || !empty_area.is_multiple_of(4)
        || (problem.exact_pieces().is_some() && max_cells != empty_area)
    {
        let reduction =
            BuildCoverageExecution::from_proved_empty_packing(query).map_err(reduction_error)?;
        return project(
            problem,
            query,
            reduction,
            universe,
            "exact-empty-packing",
            None,
            control,
        );
    }
    if active_area > u64::BITS as usize
        || empty_area / 4 > STANDARD_BOARD64_TILING_MAX_PLACEMENTS
        || empty_area == 0
        || control.partition().count() != 1
    {
        return Err(unavailable(
            "rust_legacy_cover_requires_compact_concrete_template",
        ));
    }

    let goal_mask = u64::MAX >> (u64::BITS as usize - active_area);
    let plan = AssignmentPlan::new(problem, query, initial, goal_mask)?;
    let piece_count = empty_area / 4;
    let hold_enabled = problem.supply().hold_enabled();
    let held = problem
        .core_query()
        .hold_state()
        .piece()
        .filter(|_| hold_enabled);
    let min_remaining = problem.core_query().min_remaining_queue();
    let required_queue = piece_count - usize::from(hold_enabled && held.is_some());
    let retain_full_queue = hold_enabled || min_remaining > 0;
    let multisets = (!retain_full_queue)
        .then(|| plan.compatible_multisets())
        .flatten();
    let mut queues = QueueOrdinals::new();
    for ordinal in index_range {
        if control.is_cancelled() {
            return Err(CoreExecutionError::Cancelled);
        }
        let queue = universe
            .try_sequence_at(ordinal)
            .filter(|queue| queue.len() >= required_queue)
            .ok_or_else(|| unavailable("rust_legacy_cover_missing_original_ordinals"))?;
        let queue = if retain_full_queue {
            queue.as_ref()
        } else {
            &queue[..required_queue]
        };
        if multisets.as_ref().is_none_or(|multisets| {
            multisets.contains(&PieceMultisetKey::from_pieces(queue.iter().copied()))
        }) {
            queues.entry(queue.to_vec()).or_default().push(ordinal);
        }
    }
    if queues.is_empty() {
        let reduction = plan.reduce(query, &[], None)?;
        return project(
            problem,
            query,
            reduction,
            universe,
            "exact-supply-rejection",
            None,
            control,
        );
    }
    let projected_queue_len = queues
        .first_key_value()
        .expect("nonempty queue map")
        .0
        .len();
    let input = if queues.len() == 1 {
        PcQueueInput::fixed_sequence(FixedSequence::new(
            queues
                .first_key_value()
                .expect("nonempty queue map")
                .0
                .clone(),
        ))
    } else {
        let mut expression = String::new();
        for (index, sequence) in queues.keys().enumerate() {
            if index != 0 {
                expression.push(';');
            }
            expression.extend(sequence.iter().map(|piece| piece.as_ascii()));
        }
        PcQueueInput::pattern_expression(
            QueuePatternExpression::parse(&expression, queues.len())
                .map_err(|_| unavailable("rust_legacy_cover_invalid_physical_queue"))?,
        )
    };
    let mut request = PcScenarioQuery::new(
        PcScenarioBoard::new(board.width(), active_rows as u16, initial),
        input,
        PieceWindow::new(piece_count),
    )
    .with_rule(problem.rule_profile_value())
    .with_allow_hold(hold_enabled)
    .with_hold_piece(held)
    .with_supply_window_size(SupplyWindowSize::new(projected_queue_len))
    .with_min_remaining_queue(min_remaining)
    .with_exact_pieces(Some(piece_count))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_execution_policy(
        problem
            .core_query()
            .execution_policy()
            .clone()
            .with_max_patterns(queues.len()),
    );
    if let Some(keys) = plan.precise_keys() {
        request = request.with_allowed_colored_solution_identities(
            keys.iter()
                .copied()
                .map(StandardBoard64ColoredTilingIdentity::from_standard_board64_identity),
        );
    }
    if let Some(kick_profile) = problem.rule_profile().verified_kick_profile() {
        request = request.with_verified_kick_table_profile(kick_profile.clone());
    }
    let physical = ProblemCompiler::compile_scenario_pc(&request)
        .map_err(|_| unavailable("rust_legacy_cover_invalid_physical_problem"))?;
    // The finite expression may normalize the queue order. Build the remap
    // from the physical request's actual compiled ordinal order, not from
    // map iteration or equality of the number of patterns alone.
    let projected = physical
        .piece_source()
        .materialized_universe()
        .filter(|projected| projected.complete() && projected.pattern_count() == queues.len())
        .ok_or_else(|| unavailable("rust_legacy_cover_physical_ordinal_mismatch"))?;
    let mut ordinals = Vec::with_capacity(queues.len());
    for index in 0..projected.pattern_count() {
        let sequence = projected.sequence_at(index);
        ordinals.push(
            queues
                .remove(sequence.as_ref())
                .ok_or_else(|| unavailable("rust_legacy_cover_physical_ordinal_mismatch"))?,
        );
    }
    if !queues.is_empty() {
        return Err(unavailable("rust_legacy_cover_physical_ordinal_mismatch"));
    }

    let source_label = if plan.precise_keys().is_some() {
        "rust-exact-concrete-assignment"
    } else {
        "rust-legacy-positional-coverage"
    };
    WasmCpuSearchBackend::execute_with_control_and_terminal(&physical, control, |outcome, _| {
        let execution = outcome.map_err(|error| error.into_core_execution_error())?;
        let reduction = plan.reduce(query, &ordinals, Some(&execution))?;
        project(
            problem,
            query,
            reduction,
            universe,
            source_label,
            Some(&execution),
            control,
        )
    })
}

fn physical_rows<'result>(
    identities: Option<&BTreeSet<StandardBoard64TilingIdentity>>,
    physical_pattern_count: usize,
    execution: Option<&'result CoreExecutionResult>,
) -> Result<BTreeMap<StandardBoard64TilingIdentity, &'result PatternBitSet>, CoreExecutionError> {
    let incomplete = || unavailable("rust_legacy_cover_incomplete_physical_evidence");
    let mut positive = BTreeMap::new();
    let allowed = |identity: &StandardBoard64TilingIdentity| {
        identities.is_none_or(|keys| keys.contains(identity))
    };
    if let Some(execution) = execution {
        if execution.usize_field("coverage_pattern_count") != Some(physical_pattern_count)
            || execution.bool_field("resource_truncated") != Some(false)
            || [
                "count_complete",
                "probability_complete",
                "solution_probability_complete",
                "build_variant_count_exact",
                "solution_keys_complete",
            ]
            .iter()
            .any(|key| execution.bool_field(key) != Some(true))
        {
            return Err(incomplete());
        }
        for row in execution.solution_coverages() {
            let identity = row.identity();
            if allowed(&identity) {
                let coverage = row.covered_patterns();
                if coverage.pattern_count() != physical_pattern_count
                    || coverage.count_ones() == 0
                    || positive.insert(identity, coverage).is_some()
                {
                    return Err(incomplete());
                }
            }
        }
        let mut exact_export = BTreeSet::new();
        for identity in execution.normalized_solution_identities().iter().copied() {
            if allowed(&identity) && !exact_export.insert(identity) {
                return Err(incomplete());
            }
        }
        if positive.keys().copied().ne(exact_export) {
            return Err(incomplete());
        }
    }
    Ok(positive)
}

fn remap_bits(
    physical: Option<&PatternBitSet>,
    pattern_count: usize,
    ordinals: &[Vec<usize>],
) -> Result<PatternBitSet, CoreExecutionError> {
    let mut covered = PatternBitSet::new(pattern_count);
    if let Some(physical) = physical {
        for (index, original) in ordinals.iter().enumerate() {
            if physical.contains(PatternId::new(index)) {
                for &ordinal in original {
                    covered
                        .insert(PatternId::new(ordinal))
                        .map_err(|_| unavailable("rust_legacy_cover_physical_ordinal_mismatch"))?;
                }
            }
        }
    }
    Ok(covered)
}

fn project(
    problem: &SearchProblem,
    query: &BuildCoverageQuery,
    coverage: BuildCoverageExecution,
    universe: &MaterializedPatternUniverse,
    source: &'static str,
    execution: Option<&CoreExecutionResult>,
    control: &ExecutionControl,
) -> CoverResult {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    let template = query.template();
    let limits = query.limits();
    let board = template.board_size();
    let covered = coverage.union().covered_patterns();
    let positive_rows = coverage
        .matrix()
        .matrix()
        .rows()
        .iter()
        .filter(|row| !row.coverage_bits().is_empty())
        .count();
    let complete = coverage.exact_cover_complete()
        && problem
            .build_query()
            .and_then(|bridge| bridge.selected_pattern_id())
            .is_none_or(|_| query.pattern_count() == 1);
    let mut fields = vec![
        field("status", "cover-executed"),
        field("template", template.id()),
        field("template_label", template.label().unwrap_or("none")),
        field("board_width", board.width()),
        field("board_height", board.height()),
        field("slot_count", template.slots().len()),
        field("pattern_count", query.pattern_count()),
        field("max_assignments", limits.max_assignments()),
        field("max_patterns", limits.max_patterns()),
        field("assignment_count", coverage.assignments().len()),
        field(
            "assignment_exact_cover_complete",
            coverage.exact_cover_complete(),
        ),
        field(
            "assignment_exact_cover_searched_nodes",
            coverage.exact_cover_searched_nodes(),
        ),
        field("coverage_matrix_row_count", positive_rows),
        field("union_covered_patterns", covered.count_ones()),
        // The legacy denominator is the requested ordinal range, not the
        // whole (possibly truncated) Observed source behind it.
        field(
            "build_coverage_probability",
            format_probability(coverage.result().probability().get()),
        ),
        field("coverage_row_source", source),
        field("coverage_scope", "requested-materialized-ordinals"),
        field("original_supply_complete", universe.complete()),
        field("materialized_pattern_count", universe.pattern_count()),
        field(
            "original_possible_pattern_count",
            universe.total_possible_pattern_count(),
        ),
        field("probability_complete", complete),
        field("count_complete", complete),
        field("resource_truncated", !coverage.exact_cover_complete()),
    ];
    if let Some(execution) = execution {
        // These are actually observed counters of the compressed physical
        // run. They are not mislabeled as C's retained-variant/trace counts.
        for key in [
            "packing_candidate_count",
            "worker_count",
            "observed_active_workers",
        ] {
            if let Some(value) = execution.field(key) {
                fields.push(field(key, value));
            }
        }
    } else if source == "exact-empty-packing" {
        for key in [
            "packing_candidate_count",
            "core_buildup_variant_count",
            "pattern_verified_execution_count",
            "unique_trace_count",
            "core_coverage_row_count",
        ] {
            fields.push(field(key, 0));
        }
    }
    Ok(CoreExecutionResult::new(fields, Vec::new())
        .with_coverage_pattern_words(covered.to_owned_words()))
}

fn scope_id(problem: &SearchProblem, purpose: &[u8]) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"clearra-rust-legacy-cover-v1\0");
    hash.update(purpose);
    hash.update([0]);
    hash.update(problem.problem_id().as_str().as_bytes());
    let id: [u8; 8] = hash.finalize()[..8]
        .try_into()
        .expect("fixed SHA-256 prefix");
    u64::from_le_bytes(id).max(1)
}

fn unavailable(component: &'static str) -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable { component }
}

fn reduction_error(error: BuildCoverageExecutionError) -> CoreExecutionError {
    CoreExecutionError::Cover(format!("Coverage({error:?})"))
}
