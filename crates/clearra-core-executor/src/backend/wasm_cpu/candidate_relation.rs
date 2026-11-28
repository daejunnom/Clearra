use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_postprocess::{BackToBackEdgePolicy, BeforeSpinEvidenceDecision};
use clearra_replay::{ScoringExecutionEdge, ScoringLockEvidence};

use super::{
    buildup::{BuildEdge, BuildOrderGraph, CompactRelationAdapter},
    candidate_row_projection::place_and_clear,
    catalog::{GeometryCatalog, InstantiatedRealizationIter},
    extended_board::{merge_deleted_rows, ExtendedBoard},
    extended_buildup::ExtendedRelationAdapter,
    extended_inverse_catalog::{ExtendedInstantiationIter, ExtendedInverseCatalog},
    WasmExactSearchError,
};

#[derive(Clone, Copy)]
pub(super) struct RelationInstantiation {
    pub lock_mask: ExtendedBoard,
    pub rotation: RotationState,
    pub x: i8,
    pub lock_y: i8,
}

#[derive(Clone, Copy)]
pub(super) struct RelationSubset {
    pub subset: u64,
    pub node_index: u32,
    pub board: ExtendedBoard,
    pub deleted_rows: u32,
    pub permitted_operations: u64,
}

pub(super) enum RelationSubsetStart {
    Exhausted,
    Incomplete,
    Terminal,
    Expand(RelationSubset),
}

pub(super) struct RelationMovement {
    pub reachable: bool,
    pub evidence: Option<ScoringLockEvidence>,
}

pub(super) struct CandidateRelationMode {
    pub geometric_only: bool,
    pub grounded: bool,
    pub visit_all_poses: bool,
    pub retain_all_build_poses: bool,
    pub capture_scoring: bool,
    pub early_b2b: bool,
    pub b2b_policy: Option<BackToBackEdgePolicy>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum RelationTraversalOutcome {
    Complete,
    Incomplete,
}

#[derive(Clone, Copy)]
pub(super) enum CandidateRelationCatalog<'a> {
    Compact(&'a GeometryCatalog),
    Extended(&'a ExtendedInverseCatalog),
}

impl<'a> CandidateRelationCatalog<'a> {
    fn width(self) -> u8 {
        match self {
            Self::Compact(catalog) => catalog.width(),
            Self::Extended(catalog) => catalog.width(),
        }
    }

    fn height(self) -> u8 {
        match self {
            Self::Compact(catalog) => catalog.height(),
            Self::Extended(catalog) => catalog.height(),
        }
    }

    fn piece(self, row: u32) -> PieceKind {
        match self {
            Self::Compact(catalog) => catalog.skeleton(row).piece,
            Self::Extended(catalog) => catalog.skeleton(row).piece,
        }
    }

    fn instantiations(self, row: u32, deleted: u32) -> RelationInstantiationIter<'a> {
        match self {
            Self::Compact(catalog) => {
                RelationInstantiationIter::Compact(catalog.instantiations(row, deleted as u16))
            }
            Self::Extended(catalog) => {
                RelationInstantiationIter::Extended(catalog.instantiations(row, deleted))
            }
        }
    }
}

enum RelationInstantiationIter<'a> {
    Compact(InstantiatedRealizationIter<'a>),
    Extended(ExtendedInstantiationIter<'a>),
}

impl Iterator for RelationInstantiationIter<'_> {
    type Item = RelationInstantiation;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Compact(realizations) => realizations.next().map(|pose| RelationInstantiation {
                lock_mask: ExtendedBoard::from_words([pose.lock_mask, 0, 0, 0]),
                rotation: pose.rotation,
                x: pose.x,
                lock_y: pose.lock_y,
            }),
            Self::Extended(realizations) => realizations.next().map(|pose| RelationInstantiation {
                lock_mask: pose.lock_mask,
                rotation: pose.rotation,
                x: pose.x,
                lock_y: pose.lock_y,
            }),
        }
    }
}

/// Borrow existing graph arenas and exact caches. These variants select cell
/// and storage carriers, not a different subset traversal. Static enum dispatch
/// also avoids an indirect call for every concrete lock pose.
pub(super) enum CandidateRelationOwner<'owner, 'data> {
    Compact(&'owner mut CompactRelationAdapter<'data>),
    Extended(&'owner mut ExtendedRelationAdapter<'data>),
}

macro_rules! forward_owner_methods {
    ($($name:ident($($arg:ident: $ty:ty),* $(,)?) -> $result:ty;)+) => {
        impl CandidateRelationOwner<'_, '_> {
            $(fn $name(&mut self, $($arg: $ty),*) -> $result {
                match self {
                    Self::Compact(owner) => owner.$name($($arg),*),
                    Self::Extended(owner) => owner.$name($($arg),*),
                }
            })+
        }
    };
}

forward_owner_methods! {
    begin_subset(cursor: usize) -> Result<RelationSubsetStart, WasmExactSearchError>;
    begin_operation(state: RelationSubset, operation: usize, child: u64) -> Result<bool, WasmExactSearchError>;
    prepare_realizations(state: RelationSubset, piece: PieceKind) -> ();
    projected_state(child: u64) -> (ExtendedBoard, u32);
    confirm_transition(child: u64, board: ExtendedBoard, deleted: u32) -> bool;
    admit_child(child: u64) -> bool;
    known_reachable(state: RelationSubset, piece: PieceKind, realization: RelationInstantiation) -> Option<bool>;
    movement(state: RelationSubset, piece: PieceKind, realization: RelationInstantiation, capture_scoring: bool) -> RelationMovement;
    child_node_index(child: u64) -> Result<u32, WasmExactSearchError>;
    emit_edge(child: u64, edge: BuildEdge, scoring: Option<ScoringExecutionEdge>, retain_build_edge: bool) -> Result<(), WasmExactSearchError>;
    finish_subset(state: RelationSubset) -> Result<(), WasmExactSearchError>;
}

/// One exact candidate relation traversal. Pose multiplicity and scoring are
/// requested-output policies; the field size never chooses another algorithm.
pub(super) fn traverse_candidate_relation(
    owner: &mut CandidateRelationOwner<'_, '_>,
    catalog: CandidateRelationCatalog<'_>,
    rows: &[u32],
    mode: CandidateRelationMode,
) -> Result<RelationTraversalOutcome, WasmExactSearchError> {
    let width = catalog.width();
    let height = catalog.height();
    let mut cursor = 0;
    loop {
        let state = match owner.begin_subset(cursor)? {
            RelationSubsetStart::Exhausted => return Ok(RelationTraversalOutcome::Complete),
            RelationSubsetStart::Incomplete => return Ok(RelationTraversalOutcome::Incomplete),
            RelationSubsetStart::Terminal => {
                cursor += 1;
                continue;
            }
            RelationSubsetStart::Expand(state) => state,
        };
        cursor += 1;
        let mut pending = state.permitted_operations;
        while pending != 0 {
            let operation = pending.trailing_zeros() as usize;
            pending &= pending - 1;
            let child = state.subset | (1_u64 << operation);
            if !owner.begin_operation(state, operation, child)? {
                continue;
            }
            let row = rows[operation];
            let piece = catalog.piece(row);
            if mode.early_b2b {
                if let Some(policy) = mode.b2b_policy {
                    let (child_board, child_deleted) = owner.projected_state(child);
                    let lines = (child_deleted & !state.deleted_rows).count_ones() as u8;
                    if policy.before_spin_evidence(
                        piece,
                        lines,
                        lines > 0 && child_board.is_empty(),
                    ) == BeforeSpinEvidenceDecision::Rejects
                    {
                        continue;
                    }
                }
            }
            owner.prepare_realizations(state, piece);
            let mut build_edge_added = false;
            let mut child_admitted = false;
            for pose in catalog.instantiations(row, state.deleted_rows) {
                let known_reachable = owner.known_reachable(state, piece, pose);
                if (!mode.geometric_only && known_reachable == Some(false))
                    || state.board.intersects(pose.lock_mask)
                    || (mode.grounded
                        && !placement_is_grounded(width, height, state.board, pose.lock_mask))
                {
                    continue;
                }
                let (next_board, physical_clears, cleared_lines) =
                    place_and_clear(width, height, state.board.union(pose.lock_mask));
                let Some(next_deleted) =
                    merge_deleted_rows(height, state.deleted_rows, physical_clears)
                else {
                    continue;
                };
                if !owner.confirm_transition(child, next_board, next_deleted) {
                    continue;
                }
                let b2b_decision = mode.b2b_policy.map(|policy| {
                    policy.before_spin_evidence(
                        piece,
                        cleared_lines,
                        cleared_lines > 0 && next_board.is_empty(),
                    )
                });
                if mode.early_b2b && b2b_decision == Some(BeforeSpinEvidenceDecision::Rejects) {
                    continue;
                }
                let needs_b2b_evidence = b2b_decision
                    == Some(BeforeSpinEvidenceDecision::RequiresSpinEvidence)
                    || (!mode.early_b2b
                        && b2b_decision == Some(BeforeSpinEvidenceDecision::Rejects));
                let capture_evidence = mode.capture_scoring || needs_b2b_evidence;
                if !child_admitted {
                    if !owner.admit_child(child) {
                        break;
                    }
                    child_admitted = true;
                }
                let movement = if !mode.geometric_only || capture_evidence {
                    owner.movement(state, piece, pose, capture_evidence)
                } else {
                    RelationMovement {
                        reachable: false,
                        evidence: None,
                    }
                };
                if !mode.geometric_only && !movement.reachable {
                    continue;
                }
                let child_node = owner.child_node_index(child)?;
                let scoring = if capture_evidence && movement.reachable {
                    let (corners, front) = if piece == PieceKind::T {
                        t_corner_evidence(
                            width,
                            height,
                            state.board,
                            pose.rotation,
                            pose.x,
                            pose.lock_y,
                        )
                    } else {
                        (0, 0)
                    };
                    Some(
                        ScoringExecutionEdge::new(
                            child_node,
                            operation as u8,
                            piece,
                            pose.rotation,
                            pose.x,
                            pose.lock_y,
                            cleared_lines,
                            corners,
                            front,
                            movement
                                .evidence
                                .expect("scoring query retains exact lock evidence"),
                        )
                        .with_perfect_clear(cleared_lines > 0 && next_board.is_empty()),
                    )
                } else {
                    None
                };
                if needs_b2b_evidence
                    && mode
                        .b2b_policy
                        .is_some_and(|policy| scoring.is_none_or(|edge| !policy.allows(edge)))
                {
                    continue;
                }
                let retain_build = mode.retain_all_build_poses || !build_edge_added;
                owner.emit_edge(
                    child,
                    BuildOrderGraph::edge(
                        child_node,
                        operation as u8,
                        piece,
                        pose.rotation,
                        pose.x,
                        pose.lock_y,
                        cleared_lines,
                    ),
                    scoring.filter(|_| mode.capture_scoring),
                    retain_build,
                )?;
                build_edge_added |= retain_build;
                if !mode.visit_all_poses {
                    break;
                }
            }
        }
        owner.finish_subset(state)?;
    }
}

fn placement_is_grounded(
    width: u8,
    height: u8,
    board: ExtendedBoard,
    cells: ExtendedBoard,
) -> bool {
    let board_words = board.words();
    let cell_words = cells.words();
    let floor = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    if cell_words[0] & floor != 0 {
        return true;
    }
    if usize::from(width) * usize::from(height) <= 64 {
        return width < 64 && board_words[0] & (cell_words[0] >> width) != 0;
    }
    for word in 0..4 {
        let below = if width == 64 {
            cell_words.get(word + 1).copied().unwrap_or(0)
        } else {
            (cell_words[word] >> width)
                | (cell_words.get(word + 1).copied().unwrap_or(0) << (64 - width))
        };
        if board_words[word] & below != 0 {
            return true;
        }
    }
    false
}

fn t_corner_evidence(
    width: u8,
    height: u8,
    board: ExtendedBoard,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> (u8, u8) {
    let x = i32::from(x);
    let y = i32::from(y);
    let (center_x, center_y) = match rotation.quarter_turns() {
        0 => (x + 1, y),
        1 => (x, y + 1),
        2 | 3 => (x + 1, y + 1),
        _ => return (0, 0),
    };
    let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
    let front = match rotation.quarter_turns() {
        0 => [(-1, 1), (1, 1)],
        1 => [(1, -1), (1, 1)],
        2 => [(-1, -1), (1, -1)],
        3 => [(-1, -1), (-1, 1)],
        _ => return (0, 0),
    };
    let blocked = |(dx, dy): (i32, i32)| {
        let cell_x = center_x + dx;
        let cell_y = center_y + dy;
        if cell_x < 0 || cell_y < 0 || cell_x >= i32::from(width) {
            return true;
        }
        cell_y < i32::from(height) && board.contains((cell_y * i32::from(width) + cell_x) as u16)
    };
    (
        corners
            .into_iter()
            .filter(|corner| blocked(*corner))
            .count() as u8,
        front.into_iter().filter(|corner| blocked(*corner)).count() as u8,
    )
}
