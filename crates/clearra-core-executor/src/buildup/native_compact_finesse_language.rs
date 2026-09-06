//! Opt-in finesse annotation for a prepared compact native geometry snapshot.
//!
//! This adapter is deliberately not called by the normal BuildUp/PC path. It
//! consumes the geometry-only v2 snapshot, groups all concrete lock targets by
//! `(board, piece)`, and runs one cost-only multi-target movement search per
//! group. It does not retain terminal rotation/kick evidence and therefore is
//! not an authority for spin or B2B scoring.

use std::{collections::BTreeMap, error::Error, fmt};

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::ExecutionCancellationToken,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_core_ffi::{
    BuildUpGeometryLanguageV2, BuildUpGeometryTransitionMode, CBuildUpProblem,
    NativeBuildUpWorkspace, NativeCoreError,
};
use clearra_finesse::{
    CostedGeometryEdge, CostedGeometryLanguage, FinesseBoard, FinesseError, FinesseTarget,
    FrozenFinesseQuery, GeometryActionKey, GeometryLanguageError, GeometryLanguageNode,
    GeometryNodeId,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::{kicks::KickTableProfile, spawn::SpawnProfile};

use crate::performance::{ExecutorSearchStage, SearchStageSpan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeCompactFinesseError {
    Native(NativeCoreError),
    InvalidCompactLayout,
    IncompleteSnapshot,
    GeometryOnlySnapshotRequired,
    InvalidSnapshot,
    InvalidPieceCode { edge_index: usize, code: u8 },
    InvalidRotationCode { edge_index: usize, code: u8 },
    TargetMaskMismatch { edge_index: usize },
    Movement(FinesseError),
    Language(GeometryLanguageError),
}

impl fmt::Display for NativeCompactFinesseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(error) => write!(formatter, "native geometry export failed: {error:?}"),
            Self::InvalidCompactLayout => {
                formatter.write_str("native finesse requires a valid compact Board64 layout")
            }
            Self::IncompleteSnapshot => {
                formatter.write_str("native geometry snapshot is incomplete")
            }
            Self::GeometryOnlySnapshotRequired => {
                formatter.write_str("native finesse requires a geometry-only prepared snapshot")
            }
            Self::InvalidSnapshot => formatter.write_str("native geometry snapshot is invalid"),
            Self::InvalidPieceCode { edge_index, code } => {
                write!(formatter, "edge {edge_index} has invalid piece code {code}")
            }
            Self::InvalidRotationCode { edge_index, code } => {
                write!(
                    formatter,
                    "edge {edge_index} has invalid rotation code {code}"
                )
            }
            Self::TargetMaskMismatch { edge_index } => {
                write!(
                    formatter,
                    "edge {edge_index} target pose does not match its mask"
                )
            }
            Self::Movement(error) => write!(formatter, "finesse movement search failed: {error}"),
            Self::Language(error) => {
                write!(formatter, "costed geometry language is invalid: {error}")
            }
        }
    }
}

impl Error for NativeCompactFinesseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Movement(error) => Some(error),
            Self::Language(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NativeCoreError> for NativeCompactFinesseError {
    fn from(error: NativeCoreError) -> Self {
        Self::Native(error)
    }
}

impl From<FinesseError> for NativeCompactFinesseError {
    fn from(error: FinesseError) -> Self {
        Self::Movement(error)
    }
}

impl From<GeometryLanguageError> for NativeCompactFinesseError {
    fn from(error: GeometryLanguageError) -> Self {
        Self::Language(error)
    }
}

/// Prepare a geometry-only native v2 snapshot and annotate it with minimum
/// classic-input costs. This is an explicit seam; existing search entrypoints
/// do not invoke it.
pub fn prepare_native_compact_finesse_language(
    workspace: &mut NativeBuildUpWorkspace,
    problem: &CBuildUpProblem,
    spawn: SpawnProfile,
    kicks: &KickTableProfile,
    cancellation: &ExecutionCancellationToken,
) -> Result<CostedGeometryLanguage, NativeCompactFinesseError> {
    let size = BoardSize::new(
        problem.initial_board.width,
        problem.initial_board.search_height,
    )
    .map_err(|_| NativeCompactFinesseError::InvalidCompactLayout)?;
    let layout =
        Board64Layout::new(size).map_err(|_| NativeCompactFinesseError::InvalidCompactLayout)?;
    let geometry_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseGeometry);
    let snapshot = workspace.export_geometry_language_v2_with_cancellation(
        problem,
        BuildUpGeometryTransitionMode::GeometryOnly,
        cancellation,
    )?;
    geometry_span.finish(snapshot.nodes().len() as u64);
    annotate_prepared_native_compact_finesse_language(&snapshot, layout, spawn, kicks)
}

/// Annotate a frozen native v2 snapshot. Edges that the movement BFS cannot
/// reach are removed, then exact DAG survival is recomputed from accepting
/// nodes. Node indices remain aligned with the prepared snapshot.
pub fn annotate_prepared_native_compact_finesse_language(
    snapshot: &BuildUpGeometryLanguageV2,
    layout: Board64Layout,
    spawn: SpawnProfile,
    kicks: &KickTableProfile,
) -> Result<CostedGeometryLanguage, NativeCompactFinesseError> {
    annotate_with_stats(snapshot, layout, spawn, kicks).map(|(language, _)| language)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AnnotationStats {
    board_piece_groups: usize,
    target_count: usize,
    retained_edges: usize,
}

#[derive(Clone, Copy, Debug)]
struct GroupedTarget {
    edge_index: usize,
    target: FinesseTarget,
}

fn annotate_with_stats(
    snapshot: &BuildUpGeometryLanguageV2,
    layout: Board64Layout,
    spawn: SpawnProfile,
    kicks: &KickTableProfile,
) -> Result<(CostedGeometryLanguage, AnnotationStats), NativeCompactFinesseError> {
    if !snapshot.complete() {
        return Err(NativeCompactFinesseError::IncompleteSnapshot);
    }
    if snapshot.transition_mode() != BuildUpGeometryTransitionMode::GeometryOnly {
        return Err(NativeCompactFinesseError::GeometryOnlySnapshotRequired);
    }
    let nodes = snapshot.nodes();
    let edges = snapshot.edges();
    if nodes.is_empty() || snapshot.root_node_index() >= nodes.len() {
        return Err(NativeCompactFinesseError::InvalidSnapshot);
    }

    let grouping_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseTargetGrouping);
    let mut groups = BTreeMap::<(u64, PieceKind), Vec<GroupedTarget>>::new();
    for node in nodes.iter().copied() {
        let start = node.first_edge();
        let end = start
            .checked_add(node.edge_count())
            .filter(|end| *end <= edges.len())
            .ok_or(NativeCompactFinesseError::InvalidSnapshot)?;
        for (edge_index, &edge) in edges.iter().enumerate().take(end).skip(start) {
            if edge.child_node_index() >= nodes.len()
                || nodes[edge.child_node_index()].depth() != node.depth().saturating_add(1)
            {
                return Err(NativeCompactFinesseError::InvalidSnapshot);
            }
            let piece = piece_from_native(edge.piece()).ok_or(
                NativeCompactFinesseError::InvalidPieceCode {
                    edge_index,
                    code: edge.piece(),
                },
            )?;
            let rotation = RotationState::from_quarter_turns(edge.rotation()).map_err(|_| {
                NativeCompactFinesseError::InvalidRotationCode {
                    edge_index,
                    code: edge.rotation(),
                }
            })?;
            let target =
                FinesseTarget::new(rotation, i16::from(edge.x()), i16::from(edge.adjusted_y()));
            if target_mask(layout, piece, target) != Some(edge.target_mask()) {
                return Err(NativeCompactFinesseError::TargetMaskMismatch { edge_index });
            }
            groups
                .entry((node.board_mask(), piece))
                .or_default()
                .push(GroupedTarget { edge_index, target });
        }
    }
    let mut stats = AnnotationStats {
        board_piece_groups: groups.len(),
        target_count: edges.len(),
        retained_edges: 0,
    };
    grouping_span.finish(stats.target_count as u64);

    let movement_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseMovementBfs);
    let mut edge_costs = vec![None; edges.len()];
    for ((board_mask, piece), targets) in groups {
        let board = FinesseBoard::new(layout, board_mask)?;
        let query = FrozenFinesseQuery::new(
            board,
            piece,
            spawn,
            kicks.clone(),
            targets
                .iter()
                .map(|target| target.target)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let costs = query.costs()?;
        for (target, cost) in targets.iter().zip(costs.as_slice()) {
            edge_costs[target.edge_index] = *cost;
        }
    }
    movement_span.finish(stats.target_count as u64);

    let prune_span = SearchStageSpan::begin(ExecutorSearchStage::FinesseAnnotationPrune);
    let mut live = nodes
        .iter()
        .map(|node| node.accepting())
        .collect::<Vec<_>>();
    let max_depth = nodes.iter().map(|node| node.depth()).max().unwrap_or(0);
    for depth in (0..=max_depth).rev() {
        for (node_index, node) in nodes.iter().copied().enumerate() {
            if node.depth() != depth || live[node_index] {
                continue;
            }
            let start = node.first_edge();
            let end = start + node.edge_count();
            live[node_index] = (start..end).any(|edge_index| {
                edge_costs[edge_index].is_some() && live[edges[edge_index].child_node_index()]
            });
        }
    }

    let mut costed_nodes = Vec::new();
    costed_nodes
        .try_reserve_exact(nodes.len())
        .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?;
    for (node_index, node) in nodes.iter().copied().enumerate() {
        let mut costed_edges = Vec::new();
        if live[node_index] {
            let start = node.first_edge();
            let end = start + node.edge_count();
            costed_edges
                .try_reserve_exact(node.edge_count())
                .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?;
            for edge_index in start..end {
                let edge = edges[edge_index];
                let child = edge.child_node_index();
                let Some(cost) = edge_costs[edge_index] else {
                    continue;
                };
                if !live[child] {
                    continue;
                }
                let piece = piece_from_native(edge.piece()).ok_or(
                    NativeCompactFinesseError::InvalidPieceCode {
                        edge_index,
                        code: edge.piece(),
                    },
                )?;
                let rotation =
                    RotationState::from_quarter_turns(edge.rotation()).map_err(|_| {
                        NativeCompactFinesseError::InvalidRotationCode {
                            edge_index,
                            code: edge.rotation(),
                        }
                    })?;
                costed_edges.push(
                    CostedGeometryEdge::new(
                        piece,
                        GeometryNodeId::new(
                            u32::try_from(child)
                                .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?,
                        ),
                        cost,
                        u32::try_from(edge_index)
                            .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?,
                    )
                    .with_action_key(GeometryActionKey::new(
                        piece,
                        rotation,
                        i16::from(edge.x()),
                        i16::from(edge.adjusted_y()),
                    )),
                );
            }
        }
        stats.retained_edges += costed_edges.len();
        costed_nodes.push(
            GeometryLanguageNode::new(
                u16::try_from(node.depth())
                    .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?,
                node.accepting(),
                costed_edges,
            )
            .with_source_board(FinesseBoard::new(layout, node.board_mask())?),
        );
    }
    let language = CostedGeometryLanguage::new(
        GeometryNodeId::new(
            u32::try_from(snapshot.root_node_index())
                .map_err(|_| NativeCompactFinesseError::InvalidSnapshot)?,
        ),
        costed_nodes,
    )?;
    prune_span.finish(stats.retained_edges as u64);
    Ok((language, stats))
}

fn target_mask(layout: Board64Layout, piece: PieceKind, target: FinesseTarget) -> Option<u64> {
    let pose = target.pose();
    let shape = standard_tetromino_registry().get(piece)?.rotations()
        [usize::from(pose.rotation.quarter_turns())];
    let width = i16::try_from(layout.width()).ok()?;
    let height = i16::try_from(layout.height()).ok()?;
    let mut mask = 0_u64;
    for cell in shape.cells() {
        let x = pose.x.checked_add(i16::from(cell.x()))?;
        let y = pose.y.checked_add(i16::from(cell.y()))?;
        if x < 0 || x >= width || y < 0 || y >= height {
            return None;
        }
        let index = u32::try_from(y.checked_mul(width)?.checked_add(x)?).ok()?;
        mask |= 1_u64.checked_shl(index)?;
    }
    Some(mask)
}

const fn piece_from_native(code: u8) -> Option<PieceKind> {
    match code {
        1 => Some(PieceKind::I),
        2 => Some(PieceKind::O),
        3 => Some(PieceKind::T),
        4 => Some(PieceKind::S),
        5 => Some(PieceKind::Z),
        6 => Some(PieceKind::J),
        7 => Some(PieceKind::L),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_ffi::{
        problem::C_PIECE_O, CBuildUpProblemBuilder, CPackingCandidate, CPackingOperation,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_rules::kicks::NoKick;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    fn reference_place_and_clear(
        layout: Board64Layout,
        occupied: u64,
        placement: u64,
    ) -> (u64, u32, u32) {
        assert_eq!(occupied & placement, 0);
        let placed = occupied | placement;
        let width = u32::from(layout.width());
        let full_row = (1_u64 << width) - 1;
        let mut compacted = 0_u64;
        let mut destination_y = 0_u32;
        let mut cleared_count = 0_u32;
        let mut cleared_rows = 0_u32;
        for source_y in 0..u32::from(layout.height()) {
            let row = (placed >> (source_y * width)) & full_row;
            if row == full_row {
                cleared_count += 1;
                cleared_rows |= 1_u32 << source_y;
            } else {
                compacted |= row << (destination_y * width);
                destination_y += 1;
            }
        }
        (compacted, cleared_count, cleared_rows)
    }

    #[test]
    fn native_grouped_annotation_matches_independent_single_target_costs() {
        let first_o = 0x0c03_u64;
        let second_o = 0x300c_u64;
        let initial_mask = 0x0f_ffff_u64 & !(first_o | second_o);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, initial_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let mut candidate = CPackingCandidate::default();
        candidate.candidate_id = 7;
        candidate.canonical_operation_set_id = 9;
        candidate.operation_count = 2;
        candidate.operations[0] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 0,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask: first_o,
        };
        candidate.operations[1] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 2,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask: second_o,
        };
        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("buildup");
        let cancellation = ExecutionCancellationToken::new();
        let mut workspace = NativeBuildUpWorkspace::new();
        let snapshot = workspace
            .export_geometry_language_v2_with_cancellation(
                &buildup,
                BuildUpGeometryTransitionMode::GeometryOnly,
                &cancellation,
            )
            .expect("native geometry snapshot");
        let layout = Board64Layout::new(BoardSize::new(10, 2).expect("size")).expect("layout");
        let spawn = SpawnProfile::new(4, 2);
        let kicks = NoKick::profile();
        let (language, stats) =
            annotate_with_stats(&snapshot, layout, spawn, &kicks).expect("costed language");

        assert_eq!(stats.target_count, snapshot.edges().len());
        assert!(stats.board_piece_groups < stats.target_count);
        assert_eq!(stats.retained_edges, snapshot.edges().len());
        assert_eq!(
            snapshot.nodes().iter().map(|node| node.depth()).max(),
            Some(2)
        );
        assert!(snapshot
            .nodes()
            .iter()
            .any(|node| node.accepting() && node.depth() == 2));

        let mut saw_intermediate_edge = false;
        let mut saw_two_line_clear = false;
        for (source_index, source) in snapshot.nodes().iter().copied().enumerate() {
            let output = language
                .node(GeometryNodeId::new(source_index as u32))
                .expect("aligned native node");
            assert_eq!(
                output.source_board(),
                Some(FinesseBoard::new(layout, source.board_mask()).expect("source board"))
            );
            for edge in output.edges() {
                let original_index = edge.transition_order() as usize;
                let original = snapshot.edges()[original_index];
                let rotation = RotationState::from_quarter_turns(original.rotation())
                    .expect("native rotation");
                let (expected_child_board, expected_cleared, expected_rows) =
                    reference_place_and_clear(layout, source.board_mask(), original.target_mask());
                let child = snapshot.nodes()[original.child_node_index()];
                assert_eq!(child.board_mask(), expected_child_board);
                assert_eq!(u32::from(original.cleared_lines()), expected_cleared);
                assert_eq!(u32::from(original.cleared_row_mask()), expected_rows);
                saw_intermediate_edge |= source.depth() == 1;
                saw_two_line_clear |= expected_cleared == 2;
                let reference = FrozenFinesseQuery::new(
                    FinesseBoard::new(layout, source.board_mask()).expect("source board"),
                    edge.piece(),
                    spawn,
                    kicks.clone(),
                    vec![FinesseTarget::new(
                        rotation,
                        i16::from(original.x()),
                        i16::from(original.adjusted_y()),
                    )],
                )
                .costs()
                .expect("single target search");
                assert_eq!(reference.get(0), Some(Some(edge.input_cost())));
                assert_eq!(edge.child().index(), original.child_node_index());
                assert_eq!(
                    edge.action_key(),
                    Some(GeometryActionKey::new(
                        edge.piece(),
                        rotation,
                        i16::from(original.x()),
                        i16::from(original.adjusted_y()),
                    ))
                );
            }
        }
        assert!(
            saw_intermediate_edge,
            "the parity fixture must exercise depth two"
        );
        assert!(
            saw_two_line_clear,
            "the parity fixture must exercise line deletion"
        );

        let prepared = prepare_native_compact_finesse_language(
            &mut workspace,
            &buildup,
            spawn,
            &kicks,
            &cancellation,
        )
        .expect("prepared adapter");
        assert_eq!(prepared, language);
    }
}
