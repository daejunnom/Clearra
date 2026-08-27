use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{CoreExecutionResult, CorePostProcessExecution};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcCountPolicy, PcScenarioQuery};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem, SearchProblemPreset};

pub const PC_PATH_FAMILY_RESULT_CONTRACT: &str = "pc-path-family.v2";
pub const PC_PATH_WITNESS_CONTRACT: &str = "pc-path-witness.v2";
pub const PC_PATH_ORDERING: &str =
    "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcPathIngressOrigin {
    CanonicalPcPath,
}

impl PcPathIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcPath => "canonical-pc-path",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcPathQuerySnapshot {
    Opening(Arc<OpeningPcSearchQuery>),
    Scenario(Arc<PcScenarioQuery>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcPathProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcPathProblemPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPc => "opening-pc",
            Self::ScenarioPc => "scenario-pc",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcPathCompletenessEvidence {
    query_bound: bool,
    search_complete: bool,
    execution_batch_complete: bool,
    count_complete: bool,
    objective_complete: bool,
    replay_chain_validated: bool,
}

impl PcPathCompletenessEvidence {
    pub const fn complete(self) -> bool {
        self.query_bound
            && self.search_complete
            && self.execution_batch_complete
            && self.count_complete
            && self.objective_complete
            && self.replay_chain_validated
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcPathStepV2 {
    step_index: usize,
    operation_id: u16,
    active_piece: PieceKind,
    input_cursor: usize,
    output_cursor: usize,
    input_hold_piece: Option<PieceKind>,
    output_hold_piece: Option<PieceKind>,
    hold_decision: &'static str,
    rotation: u8,
    x: u16,
    y: u16,
    placement_mask: u64,
    board_before_mask: u64,
    board_after_placement_mask: u64,
    board_after_line_clear_mask: u64,
    cleared_row_mask: u64,
    cleared_lines: u8,
    line_clear_identity: String,
}

impl PcPathStepV2 {
    pub const fn step_index(&self) -> usize {
        self.step_index
    }

    pub const fn operation_id(&self) -> u16 {
        self.operation_id
    }

    pub const fn active_piece(&self) -> PieceKind {
        self.active_piece
    }

    pub const fn input_cursor(&self) -> usize {
        self.input_cursor
    }

    pub const fn output_cursor(&self) -> usize {
        self.output_cursor
    }

    pub const fn input_hold_piece(&self) -> Option<PieceKind> {
        self.input_hold_piece
    }

    pub const fn output_hold_piece(&self) -> Option<PieceKind> {
        self.output_hold_piece
    }

    pub const fn hold_decision(&self) -> &'static str {
        self.hold_decision
    }

    pub const fn rotation(&self) -> u8 {
        self.rotation
    }

    pub const fn x(&self) -> u16 {
        self.x
    }

    pub const fn y(&self) -> u16 {
        self.y
    }

    pub const fn placement_mask(&self) -> u64 {
        self.placement_mask
    }

    pub const fn board_before_mask(&self) -> u64 {
        self.board_before_mask
    }

    pub const fn board_after_placement_mask(&self) -> u64 {
        self.board_after_placement_mask
    }

    pub const fn board_after_line_clear_mask(&self) -> u64 {
        self.board_after_line_clear_mask
    }

    pub const fn cleared_row_mask(&self) -> u64 {
        self.cleared_row_mask
    }

    pub const fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }

    pub fn line_clear_identity(&self) -> &str {
        &self.line_clear_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcPathWitnessV2 {
    candidate_id: u64,
    producer_candidate_id: u64,
    pattern_id: usize,
    trace_identity: String,
    normalized_trace_key: String,
    consumed_piece_count: usize,
    terminal_hold_piece: Option<PieceKind>,
    steps: Vec<PcPathStepV2>,
}

impl PcPathWitnessV2 {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn producer_candidate_id(&self) -> u64 {
        self.producer_candidate_id
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn normalized_trace_key(&self) -> &str {
        &self.normalized_trace_key
    }

    pub const fn consumed_piece_count(&self) -> usize {
        self.consumed_piece_count
    }

    pub const fn terminal_hold_piece(&self) -> Option<PieceKind> {
        self.terminal_hold_piece
    }

    pub fn steps(&self) -> &[PcPathStepV2] {
        &self.steps
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcPathFamilyV2Result {
    contract_id: &'static str,
    witness_contract: &'static str,
    ordering: &'static str,
    origin: PcPathIngressOrigin,
    query: PcPathQuerySnapshot,
    problem_preset: PcPathProblemPreset,
    problem_id: String,
    materialized_pattern_count: usize,
    witnesses: Vec<PcPathWitnessV2>,
    completeness: PcPathCompletenessEvidence,
}

impl PcPathFamilyV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn witness_contract(&self) -> &'static str {
        self.witness_contract
    }

    pub const fn ordering(&self) -> &'static str {
        self.ordering
    }

    pub const fn origin(&self) -> PcPathIngressOrigin {
        self.origin
    }

    pub fn query(&self) -> &PcPathQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcPathProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub fn witnesses(&self) -> &[PcPathWitnessV2] {
        &self.witnesses
    }

    pub const fn completeness(&self) -> PcPathCompletenessEvidence {
        self.completeness
    }
}

pub(crate) enum PcPathQueryBinding<'a> {
    Opening(&'a Arc<OpeningPcSearchQuery>),
    Scenario(&'a Arc<PcScenarioQuery>),
}

impl PcPathQueryBinding<'_> {
    fn snapshot(&self) -> PcPathQuerySnapshot {
        match self {
            Self::Opening(query) => PcPathQuerySnapshot::Opening(Arc::clone(query)),
            Self::Scenario(query) => PcPathQuerySnapshot::Scenario(Arc::clone(query)),
        }
    }

    fn preset(&self) -> PcPathProblemPreset {
        match self {
            Self::Opening(_) => PcPathProblemPreset::OpeningPc,
            Self::Scenario(_) => PcPathProblemPreset::ScenarioPc,
        }
    }

    fn compile_expected(&self) -> Result<SearchProblem, &'static str> {
        match self {
            Self::Opening(query) => ProblemCompiler::compile_opening_pc(query.as_ref()),
            Self::Scenario(query) => ProblemCompiler::compile_scenario_pc(query.as_ref()),
        }
        .map_err(|_| "pc path expected problem did not compile")
    }
}

pub(crate) fn validate_pc_path_family_v2_result(
    query: PcPathQueryBinding<'_>,
    origin: PcPathIngressOrigin,
    result: &CoreExecutionResult,
) -> Result<PcPathFamilyV2Result, &'static str> {
    let problem = query.compile_expected()?;
    let preset = query.preset();
    let expected_preset = match preset {
        PcPathProblemPreset::OpeningPc => SearchProblemPreset::OpeningPc,
        PcPathProblemPreset::ScenarioPc => SearchProblemPreset::ScenarioPc,
    };
    if problem.preset() != expected_preset
        || problem.goal().as_str() != "clear-to-empty"
        || problem.objective().kind() != ObjectivePolicy::all().kind()
        || problem.count_policy() != PcCountPolicy::CountAll
        || problem.output_policy() != SearchOutputPolicy::Trace
    {
        return Err("pc path compiled problem contract mismatch");
    }
    if result.bool_field("resource_truncated") == Some(true) {
        return Err("pc path execution was resource truncated");
    }
    if result.bool_field("count_complete") != Some(true) {
        return Err("pc path count evidence is incomplete");
    }
    if result.bool_field("objective_complete") != Some(true)
        || result.bool_field("objective_search_complete") != Some(true)
    {
        return Err("pc path objective evidence is incomplete");
    }
    if result.bool_field("build_variant_count_exact") != Some(true) {
        return Err("pc path build-variant count is inexact");
    }
    if !result.postprocess_execution_complete() {
        return Err("pc path replay batch is incomplete");
    }
    if result
        .field("problem_preset")
        .is_some_and(|value| value != preset.as_str())
        || result
            .field("compiled_goal")
            .is_some_and(|value| value != "clear-to-empty")
        || result
            .field("objective")
            .is_some_and(|value| value != "all")
    {
        return Err("pc path result is not bound to the typed request");
    }

    let materialized_pattern_count = problem
        .piece_source()
        .materialized_universe()
        .map(|universe| universe.pattern_count())
        .or_else(|| result.usize_field("materialized_pattern_count"))
        .ok_or("pc path materialized pattern count is unavailable")?;
    if materialized_pattern_count == 0
        || result
            .usize_field("materialized_pattern_count")
            .is_some_and(|count| count != materialized_pattern_count)
    {
        return Err("pc path materialized pattern count mismatch");
    }

    let producer_to_canonical = result
        .postprocess_executions()
        .iter()
        .map(CorePostProcessExecution::candidate_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, producer_id)| {
            let canonical = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or("pc path canonical candidate id overflow")?;
            Ok((producer_id, canonical))
        })
        .collect::<Result<BTreeMap<_, _>, &'static str>>()?;

    let mut witnesses = Vec::with_capacity(result.postprocess_executions().len());
    for execution in result.postprocess_executions() {
        witnesses.push(project_execution(
            &problem,
            execution,
            materialized_pattern_count,
            *producer_to_canonical
                .get(&execution.candidate_id())
                .ok_or("pc path canonical candidate id missing")?,
        )?);
    }
    witnesses.sort_by(|left, right| {
        (
            left.candidate_id,
            left.pattern_id,
            left.normalized_trace_key.as_str(),
            left.trace_identity.as_str(),
        )
            .cmp(&(
                right.candidate_id,
                right.pattern_id,
                right.normalized_trace_key.as_str(),
                right.trace_identity.as_str(),
            ))
    });
    if witnesses.windows(2).any(|pair| {
        pair[0].candidate_id == pair[1].candidate_id
            && pair[0].pattern_id == pair[1].pattern_id
            && pair[0].normalized_trace_key == pair[1].normalized_trace_key
            && pair[0].trace_identity == pair[1].trace_identity
    }) {
        return Err("pc path execution identity is duplicated");
    }

    Ok(PcPathFamilyV2Result {
        contract_id: PC_PATH_FAMILY_RESULT_CONTRACT,
        witness_contract: PC_PATH_WITNESS_CONTRACT,
        ordering: PC_PATH_ORDERING,
        origin,
        query: query.snapshot(),
        problem_preset: preset,
        problem_id: problem.problem_id().as_str().to_owned(),
        materialized_pattern_count,
        witnesses,
        completeness: PcPathCompletenessEvidence {
            query_bound: true,
            search_complete: true,
            execution_batch_complete: true,
            count_complete: true,
            objective_complete: true,
            replay_chain_validated: true,
        },
    })
}

fn project_execution(
    problem: &SearchProblem,
    execution: &CorePostProcessExecution,
    pattern_count: usize,
    candidate_id: u64,
) -> Result<PcPathWitnessV2, &'static str> {
    if execution.pattern_id() >= pattern_count || execution.trace_identity().is_empty() {
        return Err("pc path execution identity is invalid");
    }
    let trace = execution.replay_trace();
    let source_steps = trace.solution_trace().steps();
    if source_steps.is_empty() {
        return Err("pc path replay trace is empty");
    }
    let mut expected_board = problem.initial_board().occupied_mask();
    let mut expected_cursor = usize::from(problem.initial_hold().cursor());
    let mut expected_hold = problem.initial_hold().hold_piece();
    let mut projected = Vec::with_capacity(source_steps.len());
    for (index, step) in source_steps.iter().enumerate() {
        let decision = step.piece_decision();
        let placement = step.placement();
        let before = step.board_before();
        let after = step.board_after();
        if step.step_index() != index
            || decision.input_cursor() != expected_cursor
            || decision.input_hold_piece() != expected_hold
            || placement.piece_kind() != decision.active_piece()
            || before.occupied() != expected_board
            || before.layout() != after.after_placement().layout()
            || before.layout() != after.after_line_clear().layout()
        {
            return Err("pc path replay chain is invalid");
        }
        let layout = before.layout();
        let width = u32::from(layout.width());
        if width == 0 || width > 64 {
            return Err("pc path replay layout width is invalid");
        }
        let row_bits = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        let mut cleared_row_mask = 0_u64;
        for row in 0..u32::from(layout.height()) {
            let shift = row
                .checked_mul(width)
                .ok_or("pc path replay row shift overflow")?;
            if shift >= 64 {
                return Err("pc path replay row shift is out of range");
            }
            let mask = row_bits
                .checked_shl(shift)
                .ok_or("pc path replay row mask overflow")?;
            if after.after_placement().occupied() & mask == mask {
                cleared_row_mask |= 1_u64 << row;
            }
        }
        let cleared_lines = step.line_clear().cleared_lines();
        if cleared_row_mask.count_ones() != u32::from(cleared_lines) {
            return Err("pc path line-clear identity is invalid");
        }
        let line_clear_identity = format!("rows:{cleared_row_mask:016x}:count:{cleared_lines}");
        projected.push(PcPathStepV2 {
            step_index: index,
            operation_id: step.operation_id().0,
            active_piece: decision.active_piece(),
            input_cursor: decision.input_cursor(),
            output_cursor: decision.output_cursor(),
            input_hold_piece: decision.input_hold_piece(),
            output_hold_piece: decision.output_hold_piece(),
            hold_decision: decision.hold_decision().as_str(),
            rotation: placement.rotation().quarter_turns(),
            x: placement.x(),
            y: placement.y(),
            placement_mask: placement.mask(),
            board_before_mask: before.occupied(),
            board_after_placement_mask: after.after_placement().occupied(),
            board_after_line_clear_mask: after.after_line_clear().occupied(),
            cleared_row_mask,
            cleared_lines,
            line_clear_identity,
        });
        expected_board = after.after_line_clear().occupied();
        expected_cursor = decision.output_cursor();
        expected_hold = decision.output_hold_piece();
    }
    if expected_board != 0 {
        return Err("pc path replay does not clear to empty");
    }

    Ok(PcPathWitnessV2 {
        candidate_id,
        producer_candidate_id: execution.candidate_id(),
        pattern_id: execution.pattern_id(),
        trace_identity: execution.trace_identity().to_owned(),
        normalized_trace_key: trace.canonical_key(),
        consumed_piece_count: expected_cursor,
        terminal_hold_piece: expected_hold,
        steps: projected,
    })
}
