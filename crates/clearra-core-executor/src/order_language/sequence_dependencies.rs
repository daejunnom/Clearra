use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    time::{Duration, Instant},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

use crate::backend::{DocumentLockReachability, DocumentReachabilityEngine};

use super::build_order_language::{
    CandidateId, ExactDecimalCount, ExactOperationOrderAutomaton, OperationBitSet,
    OperationDependencyEdge, OperationDependencyEvidenceKind, OperationDependencyGraph,
    OperationOrderAutomatonState, OperationOrderAutomatonTransition, OperationOrderLanguage,
    OperationOrderLanguageError, OperationSetKey, DEFAULT_OPERATION_ORDER_TIMEOUT_SECONDS,
    MAX_OPERATION_ORDER_OPERATIONS, MAX_OPERATION_ORDER_TIMEOUT_SECONDS,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConcreteDocumentOperation {
    pub operation_id: OperationId,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
}

impl ConcreteDocumentOperation {
    /// Converts the centered true-rotation coordinates shared by CTK3 and Fumen into the
    /// executor's lower-left normalized shape anchor without changing occupied cells.
    pub fn from_centered(
        operation_id: OperationId,
        piece: PieceKind,
        rotation: RotationState,
        x: i32,
        y: i32,
    ) -> Option<Self> {
        let mut offsets = centered_spawn_offsets(piece);
        for _ in 0..rotation.quarter_turns() {
            for (cell_x, cell_y) in &mut offsets {
                (*cell_x, *cell_y) = (*cell_y, -*cell_x);
            }
        }
        let min_x = offsets.iter().map(|(cell_x, _)| *cell_x).min()?;
        let min_y = offsets.iter().map(|(_, cell_y)| *cell_y).min()?;
        Some(Self {
            operation_id,
            piece,
            rotation,
            x: i8::try_from(x.checked_add(min_x)?).ok()?,
            y: i8::try_from(y.checked_add(min_y)?).ok()?,
        })
    }

    /// Recovers the centered true-rotation coordinates accepted by CTK3 and
    /// operation-preserving Fumen. Together with [`Self::from_centered`] this
    /// makes operation-document normalization lossless for concrete locks.
    pub fn centered_coordinates(self) -> Option<(i16, i16)> {
        let mut offsets = centered_spawn_offsets(self.piece);
        for _ in 0..self.rotation.quarter_turns() {
            for (cell_x, cell_y) in &mut offsets {
                (*cell_x, *cell_y) = (*cell_y, -*cell_x);
            }
        }
        let min_x = offsets.iter().map(|(cell_x, _)| *cell_x).min()?;
        let min_y = offsets.iter().map(|(_, cell_y)| *cell_y).min()?;
        Some((
            i16::from(self.x).checked_sub(i16::try_from(min_x).ok()?)?,
            i16::from(self.y).checked_sub(i16::try_from(min_y).ok()?)?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationDocumentProblem {
    pub width: u8,
    pub height: u8,
    pub initial_board: u64,
    pub operations: Vec<ConcreteDocumentOperation>,
    /// When decoded from a multipage document, every page's exact pre-operation board.
    /// `None` is reserved for trusted in-process callers that already supplied a concrete trace.
    pub document_boards: Option<Vec<u64>>,
    pub rule_profile: RuleProfileId,
    pub kick_profile: KickTableProfileId,
    pub timeout_seconds: u16,
}

impl OperationDocumentProblem {
    pub fn canonical(
        width: u8,
        height: u8,
        initial_board: u64,
        operations: Vec<ConcreteDocumentOperation>,
    ) -> Self {
        Self {
            width,
            height,
            initial_board,
            operations,
            document_boards: None,
            rule_profile: RuleProfileId::SrsPlus,
            kick_profile: KickTableProfileId::SrsPlus,
            timeout_seconds: DEFAULT_OPERATION_ORDER_TIMEOUT_SECONDS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationReachabilityEvidence {
    pub operation_id: OperationId,
    pub visited_state_count: usize,
    pub used_first_success_kick: bool,
    pub line_clear_adjusted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequenceDependencyReport {
    pub language: OperationOrderLanguage,
    pub representative_order: Option<Vec<OperationId>>,
    pub reachability_evidence: Vec<OperationReachabilityEvidence>,
    pub rule_profile: RuleProfileId,
    pub kick_profile: KickTableProfileId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceDependenciesError {
    InvalidInput(&'static str),
    Language(OperationOrderLanguageError),
    Cancelled,
    TimedOut { timeout_seconds: u16 },
    Incomplete { reason: &'static str },
}

impl fmt::Display for SequenceDependenciesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for SequenceDependenciesError {}
impl From<OperationOrderLanguageError> for SequenceDependenciesError {
    fn from(value: OperationOrderLanguageError) -> Self {
        Self::Language(value)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StateKey {
    placed: OperationBitSet,
    board: u64,
    deleted_rows: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Transition {
    operation_index: usize,
    child: usize,
    reachability: OperationReachabilityEvidence,
}

#[derive(Clone, Debug)]
struct Node {
    key: StateKey,
    depth: usize,
    outgoing: Vec<Transition>,
}

#[derive(Clone, Copy, Debug)]
struct TargetOperation {
    source: ConcreteDocumentOperation,
    target_anchor_y: u8,
    target_cells: u64,
}

pub struct SequenceDependenciesAnalyzer;

impl SequenceDependenciesAnalyzer {
    pub fn analyze(
        problem: &OperationDocumentProblem,
        control: &ExecutionControl,
    ) -> Result<SequenceDependencyReport, SequenceDependenciesError> {
        validate_problem(problem)?;
        let started = Instant::now();
        let deadline = Duration::from_secs(u64::from(problem.timeout_seconds));
        let mut reachability =
            DocumentReachabilityEngine::new(problem.width, problem.height, problem.kick_profile)
                .ok_or(SequenceDependenciesError::InvalidInput(
                    "unsupported kick profile or Board64 dimensions",
                ))?;
        let targets = normalize_document_trace(problem, &mut reachability)?;
        poll(control, started, deadline, problem.timeout_seconds)?;

        let operation_count = targets.len();
        let root = StateKey {
            placed: OperationBitSet::new(operation_count)?,
            board: problem.initial_board,
            deleted_rows: 0,
        };
        let mut nodes = vec![Node {
            key: root.clone(),
            depth: 0,
            outgoing: Vec::new(),
        }];
        let mut index_by_key = BTreeMap::from([(root, 0_usize)]);
        let mut queue = VecDeque::from([0_usize]);
        let mut expansion_counter = 0_usize;

        while let Some(node_index) = queue.pop_front() {
            expansion_counter += 1;
            if expansion_counter & 0xff == 0 {
                poll(control, started, deadline, problem.timeout_seconds)?;
                control.report_progress(
                    "sequence_dependencies",
                    nodes[node_index].depth as u64,
                    Some(operation_count as u64),
                );
            }
            let parent = nodes[node_index].key.clone();
            let mut outgoing = Vec::new();
            outgoing
                .try_reserve(operation_count.saturating_sub(parent.placed.count_ones()))
                .map_err(|_| SequenceDependenciesError::Incomplete {
                    reason: "operation graph transition allocation failed",
                })?;
            for (operation_index, target) in targets.iter().enumerate() {
                if parent.placed.contains(operation_index) {
                    continue;
                }
                let Some((child_key, evidence)) = apply_target_operation(
                    problem,
                    target,
                    operation_index,
                    &parent,
                    &mut reachability,
                )?
                else {
                    continue;
                };
                let child = if let Some(existing) = index_by_key.get(&child_key) {
                    *existing
                } else {
                    nodes
                        .try_reserve(1)
                        .map_err(|_| SequenceDependenciesError::Incomplete {
                            reason: "operation graph node allocation failed",
                        })?;
                    let child = nodes.len();
                    nodes.push(Node {
                        key: child_key.clone(),
                        depth: nodes[node_index].depth + 1,
                        outgoing: Vec::new(),
                    });
                    index_by_key.insert(child_key, child);
                    queue.push_back(child);
                    child
                };
                outgoing.push(Transition {
                    operation_index,
                    child,
                    reachability: evidence,
                });
            }
            outgoing.sort_unstable_by_key(|edge| {
                (
                    targets[edge.operation_index].source.operation_id,
                    edge.child,
                )
            });
            nodes[node_index].outgoing = outgoing;
        }
        poll(control, started, deadline, problem.timeout_seconds)?;

        let mut live = vec![false; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            live[index] = node.depth == operation_count;
        }
        let mut by_depth: Vec<Vec<usize>> = vec![Vec::new(); operation_count + 1];
        for (index, node) in nodes.iter().enumerate() {
            by_depth[node.depth].push(index);
        }
        for depth in (0..operation_count).rev() {
            for index in by_depth[depth].iter().copied() {
                live[index] = nodes[index].outgoing.iter().any(|edge| live[edge.child]);
            }
        }

        let operation_ids: Vec<_> = targets
            .iter()
            .map(|target| target.source.operation_id)
            .collect();
        let mut universal = if live[0] {
            let mut sets = vec![OperationBitSet::full(operation_count)?; operation_count];
            for (index, set) in sets.iter_mut().enumerate() {
                set.remove(index)?;
            }
            sets
        } else {
            vec![OperationBitSet::new(operation_count)?; operation_count]
        };
        let mut ways = vec![ExactDecimalCount::zero(); nodes.len()];
        ways[0] = ExactDecimalCount::one();
        let mut representative = Vec::new();
        let mut representative_node = 0_usize;
        let mut evidence_by_operation: Vec<_> = targets
            .iter()
            .map(|target| OperationReachabilityEvidence {
                operation_id: target.source.operation_id,
                visited_state_count: 0,
                used_first_success_kick: false,
                line_clear_adjusted: false,
            })
            .collect();
        let mut live_transition_count = 0_usize;
        for nodes_at_depth in by_depth.iter().take(operation_count) {
            if live[representative_node] {
                if let Some(edge) = nodes[representative_node]
                    .outgoing
                    .iter()
                    .find(|edge| live[edge.child])
                {
                    representative.push(targets[edge.operation_index].source.operation_id);
                    representative_node = edge.child;
                }
            }
            for node_index in nodes_at_depth.iter().copied().filter(|index| live[*index]) {
                for edge in nodes[node_index]
                    .outgoing
                    .iter()
                    .filter(|edge| live[edge.child])
                {
                    live_transition_count += 1;
                    universal[edge.operation_index]
                        .intersect_assign(&nodes[node_index].key.placed)?;
                    let source_ways = ways[node_index].clone();
                    ways[edge.child].add_assign(&source_ways);
                    let aggregate = &mut evidence_by_operation[edge.operation_index];
                    aggregate.operation_id = targets[edge.operation_index].source.operation_id;
                    aggregate.visited_state_count = aggregate
                        .visited_state_count
                        .max(edge.reachability.visited_state_count);
                    aggregate.used_first_success_kick |= edge.reachability.used_first_success_kick;
                    aggregate.line_clear_adjusted |= edge.reachability.line_clear_adjusted;
                }
            }
        }
        let mut exact_count = ExactDecimalCount::zero();
        for index in by_depth[operation_count]
            .iter()
            .copied()
            .filter(|index| live[*index])
        {
            exact_count.add_assign(&ways[index]);
        }
        let mut evidence_kinds = BTreeMap::new();
        if live[0] {
            for (successor, predecessors) in universal.iter().enumerate() {
                for predecessor in predecessors.iter() {
                    let edge = OperationDependencyEdge {
                        predecessor: operation_ids[predecessor],
                        successor: operation_ids[successor],
                    };
                    let mut kinds = vec![OperationDependencyEvidenceKind::Reachability];
                    if evidence_by_operation[successor].used_first_success_kick {
                        kinds.push(OperationDependencyEvidenceKind::FirstSuccessKick);
                    }
                    if evidence_by_operation[successor].line_clear_adjusted {
                        kinds.push(OperationDependencyEvidenceKind::LineClearRemap);
                    }
                    evidence_kinds.insert(edge, kinds);
                }
            }
        }
        let graph = OperationDependencyGraph::from_complete_analysis(
            operation_ids.clone(),
            universal,
            exact_count,
            nodes.len(),
            live_transition_count,
            evidence_kinds,
        )?;
        let live_indices: Vec<_> = if live[0] {
            (0..nodes.len()).filter(|index| live[*index]).collect()
        } else {
            vec![0]
        };
        let mut compact_index = vec![usize::MAX; nodes.len()];
        for (next, source) in live_indices.iter().copied().enumerate() {
            compact_index[source] = next;
        }
        let automaton_states = live_indices
            .iter()
            .copied()
            .map(|source| OperationOrderAutomatonState {
                accepting: nodes[source].depth == operation_count,
                transitions: nodes[source]
                    .outgoing
                    .iter()
                    .filter(|edge| live[edge.child])
                    .map(|edge| OperationOrderAutomatonTransition {
                        operation_id: targets[edge.operation_index].source.operation_id,
                        target_state: compact_index[edge.child],
                    })
                    .collect(),
            })
            .collect();
        let automaton = ExactOperationOrderAutomaton::try_new(operation_ids, 0, automaton_states)?;
        let key = operation_set_key(problem, &targets);
        let language = OperationOrderLanguage::from_complete_automaton(
            CandidateId(0),
            OperationSetKey(key),
            graph,
            automaton,
        )?;
        Ok(SequenceDependencyReport {
            language,
            representative_order: live[0].then_some(representative),
            reachability_evidence: evidence_by_operation,
            rule_profile: problem.rule_profile,
            kick_profile: problem.kick_profile,
        })
    }
}

fn validate_problem(problem: &OperationDocumentProblem) -> Result<(), SequenceDependenciesError> {
    let cells = usize::from(problem.width)
        .checked_mul(usize::from(problem.height))
        .ok_or(SequenceDependenciesError::InvalidInput(
            "board dimensions overflow",
        ))?;
    if problem.width == 0 || problem.height == 0 || cells > 64 {
        return Err(SequenceDependenciesError::InvalidInput(
            "operation dependencies require a non-empty Board64 document",
        ));
    }
    if problem.operations.is_empty() {
        return Err(SequenceDependenciesError::InvalidInput(
            "document must contain a concrete operation multiset",
        ));
    }
    if problem
        .document_boards
        .as_ref()
        .is_some_and(|boards| boards.len() != problem.operations.len())
    {
        return Err(SequenceDependenciesError::InvalidInput(
            "document board/page count differs from concrete operation count",
        ));
    }
    if problem.operations.len() > MAX_OPERATION_ORDER_OPERATIONS {
        return Err(OperationOrderLanguageError::OperationLimitExceeded {
            operation_count: problem.operations.len(),
            maximum: MAX_OPERATION_ORDER_OPERATIONS,
        }
        .into());
    }
    if !(1..=MAX_OPERATION_ORDER_TIMEOUT_SECONDS).contains(&problem.timeout_seconds) {
        return Err(OperationOrderLanguageError::InvalidTimeoutSeconds {
            requested: problem.timeout_seconds,
            maximum: MAX_OPERATION_ORDER_TIMEOUT_SECONDS,
        }
        .into());
    }
    let mut ids: Vec<_> = problem
        .operations
        .iter()
        .map(|operation| operation.operation_id)
        .collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(SequenceDependenciesError::InvalidInput(
            "operation ids must be unique",
        ));
    }
    if cells < 64 && problem.initial_board >> cells != 0 {
        return Err(SequenceDependenciesError::InvalidInput(
            "initial board contains cells outside document dimensions",
        ));
    }
    if matches!(problem.rule_profile, RuleProfileId::Custom) {
        return Err(SequenceDependenciesError::InvalidInput(
            "custom rule profiles require an explicit connected runtime profile",
        ));
    }
    Ok(())
}

fn normalize_document_trace(
    problem: &OperationDocumentProblem,
    reachability: &mut DocumentReachabilityEngine,
) -> Result<Vec<TargetOperation>, SequenceDependenciesError> {
    let mut board = problem.initial_board;
    let mut deleted_rows = 0_u64;
    let mut targets = Vec::new();
    targets.try_reserve(problem.operations.len()).map_err(|_| {
        SequenceDependenciesError::Incomplete {
            reason: "operation normalization allocation failed",
        }
    })?;
    for (operation_index, operation) in problem.operations.iter().enumerate() {
        if problem
            .document_boards
            .as_ref()
            .is_some_and(|boards| boards[operation_index] != board)
        {
            return Err(SequenceDependenciesError::InvalidInput(
                "document page board does not match concrete operation replay",
            ));
        }
        let result = reachability.analyze_lock(
            board,
            operation.piece,
            operation.rotation,
            operation.x,
            operation.y,
        );
        if !result.valid_target {
            return Err(SequenceDependenciesError::InvalidInput(
                "document contains an out-of-bounds concrete operation",
            ));
        }
        if !result.reachable {
            return Err(SequenceDependenciesError::InvalidInput(
                "document operation trace is not reachable under the selected kick profile",
            ));
        }
        let target_cells = physical_mask_to_logical(
            result.lock_mask,
            problem.width,
            problem.height,
            deleted_rows,
        )
        .ok_or(SequenceDependenciesError::InvalidInput(
            "document loses concrete operation coordinates after line-clear remap",
        ))?;
        let target_anchor_y = physical_row_to_logical(operation.y, problem.height, deleted_rows)
            .ok_or(SequenceDependenciesError::InvalidInput(
                "document operation anchor cannot be represented losslessly",
            ))?;
        targets.push(TargetOperation {
            source: *operation,
            target_anchor_y,
            target_cells,
        });
        board |= result.lock_mask;
        let (next_board, newly_deleted) =
            clear_full_rows(board, problem.width, problem.height, deleted_rows);
        board = next_board;
        deleted_rows |= newly_deleted;
    }
    targets.sort_unstable_by_key(|target| target.source.operation_id);
    Ok(targets)
}

fn apply_target_operation(
    problem: &OperationDocumentProblem,
    target: &TargetOperation,
    operation_index: usize,
    parent: &StateKey,
    reachability: &mut DocumentReachabilityEngine,
) -> Result<Option<(StateKey, OperationReachabilityEvidence)>, SequenceDependenciesError> {
    let Some(lock_y) = logical_row_to_physical(target.target_anchor_y, parent.deleted_rows) else {
        return Ok(None);
    };
    let lock_y = i8::try_from(lock_y).map_err(|_| {
        SequenceDependenciesError::InvalidInput("adjusted lock coordinate overflow")
    })?;
    let Some(projected_cells) = logical_mask_to_physical(
        target.target_cells,
        problem.width,
        problem.height,
        parent.deleted_rows,
    ) else {
        return Ok(None);
    };
    let concrete_mask = concrete_operation_mask(
        problem.width,
        problem.height,
        target.source.piece,
        target.source.rotation,
        target.source.x,
        lock_y,
    );
    if concrete_mask != Some(projected_cells) {
        return Ok(None);
    }
    let result: DocumentLockReachability = reachability.analyze_lock(
        parent.board,
        target.source.piece,
        target.source.rotation,
        target.source.x,
        lock_y,
    );
    if !result.reachable || result.lock_mask != projected_cells {
        return Ok(None);
    }
    let board = parent.board | projected_cells;
    let (board, newly_deleted) =
        clear_full_rows(board, problem.width, problem.height, parent.deleted_rows);
    let mut placed = parent.placed.clone();
    placed.insert(operation_index)?;
    Ok(Some((
        StateKey {
            placed,
            board,
            deleted_rows: parent.deleted_rows | newly_deleted,
        },
        OperationReachabilityEvidence {
            operation_id: target.source.operation_id,
            visited_state_count: result.visited_state_count,
            used_first_success_kick: result.first_success_kick_evidence,
            line_clear_adjusted: lock_y != target.source.y,
        },
    )))
}

fn concrete_operation_mask(
    width: u8,
    height: u8,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> Option<u64> {
    let definition = standard_tetromino_registry().get(piece)?;
    let mut mask = 0_u64;
    for cell in definition.shape(rotation).cells() {
        let cell_x = i16::from(x) + i16::from(cell.x());
        let cell_y = i16::from(y) + i16::from(cell.y());
        if cell_x < 0 || cell_x >= i16::from(width) || cell_y < 0 || cell_y >= i16::from(height) {
            return None;
        }
        mask |= 1_u64 << (cell_y as usize * usize::from(width) + cell_x as usize);
    }
    Some(mask)
}

fn physical_row_to_logical(row: i8, height: u8, deleted_rows: u64) -> Option<u8> {
    if row < 0 {
        return None;
    }
    let wanted = row as usize;
    let mut present = 0_usize;
    for logical in 0..usize::from(height) {
        if deleted_rows & (1_u64 << logical) != 0 {
            continue;
        }
        if present == wanted {
            return Some(logical as u8);
        }
        present += 1;
    }
    None
}
fn logical_row_to_physical(row: u8, deleted_rows: u64) -> Option<u8> {
    let bit = 1_u64 << row;
    if deleted_rows & bit != 0 {
        return None;
    }
    Some(row - (deleted_rows & (bit - 1)).count_ones() as u8)
}
fn physical_mask_to_logical(mask: u64, width: u8, height: u8, deleted_rows: u64) -> Option<u64> {
    let mut result = 0_u64;
    for physical in 0..usize::from(height) {
        let row = (mask >> (physical * usize::from(width))) & row_mask(width);
        if row == 0 {
            continue;
        }
        let logical = usize::from(physical_row_to_logical(
            physical as i8,
            height,
            deleted_rows,
        )?);
        result |= row << (logical * usize::from(width));
    }
    Some(result)
}
fn logical_mask_to_physical(mask: u64, width: u8, height: u8, deleted_rows: u64) -> Option<u64> {
    let mut result = 0_u64;
    for logical in 0..usize::from(height) {
        let row = (mask >> (logical * usize::from(width))) & row_mask(width);
        if row == 0 {
            continue;
        }
        let physical = usize::from(logical_row_to_physical(logical as u8, deleted_rows)?);
        result |= row << (physical * usize::from(width));
    }
    Some(result)
}
fn clear_full_rows(board: u64, width: u8, height: u8, deleted_rows: u64) -> (u64, u64) {
    let row_mask = row_mask(width);
    let mut compacted = 0_u64;
    let mut destination = 0_usize;
    let mut newly_deleted = 0_u64;
    for source in 0..usize::from(height) {
        let row = (board >> (source * usize::from(width))) & row_mask;
        if row == row_mask {
            if let Some(logical) = physical_row_to_logical(source as i8, height, deleted_rows) {
                newly_deleted |= 1_u64 << logical;
            }
        } else {
            compacted |= row << (destination * usize::from(width));
            destination += 1;
        }
    }
    (compacted, newly_deleted)
}
fn row_mask(width: u8) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}
fn poll(
    control: &ExecutionControl,
    started: Instant,
    deadline: Duration,
    timeout_seconds: u16,
) -> Result<(), SequenceDependenciesError> {
    if control.is_cancelled() {
        return Err(SequenceDependenciesError::Cancelled);
    }
    if started.elapsed() >= deadline {
        return Err(SequenceDependenciesError::TimedOut { timeout_seconds });
    }
    Ok(())
}
fn operation_set_key(problem: &OperationDocumentProblem, targets: &[TargetOperation]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    mix(u64::from(problem.width));
    mix(u64::from(problem.height));
    mix(problem.initial_board);
    for target in targets {
        mix(u64::from(target.source.operation_id.0));
        mix(target.target_cells);
        mix(u64::from(target.target_anchor_y));
    }
    hash
}

fn centered_spawn_offsets(piece: PieceKind) -> [(i32, i32); 4] {
    match piece {
        PieceKind::I => [(-1, 0), (0, 0), (1, 0), (2, 0)],
        PieceKind::O => [(0, 0), (1, 0), (0, 1), (1, 1)],
        PieceKind::T => [(-1, 0), (0, 0), (1, 0), (0, 1)],
        PieceKind::S => [(-1, 0), (0, 0), (0, 1), (1, 1)],
        PieceKind::Z => [(-1, 1), (0, 1), (0, 0), (1, 0)],
        PieceKind::J => [(-1, 1), (-1, 0), (0, 0), (1, 0)],
        PieceKind::L => [(1, 1), (-1, 0), (0, 0), (1, 0)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: u16, piece: PieceKind, x: i8) -> ConcreteDocumentOperation {
        ConcreteDocumentOperation {
            operation_id: OperationId(id),
            piece,
            rotation: RotationState::Zero,
            x,
            y: 0,
        }
    }

    #[test]
    fn independent_grounded_operations_have_factorial_language() {
        let problem = OperationDocumentProblem::canonical(
            10,
            4,
            0,
            vec![op(0, PieceKind::O, 0), op(1, PieceKind::O, 4)],
        );
        let report =
            SequenceDependenciesAnalyzer::analyze(&problem, &ExecutionControl::default()).unwrap();
        assert_eq!(report.language.exact_order_count().to_string(), "2");
        assert!(report
            .language
            .dependency_constraints
            .universal_precedence_closure()
            .is_empty());
        assert_eq!(
            report
                .language
                .dependency_constraints
                .independent_pair_count(),
            1
        );
    }

    #[test]
    fn cancelled_analysis_never_publishes_dependencies() {
        let token = clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        token.handle().cancel();
        let problem = OperationDocumentProblem::canonical(10, 4, 0, vec![op(0, PieceKind::O, 0)]);
        let error = SequenceDependenciesAnalyzer::analyze(&problem, &ExecutionControl::new(token))
            .unwrap_err();
        assert_eq!(error, SequenceDependenciesError::Cancelled);
    }
}
