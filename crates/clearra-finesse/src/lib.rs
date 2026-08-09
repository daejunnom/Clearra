//! Deterministic classic-input finesse costs from an exact spawn pose.
//!
//! The default multi-target search stores costs only. A witness is produced by
//! rerunning the same FIFO search for one selected target, keeping the hot path
//! free of predecessor and action-path storage.
//! SRP rationale: this module has one behavior-level change reason: computing and replaying deterministic classic-input movement routes.

use std::{error::Error, fmt};

use clearra_core_domain::{
    board::{
        board_size::BoardSize,
        standard_pc_board::{Board256Mask, StandardPcBoard},
    },
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_geometry::layout::{
    board128_layout::Board128Layout, board256_layout::Board256Layout, board64_layout::Board64Layout,
};
use clearra_piece_registry::{
    registry::piece_registry::PieceRotationShape,
    standard::tetromino_registry::standard_tetromino_registry,
};
use clearra_rules::{
    kicks::{KickTableProfile, KickTransition},
    spawn::SpawnProfile,
};

mod language;

pub use language::{
    aggregate_overall_costs, aggregate_unique_queue_costs, union_costed_geometry_languages,
    CostVector64, CostVectorStorageKind, CostedGeometryEdge, CostedGeometryLanguage,
    FinesseRouteWitnessError, FinesseSequenceInput, FixedQueueRouteStep, FixedQueueWitness,
    GeometryActionKey, GeometryLanguageError, GeometryLanguageNode, GeometryNodeId,
    OracleQueueEvaluation, QueueClass, QueueClassId, QueueClassProductEvaluator, QueueClassSet,
    QueueCostAggregation, QueueCostTable, QueuePattern, QueueSupplyAction, QueueUniverseMetadata,
    ReplayedFixedQueueWitness, VisibleSevenEvaluation,
};

const UNREACHED: u32 = u32::MAX;
const NO_TARGET: usize = usize::MAX;

/// One charged classic input.
///
/// `HardDrop` is a terminal input. It is evaluated when a state leaves the
/// FIFO queue and is therefore deliberately absent from `EXPANSION_ORDER`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClassicInputAction {
    TapLeft,
    TapRight,
    DasLeft,
    DasRight,
    RotateClockwise,
    RotateCounterClockwise,
    Rotate180,
    SoftDrop,
    HardDrop,
}

impl ClassicInputAction {
    /// Stable tie-breaking order for non-terminal successors.
    pub const EXPANSION_ORDER: [Self; 8] = [
        Self::TapLeft,
        Self::TapRight,
        Self::DasLeft,
        Self::DasRight,
        Self::RotateClockwise,
        Self::RotateCounterClockwise,
        Self::Rotate180,
        Self::SoftDrop,
    ];

    const NON_ROTATION_EXPANSION_ORDER: [Self; 5] = [
        Self::TapLeft,
        Self::TapRight,
        Self::DasLeft,
        Self::DasRight,
        Self::SoftDrop,
    ];

    fn rotation_target(self, from: RotationState) -> Option<RotationState> {
        match self {
            Self::RotateClockwise => Some(from.clockwise()),
            Self::RotateCounterClockwise => Some(from.counter_clockwise()),
            Self::Rotate180 => Some(from.rotated_180()),
            _ => None,
        }
    }
}

/// A normalized piece pose. `x` and `y` are the lower-left coordinates used by
/// the standard piece registry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PiecePose {
    pub rotation: RotationState,
    pub x: i16,
    pub y: i16,
}

impl PiecePose {
    pub const fn new(rotation: RotationState, x: i16, y: i16) -> Self {
        Self { rotation, x, y }
    }
}

/// A lock pose requested by a geometry search.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FinesseTarget {
    pose: PiecePose,
}

impl FinesseTarget {
    pub const fn new(rotation: RotationState, x: i16, y: i16) -> Self {
        Self {
            pose: PiecePose::new(rotation, x, y),
        }
    }

    pub const fn pose(self) -> PiecePose {
        self.pose
    }
}

/// Immutable board input. Bits outside the layout are rejected at creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinesseBoard {
    size: BoardSize,
    occupied: Board256Mask,
}

impl FinesseBoard {
    pub fn new(layout: Board64Layout, occupied: u64) -> Result<Self, FinesseError> {
        Self::from_parts(layout.size(), Board256Mask::from_words([occupied, 0, 0, 0]))
    }

    pub fn from_board128(layout: Board128Layout, occupied: u128) -> Result<Self, FinesseError> {
        Self::from_parts(
            layout.size(),
            Board256Mask::from_words([occupied as u64, (occupied >> 64) as u64, 0, 0]),
        )
    }

    pub fn from_board256(
        layout: Board256Layout,
        occupied: Board256Mask,
    ) -> Result<Self, FinesseError> {
        Self::from_parts(layout.size(), occupied)
    }

    pub fn from_standard_pc(board: StandardPcBoard) -> Self {
        let size = BoardSize::new(board.width(), u16::from(board.lines()))
            .expect("validated standard PC board has a non-zero size");
        Self {
            size,
            occupied: Board256Mask::from_words(board.occupied().words()),
        }
    }

    pub const fn size(self) -> BoardSize {
        self.size
    }

    pub fn width(self) -> u16 {
        self.size.width()
    }

    pub fn height(self) -> u16 {
        self.size.height()
    }

    pub const fn occupied(self) -> Board256Mask {
        self.occupied
    }

    fn from_parts(size: BoardSize, occupied: Board256Mask) -> Result<Self, FinesseError> {
        if !occupied
            .fits_cell_count(size.area() as u16)
            .map_err(|_| FinesseError::BoardTooLarge { area: size.area() })?
        {
            return Err(FinesseError::OccupiedOutsideLayout);
        }
        Ok(Self { size, occupied })
    }
}

/// Frozen board, piece, rules, and target order for one multi-target query.
///
/// Target order is preserved exactly, including duplicates. No caller-owned
/// slice or kick profile is borrowed after construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenFinesseQuery {
    board: FinesseBoard,
    piece: PieceKind,
    spawn: SpawnProfile,
    kicks: KickTableProfile,
    targets: Box<[FinesseTarget]>,
}

impl FrozenFinesseQuery {
    pub fn new(
        board: FinesseBoard,
        piece: PieceKind,
        spawn: SpawnProfile,
        kicks: KickTableProfile,
        targets: impl AsRef<[FinesseTarget]>,
    ) -> Self {
        Self {
            board,
            piece,
            spawn,
            kicks,
            targets: targets.as_ref().to_vec().into_boxed_slice(),
        }
    }

    pub const fn board(&self) -> FinesseBoard {
        self.board
    }

    pub const fn piece(&self) -> PieceKind {
        self.piece
    }

    pub const fn spawn(&self) -> SpawnProfile {
        self.spawn
    }

    pub fn kick_profile(&self) -> &KickTableProfile {
        &self.kicks
    }

    pub fn targets(&self) -> &[FinesseTarget] {
        &self.targets
    }

    /// Run the default cost-only FIFO search for every frozen target.
    pub fn costs(&self) -> Result<FrozenFinesseCosts, FinesseError> {
        let Some(kernel) = self.search_kernel()? else {
            return Ok(FrozenFinesseCosts::unreachable(self.targets.len()));
        };
        kernel.costs(&self.targets)
    }

    /// Run the coarse evidence-class product search.
    ///
    /// Unlike `costs`, this deliberately keeps one first arrival for each
    /// `(pose, terminal evidence class)` pair. It is suitable for presenting
    /// route alternatives, but exact spin/B2B scoring must use
    /// `costs_for_terminal_evidence` with the scorer's full signature.
    pub fn route_labels(&self) -> Result<FrozenRouteLabels, FinesseError> {
        let Some(kernel) = self.search_kernel()? else {
            return Ok(FrozenRouteLabels::unreachable(self.targets.len()));
        };
        kernel.route_labels(&self.targets)
    }

    /// Return the minimum cost whose terminal lock evidence exactly matches
    /// the supplied scoring signature. This deliberately ignores a faster
    /// route to the same pose when that route ends with different evidence.
    pub fn cost_for_terminal_evidence(
        &self,
        target_index: usize,
        evidence: TerminalEvidenceLabel,
    ) -> Result<Option<u32>, FinesseError> {
        let Some(&target) = self.targets.get(target_index) else {
            return Err(FinesseError::TargetIndexOutOfBounds {
                index: target_index,
                len: self.targets.len(),
            });
        };
        let Some(kernel) = self.search_kernel()? else {
            return Ok(None);
        };
        kernel.cost_for_terminal_evidence(target, evidence)
    }

    /// Resolve one terminal-evidence requirement per frozen target with a
    /// single board+piece FIFO traversal.
    ///
    /// `Some(signature)` requires an exact scoring signature. `None` leaves
    /// that target unconstrained and returns its ordinary minimum input cost.
    pub fn costs_for_terminal_evidence(
        &self,
        evidence: &[Option<TerminalEvidenceLabel>],
    ) -> Result<FrozenFinesseCosts, FinesseError> {
        if evidence.len() != self.targets.len() {
            return Err(FinesseError::TargetEvidenceLengthMismatch {
                targets: self.targets.len(),
                evidence: evidence.len(),
            });
        }
        let Some(kernel) = self.search_kernel()? else {
            return Ok(FrozenFinesseCosts::unreachable(self.targets.len()));
        };
        kernel.costs_for_terminal_evidence(&self.targets, evidence)
    }

    /// Rerun the FIFO search with parents for one selected target only.
    pub fn witness(&self, target_index: usize) -> Result<Option<FinesseWitness>, FinesseError> {
        self.witness_with_cancel(target_index, || false)
    }

    /// Rerun the FIFO search for one selected target while allowing the
    /// caller to stop the parent-retaining traversal.
    pub fn witness_with_cancel(
        &self,
        target_index: usize,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        if is_cancelled() {
            return Err(FinesseError::Cancelled);
        }
        let Some(&target) = self.targets.get(target_index) else {
            return Err(FinesseError::TargetIndexOutOfBounds {
                index: target_index,
                len: self.targets.len(),
            });
        };
        let Some(kernel) = self.search_kernel()? else {
            return Ok(None);
        };
        kernel.witness_with_cancel(target, &mut is_cancelled)
    }

    /// Rerun one selected route while requiring an exact terminal-evidence
    /// signature. This is used only after cost-only policy selection.
    pub fn witness_for_terminal_evidence(
        &self,
        target_index: usize,
        evidence: TerminalEvidenceLabel,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        self.witness_for_terminal_evidence_with_cancel(target_index, evidence, || false)
    }

    /// Rerun one selected evidence-constrained route while allowing the
    /// caller to stop the parent-retaining traversal.
    pub fn witness_for_terminal_evidence_with_cancel(
        &self,
        target_index: usize,
        evidence: TerminalEvidenceLabel,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        if is_cancelled() {
            return Err(FinesseError::Cancelled);
        }
        let Some(&target) = self.targets.get(target_index) else {
            return Err(FinesseError::TargetIndexOutOfBounds {
                index: target_index,
                len: self.targets.len(),
            });
        };
        let Some(kernel) = self.search_kernel()? else {
            return Ok(None);
        };
        kernel.witness_for_terminal_evidence_with_cancel(target, evidence, &mut is_cancelled)
    }

    fn search_kernel(&self) -> Result<Option<SearchKernel<'_>>, FinesseError> {
        match SearchKernel::new(self) {
            Ok(kernel) => Ok(Some(kernel)),
            Err(FinesseError::SpawnBlocked(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// Costs aligned one-for-one with `FrozenFinesseQuery::targets`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenFinesseCosts {
    costs: Box<[Option<u32>]>,
}

/// Coarse terminal evidence identity required by spin/B2B aggregation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalEvidenceClass {
    NoRotation,
    Rotation,
    FinalKickOverride,
}

impl TerminalEvidenceClass {
    const COUNT: usize = 3;

    const ALL: [Self; Self::COUNT] = [Self::NoRotation, Self::Rotation, Self::FinalKickOverride];

    const fn index(self) -> usize {
        match self {
            Self::NoRotation => 0,
            Self::Rotation => 1,
            Self::FinalKickOverride => 2,
        }
    }
}

impl FrozenFinesseCosts {
    fn unreachable(target_count: usize) -> Self {
        Self {
            costs: vec![None; target_count].into_boxed_slice(),
        }
    }

    pub fn get(&self, target_index: usize) -> Option<Option<u32>> {
        self.costs.get(target_index).copied()
    }

    pub fn as_slice(&self) -> &[Option<u32>] {
        &self.costs
    }

    pub fn into_boxed_slice(self) -> Box<[Option<u32>]> {
        self.costs
    }
}

/// Evidence supplied by the last non-locking input at the lock pose.
///
/// A hard drop that moves at least one row clears rotation evidence. When it
/// moves zero rows, a terminal rotation retains the first successful kick.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalEvidenceLabel {
    NoRotation,
    Rotation {
        from: RotationState,
        to: RotationState,
        request: ClassicInputAction,
        kick_index: u8,
        kick_dx: i8,
        kick_dy: i8,
        predecessor: PiecePose,
    },
}

impl TerminalEvidenceLabel {
    pub const fn class(self) -> TerminalEvidenceClass {
        match self {
            Self::NoRotation => TerminalEvidenceClass::NoRotation,
            Self::Rotation {
                request:
                    ClassicInputAction::RotateClockwise | ClassicInputAction::RotateCounterClockwise,
                kick_index: 4,
                ..
            } => TerminalEvidenceClass::FinalKickOverride,
            Self::Rotation { .. } => TerminalEvidenceClass::Rotation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LabeledFinesseCost {
    pub cost: u32,
    pub terminal_evidence: TerminalEvidenceLabel,
}

/// Per-target first cost for each evidence class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetRouteLabels {
    labels: [Option<LabeledFinesseCost>; TerminalEvidenceClass::COUNT],
}

impl TargetRouteLabels {
    pub fn get(&self, class: TerminalEvidenceClass) -> Option<LabeledFinesseCost> {
        self.labels[class.index()]
    }

    pub fn iter(&self) -> impl Iterator<Item = (TerminalEvidenceClass, LabeledFinesseCost)> + '_ {
        TerminalEvidenceClass::ALL
            .into_iter()
            .filter_map(|class| self.get(class).map(|cost| (class, cost)))
    }
}

/// Evidence-preserving results aligned with the frozen target order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenRouteLabels {
    targets: Box<[TargetRouteLabels]>,
}

impl FrozenRouteLabels {
    fn unreachable(target_count: usize) -> Self {
        Self {
            targets: vec![
                TargetRouteLabels {
                    labels: [None; TerminalEvidenceClass::COUNT],
                };
                target_count
            ]
            .into_boxed_slice(),
        }
    }

    pub fn get(&self, target_index: usize) -> Option<&TargetRouteLabels> {
        self.targets.get(target_index)
    }

    pub fn as_slice(&self) -> &[TargetRouteLabels] {
        &self.targets
    }
}

/// A deterministic minimum-input path for one target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinesseWitness {
    pub target: FinesseTarget,
    pub cost: u32,
    pub actions: Box<[ClassicInputAction]>,
    pub terminal_evidence: TerminalEvidenceLabel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinesseError {
    BoardTooLarge { area: u32 },
    OccupiedOutsideLayout,
    MissingStandardPiece(PieceKind),
    NegativeSpawn { x: i16, y: i16 },
    SpawnOutsideSearchSpace(PiecePose),
    SpawnBlocked(PiecePose),
    StateSpaceTooLarge,
    CostOverflow,
    TargetIndexOutOfBounds { index: usize, len: usize },
    TargetEvidenceLengthMismatch { targets: usize, evidence: usize },
    Cancelled,
}

impl fmt::Display for FinesseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardTooLarge { area } => {
                write!(formatter, "finesse board area {area} exceeds 256 cells")
            }
            Self::OccupiedOutsideLayout => {
                formatter.write_str("occupied bits outside the board layout")
            }
            Self::MissingStandardPiece(piece) => {
                write!(formatter, "missing standard piece definition for {piece:?}")
            }
            Self::NegativeSpawn { x, y } => {
                write!(formatter, "spawn has a negative coordinate: ({x}, {y})")
            }
            Self::SpawnOutsideSearchSpace(pose) => {
                write!(
                    formatter,
                    "spawn is outside the compiled state space: {pose:?}"
                )
            }
            Self::SpawnBlocked(pose) => write!(formatter, "spawn pose is blocked: {pose:?}"),
            Self::StateSpaceTooLarge => formatter.write_str("finesse state space is too large"),
            Self::CostOverflow => formatter.write_str("finesse input cost overflowed u32"),
            Self::TargetIndexOutOfBounds { index, len } => {
                write!(formatter, "target index {index} is outside 0..{len}")
            }
            Self::TargetEvidenceLengthMismatch { targets, evidence } => write!(
                formatter,
                "target/evidence length mismatch: {targets} targets, {evidence} signatures"
            ),
            Self::Cancelled => formatter.write_str("finesse search cancelled"),
        }
    }
}

impl Error for FinesseError {}

#[derive(Clone, Copy, Debug)]
struct RotationArrival {
    from: RotationState,
    to: RotationState,
    request: ClassicInputAction,
    kick_index: u8,
    kick_dx: i8,
    kick_dy: i8,
    predecessor: PiecePose,
}

impl RotationArrival {
    const fn label(self) -> TerminalEvidenceLabel {
        TerminalEvidenceLabel::Rotation {
            from: self.from,
            to: self.to,
            request: self.request,
            kick_index: self.kick_index,
            kick_dx: self.kick_dx,
            kick_dy: self.kick_dy,
            predecessor: self.predecessor,
        }
    }

    const fn class(self) -> TerminalEvidenceClass {
        self.label().class()
    }
}

#[derive(Clone, Copy, Debug)]
struct Successor {
    pose: PiecePose,
    rotation: Option<RotationArrival>,
}

#[derive(Clone, Copy, Debug)]
struct Parent {
    previous: usize,
    action: ClassicInputAction,
    rotation: Option<RotationArrival>,
}

#[derive(Clone, Copy, Debug)]
struct LabeledState {
    pose: PiecePose,
    class: TerminalEvidenceClass,
}

struct SearchKernel<'a> {
    board: FinesseBoard,
    piece: PieceKind,
    kicks: &'a KickTableProfile,
    shapes: [PieceRotationShape; 4],
    spawn: PiecePose,
    ceiling: i16,
    interaction_ceiling: i16,
    state_count: usize,
}

impl<'a> SearchKernel<'a> {
    fn new(query: &'a FrozenFinesseQuery) -> Result<Self, FinesseError> {
        let definition = standard_tetromino_registry()
            .get(query.piece)
            .ok_or(FinesseError::MissingStandardPiece(query.piece))?;
        let spawn = PiecePose::new(RotationState::Zero, query.spawn.x(), query.spawn.y());
        if spawn.x < 0 || spawn.y < 0 {
            return Err(FinesseError::NegativeSpawn {
                x: spawn.x,
                y: spawn.y,
            });
        }
        let ceiling = source_ceiling(query.board.height(), query.piece, spawn.y, &query.kicks);
        let interaction_ceiling =
            soft_drop_interaction_ceiling(query.board, query.piece, &query.kicks).min(ceiling);
        let state_count = 4_usize
            .checked_mul(usize::from(ceiling as u16).saturating_add(1))
            .and_then(|count| count.checked_mul(usize::from(query.board.width())))
            .ok_or(FinesseError::StateSpaceTooLarge)?;
        let kernel = Self {
            board: query.board,
            piece: query.piece,
            kicks: &query.kicks,
            shapes: definition.rotations(),
            spawn,
            ceiling,
            interaction_ceiling,
            state_count,
        };
        let Some(_) = kernel.state_index(spawn) else {
            return Err(FinesseError::SpawnOutsideSearchSpace(spawn));
        };
        if !kernel.placeable(spawn) {
            return Err(FinesseError::SpawnBlocked(spawn));
        }
        Ok(kernel)
    }

    fn costs(&self, targets: &[FinesseTarget]) -> Result<FrozenFinesseCosts, FinesseError> {
        let mut output = vec![None; targets.len()];
        if targets.is_empty() {
            return Ok(FrozenFinesseCosts {
                costs: output.into_boxed_slice(),
            });
        }

        let (target_heads, target_next, mut remaining) = self.compile_targets(targets);
        if remaining == 0 {
            return Ok(FrozenFinesseCosts {
                costs: output.into_boxed_slice(),
            });
        }

        let mut distance = vec![UNREACHED; self.state_count];
        let mut queue = Vec::with_capacity(self.state_count.min(4096));
        let spawn_index = self
            .state_index(self.spawn)
            .expect("validated spawn belongs to the state space");
        distance[spawn_index] = 0;
        queue.push(self.spawn);

        let mut cursor = 0;
        while cursor < queue.len() && remaining != 0 {
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            let lock = self.hard_drop(source);
            if self.inside_layout(lock) {
                let lock_index = self
                    .state_index(lock)
                    .expect("hard-drop lock belongs to the state space");
                let mut target = target_heads[lock_index];
                if target != NO_TARGET {
                    let cost = distance[source_index]
                        .checked_add(1)
                        .ok_or(FinesseError::CostOverflow)?;
                    while target != NO_TARGET {
                        if output[target].is_none() {
                            output[target] = Some(cost);
                            remaining -= 1;
                        }
                        target = target_next[target];
                    }
                }
            }

            let next_distance = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source, action, |successor| {
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    // First enqueue wins. Equal-cost paths are never compared or replaced.
                    if distance[successor_index] == UNREACHED {
                        distance[successor_index] = next_distance;
                        queue.push(successor.pose);
                    }
                });
            }
        }

        Ok(FrozenFinesseCosts {
            costs: output.into_boxed_slice(),
        })
    }

    fn route_labels(&self, targets: &[FinesseTarget]) -> Result<FrozenRouteLabels, FinesseError> {
        let empty = || TargetRouteLabels {
            labels: [None; TerminalEvidenceClass::COUNT],
        };
        let mut output = (0..targets.len()).map(|_| empty()).collect::<Vec<_>>();
        if targets.is_empty() {
            return Ok(FrozenRouteLabels {
                targets: output.into_boxed_slice(),
            });
        }

        let (target_heads, target_next, mapped_targets) = self.compile_targets(targets);
        if mapped_targets == 0 {
            return Ok(FrozenRouteLabels {
                targets: output.into_boxed_slice(),
            });
        }

        let product_count = self
            .state_count
            .checked_mul(TerminalEvidenceClass::COUNT)
            .ok_or(FinesseError::StateSpaceTooLarge)?;
        let mut distance = vec![UNREACHED; product_count];
        let mut arrival: Vec<Option<RotationArrival>> = vec![None; product_count];
        let mut queue = Vec::with_capacity(product_count.min(8192));
        let spawn = LabeledState {
            pose: self.spawn,
            class: TerminalEvidenceClass::NoRotation,
        };
        let spawn_index = self
            .labeled_state_index(spawn)
            .expect("validated spawn belongs to the product state space");
        distance[spawn_index] = 0;
        queue.push(spawn);

        let mut cursor = 0;
        while cursor < queue.len() {
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .labeled_state_index(source)
                .expect("queued state belongs to the product state space");
            let lock = self.hard_drop(source.pose);
            if self.inside_layout(lock) {
                let lock_index = self
                    .state_index(lock)
                    .expect("hard-drop lock belongs to the state space");
                let (lock_class, terminal_evidence) = if source.pose.y == lock.y {
                    match source.class {
                        TerminalEvidenceClass::NoRotation => (
                            TerminalEvidenceClass::NoRotation,
                            TerminalEvidenceLabel::NoRotation,
                        ),
                        TerminalEvidenceClass::Rotation
                        | TerminalEvidenceClass::FinalKickOverride => {
                            let evidence = arrival[source_index]
                                .expect("rotation-class state retains its first arrival")
                                .label();
                            (source.class, evidence)
                        }
                    }
                } else {
                    (
                        TerminalEvidenceClass::NoRotation,
                        TerminalEvidenceLabel::NoRotation,
                    )
                };
                let labeled_cost = LabeledFinesseCost {
                    cost: distance[source_index]
                        .checked_add(1)
                        .ok_or(FinesseError::CostOverflow)?,
                    terminal_evidence,
                };
                let mut target = target_heads[lock_index];
                while target != NO_TARGET {
                    // FIFO order finalizes the first cost in this evidence class.
                    let slot = &mut output[target].labels[lock_class.index()];
                    if slot.is_none() {
                        *slot = Some(labeled_cost);
                    }
                    target = target_next[target];
                }
            }

            let next_distance = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source.pose, action, |successor| {
                    let class = successor
                        .rotation
                        .map_or(TerminalEvidenceClass::NoRotation, RotationArrival::class);
                    let target = LabeledState {
                        pose: successor.pose,
                        class,
                    };
                    let target_index = self
                        .labeled_state_index(target)
                        .expect("successor belongs to the product state space");
                    // No path hashes and no same-class tie replacement.
                    if distance[target_index] == UNREACHED {
                        distance[target_index] = next_distance;
                        arrival[target_index] = successor.rotation;
                        queue.push(target);
                    }
                });
            }
        }

        Ok(FrozenRouteLabels {
            targets: output.into_boxed_slice(),
        })
    }

    fn witness_with_cancel(
        &self,
        target: FinesseTarget,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        let Some(target_index) = self.state_index(target.pose) else {
            return Ok(None);
        };
        if !self.inside_layout(target.pose) || !self.grounded(target.pose) {
            return Ok(None);
        }

        let mut distance = vec![UNREACHED; self.state_count];
        let mut parents = vec![None; self.state_count];
        let mut queue = Vec::with_capacity(self.state_count.min(4096));
        let spawn_index = self
            .state_index(self.spawn)
            .expect("validated spawn belongs to the state space");
        distance[spawn_index] = 0;
        queue.push(self.spawn);

        let mut cursor = 0;
        while cursor < queue.len() {
            if is_cancelled() {
                return Err(FinesseError::Cancelled);
            }
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            let lock = self.hard_drop(source);
            if lock == target.pose {
                let cost = distance[source_index]
                    .checked_add(1)
                    .ok_or(FinesseError::CostOverflow)?;
                let mut actions = reconstruct_actions(source_index, spawn_index, &parents);
                actions.push(ClassicInputAction::HardDrop);
                let terminal_evidence = if source.y == lock.y {
                    parents[source_index]
                        .and_then(|parent| parent.rotation)
                        .map_or(TerminalEvidenceLabel::NoRotation, RotationArrival::label)
                } else {
                    TerminalEvidenceLabel::NoRotation
                };
                debug_assert_eq!(cost as usize, actions.len());
                return Ok(Some(FinesseWitness {
                    target,
                    cost,
                    actions: actions.into_boxed_slice(),
                    terminal_evidence,
                }));
            }

            let next_distance = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source, action, |successor| {
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    if distance[successor_index] == UNREACHED {
                        distance[successor_index] = next_distance;
                        parents[successor_index] = Some(Parent {
                            previous: source_index,
                            action,
                            rotation: successor.rotation,
                        });
                        queue.push(successor.pose);
                    }
                });
            }
        }

        // A target state can be valid but not be the hard-drop lock of a reachable state.
        let _ = target_index;
        Ok(None)
    }

    fn cost_for_terminal_evidence(
        &self,
        target: FinesseTarget,
        evidence: TerminalEvidenceLabel,
    ) -> Result<Option<u32>, FinesseError> {
        self.costs_for_terminal_evidence(&[target], &[Some(evidence)])
            .map(|costs| costs.get(0).flatten())
    }

    fn witness_for_terminal_evidence_with_cancel(
        &self,
        target: FinesseTarget,
        evidence: TerminalEvidenceLabel,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        if evidence == TerminalEvidenceLabel::NoRotation {
            return self.witness_without_terminal_rotation_with_cancel(target, is_cancelled);
        }
        let TerminalEvidenceLabel::Rotation {
            from,
            to,
            request,
            kick_index,
            kick_dx,
            kick_dy,
            predecessor,
        } = evidence
        else {
            unreachable!("no-rotation evidence returned above")
        };
        if !self.inside_layout(target.pose)
            || !self.grounded(target.pose)
            || target.pose.rotation != to
            || predecessor.rotation != from
        {
            return Ok(None);
        }
        let Some(successor) = self.single_successor(predecessor, request) else {
            return Ok(None);
        };
        if successor.pose != target.pose
            || successor.rotation.map(RotationArrival::label) != Some(evidence)
        {
            return Ok(None);
        }
        let Some(predecessor_index) = self.state_index(predecessor) else {
            return Ok(None);
        };

        let mut distance = vec![UNREACHED; self.state_count];
        let mut parents = vec![None; self.state_count];
        let mut queue = Vec::with_capacity(self.state_count.min(4096));
        let spawn_index = self
            .state_index(self.spawn)
            .expect("validated spawn belongs to the state space");
        distance[spawn_index] = 0;
        queue.push(self.spawn);
        let mut cursor = 0;
        while cursor < queue.len() {
            if is_cancelled() {
                return Err(FinesseError::Cancelled);
            }
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            if source_index == predecessor_index {
                let mut actions = reconstruct_actions(source_index, spawn_index, &parents);
                actions.push(request);
                actions.push(ClassicInputAction::HardDrop);
                let cost = u32::try_from(actions.len()).map_err(|_| FinesseError::CostOverflow)?;
                return Ok(Some(FinesseWitness {
                    target,
                    cost,
                    actions: actions.into_boxed_slice(),
                    terminal_evidence: TerminalEvidenceLabel::Rotation {
                        from,
                        to,
                        request,
                        kick_index,
                        kick_dx,
                        kick_dy,
                        predecessor,
                    },
                }));
            }
            let next_distance = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source, action, |successor| {
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    if distance[successor_index] == UNREACHED {
                        distance[successor_index] = next_distance;
                        parents[successor_index] = Some(Parent {
                            previous: source_index,
                            action,
                            rotation: successor.rotation,
                        });
                        queue.push(successor.pose);
                    }
                });
            }
        }
        Ok(None)
    }

    fn witness_without_terminal_rotation_with_cancel(
        &self,
        target: FinesseTarget,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FinesseWitness>, FinesseError> {
        if !self.inside_layout(target.pose) || !self.grounded(target.pose) {
            return Ok(None);
        }
        let mut distance = vec![UNREACHED; self.state_count];
        let mut parents = vec![None; self.state_count];
        let mut queue = Vec::with_capacity(self.state_count.min(4096));
        let spawn_index = self
            .state_index(self.spawn)
            .expect("validated spawn belongs to the state space");
        distance[spawn_index] = 0;
        queue.push(self.spawn);

        // (cost, source before the optional final non-rotation action, action)
        let mut best =
            (self.spawn == target.pose).then_some((1_u32, spawn_index, None::<ClassicInputAction>));
        let mut cursor = 0;
        while cursor < queue.len() {
            if is_cancelled() {
                return Err(FinesseError::Cancelled);
            }
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            let source_distance = distance[source_index];
            let lock = self.hard_drop(source);
            if lock == target.pose && source.y != lock.y {
                let cost = source_distance
                    .checked_add(1)
                    .ok_or(FinesseError::CostOverflow)?;
                if best.is_none_or(|(current, _, _)| cost < current) {
                    best = Some((cost, source_index, None));
                }
            }

            let next_distance = source_distance
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            let grounded_successor_cost = next_distance
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source, action, |successor| {
                    if successor.rotation.is_none()
                        && successor.pose == target.pose
                        && best.is_none_or(|(current, _, _)| grounded_successor_cost < current)
                    {
                        best = Some((grounded_successor_cost, source_index, Some(action)));
                    }
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    if distance[successor_index] == UNREACHED {
                        distance[successor_index] = next_distance;
                        parents[successor_index] = Some(Parent {
                            previous: source_index,
                            action,
                            rotation: successor.rotation,
                        });
                        queue.push(successor.pose);
                    }
                });
            }
        }

        let Some((cost, source_index, terminal_action)) = best else {
            return Ok(None);
        };
        let mut actions = reconstruct_actions(source_index, spawn_index, &parents);
        if let Some(action) = terminal_action {
            actions.push(action);
        }
        actions.push(ClassicInputAction::HardDrop);
        if u32::try_from(actions.len()).map_err(|_| FinesseError::CostOverflow)? != cost {
            return Err(FinesseError::CostOverflow);
        }
        Ok(Some(FinesseWitness {
            target,
            cost,
            actions: actions.into_boxed_slice(),
            terminal_evidence: TerminalEvidenceLabel::NoRotation,
        }))
    }

    fn costs_for_terminal_evidence(
        &self,
        targets: &[FinesseTarget],
        evidence: &[Option<TerminalEvidenceLabel>],
    ) -> Result<FrozenFinesseCosts, FinesseError> {
        debug_assert_eq!(targets.len(), evidence.len());
        let mut output = vec![None; targets.len()];
        let (target_heads, target_next, mapped_targets) = self.compile_targets(targets);
        if mapped_targets == 0 {
            return Ok(FrozenFinesseCosts {
                costs: output.into_boxed_slice(),
            });
        }
        let (distance, queue) = self.movement_distances()?;

        // A descending hard drop erases prior rotation evidence. A zero-row
        // hard drop does not, so grounded targets require either the spawn
        // state itself or a final non-rotation movement input.
        for source in queue.iter().copied() {
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            let lock = self.hard_drop(source);
            if !self.inside_layout(lock) {
                continue;
            }
            let lock_index = self
                .state_index(lock)
                .expect("hard-drop lock belongs to the state space");
            let cost = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            let mut target_index = target_heads[lock_index];
            while target_index != NO_TARGET {
                let matches = evidence[target_index].is_none()
                    || (source.y != lock.y
                        && evidence[target_index] == Some(TerminalEvidenceLabel::NoRotation));
                if matches && output[target_index].is_none_or(|current| cost < current) {
                    output[target_index] = Some(cost);
                }
                target_index = target_next[target_index];
            }

            for action in ClassicInputAction::NON_ROTATION_EXPANSION_ORDER {
                let cost = distance[source_index]
                    .checked_add(2)
                    .ok_or(FinesseError::CostOverflow)?;
                self.for_each_successor(source, action, |successor| {
                    if !self.grounded(successor.pose) {
                        return;
                    }
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    let mut target_index = target_heads[successor_index];
                    while target_index != NO_TARGET {
                        if evidence[target_index] == Some(TerminalEvidenceLabel::NoRotation)
                            && output[target_index].is_none_or(|current| cost < current)
                        {
                            output[target_index] = Some(cost);
                        }
                        target_index = target_next[target_index];
                    }
                });
            }
        }
        if self.grounded(self.spawn) {
            let mut target_index = target_heads[self
                .state_index(self.spawn)
                .expect("validated spawn belongs to the state space")];
            while target_index != NO_TARGET {
                if evidence[target_index] == Some(TerminalEvidenceLabel::NoRotation) {
                    output[target_index] = Some(1);
                }
                target_index = target_next[target_index];
            }
        }

        for (index, (&target, &label)) in targets.iter().zip(evidence).enumerate() {
            let Some(label) = label else {
                continue;
            };
            let TerminalEvidenceLabel::Rotation {
                from,
                to,
                request,
                kick_index,
                kick_dx,
                kick_dy,
                predecessor,
            } = label
            else {
                continue;
            };
            if !self.inside_layout(target.pose)
                || !self.grounded(target.pose)
                || target.pose.rotation != to
                || predecessor.rotation != from
            {
                continue;
            }
            let Some(successor) = self.single_successor(predecessor, request) else {
                continue;
            };
            if successor.pose != target.pose
                || successor.rotation.map(RotationArrival::label)
                    != Some(TerminalEvidenceLabel::Rotation {
                        from,
                        to,
                        request,
                        kick_index,
                        kick_dx,
                        kick_dy,
                        predecessor,
                    })
            {
                continue;
            }
            let Some(predecessor_index) = self.state_index(predecessor) else {
                continue;
            };
            let predecessor_cost = distance[predecessor_index];
            if predecessor_cost != UNREACHED {
                output[index] = Some(
                    predecessor_cost
                        .checked_add(2)
                        .ok_or(FinesseError::CostOverflow)?,
                );
            }
        }
        Ok(FrozenFinesseCosts {
            costs: output.into_boxed_slice(),
        })
    }

    fn movement_distances(&self) -> Result<(Vec<u32>, Vec<PiecePose>), FinesseError> {
        let mut distance = vec![UNREACHED; self.state_count];
        let mut queue = Vec::with_capacity(self.state_count.min(4096));
        let spawn_index = self
            .state_index(self.spawn)
            .expect("validated spawn belongs to the state space");
        distance[spawn_index] = 0;
        queue.push(self.spawn);
        let mut cursor = 0;
        while cursor < queue.len() {
            let source = queue[cursor];
            cursor += 1;
            let source_index = self
                .state_index(source)
                .expect("queued state belongs to the state space");
            let next_distance = distance[source_index]
                .checked_add(1)
                .ok_or(FinesseError::CostOverflow)?;
            for action in ClassicInputAction::EXPANSION_ORDER {
                self.for_each_successor(source, action, |successor| {
                    let successor_index = self
                        .state_index(successor.pose)
                        .expect("successor belongs to the state space");
                    if distance[successor_index] == UNREACHED {
                        distance[successor_index] = next_distance;
                        queue.push(successor.pose);
                    }
                });
            }
        }
        Ok((distance, queue))
    }

    fn compile_targets(&self, targets: &[FinesseTarget]) -> (Vec<usize>, Vec<usize>, usize) {
        let mut heads = vec![NO_TARGET; self.state_count];
        let mut next = vec![NO_TARGET; targets.len()];
        let mut count = 0;
        for (target_index, target) in targets.iter().copied().enumerate().rev() {
            let Some(state_index) = self.state_index(target.pose) else {
                continue;
            };
            if !self.inside_layout(target.pose) || !self.grounded(target.pose) {
                continue;
            }
            next[target_index] = heads[state_index];
            heads[state_index] = target_index;
            count += 1;
        }
        (heads, next, count)
    }

    fn for_each_successor(
        &self,
        source: PiecePose,
        action: ClassicInputAction,
        mut visit: impl FnMut(Successor),
    ) {
        if action == ClassicInputAction::SoftDrop {
            let mut current = source;
            while let Some(successor) = self.translation(current, 0, -1) {
                current = successor.pose;
                // Empty sky is vertically translation-invariant. Stopping a
                // soft drop above every occupied cell (plus the largest
                // possible downward kick predecessor) cannot improve a route,
                // so do not enqueue those impossible lock-height states.
                if current.y <= self.interaction_ceiling {
                    visit(successor);
                }
            }
            return;
        }
        if let Some(successor) = self.single_successor(source, action) {
            visit(successor);
        }
    }

    fn single_successor(&self, source: PiecePose, action: ClassicInputAction) -> Option<Successor> {
        match action {
            ClassicInputAction::TapLeft => self.translation(source, -1, 0),
            ClassicInputAction::TapRight => self.translation(source, 1, 0),
            ClassicInputAction::DasLeft => self.das(source, -1),
            ClassicInputAction::DasRight => self.das(source, 1),
            ClassicInputAction::RotateClockwise
            | ClassicInputAction::RotateCounterClockwise
            | ClassicInputAction::Rotate180 => self.rotation(source, action),
            ClassicInputAction::SoftDrop | ClassicInputAction::HardDrop => None,
        }
    }

    fn translation(&self, source: PiecePose, dx: i16, dy: i16) -> Option<Successor> {
        let target = PiecePose::new(
            source.rotation,
            source.x.checked_add(dx)?,
            source.y.checked_add(dy)?,
        );
        self.placeable(target).then_some(Successor {
            pose: target,
            rotation: None,
        })
    }

    fn das(&self, source: PiecePose, dx: i16) -> Option<Successor> {
        let mut target = source;
        loop {
            let candidate = PiecePose::new(target.rotation, target.x.checked_add(dx)?, target.y);
            if !self.placeable(candidate) {
                break;
            }
            target = candidate;
        }
        (target != source).then_some(Successor {
            pose: target,
            rotation: None,
        })
    }

    fn rotation(&self, source: PiecePose, action: ClassicInputAction) -> Option<Successor> {
        let to = action.rotation_target(source.rotation)?;
        let sequence =
            self.kicks
                .sequence_for(KickTransition::new(self.piece, source.rotation, to))?;
        for (kick_index, offset) in sequence.offsets().iter().copied().enumerate() {
            let (dx, dy) =
                normalized_kick_delta(self.piece, source.rotation, to, offset.dx(), offset.dy());
            let target = PiecePose::new(
                to,
                source.x.checked_add(i16::from(dx))?,
                source.y.checked_add(i16::from(dy))?,
            );
            if self.placeable(target) {
                return Some(Successor {
                    pose: target,
                    rotation: Some(RotationArrival {
                        from: source.rotation,
                        to,
                        request: action,
                        kick_index: u8::try_from(kick_index).unwrap_or(u8::MAX),
                        // Scoring evidence stores the normalized pose
                        // displacement, not the raw table offset.
                        kick_dx: dx,
                        kick_dy: dy,
                        predecessor: source,
                    }),
                });
            }
        }
        None
    }

    fn hard_drop(&self, source: PiecePose) -> PiecePose {
        let mut lock = source;
        loop {
            let Some(y) = lock.y.checked_sub(1) else {
                return lock;
            };
            let below = PiecePose::new(lock.rotation, lock.x, y);
            if !self.placeable(below) {
                return lock;
            }
            lock = below;
        }
    }

    fn grounded(&self, source: PiecePose) -> bool {
        if !self.placeable(source) {
            return false;
        }
        let below = PiecePose::new(source.rotation, source.x, source.y.saturating_sub(1));
        source.y == 0 || !self.placeable(below)
    }

    fn placeable(&self, pose: PiecePose) -> bool {
        if self.state_index(pose).is_none() {
            return false;
        }
        let width = i16::try_from(self.board.width()).expect("finesse board width fits i16");
        let height = i16::try_from(self.board.height()).expect("finesse board height fits i16");
        for cell in self.shape(pose.rotation).cells() {
            let Some(x) = pose.x.checked_add(i16::from(cell.x())) else {
                return false;
            };
            let Some(y) = pose.y.checked_add(i16::from(cell.y())) else {
                return false;
            };
            if x < 0 || x >= width || y < 0 {
                return false;
            }
            if y < height {
                let bit = u32::try_from(y * width + x).expect("non-negative Board64 cell index");
                if self.board.occupied.contains_index(bit as u16) {
                    return false;
                }
            }
        }
        true
    }

    fn inside_layout(&self, pose: PiecePose) -> bool {
        let width = i16::try_from(self.board.width()).expect("finesse board width fits i16");
        let height = i16::try_from(self.board.height()).expect("finesse board height fits i16");
        self.shape(pose.rotation).cells().into_iter().all(|cell| {
            let Some(x) = pose.x.checked_add(i16::from(cell.x())) else {
                return false;
            };
            let Some(y) = pose.y.checked_add(i16::from(cell.y())) else {
                return false;
            };
            x >= 0 && x < width && y >= 0 && y < height
        })
    }

    fn shape(&self, rotation: RotationState) -> PieceRotationShape {
        self.shapes[usize::from(rotation.quarter_turns())]
    }

    fn state_index(&self, pose: PiecePose) -> Option<usize> {
        let width = usize::from(self.board.width());
        if pose.x < 0
            || usize::try_from(pose.x).ok()? >= width
            || pose.y < 0
            || pose.y > self.ceiling
        {
            return None;
        }
        Some(
            (usize::from(pose.rotation.quarter_turns()) * (usize::from(self.ceiling as u16) + 1)
                + usize::try_from(pose.y).ok()?)
                * width
                + usize::try_from(pose.x).ok()?,
        )
    }

    fn labeled_state_index(&self, state: LabeledState) -> Option<usize> {
        self.state_index(state.pose)?
            .checked_mul(TerminalEvidenceClass::COUNT)?
            .checked_add(state.class.index())
    }
}

fn reconstruct_actions(
    mut state: usize,
    spawn: usize,
    parents: &[Option<Parent>],
) -> Vec<ClassicInputAction> {
    let mut reversed = Vec::new();
    while state != spawn {
        let parent = parents[state].expect("every non-spawn witness state has a parent");
        reversed.push(parent.action);
        state = parent.previous;
    }
    reversed.reverse();
    reversed
}

fn source_ceiling(height: u16, piece: PieceKind, spawn_y: i16, profile: &KickTableProfile) -> i16 {
    let vertical_margin = profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .flat_map(|entry| {
            entry.sequence().offsets().iter().map(move |offset| {
                normalized_kick_delta(
                    piece,
                    entry.transition().from(),
                    entry.transition().to(),
                    offset.dx(),
                    offset.dy(),
                )
                .1
                .unsigned_abs()
            })
        })
        .max()
        .unwrap_or(0);
    let base = spawn_y.max(i16::try_from(height).expect("Board64 height fits i16"));
    base.saturating_add(i16::from(vertical_margin))
        .saturating_add(4)
}

fn soft_drop_interaction_ceiling(
    board: FinesseBoard,
    piece: PieceKind,
    profile: &KickTableProfile,
) -> i16 {
    let width = u32::from(board.width());
    let occupied_height = board
        .occupied()
        .words()
        .into_iter()
        .enumerate()
        .rev()
        .find_map(|(word_index, word)| {
            (word != 0).then(|| {
                let highest_bit = word_index as u32 * 64 + (63 - word.leading_zeros());
                i16::try_from(highest_bit / width + 1).expect("Board256 height fits i16")
            })
        })
        .unwrap_or(0);
    let downward_kick_margin = profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .flat_map(|entry| {
            entry.sequence().offsets().iter().map(move |offset| {
                normalized_kick_delta(
                    piece,
                    entry.transition().from(),
                    entry.transition().to(),
                    offset.dx(),
                    offset.dy(),
                )
                .1
            })
        })
        .filter_map(|dy| (dy < 0).then_some(i16::from(dy.unsigned_abs())))
        .max()
        .unwrap_or(0);
    occupied_height.saturating_add(downward_kick_margin)
}

fn normalized_kick_delta(
    piece: PieceKind,
    from: RotationState,
    to: RotationState,
    kick_dx: i8,
    kick_dy: i8,
) -> (i8, i8) {
    let (from_x, from_y) = normalized_rotation_center(piece, from);
    let (to_x, to_y) = normalized_rotation_center(piece, to);
    (kick_dx + from_x - to_x, kick_dy + from_y - to_y)
}

fn normalized_rotation_center(piece: PieceKind, rotation: RotationState) -> (i8, i8) {
    const JLSTZ: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, 1)];
    const I: [(i8, i8); 4] = [(0, 0), (-2, 2), (0, 1), (-1, 2)];
    let index = usize::from(rotation.quarter_turns());
    match piece {
        PieceKind::I => I[index],
        PieceKind::O => (0, 0),
        PieceKind::T | PieceKind::S | PieceKind::Z | PieceKind::J | PieceKind::L => JLSTZ[index],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use clearra_core_domain::{
        board::board_size::BoardSize, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::kicks::{NoKick, SrsKicks};

    use super::*;

    fn board(width: u16, height: u16, occupied: u64) -> FinesseBoard {
        let layout = Board64Layout::new(BoardSize::new(width, height).expect("test board size"))
            .expect("test Board64 layout");
        FinesseBoard::new(layout, occupied).expect("test board bits")
    }

    struct ReferenceSearch {
        locks: HashMap<PiecePose, (u32, PiecePose)>,
        parents: HashMap<PiecePose, (PiecePose, ClassicInputAction)>,
        spawn: PiecePose,
    }

    fn reference_search(
        width: u16,
        height: u16,
        occupied: u64,
        piece: PieceKind,
        spawn: PiecePose,
        kicks: &KickTableProfile,
    ) -> Option<ReferenceSearch> {
        let shapes = standard_tetromino_registry().get(piece)?.rotations();
        let ceiling = i16::try_from(height).ok()?.saturating_add(8);
        let placeable = |pose: PiecePose| {
            if pose.x < 0
                || pose.x >= i16::try_from(width).unwrap()
                || pose.y < 0
                || pose.y > ceiling
            {
                return false;
            }
            shapes[usize::from(pose.rotation.quarter_turns())]
                .cells()
                .iter()
                .copied()
                .all(|cell| {
                    let x = pose.x + i16::from(cell.x());
                    let y = pose.y + i16::from(cell.y());
                    if x < 0 || x >= i16::try_from(width).unwrap() || y < 0 {
                        return false;
                    }
                    y >= i16::try_from(height).unwrap()
                        || occupied & (1_u64 << (y as u32 * u32::from(width) + x as u32)) == 0
                })
        };
        if !placeable(spawn) {
            return None;
        }
        let hard_drop = |source: PiecePose| {
            let mut lock = source;
            loop {
                let below = PiecePose::new(lock.rotation, lock.x, lock.y - 1);
                if !placeable(below) {
                    return lock;
                }
                lock = below;
            }
        };
        let inside = |pose: PiecePose| {
            shapes[usize::from(pose.rotation.quarter_turns())]
                .cells()
                .iter()
                .copied()
                .all(|cell| {
                    let x = pose.x + i16::from(cell.x());
                    let y = pose.y + i16::from(cell.y());
                    x >= 0
                        && x < i16::try_from(width).unwrap()
                        && y >= 0
                        && y < i16::try_from(height).unwrap()
                })
        };
        let occupied_height = if occupied != 0 {
            let highest_bit = 63 - occupied.leading_zeros();
            i16::try_from(highest_bit / u32::from(width) + 1).unwrap()
        } else {
            0
        };
        let downward_kick_margin = kicks
            .entries()
            .iter()
            .filter(|entry| entry.transition().piece() == piece)
            .flat_map(|entry| {
                entry.sequence().offsets().iter().map(move |offset| {
                    reference_kick_delta(
                        piece,
                        entry.transition().from(),
                        entry.transition().to(),
                        offset.dx(),
                        offset.dy(),
                    )
                    .1
                })
            })
            .filter_map(|dy| (dy < 0).then_some(i16::from(dy.unsigned_abs())))
            .max()
            .unwrap_or(0);
        let interaction_ceiling = occupied_height.saturating_add(downward_kick_margin);
        let successor = |source: PiecePose, action: ClassicInputAction| {
            let translated = |dx: i16, dy: i16| {
                let target = PiecePose::new(source.rotation, source.x + dx, source.y + dy);
                placeable(target).then_some(target)
            };
            match action {
                ClassicInputAction::TapLeft => translated(-1, 0),
                ClassicInputAction::TapRight => translated(1, 0),
                ClassicInputAction::SoftDrop => None,
                ClassicInputAction::DasLeft | ClassicInputAction::DasRight => {
                    let dx = if action == ClassicInputAction::DasLeft {
                        -1
                    } else {
                        1
                    };
                    let mut target = source;
                    loop {
                        let next = PiecePose::new(target.rotation, target.x + dx, target.y);
                        if !placeable(next) {
                            break;
                        }
                        target = next;
                    }
                    (target != source).then_some(target)
                }
                ClassicInputAction::RotateClockwise
                | ClassicInputAction::RotateCounterClockwise
                | ClassicInputAction::Rotate180 => {
                    let to = match action {
                        ClassicInputAction::RotateClockwise => source.rotation.clockwise(),
                        ClassicInputAction::RotateCounterClockwise => {
                            source.rotation.counter_clockwise()
                        }
                        ClassicInputAction::Rotate180 => source.rotation.rotated_180(),
                        _ => unreachable!(),
                    };
                    let sequence =
                        kicks.sequence_for(KickTransition::new(piece, source.rotation, to))?;
                    sequence.offsets().iter().copied().find_map(|offset| {
                        let (dx, dy) = reference_kick_delta(
                            piece,
                            source.rotation,
                            to,
                            offset.dx(),
                            offset.dy(),
                        );
                        let target =
                            PiecePose::new(to, source.x + i16::from(dx), source.y + i16::from(dy));
                        placeable(target).then_some(target)
                    })
                }
                ClassicInputAction::HardDrop => None,
            }
        };

        let mut distance = HashMap::new();
        let mut parents = HashMap::new();
        let mut locks = HashMap::new();
        let mut queue = VecDeque::new();
        distance.insert(spawn, 0_u32);
        queue.push_back(spawn);
        while let Some(source) = queue.pop_front() {
            let source_cost = distance[&source];
            let lock = hard_drop(source);
            if inside(lock) {
                locks.entry(lock).or_insert((source_cost + 1, source));
            }
            for action in ClassicInputAction::EXPANSION_ORDER {
                if action == ClassicInputAction::SoftDrop {
                    let mut target = source;
                    loop {
                        let next = PiecePose::new(target.rotation, target.x, target.y - 1);
                        if !placeable(next) {
                            break;
                        }
                        target = next;
                        if target.y > interaction_ceiling {
                            continue;
                        }
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            distance.entry(target)
                        {
                            entry.insert(source_cost + 1);
                            parents.insert(target, (source, action));
                            queue.push_back(target);
                        }
                    }
                    continue;
                }
                let Some(target) = successor(source, action) else {
                    continue;
                };
                if let std::collections::hash_map::Entry::Vacant(entry) = distance.entry(target) {
                    entry.insert(source_cost + 1);
                    parents.insert(target, (source, action));
                    queue.push_back(target);
                }
            }
        }
        Some(ReferenceSearch {
            locks,
            parents,
            spawn,
        })
    }

    fn reference_kick_delta(
        piece: PieceKind,
        from: RotationState,
        to: RotationState,
        kick_dx: i8,
        kick_dy: i8,
    ) -> (i8, i8) {
        let center = |rotation: RotationState| match piece {
            PieceKind::I => {
                [(0, 0), (-2, 2), (0, 1), (-1, 2)][usize::from(rotation.quarter_turns())]
            }
            PieceKind::O => (0, 0),
            PieceKind::T | PieceKind::S | PieceKind::Z | PieceKind::J | PieceKind::L => {
                [(1, 0), (0, 1), (1, 1), (1, 1)][usize::from(rotation.quarter_turns())]
            }
        };
        let (from_x, from_y) = center(from);
        let (to_x, to_y) = center(to);
        (kick_dx + from_x - to_x, kick_dy + from_y - to_y)
    }

    fn reference_actions(
        search: &ReferenceSearch,
        mut source: PiecePose,
    ) -> Vec<ClassicInputAction> {
        let mut actions = Vec::new();
        while source != search.spawn {
            let (parent, action) = search.parents[&source];
            actions.push(action);
            source = parent;
        }
        actions.reverse();
        actions.push(ClassicInputAction::HardDrop);
        actions
    }

    fn reference_place_and_clear(
        width: u16,
        height: u16,
        occupied: u64,
        piece: PieceKind,
        pose: PiecePose,
    ) -> Option<(u64, u32)> {
        let shape = standard_tetromino_registry().get(piece)?.rotations()
            [usize::from(pose.rotation.quarter_turns())];
        let mut placement = 0_u64;
        for cell in shape.cells() {
            let x = pose.x.checked_add(i16::from(cell.x()))?;
            let y = pose.y.checked_add(i16::from(cell.y()))?;
            if x < 0
                || x >= i16::try_from(width).ok()?
                || y < 0
                || y >= i16::try_from(height).ok()?
            {
                return None;
            }
            placement |=
                1_u64 << (u32::try_from(y).ok()? * u32::from(width) + u32::try_from(x).ok()?);
        }
        if placement & occupied != 0 {
            return None;
        }

        let placed = occupied | placement;
        let full_row = (1_u64 << u32::from(width)) - 1;
        let mut compacted = 0_u64;
        let mut destination_y = 0_u32;
        let mut cleared = 0_u32;
        for source_y in 0..u32::from(height) {
            let row = (placed >> (source_y * u32::from(width))) & full_row;
            if row == full_row {
                cleared += 1;
            } else {
                compacted |= row << (destination_y * u32::from(width));
                destination_y += 1;
            }
        }
        Some((compacted, cleared))
    }

    fn independent_fixed_supply_reference(
        required: &[PieceKind],
        movement_costs: &[u32],
        queue: &[PieceKind],
        initial_hold: Option<PieceKind>,
    ) -> Option<(u32, u32)> {
        fn visit(
            required: &[PieceKind],
            movement_costs: &[u32],
            queue: &[PieceKind],
            depth: usize,
            cursor: usize,
            hold: Option<PieceKind>,
        ) -> Option<(u32, u32)> {
            if depth == required.len() {
                return Some((0, 0));
            }
            let mut supplies = Vec::with_capacity(2);
            if let Some(current) = queue.get(cursor).copied() {
                supplies.push((current, cursor + 1, hold, 0_u32));
                if let Some(held) = hold {
                    supplies.push((held, cursor + 1, Some(current), 1));
                } else if let Some(next) = queue.get(cursor + 1).copied() {
                    supplies.push((next, cursor + 2, Some(current), 1));
                }
            } else if let Some(held) = hold {
                supplies.push((held, cursor, None, 1));
            }

            supplies
                .into_iter()
                .filter(|(piece, _, _, _)| *piece == required[depth])
                .filter_map(|(_, next_cursor, next_hold, hold_inputs)| {
                    let (suffix_cost, suffix_holds) = visit(
                        required,
                        movement_costs,
                        queue,
                        depth + 1,
                        next_cursor,
                        next_hold,
                    )?;
                    Some((
                        movement_costs[depth]
                            .checked_add(hold_inputs)?
                            .checked_add(suffix_cost)?,
                        hold_inputs.checked_add(suffix_holds)?,
                    ))
                })
                .min()
        }

        (required.len() == movement_costs.len())
            .then(|| visit(required, movement_costs, queue, 0, 0, initial_hold))
            .flatten()
    }

    #[test]
    fn small_board_geometry_queue_product_matches_independent_hold_and_line_clear_reference() {
        const WIDTH: u16 = 4;
        const HEIGHT: u16 = 4;
        const INITIAL: u64 = 0x00cc;
        let spawn = PiecePose::new(RotationState::Zero, 0, i16::try_from(HEIGHT).unwrap());
        let spawn_profile = SpawnProfile::new(spawn.x, spawn.y);
        let kicks = NoKick::profile();

        let o_reference = reference_search(WIDTH, HEIGHT, INITIAL, PieceKind::O, spawn, &kicks)
            .expect("O spawn is reachable");
        let mut clearing_o_locks = o_reference
            .locks
            .iter()
            .filter_map(|(pose, (cost, _))| {
                let (next, cleared) =
                    reference_place_and_clear(WIDTH, HEIGHT, INITIAL, PieceKind::O, *pose)?;
                (cleared == 2).then_some((*pose, *cost, next))
            })
            .collect::<Vec<_>>();
        clearing_o_locks
            .sort_unstable_by_key(|(pose, _, _)| (pose.rotation.quarter_turns(), pose.y, pose.x));
        let (o_pose, o_cost, after_o) = clearing_o_locks[0];
        assert_eq!(
            after_o, 0,
            "the independent transition clears both full rows"
        );

        let i_reference = reference_search(WIDTH, HEIGHT, after_o, PieceKind::I, spawn, &kicks)
            .expect("I spawn is reachable after the clear");
        let mut i_locks = i_reference
            .locks
            .iter()
            .filter_map(|(pose, (cost, _))| {
                reference_place_and_clear(WIDTH, HEIGHT, after_o, PieceKind::I, *pose)
                    .map(|(next, cleared)| (*pose, *cost, next, cleared))
            })
            .collect::<Vec<_>>();
        i_locks.sort_unstable_by_key(|(pose, _, _, _)| {
            (pose.rotation.quarter_turns(), pose.y, pose.x)
        });
        let (i_pose, i_cost, final_board, i_clears) = i_locks[0];
        assert_eq!(i_clears, 1);
        assert_eq!(
            final_board, 0,
            "the second independent transition clears its row"
        );

        let layout = Board64Layout::new(BoardSize::new(WIDTH, HEIGHT).unwrap()).unwrap();
        let initial_board = FinesseBoard::new(layout, INITIAL).unwrap();
        let cleared_board = FinesseBoard::new(layout, after_o).unwrap();
        let actual_o = FrozenFinesseQuery::new(
            initial_board,
            PieceKind::O,
            spawn_profile,
            kicks.clone(),
            [FinesseTarget::new(o_pose.rotation, o_pose.x, o_pose.y)],
        )
        .costs()
        .unwrap();
        let actual_i = FrozenFinesseQuery::new(
            cleared_board,
            PieceKind::I,
            spawn_profile,
            kicks.clone(),
            [FinesseTarget::new(i_pose.rotation, i_pose.x, i_pose.y)],
        )
        .costs()
        .unwrap();
        assert_eq!(actual_o.get(0), Some(Some(o_cost)));
        assert_eq!(actual_i.get(0), Some(Some(i_cost)));

        let o_action = GeometryActionKey::new(PieceKind::O, o_pose.rotation, o_pose.x, o_pose.y);
        let i_action = GeometryActionKey::new(PieceKind::I, i_pose.rotation, i_pose.x, i_pose.y);
        let language = CostedGeometryLanguage::new(
            GeometryNodeId::new(0),
            vec![
                GeometryLanguageNode::new(
                    0,
                    false,
                    vec![
                        CostedGeometryEdge::new(PieceKind::O, GeometryNodeId::new(1), o_cost, 0)
                            .with_action_key(o_action),
                    ],
                )
                .with_source_board(initial_board),
                GeometryLanguageNode::new(
                    1,
                    false,
                    vec![
                        CostedGeometryEdge::new(PieceKind::I, GeometryNodeId::new(2), i_cost, 1)
                            .with_action_key(i_action),
                    ],
                )
                .with_source_board(cleared_board),
                GeometryLanguageNode::new(2, true, Vec::<CostedGeometryEdge>::new()),
            ],
        )
        .unwrap();
        let queue = [PieceKind::I, PieceKind::O];
        let classes = QueueClassSet::group(
            &[
                QueuePattern::new(
                    PatternId::new(3),
                    queue.to_vec(),
                    ProbabilityValue::new(0.4).unwrap(),
                ),
                QueuePattern::new(
                    PatternId::new(8),
                    queue.to_vec(),
                    ProbabilityValue::new(0.6).unwrap(),
                ),
            ],
            true,
        )
        .unwrap();

        let reference = independent_fixed_supply_reference(
            &[PieceKind::O, PieceKind::I],
            &[o_cost, i_cost],
            &queue,
            None,
        )
        .expect("independent supply search finds the two-hold route");
        assert_eq!(reference, (o_cost + i_cost + 2, 2));

        let evaluator = QueueClassProductEvaluator::new(&language);
        let product = evaluator.oracle(&classes, None).unwrap();
        assert_eq!(classes.metadata().unique_queue_count, 1);
        assert_eq!(product.costs.get(0), Some(Some(reference.0)));
        let witness = evaluator
            .fixed_queue_witness(&queue, None)
            .unwrap()
            .unwrap();
        assert_eq!(witness.total_cost(), reference.0);
        assert_eq!(
            witness
                .steps()
                .iter()
                .filter(|step| step.supply_action() != QueueSupplyAction::UseCurrent)
                .count(),
            reference.1 as usize
        );
        let replayed = evaluator
            .replay_fixed_queue_witness(&queue, None, spawn_profile, &kicks)
            .unwrap()
            .unwrap();
        assert_eq!(replayed.total_cost(), reference.0);
        assert_eq!(replayed.placements(), [o_action, i_action]);
    }

    #[test]
    fn empty_small_board_costs_are_finalized_when_dequeued() {
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            vec![
                FinesseTarget::new(RotationState::Zero, 1, 0),
                FinesseTarget::new(RotationState::Zero, 0, 0),
            ],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[Some(1), Some(2)]);
    }

    #[test]
    fn exhaustive_small_boards_match_an_independent_fifo_reference() {
        let kicks = SrsKicks::srs_plus_profile();
        for (width, height) in [(3_u16, 4_u16), (4_u16, 3_u16)] {
            let targets = RotationState::ALL
                .into_iter()
                .flat_map(|rotation| {
                    (0..height).flat_map(move |y| {
                        (0..width).map(move |x| {
                            FinesseTarget::new(
                                rotation,
                                i16::try_from(x).unwrap(),
                                i16::try_from(y).unwrap(),
                            )
                        })
                    })
                })
                .collect::<Vec<_>>();
            let occupancy_count = 1_u64 << (u32::from(width) * u32::from(height));
            for occupied in 0..occupancy_count {
                for piece in PieceKind::STANDARD_TETROMINOES {
                    let spawn =
                        PiecePose::new(RotationState::Zero, 0, i16::try_from(height).unwrap());
                    let reference = reference_search(width, height, occupied, piece, spawn, &kicks);
                    let query = FrozenFinesseQuery::new(
                        board(width, height, occupied),
                        piece,
                        SpawnProfile::new(spawn.x, spawn.y),
                        kicks.clone(),
                        targets.clone(),
                    );
                    let Some(reference) = reference else {
                        assert_eq!(query.costs().unwrap().as_slice(), vec![None; targets.len()]);
                        continue;
                    };
                    let actual = query.costs().unwrap();
                    let mut first_reachable = None;
                    for (target_index, target) in targets.iter().copied().enumerate() {
                        let expected = reference.locks.get(&target.pose()).map(|(cost, _)| *cost);
                        assert_eq!(
                            actual.get(target_index).flatten(),
                            expected,
                            "{width}x{height} occupied={occupied:#x} piece={piece:?} target={target:?}"
                        );
                        if first_reachable.is_none() && expected.is_some() {
                            first_reachable = Some(target_index);
                        }
                    }
                    if let Some(target_index) = first_reachable {
                        let target = targets[target_index];
                        let (_, source) = reference.locks[&target.pose()];
                        let expected_actions = reference_actions(&reference, source);
                        let actual_witness = query.witness(target_index).unwrap().unwrap();
                        assert_eq!(
                            actual_witness.actions.as_ref(),
                            expected_actions,
                            "FIFO tie mismatch for {width}x{height} occupied={occupied:#x} piece={piece:?} target={target:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn equal_cost_path_keeps_tap_before_das_without_replacement() {
        let target = FinesseTarget::new(RotationState::Zero, 0, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            vec![target],
        );

        let witness = query.witness(0).unwrap().expect("reachable target");
        assert_eq!(
            witness.actions.as_ref(),
            [ClassicInputAction::TapLeft, ClassicInputAction::HardDrop]
        );
        assert_eq!(witness.cost, 2);
    }

    #[test]
    fn soft_drop_can_enter_below_an_overhang() {
        let roof = (1_u64 << (2 * 4)) | (1_u64 << (2 * 4 + 1));
        let target = FinesseTarget::new(RotationState::Zero, 0, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, roof),
            PieceKind::O,
            SpawnProfile::new(2, 4),
            NoKick::profile(),
            vec![target],
        );

        let witness = query.witness(0).unwrap().expect("reachable tunnel target");
        assert_eq!(witness.cost, 3);
        assert_eq!(
            witness.actions.as_ref(),
            [
                ClassicInputAction::SoftDrop,
                ClassicInputAction::DasLeft,
                ClassicInputAction::HardDrop,
            ]
        );
    }

    #[test]
    fn direct_hard_drop_is_preferred_without_soft_drop() {
        let target = FinesseTarget::new(RotationState::Zero, 1, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            [target],
        );

        let witness = query.witness(0).unwrap().expect("reachable target");
        assert_eq!(witness.cost, 1);
        assert_eq!(witness.actions.as_ref(), [ClassicInputAction::HardDrop]);
        assert_eq!(
            query
                .costs_for_terminal_evidence(&[None])
                .unwrap()
                .as_slice(),
            &[Some(1)]
        );
    }

    #[test]
    fn ungrounded_target_is_rejected_before_fifo_traversal() {
        let target = FinesseTarget::new(RotationState::Zero, 1, 2);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            [target],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[None]);
        assert_eq!(
            query
                .costs_for_terminal_evidence(&[None])
                .unwrap()
                .as_slice(),
            &[None]
        );
        assert!(query
            .route_labels()
            .unwrap()
            .get(0)
            .unwrap()
            .iter()
            .next()
            .is_none());
        let mut cancellation_checks = 0;
        let witness = query
            .witness_with_cancel(0, || {
                cancellation_checks += 1;
                cancellation_checks > 1
            })
            .unwrap();
        assert_eq!(witness, None);
        assert_eq!(cancellation_checks, 1);
    }

    #[test]
    fn soft_drop_does_not_enqueue_non_interacting_sky_heights() {
        // The only occupied cell is on the eighth row. The active O is in a
        // different column, so its vertical ray reaches the floor, but clear
        // sky above row eight must not become a set of searchable stops.
        let query = FrozenFinesseQuery::new(
            board(4, 12, 1_u64 << (7 * 4)),
            PieceKind::O,
            SpawnProfile::new(1, 20),
            NoKick::profile(),
            Vec::<FinesseTarget>::new(),
        );
        let kernel = SearchKernel::new(&query).unwrap();
        let mut stops = Vec::new();
        kernel.for_each_successor(kernel.spawn, ClassicInputAction::SoftDrop, |successor| {
            stops.push(successor.pose.y);
        });

        assert_eq!(kernel.interaction_ceiling, 8);
        assert_eq!(stops, (0_i16..=8).rev().collect::<Vec<_>>());
    }

    #[test]
    fn route_labels_keep_a_slower_rotation_route_beside_the_fast_non_rotation_route() {
        let target = FinesseTarget::new(RotationState::Zero, 1, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            vec![target],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[Some(1)]);
        let labels = query.route_labels().unwrap();
        let target_labels = labels.get(0).unwrap();
        assert_eq!(
            target_labels
                .get(TerminalEvidenceClass::NoRotation)
                .unwrap()
                .cost,
            1
        );
        let rotation = target_labels
            .get(TerminalEvidenceClass::Rotation)
            .expect("slower grounded rotation route remains available");
        assert!(rotation.cost > 1);
        assert!(matches!(
            rotation.terminal_evidence,
            TerminalEvidenceLabel::Rotation {
                request: ClassicInputAction::RotateClockwise
                    | ClassicInputAction::RotateCounterClockwise,
                ..
            }
        ));
        assert_eq!(
            query
                .cost_for_terminal_evidence(0, rotation.terminal_evidence)
                .unwrap(),
            Some(rotation.cost)
        );
    }

    #[test]
    fn exact_no_rotation_cost_does_not_borrow_a_faster_terminal_rotation() {
        let target = FinesseTarget::new(RotationState::Zero, 0, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0x20),
            PieceKind::L,
            SpawnProfile::new(1, 4),
            SrsKicks::srs_plus_profile(),
            [target],
        );
        let labels = query.route_labels().unwrap();
        let target_labels = labels.get(0).unwrap();
        let expected_no_rotation = target_labels
            .get(TerminalEvidenceClass::NoRotation)
            .expect("slower non-rotation arrival exists");
        let faster_rotation = target_labels
            .get(TerminalEvidenceClass::Rotation)
            .expect("faster terminal rotation exists");
        let unconstrained = query.witness(0).unwrap().expect("ordinary witness");
        assert_eq!(unconstrained.cost, faster_rotation.cost);
        assert!(matches!(
            unconstrained.terminal_evidence,
            TerminalEvidenceLabel::Rotation { .. }
        ));

        let costs = query
            .costs_for_terminal_evidence(&[Some(TerminalEvidenceLabel::NoRotation)])
            .unwrap();
        assert_eq!(costs.get(0).flatten(), Some(expected_no_rotation.cost));
        assert!(faster_rotation.cost < expected_no_rotation.cost);

        let witness = query
            .witness_for_terminal_evidence(0, TerminalEvidenceLabel::NoRotation)
            .unwrap()
            .expect("exact no-rotation witness");
        assert_eq!(witness.cost, expected_no_rotation.cost);
        assert_eq!(witness.actions.len(), expected_no_rotation.cost as usize);
        assert_eq!(witness.terminal_evidence, TerminalEvidenceLabel::NoRotation);
    }

    #[test]
    fn rotation_uses_the_first_placeable_kick() {
        let profile = SrsKicks::srs_plus_profile();
        let empty_query = FrozenFinesseQuery::new(
            board(10, 6, 0),
            PieceKind::T,
            SpawnProfile::new(4, 6),
            profile.clone(),
            Vec::<FinesseTarget>::new(),
        );
        let empty_kernel = SearchKernel::new(&empty_query).unwrap();

        for rotation in RotationState::ALL {
            for y in 0..6_i16 {
                for x in 0..10_i16 {
                    let source = PiecePose::new(rotation, x, y);
                    if !empty_kernel.placeable(source) {
                        continue;
                    }
                    for blocker in 0..60_u32 {
                        let occupied = 1_u64 << blocker;
                        let blocked_query = FrozenFinesseQuery::new(
                            board(10, 6, occupied),
                            PieceKind::T,
                            SpawnProfile::new(4, 6),
                            profile.clone(),
                            Vec::<FinesseTarget>::new(),
                        );
                        let kernel = SearchKernel::new(&blocked_query).unwrap();
                        if !kernel.placeable(source) {
                            continue;
                        }
                        let Some(successor) =
                            kernel.rotation(source, ClassicInputAction::RotateClockwise)
                        else {
                            continue;
                        };
                        let Some(evidence) = successor.rotation else {
                            continue;
                        };
                        if evidence.kick_index > 0 {
                            assert_eq!(evidence.request, ClassicInputAction::RotateClockwise);
                            assert!(kernel.placeable(successor.pose));
                            return;
                        }
                    }
                }
            }
        }
        panic!("no deterministic non-origin SRS+ kick case found");
    }

    #[test]
    fn rotation_evidence_uses_normalized_pose_displacement() {
        let query = FrozenFinesseQuery::new(
            board(10, 6, 0),
            PieceKind::T,
            SpawnProfile::new(4, 6),
            SrsKicks::srs_plus_profile(),
            Vec::<FinesseTarget>::new(),
        );
        let kernel = SearchKernel::new(&query).unwrap();
        let source = PiecePose::new(RotationState::Zero, 4, 3);
        let successor = kernel
            .rotation(source, ClassicInputAction::RotateClockwise)
            .expect("empty-board rotation succeeds");
        let TerminalEvidenceLabel::Rotation {
            kick_dx,
            kick_dy,
            predecessor,
            ..
        } = successor.rotation.unwrap().label()
        else {
            panic!("rotation successor carries evidence")
        };

        assert_eq!(predecessor, source);
        assert_eq!((kick_dx, kick_dy), (1, -1));
        assert_eq!(
            successor.pose,
            PiecePose::new(RotationState::Right, source.x + 1, source.y - 1)
        );
    }

    #[test]
    fn frozen_duplicate_targets_keep_input_order_and_cost() {
        let target = FinesseTarget::new(RotationState::Zero, 1, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(1, 4),
            NoKick::profile(),
            vec![target, target],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[Some(1), Some(1)]);
    }

    #[test]
    fn extended_board_supports_build_searches_above_six_rows() {
        let layout = Board128Layout::standard_10_by_lines(7).expect("10x7 Board128 layout");
        let extended = FinesseBoard::from_board128(layout, 0).expect("empty extended board");
        let query = FrozenFinesseQuery::new(
            extended,
            PieceKind::O,
            SpawnProfile::STANDARD_10,
            NoKick::profile(),
            vec![FinesseTarget::new(RotationState::Zero, 4, 0)],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[Some(1)]);
    }

    #[test]
    fn fifth_quarter_turn_kick_has_its_own_evidence_class() {
        let predecessor = PiecePose::new(RotationState::Zero, 4, 0);
        let quarter_turn = RotationArrival {
            from: RotationState::Zero,
            to: RotationState::Right,
            request: ClassicInputAction::RotateClockwise,
            kick_index: 4,
            kick_dx: 0,
            kick_dy: 0,
            predecessor,
        };
        let half_turn = RotationArrival {
            request: ClassicInputAction::Rotate180,
            to: RotationState::Two,
            ..quarter_turn
        };

        assert_eq!(
            quarter_turn.class(),
            TerminalEvidenceClass::FinalKickOverride
        );
        assert_eq!(half_turn.class(), TerminalEvidenceClass::Rotation);
    }

    #[test]
    fn blocked_spawn_is_an_unreachable_query_not_a_request_error() {
        let target = FinesseTarget::new(RotationState::Zero, 1, 0);
        let query = FrozenFinesseQuery::new(
            board(4, 4, u64::MAX >> (u64::BITS - 16)),
            PieceKind::O,
            SpawnProfile::new(1, 2),
            NoKick::profile(),
            [target],
        );

        assert_eq!(query.costs().unwrap().as_slice(), &[None]);
        assert!(query
            .route_labels()
            .unwrap()
            .get(0)
            .unwrap()
            .iter()
            .next()
            .is_none());
        assert_eq!(
            query
                .costs_for_terminal_evidence(&[Some(TerminalEvidenceLabel::NoRotation)])
                .unwrap()
                .as_slice(),
            &[None]
        );
        assert_eq!(
            query
                .cost_for_terminal_evidence(0, TerminalEvidenceLabel::NoRotation)
                .unwrap(),
            None
        );
        assert_eq!(query.witness(0).unwrap(), None);
        assert_eq!(
            query
                .witness_for_terminal_evidence(0, TerminalEvidenceLabel::NoRotation)
                .unwrap(),
            None
        );
    }

    #[test]
    fn invalid_spawn_coordinates_remain_errors() {
        let negative = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(-1, 4),
            NoKick::profile(),
            [FinesseTarget::new(RotationState::Zero, 1, 0)],
        );
        assert!(matches!(
            negative.costs(),
            Err(FinesseError::NegativeSpawn { .. })
        ));

        let outside = FrozenFinesseQuery::new(
            board(4, 4, 0),
            PieceKind::O,
            SpawnProfile::new(8, 4),
            NoKick::profile(),
            [FinesseTarget::new(RotationState::Zero, 1, 0)],
        );
        assert!(matches!(
            outside.costs(),
            Err(FinesseError::SpawnOutsideSearchSpace(_))
        ));
    }
}
