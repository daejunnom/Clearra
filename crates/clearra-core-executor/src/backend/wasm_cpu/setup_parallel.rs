// SRP rationale: one change reason owns the exact multiworker Setup Finder execution protocol.
// It includes deterministic task partitioning, worker-local coverage
// evaluation, and ordered coordinator reduction; wire encoding and segmented
// storage remain separate owners.
use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
use clearra_problem::{
    compile_setup_search_condition, setup_search_condition_count, SetupSearchCondition,
    SetupSearchQuery, SetupTerminalSupplyTarget,
};
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use crate::{CoreExecutionResult, SetupCandidateReport, SetupHoldConditionReport};

use super::{
    piece_index,
    setup_all_paths::{enumerate_setup_completion_paths, SetupSolutionPath},
    setup_coverage_graph::SetupCoverageGraph,
    setup_finder::{
        compare_setup_candidates, compile_setup_pattern_index, covered_word_weight,
        finish_setup_result, include_setup_depth_range, probability_string,
        retain_best_setup_state_per_board, terminal_supply_target_word, CompletedSetupCoverage,
        SetupSupplyStateLayout, SetupSupplyTransitionCatalog, COVERAGE_WORD_LANES,
    },
    setup_graph_builder::{SetupGraphBuildAdvance, SetupGraphBuildSession, SetupSharedGraph},
    setup_parallel_segmented::{SegmentedArray, SegmentedGenerationArray},
    setup_parallel_wire::{
        decode_initialization, decode_results, decode_tasks, encode_initialization, encode_results,
        encode_tasks, is_setup_parallel_initialization, SetupParallelShapeResult,
        SetupParallelTask, SetupParallelTaskResult,
    },
    setup_representative::{SetupRepresentativeResolver, SetupWitness},
    WasmExactSearchError,
};

const EMPTY_WORDS: [u64; COVERAGE_WORD_LANES] = [0; COVERAGE_WORD_LANES];
const NO_WITNESS: u32 = u32::MAX;

pub(crate) enum WasmSetupParallelProduce {
    Pending,
    Initialization(Vec<u8>),
    Batch(Vec<u8>),
    Completed,
    Cancelled,
}

pub(crate) struct WasmSetupParallelCoordinator {
    builder: Option<SetupGraphBuildSession>,
    shared: Option<SetupSharedGraph>,
    tasks: Vec<SetupParallelTask>,
    next_task: usize,
    pending_results: Vec<Option<SetupParallelTaskResult>>,
    received_task_flags: Vec<bool>,
    received_tasks: usize,
    next_merge_task: usize,
    condition_merges: Option<Vec<SetupConditionMerge>>,
    solution_paths: Vec<SetupSolutionPath>,
    worker_count: usize,
}

impl WasmSetupParallelCoordinator {
    pub(crate) fn new(
        query: &SetupSearchQuery,
        worker_count: usize,
    ) -> Result<Self, WasmExactSearchError> {
        let builder = SetupGraphBuildSession::new(query)?;
        let condition_word_counts = builder.condition_pattern_word_counts()?;
        let tasks = plan_parallel_tasks(&condition_word_counts, worker_count)?;
        let task_count = tasks.len();
        Ok(Self {
            builder: Some(builder),
            shared: None,
            tasks,
            next_task: 0,
            pending_results: (0..task_count).map(|_| None).collect(),
            received_task_flags: vec![false; task_count],
            received_tasks: 0,
            next_merge_task: 0,
            condition_merges: None,
            solution_paths: Vec::new(),
            worker_count: worker_count.max(1),
        })
    }

    pub(crate) fn condition_count(&self) -> usize {
        self.builder.as_ref().map_or_else(
            || self.condition_merges.as_ref().map_or(0, Vec::len),
            SetupGraphBuildSession::condition_count,
        )
    }

    pub(crate) fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn geometry_nodes(&self) -> usize {
        self.builder.as_ref().map_or_else(
            || {
                self.shared
                    .as_ref()
                    .map_or(0, |shared| shared.geometry_expanded_nodes)
            },
            SetupGraphBuildSession::geometry_nodes,
        )
    }

    pub fn partial_build_nodes(&self) -> usize {
        self.builder.as_ref().map_or_else(
            || {
                self.shared
                    .as_ref()
                    .map_or(0, |shared| shared.graph.nodes.len())
            },
            SetupGraphBuildSession::partial_build_nodes,
        )
    }

    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        batch_capacity: usize,
        control: &ExecutionControl,
    ) -> Result<WasmSetupParallelProduce, WasmExactSearchError> {
        if control.is_cancelled() {
            return Ok(WasmSetupParallelProduce::Cancelled);
        }
        if let Some(builder) = &mut self.builder {
            return match builder.advance(work_budget, control)? {
                SetupGraphBuildAdvance::Pending => Ok(WasmSetupParallelProduce::Pending),
                SetupGraphBuildAdvance::Cancelled => Ok(WasmSetupParallelProduce::Cancelled),
                SetupGraphBuildAdvance::Complete(shared) => {
                    self.solution_paths = shared
                        .query
                        .path_detail()
                        .map(|detail| {
                            enumerate_setup_completion_paths(&shared.graph, detail, control)
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let initialization = encode_initialization(
                        &shared.query,
                        &shared.coverage_graph,
                        &shared.graph.shapes,
                    )?;
                    let mut condition_task_counts = vec![0_usize; shared.conditions.len()];
                    for task in &self.tasks {
                        let count = condition_task_counts
                            .get_mut(task.condition_index as usize)
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "setup_parallel_task_condition_out_of_range",
                            ))?;
                        *count += 1;
                    }
                    let shape_count = shared.graph.shapes.len();
                    let condition_merges = condition_task_counts
                        .into_iter()
                        .map(|task_count| SetupConditionMerge::new(shape_count, task_count))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.shared = Some(shared);
                    self.condition_merges = Some(condition_merges);
                    self.builder = None;
                    Ok(WasmSetupParallelProduce::Initialization(initialization))
                }
            };
        }
        if self.next_task == self.tasks.len() {
            return Ok(WasmSetupParallelProduce::Completed);
        }
        let count = batch_capacity
            .max(1)
            .min(self.tasks.len() - self.next_task)
            .min(1);
        let start = self.next_task;
        self.next_task += count;
        Ok(WasmSetupParallelProduce::Batch(encode_tasks(
            &self.tasks[start..start + count],
        )))
    }

    pub(crate) fn absorb(&mut self, input: &[u8]) -> Result<(), WasmExactSearchError> {
        for result in decode_results(input)? {
            let task_index = result.task_index as usize;
            let expected =
                self.tasks
                    .get(task_index)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_parallel_result_task_out_of_range",
                    ))?;
            if expected.condition_index != result.condition_index
                || expected.word_start != result.word_start
                || expected.word_end != result.word_end
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_result_task_mismatch",
                ));
            }
            let received = self.received_task_flags.get_mut(task_index).ok_or(
                WasmExactSearchError::InvalidProblem("setup_parallel_result_task_out_of_range"),
            )?;
            if *received {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_duplicate_task_result",
                ));
            }
            *received = true;
            self.pending_results[task_index] = Some(result);
            self.received_tasks += 1;
        }
        self.merge_ready_results()?;
        Ok(())
    }

    fn merge_ready_results(&mut self) -> Result<(), WasmExactSearchError> {
        let shape_count = self
            .shared
            .as_ref()
            .map_or(0, |shared| shared.graph.shapes.len());
        while self.next_merge_task < self.pending_results.len() {
            let Some(result) = self.pending_results[self.next_merge_task].take() else {
                break;
            };
            let merge = self
                .condition_merges
                .as_mut()
                .and_then(|merges| merges.get_mut(result.condition_index as usize))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_result_condition_out_of_range",
                ))?;
            merge.absorb(result, shape_count)?;
            self.next_merge_task += 1;
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        workers_used: usize,
    ) -> Result<CoreExecutionResult, WasmExactSearchError> {
        let shared = self
            .shared
            .take()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_parallel_graph_not_ready",
            ))?;
        if self.next_task != self.tasks.len()
            || self.received_tasks != self.tasks.len()
            || self.next_merge_task != self.tasks.len()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_task_results_incomplete",
            ));
        }
        let condition_merges =
            self.condition_merges
                .take()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_condition_merges_missing",
                ))?;
        let mut completed = Vec::new();
        completed
            .try_reserve_exact(condition_merges.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_parallel_completed_result_storage_unavailable",
                )
            })?;
        let mut peak_segment_pages = 0_u32;
        let mut solution_paths = Some(
            std::mem::take(&mut self.solution_paths)
                .into_iter()
                .map(SetupSolutionPath::into_core_path)
                .collect::<Vec<_>>(),
        );
        let path_target_shape_index = shared
            .query
            .path_detail()
            .map(|detail| {
                shared.graph.shape_index_for_detail(detail).ok_or(
                    WasmExactSearchError::InvalidProblem("setup_path_detail_shape_not_found"),
                )
            })
            .transpose()?;
        for (condition_index, merge) in condition_merges.into_iter().enumerate() {
            let condition = &shared.conditions[condition_index];
            peak_segment_pages = peak_segment_pages.max(merge.peak_segment_pages);
            let result = merge.finish(
                &shared.graph.shapes,
                shared.query.limits().max_results(),
                shared.query.candidate_priority(),
                shared.query.length_preference(),
                path_target_shape_index,
            )?;
            let resolver = SetupRepresentativeResolver::new(
                condition,
                &shared.graph,
                shared.query.candidate_priority(),
                shared.query.length_preference(),
                shared.query.max_setup_pieces(),
            )?;
            let targets = result
                .selected_shapes
                .iter()
                .map(|shape| {
                    (
                        shape.shape_index as usize,
                        SetupWitness {
                            pattern_id: shape.witness_pattern_id,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let paths = resolver.paths(&targets)?;
            let candidates = result
                .selected_shapes
                .into_iter()
                .zip(paths)
                .map(|(coverage, representative_path)| {
                    let shape_index = coverage.shape_index as usize;
                    let shape = &shared.graph.shapes[shape_index];
                    let conditional = if coverage.build_weight == 0.0 {
                        0.0
                    } else {
                        coverage.joint_weight / coverage.build_weight
                    };
                    let setup_id = shared.graph.setup_id_for_shape(shape_index).ok_or(
                        WasmExactSearchError::InvalidProblem("setup_candidate_identity_missing"),
                    )?;
                    let candidate = SetupCandidateReport::new(
                        setup_id,
                        shape.board,
                        coverage.min_covered_locks,
                        coverage.max_covered_locks,
                        coverage.build_covered_patterns as usize,
                        coverage.joint_covered_patterns as usize,
                        probability_string(coverage.build_weight),
                        probability_string(coverage.joint_weight),
                        probability_string(conditional),
                        representative_path,
                    );
                    if path_target_shape_index == Some(shape_index) {
                        Ok(
                            candidate
                                .with_solution_paths(solution_paths.take().unwrap_or_default()),
                        )
                    } else {
                        Ok(candidate)
                    }
                })
                .collect::<Result<Vec<_>, WasmExactSearchError>>()?;
            completed.push(CompletedSetupCoverage {
                report: SetupHoldConditionReport::new(
                    condition.condition_id().to_owned(),
                    condition.initial_hold(),
                    condition.pattern_expression().to_owned(),
                    result.global_pattern_count as usize,
                    result.candidate_count as usize,
                    result.candidate_count as usize > shared.query.limits().max_results(),
                    true,
                    candidates,
                ),
                candidate_boards: result.candidate_boards,
            });
        }
        Ok(finish_setup_result(
            &shared.query,
            &shared.graph,
            completed,
            shared.geometry_family_count,
            shared.geometry_expanded_nodes,
            shared.tablebase_status,
            shared.tablebase_pruned_states,
            workers_used.min(self.worker_count).max(1),
            true,
            "setup-family-quotient-segmented-task-multiworker",
        )
        .with_additional_fields(vec![
            (
                "setup_parallel_task_count".to_owned(),
                self.tasks.len().to_string(),
            ),
            (
                "setup_parallel_peak_segment_pages".to_owned(),
                peak_segment_pages.to_string(),
            ),
        ]))
    }

    pub(crate) fn producer_completed(&self) -> bool {
        self.builder.is_none() && self.next_task == self.tasks.len()
    }

    pub(crate) fn dispatched_conditions(&self) -> usize {
        self.next_task
    }

    pub(crate) fn received_conditions(&self) -> usize {
        self.received_tasks
    }
}

struct SetupConditionMerge {
    expected_tasks: usize,
    received_tasks: usize,
    global_pattern_count: Option<u32>,
    accumulators: SegmentedArray<ShapeAccumulator>,
    covered_shapes: Vec<usize>,
    peak_segment_pages: u32,
}

impl SetupConditionMerge {
    fn new(shape_count: usize, expected_tasks: usize) -> Result<Self, WasmExactSearchError> {
        Ok(Self {
            expected_tasks,
            received_tasks: 0,
            global_pattern_count: None,
            accumulators: SegmentedArray::new(shape_count)?,
            covered_shapes: Vec::new(),
            peak_segment_pages: 0,
        })
    }

    fn absorb(
        &mut self,
        result: SetupParallelTaskResult,
        shape_count: usize,
    ) -> Result<(), WasmExactSearchError> {
        match self.global_pattern_count {
            Some(count) if count != result.global_pattern_count => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_global_pattern_count_mismatch",
                ));
            }
            None => self.global_pattern_count = Some(result.global_pattern_count),
            _ => {}
        }
        for shape in result.covered_shapes {
            let shape_index = shape.shape_index as usize;
            if shape_index >= shape_count {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_result_shape_out_of_range",
                ));
            }
            let accumulator = self.accumulators.get_mut_or_default(shape_index)?;
            if accumulator.build_covered_patterns == 0 {
                self.covered_shapes.push(shape_index);
            }
            accumulator.build_covered_patterns = accumulator
                .build_covered_patterns
                .checked_add(shape.build_covered_patterns)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_build_coverage_count_overflow",
                ))?;
            accumulator.joint_covered_patterns = accumulator
                .joint_covered_patterns
                .checked_add(shape.joint_covered_patterns)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_joint_coverage_count_overflow",
                ))?;
            if accumulator.build_covered_patterns > result.global_pattern_count
                || accumulator.joint_covered_patterns > accumulator.build_covered_patterns
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_merged_coverage_invalid",
                ));
            }
            accumulator.build_weight += shape.build_weight;
            accumulator.joint_weight += shape.joint_weight;
            if shape.joint_covered_patterns != 0 {
                include_setup_depth_range(
                    &mut accumulator.min_covered_locks,
                    &mut accumulator.max_covered_locks,
                    shape.min_covered_locks,
                );
                include_setup_depth_range(
                    &mut accumulator.min_covered_locks,
                    &mut accumulator.max_covered_locks,
                    shape.max_covered_locks,
                );
            }
            let weight_tolerance =
                f64::EPSILON * f64::from(result.global_pattern_count.max(1)) * 2.0;
            if !accumulator.build_weight.is_finite()
                || !accumulator.joint_weight.is_finite()
                || accumulator.joint_weight > accumulator.build_weight + weight_tolerance
                || accumulator.build_weight > 1.0 + weight_tolerance
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_merged_weight_invalid",
                ));
            }
            if shape.witness_pattern_id != NO_WITNESS {
                accumulator.witness_pattern_id =
                    accumulator.witness_pattern_id.min(shape.witness_pattern_id);
            }
        }
        self.received_tasks += 1;
        self.peak_segment_pages = self.peak_segment_pages.max(result.peak_segment_pages);
        Ok(())
    }

    fn finish(
        mut self,
        shapes: &[super::setup_partial_build::SetupShape],
        max_results: usize,
        candidate_priority: clearra_problem::SetupCandidatePriority,
        length_preference: clearra_problem::SetupLengthPreference,
        path_target_shape_index: Option<usize>,
    ) -> Result<CompletedParallelCondition, WasmExactSearchError> {
        if self.received_tasks != self.expected_tasks {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_condition_tasks_incomplete",
            ));
        }
        let global_pattern_count =
            self.global_pattern_count
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_condition_pattern_count_missing",
                ))?;
        self.covered_shapes.retain(|shape_index| {
            self.accumulators
                .get(*shape_index)
                .is_some_and(|coverage| coverage.joint_covered_patterns != 0)
        });
        if let Some(target_shape_index) = path_target_shape_index {
            self.covered_shapes
                .retain(|shape_index| *shape_index == target_shape_index);
        }
        if self.covered_shapes.iter().any(|shape_index| {
            self.accumulators.get(*shape_index).is_none_or(|coverage| {
                coverage.min_covered_locks == u8::MAX
                    || coverage.min_covered_locks > coverage.max_covered_locks
            })
        }) {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_covered_depth_range_missing",
            ));
        }
        self.covered_shapes.sort_unstable_by(|left, right| {
            let left_shape = &shapes[*left];
            let right_shape = &shapes[*right];
            let left_coverage = self.accumulators.get(*left).copied().unwrap_or_default();
            let right_coverage = self.accumulators.get(*right).copied().unwrap_or_default();
            compare_setup_candidates(
                candidate_priority,
                length_preference,
                left_coverage.build_weight,
                left_coverage.joint_weight,
                left_coverage.min_covered_locks,
                left_coverage.max_covered_locks,
                left_shape.board,
                right_coverage.build_weight,
                right_coverage.joint_weight,
                right_coverage.min_covered_locks,
                right_coverage.max_covered_locks,
                right_shape.board,
            )
        });
        retain_best_setup_state_per_board(&mut self.covered_shapes, shapes)?;
        let candidate_count = self.covered_shapes.len();
        let mut candidate_boards = self
            .covered_shapes
            .iter()
            .map(|shape_index| shapes[*shape_index].board)
            .collect::<Vec<_>>();
        candidate_boards.sort_unstable();
        self.covered_shapes.truncate(max_results);
        let selected_shapes = self
            .covered_shapes
            .into_iter()
            .map(|shape_index| {
                let coverage = self.accumulators.get(shape_index).copied().ok_or(
                    WasmExactSearchError::InvalidProblem(
                        "setup_parallel_candidate_accumulator_missing",
                    ),
                )?;
                Ok(SetupParallelShapeResult {
                    shape_index: u32::try_from(shape_index).map_err(|_| {
                        WasmExactSearchError::InvalidProblem("setup_parallel_shape_index_overflow")
                    })?,
                    build_covered_patterns: coverage.build_covered_patterns,
                    joint_covered_patterns: coverage.joint_covered_patterns,
                    build_weight: coverage.build_weight,
                    joint_weight: coverage.joint_weight,
                    min_covered_locks: coverage.min_covered_locks,
                    max_covered_locks: coverage.max_covered_locks,
                    witness_pattern_id: coverage.witness_pattern_id,
                })
            })
            .collect::<Result<Vec<_>, WasmExactSearchError>>()?;
        Ok(CompletedParallelCondition {
            global_pattern_count,
            candidate_count,
            candidate_boards,
            selected_shapes,
        })
    }
}

struct CompletedParallelCondition {
    global_pattern_count: u32,
    candidate_count: usize,
    candidate_boards: Vec<u64>,
    selected_shapes: Vec<SetupParallelShapeResult>,
}

const TARGET_TASKS_PER_VERIFIER: usize = 4;
const MIN_WORDS_PER_TASK: usize = 128;

fn plan_parallel_tasks(
    condition_word_counts: &[usize],
    worker_count: usize,
) -> Result<Vec<SetupParallelTask>, WasmExactSearchError> {
    let total_words = condition_word_counts
        .iter()
        .try_fold(0_usize, |sum, count| {
            sum.checked_add(*count)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_pattern_word_count_overflow",
                ))
        })?;
    let verifier_count = worker_count.saturating_sub(1).max(1);
    let target_tasks = verifier_count.saturating_mul(TARGET_TASKS_PER_VERIFIER);
    let mut words_per_task = total_words
        .div_ceil(target_tasks.max(1))
        .max(MIN_WORDS_PER_TASK);
    words_per_task = words_per_task.div_ceil(COVERAGE_WORD_LANES) * COVERAGE_WORD_LANES;
    let mut tasks = Vec::new();
    for (condition_index, word_count) in condition_word_counts.iter().copied().enumerate() {
        let mut word_start = 0;
        while word_start < word_count {
            let word_end = (word_start + words_per_task).min(word_count);
            tasks.push(SetupParallelTask {
                task_index: u32::try_from(tasks.len()).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_parallel_task_index_overflow")
                })?,
                condition_index: u32::try_from(condition_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_parallel_condition_index_overflow")
                })?,
                word_start: u32::try_from(word_start).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_parallel_word_index_overflow")
                })?,
                word_end: u32::try_from(word_end).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_parallel_word_index_overflow")
                })?,
            });
            word_start = word_end;
        }
    }
    Ok(tasks)
}

pub(crate) struct WasmSetupParallelWorker {
    query: SetupSearchQuery,
    runtimes: Vec<Option<SetupParallelConditionRuntime>>,
    workspace: SegmentedCoverageWorkspace,
}

impl WasmSetupParallelWorker {
    pub(crate) fn accepts_initialization(input: &[u8]) -> bool {
        is_setup_parallel_initialization(input)
    }

    pub(crate) fn new(input: &[u8]) -> Result<Self, WasmExactSearchError> {
        let initialization = decode_initialization(input)?;
        let condition_count =
            setup_search_condition_count(&initialization.query).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_parallel_worker_condition_count_failed")
            })?;
        let runtimes = (0..condition_count).map(|_| None).collect();
        let workspace = SegmentedCoverageWorkspace::new(
            Arc::clone(&initialization.graph),
            initialization.shape_count,
            SetupSupplyStateLayout::new(),
        )?;
        Ok(Self {
            query: initialization.query,
            runtimes,
            workspace,
        })
    }

    pub(crate) fn consume(
        &mut self,
        input: &[u8],
        control: &ExecutionControl,
    ) -> Result<(usize, Vec<u8>), WasmExactSearchError> {
        let tasks = decode_tasks(input)?;
        let mut results = Vec::new();
        results.try_reserve_exact(tasks.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_worker_result_storage_unavailable")
        })?;
        for task in &tasks {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let condition_index = task.condition_index as usize;
            let runtime_slot = self.runtimes.get_mut(condition_index).ok_or(
                WasmExactSearchError::InvalidProblem(
                    "setup_parallel_worker_condition_out_of_range",
                ),
            )?;
            if runtime_slot.is_none() {
                let condition = compile_setup_search_condition(&self.query, condition_index)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "setup_parallel_worker_condition_compile_failed",
                        )
                    })?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_parallel_worker_condition_out_of_range",
                    ))?;
                *runtime_slot = Some(SetupParallelConditionRuntime::compile(
                    condition_index,
                    &condition,
                    self.query.max_setup_pieces(),
                )?);
            }
            let runtime = runtime_slot
                .as_ref()
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_parallel_worker_runtime_missing",
                ))?;
            results.push(self.workspace.run_task(*task, runtime, control)?);
        }
        Ok((tasks.len(), encode_results(&results)?))
    }
}

struct SetupParallelConditionRuntime {
    condition_index: u32,
    initial_hold_code: u8,
    pattern_index: PatternPiecePositionIndex,
    weights: WeightedPatternSet,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    initial_cursor: u16,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    max_setup_pieces: u8,
    state_layout: SetupSupplyStateLayout,
}

impl SetupParallelConditionRuntime {
    fn compile(
        condition_index: usize,
        condition: &SetupSearchCondition,
        max_setup_pieces: u8,
    ) -> Result<Self, WasmExactSearchError> {
        let problem = condition.problem();
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("setup_pattern_universe_not_materialized"),
        )?;
        Ok(Self {
            condition_index: u32::try_from(condition_index).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_parallel_condition_index_overflow")
            })?,
            initial_hold_code: condition
                .initial_hold()
                .map_or(0, |piece| piece_index(piece) as u8 + 1),
            pattern_index: compile_setup_pattern_index(condition)?,
            weights: universe.weights().clone(),
            hold_enabled: problem.supply().hold_enabled(),
            projects_unplaced_lookahead: problem.supply().projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: problem.supply().projects_standard_bag_lookahead(),
            initial_cursor: problem.initial_hold().cursor(),
            terminal_supply_target: condition.terminal_supply_target(),
            max_setup_pieces,
            state_layout: SetupSupplyStateLayout::new(),
        })
    }
}

#[derive(Clone, Copy, Default)]
struct StateCoverage {
    alpha: [u64; COVERAGE_WORD_LANES],
    beta: [u64; COVERAGE_WORD_LANES],
}

#[derive(Clone, Copy)]
struct ShapeWords {
    build: [u64; COVERAGE_WORD_LANES],
    joint: [u64; COVERAGE_WORD_LANES],
    min_covered_locks: u8,
    max_covered_locks: u8,
}

impl Default for ShapeWords {
    fn default() -> Self {
        Self {
            build: EMPTY_WORDS,
            joint: EMPTY_WORDS,
            min_covered_locks: u8::MAX,
            max_covered_locks: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct ShapeAccumulator {
    build_covered_patterns: u32,
    joint_covered_patterns: u32,
    build_weight: f64,
    joint_weight: f64,
    min_covered_locks: u8,
    max_covered_locks: u8,
    witness_pattern_id: u32,
}

impl Default for ShapeAccumulator {
    fn default() -> Self {
        Self {
            build_covered_patterns: 0,
            joint_covered_patterns: 0,
            build_weight: 0.0,
            joint_weight: 0.0,
            min_covered_locks: u8::MAX,
            max_covered_locks: 0,
            witness_pattern_id: NO_WITNESS,
        }
    }
}

struct SegmentedCoverageWorkspace {
    graph: Arc<SetupCoverageGraph>,
    state_layout: SetupSupplyStateLayout,
    state_values: SegmentedGenerationArray<StateCoverage>,
    shape_words: SegmentedGenerationArray<ShapeWords>,
    accumulators: SegmentedGenerationArray<ShapeAccumulator>,
    depth_states: Vec<Vec<usize>>,
    touched_states: Vec<usize>,
    touched_shapes: Vec<usize>,
    covered_shapes: Vec<usize>,
    peak_segment_pages: usize,
}

impl SegmentedCoverageWorkspace {
    fn new(
        graph: Arc<SetupCoverageGraph>,
        shape_count: usize,
        state_layout: SetupSupplyStateLayout,
    ) -> Result<Self, WasmExactSearchError> {
        let state_capacity = state_layout.state_capacity(graph.nodes.len()).ok_or(
            WasmExactSearchError::InvalidProblem("setup_parallel_state_capacity_overflow"),
        )?;
        Ok(Self {
            graph,
            state_layout,
            state_values: SegmentedGenerationArray::new(state_capacity)?,
            shape_words: SegmentedGenerationArray::new(shape_count)?,
            accumulators: SegmentedGenerationArray::new(shape_count)?,
            depth_states: (0..=10).map(|_| Vec::new()).collect(),
            touched_states: Vec::new(),
            touched_shapes: Vec::new(),
            covered_shapes: Vec::new(),
            peak_segment_pages: 0,
        })
    }

    fn run_task(
        &mut self,
        task: SetupParallelTask,
        runtime: &SetupParallelConditionRuntime,
        control: &ExecutionControl,
    ) -> Result<SetupParallelTaskResult, WasmExactSearchError> {
        if task.condition_index != runtime.condition_index
            || task.word_start >= task.word_end
            || task.word_end as usize > runtime.pattern_index.word_count()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_task_word_range_invalid",
            ));
        }
        self.accumulators.begin_generation();
        self.covered_shapes.clear();
        self.peak_segment_pages = 0;
        let mut word_start = task.word_start as usize;
        let word_end = task.word_end as usize;
        while word_start < word_end {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            let lane_count = (word_end - word_start).min(COVERAGE_WORD_LANES);
            self.process_word_block(runtime, word_start, lane_count, control)?;
            word_start += lane_count;
        }
        self.finish_task(task, runtime)
    }

    fn process_word_block(
        &mut self,
        runtime: &SetupParallelConditionRuntime,
        word_start: usize,
        lane_count: usize,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.state_values.begin_generation();
        self.shape_words.begin_generation();
        self.touched_states.clear();
        self.touched_shapes.clear();
        for queue in &mut self.depth_states {
            queue.clear();
        }
        let mut root_bits = EMPTY_WORDS;
        for (lane, bits) in root_bits.iter_mut().enumerate().take(lane_count) {
            *bits = runtime.pattern_index.active_word(word_start + lane);
        }
        let transition_catalog = SetupSupplyTransitionCatalog::compile(
            &runtime.pattern_index,
            runtime.initial_cursor,
            runtime.hold_enabled,
            runtime.projects_unplaced_lookahead,
            runtime.projects_standard_bag_lookahead,
            word_start,
            root_bits,
            lane_count,
        )?;
        self.activate_alpha(
            runtime
                .state_layout
                .encode(self.graph.root as usize, 0, runtime.initial_hold_code),
            root_bits,
        )?;
        let mut cancellation_work = 0_usize;

        for depth in 0..self.depth_states.len() {
            let mut cursor = 0;
            while cursor < self.depth_states[depth].len() {
                check_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                cursor += 1;
                let active = self
                    .state_values
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                let (node_index, extra_draw, hold_code) = runtime.state_layout.decode(state_index);
                let node = self.graph.nodes[node_index];
                if node.accepting() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                for edge_index in edge_start..edge_end {
                    let edge = self.graph.edges[edge_index];
                    let target_node = edge.child() as usize;
                    let terminal = self.graph.nodes[target_node].accepting();
                    for lane in 0..lane_count {
                        for transition in transition_catalog
                            .get(
                                node.depth,
                                edge.piece_code(),
                                extra_draw,
                                hold_code,
                                terminal,
                                lane,
                            )
                            .iter()
                        {
                            self.activate_alpha_lane(
                                runtime.state_layout.encode(
                                    target_node,
                                    transition.extra_draw,
                                    transition.hold_code,
                                ),
                                lane,
                                active[lane] & transition.mask,
                            )?;
                        }
                    }
                }
            }
        }

        for cursor in 0..self.touched_states.len() {
            let state_index = self.touched_states[cursor];
            let (node_index, extra_draw, hold_code) = runtime.state_layout.decode(state_index);
            if self.graph.nodes[node_index].accepting() {
                let mut alpha = self
                    .state_values
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                if let Some(target) = runtime.terminal_supply_target {
                    for lane in 0..lane_count {
                        alpha[lane] &= terminal_supply_target_word(
                            &runtime.pattern_index,
                            target,
                            runtime.initial_cursor,
                            self.graph.nodes[node_index].depth,
                            extra_draw,
                            hold_code,
                            word_start + lane,
                            alpha[lane],
                        );
                    }
                }
                self.activate_beta(state_index, alpha)?;
            }
        }
        for depth in (0..self.depth_states.len()).rev() {
            for cursor in 0..self.depth_states[depth].len() {
                check_cancel(control, &mut cancellation_work)?;
                let state_index = self.depth_states[depth][cursor];
                let active = self
                    .state_values
                    .get(state_index)
                    .map_or(EMPTY_WORDS, |state| state.alpha);
                let (node_index, extra_draw, hold_code) = runtime.state_layout.decode(state_index);
                let node = self.graph.nodes[node_index];
                if node.accepting() {
                    continue;
                }
                let edge_start = node.edge_start as usize;
                let edge_end = edge_start + node.edge_count as usize;
                let mut successful = EMPTY_WORDS;
                for edge_index in edge_start..edge_end {
                    let edge = self.graph.edges[edge_index];
                    let target_node = edge.child() as usize;
                    let terminal = self.graph.nodes[target_node].accepting();
                    for lane in 0..lane_count {
                        for transition in transition_catalog
                            .get(
                                node.depth,
                                edge.piece_code(),
                                extra_draw,
                                hold_code,
                                terminal,
                                lane,
                            )
                            .iter()
                        {
                            let target = runtime.state_layout.encode(
                                target_node,
                                transition.extra_draw,
                                transition.hold_code,
                            );
                            let backward = self
                                .state_values
                                .get(target)
                                .map_or(0, |state| state.beta[lane]);
                            successful[lane] |= active[lane] & transition.mask & backward;
                        }
                    }
                }
                self.activate_beta(state_index, successful)?;
            }
        }

        for cursor in 0..self.touched_states.len() {
            let state_index = self.touched_states[cursor];
            let (node_index, _, _) = runtime.state_layout.decode(state_index);
            let node = self.graph.nodes[node_index];
            if node.depth > runtime.max_setup_pieces {
                continue;
            }
            let Some(shape_index) = node.shape_index().map(|index| index as usize) else {
                continue;
            };
            let state = self
                .state_values
                .get(state_index)
                .copied()
                .unwrap_or_default();
            let (words, first) = self.shape_words.get_mut_or_default(shape_index)?;
            if first {
                self.touched_shapes.push(shape_index);
            }
            for lane in 0..lane_count {
                let build = state.alpha[lane] & root_bits[lane];
                let joint = build & state.beta[lane];
                words.build[lane] |= build;
                words.joint[lane] |= joint;
                if joint != 0 {
                    include_setup_depth_range(
                        &mut words.min_covered_locks,
                        &mut words.max_covered_locks,
                        node.depth,
                    );
                }
            }
        }
        for cursor in 0..self.touched_shapes.len() {
            let shape_index = self.touched_shapes[cursor];
            let words = self
                .shape_words
                .get(shape_index)
                .copied()
                .unwrap_or_default();
            let (accumulator, first_accumulator) =
                self.accumulators.get_mut_or_default(shape_index)?;
            let previously_empty = first_accumulator || accumulator.build_covered_patterns == 0;
            if words.min_covered_locks != u8::MAX {
                include_setup_depth_range(
                    &mut accumulator.min_covered_locks,
                    &mut accumulator.max_covered_locks,
                    words.min_covered_locks,
                );
                include_setup_depth_range(
                    &mut accumulator.min_covered_locks,
                    &mut accumulator.max_covered_locks,
                    words.max_covered_locks,
                );
            }
            for lane in 0..lane_count {
                accumulator.build_covered_patterns = accumulator
                    .build_covered_patterns
                    .checked_add(words.build[lane].count_ones())
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_parallel_build_coverage_count_overflow",
                    ))?;
                accumulator.joint_covered_patterns = accumulator
                    .joint_covered_patterns
                    .checked_add(words.joint[lane].count_ones())
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_parallel_joint_coverage_count_overflow",
                    ))?;
                accumulator.build_weight += covered_word_weight(
                    &runtime.pattern_index,
                    &runtime.weights,
                    word_start + lane,
                    words.build[lane],
                );
                accumulator.joint_weight += covered_word_weight(
                    &runtime.pattern_index,
                    &runtime.weights,
                    word_start + lane,
                    words.joint[lane],
                );
                if words.joint[lane] != 0 && accumulator.witness_pattern_id == NO_WITNESS {
                    let local = words.joint[lane].trailing_zeros() as usize;
                    accumulator.witness_pattern_id =
                        u32::try_from((word_start + lane) * 64 + local).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "setup_parallel_witness_pattern_overflow",
                            )
                        })?;
                }
            }
            if previously_empty && accumulator.build_covered_patterns != 0 {
                self.covered_shapes.push(shape_index);
            }
        }
        self.peak_segment_pages = self.peak_segment_pages.max(
            self.state_values.active_page_count()
                + self.shape_words.active_page_count()
                + self.accumulators.active_page_count(),
        );
        Ok(())
    }

    fn activate_alpha(
        &mut self,
        state_index: usize,
        mask: [u64; COVERAGE_WORD_LANES],
    ) -> Result<(), WasmExactSearchError> {
        if mask == EMPTY_WORDS {
            return Ok(());
        }
        let (state, first) = self.state_values.get_mut_or_default(state_index)?;
        let was_empty = first || state.alpha == EMPTY_WORDS;
        for lane in 0..COVERAGE_WORD_LANES {
            state.alpha[lane] |= mask[lane];
        }
        if was_empty {
            let (node, _, _) = self.state_layout.decode(state_index);
            self.depth_states[self.graph.nodes[node].depth as usize].push(state_index);
            self.touched_states.push(state_index);
        }
        Ok(())
    }

    fn activate_alpha_lane(
        &mut self,
        state_index: usize,
        lane: usize,
        mask: u64,
    ) -> Result<(), WasmExactSearchError> {
        if mask == 0 {
            return Ok(());
        }
        let (state, first) = self.state_values.get_mut_or_default(state_index)?;
        let was_empty = first || state.alpha == EMPTY_WORDS;
        state.alpha[lane] |= mask;
        if was_empty {
            let (node, _, _) = self.state_layout.decode(state_index);
            self.depth_states[self.graph.nodes[node].depth as usize].push(state_index);
            self.touched_states.push(state_index);
        }
        Ok(())
    }

    fn activate_beta(
        &mut self,
        state_index: usize,
        mask: [u64; COVERAGE_WORD_LANES],
    ) -> Result<(), WasmExactSearchError> {
        if mask == EMPTY_WORDS {
            return Ok(());
        }
        let (state, _) = self.state_values.get_mut_or_default(state_index)?;
        for lane in 0..COVERAGE_WORD_LANES {
            state.beta[lane] |= mask[lane];
        }
        Ok(())
    }

    fn finish_task(
        &mut self,
        task: SetupParallelTask,
        runtime: &SetupParallelConditionRuntime,
    ) -> Result<SetupParallelTaskResult, WasmExactSearchError> {
        self.covered_shapes.sort_unstable();
        self.covered_shapes.dedup();
        let covered_shapes = self
            .covered_shapes
            .iter()
            .copied()
            .map(|shape_index| {
                let coverage = self.accumulators.get(shape_index).copied().ok_or(
                    WasmExactSearchError::InvalidProblem("setup_parallel_task_accumulator_missing"),
                )?;
                Ok(SetupParallelShapeResult {
                    shape_index: u32::try_from(shape_index).map_err(|_| {
                        WasmExactSearchError::InvalidProblem("setup_parallel_shape_index_overflow")
                    })?,
                    build_covered_patterns: coverage.build_covered_patterns,
                    joint_covered_patterns: coverage.joint_covered_patterns,
                    build_weight: coverage.build_weight,
                    joint_weight: coverage.joint_weight,
                    min_covered_locks: coverage.min_covered_locks,
                    max_covered_locks: coverage.max_covered_locks,
                    witness_pattern_id: coverage.witness_pattern_id,
                })
            })
            .collect::<Result<Vec<_>, WasmExactSearchError>>()?;
        Ok(SetupParallelTaskResult {
            task_index: task.task_index,
            condition_index: task.condition_index,
            word_start: task.word_start,
            word_end: task.word_end,
            global_pattern_count: u32::try_from(runtime.pattern_index.global_pattern_count())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem("setup_parallel_pattern_count_overflow")
                })?,
            covered_shapes,
            peak_segment_pages: u32::try_from(self.peak_segment_pages).unwrap_or(u32::MAX),
        })
    }
}

#[inline]
fn check_cancel(control: &ExecutionControl, work: &mut usize) -> Result<(), WasmExactSearchError> {
    *work = work.wrapping_add(1);
    if *work & 4095 == 0 && control.is_cancelled() {
        Err(WasmExactSearchError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "setup_parallel_tests.rs"]
mod tests;
