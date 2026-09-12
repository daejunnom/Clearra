use crate::{Pc4RuleProfile, QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity};

/// Standard tetromino carried by a qualified PC4 graph edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Pc4GraphPiece {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

/// Canonical rotation component of a concrete Clearra placement identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlacementRotation {
    Zero,
    Right,
    Two,
    Left,
}

/// Stable identity of one concrete placement emitted by a Clearra materializer.
///
/// This mirrors the semantic components of Clearra's geometry `PlacementMask`:
/// piece, rotation, origin, and occupied cells. It is deliberately unrelated to
/// opaque graph-record bytes and is not, by itself, a replay or reachability
/// proof.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClearraPlacementIdentity {
    piece: Pc4GraphPiece,
    rotation: PlacementRotation,
    x: u16,
    y: u16,
    occupied_cells: u64,
    // Edge-local normalized frame until the concrete path lifts it once.
    graph_row_transition: Option<(u8, u8)>,
}

impl ClearraPlacementIdentity {
    pub fn new(
        piece: Pc4GraphPiece,
        rotation: PlacementRotation,
        x: u16,
        y: u16,
        occupied_cells: u64,
    ) -> Result<Self, PlacementIdentityError> {
        let occupied_cell_count = occupied_cells.count_ones();
        if occupied_cell_count != 4 {
            return Err(PlacementIdentityError::TetrominoAreaMismatch {
                occupied_cell_count,
            });
        }
        Ok(Self {
            piece,
            rotation,
            x,
            y,
            occupied_cells,
            graph_row_transition: None,
        })
    }

    pub const fn piece(self) -> Pc4GraphPiece {
        self.piece
    }

    pub const fn rotation(self) -> PlacementRotation {
        self.rotation
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn occupied_cells(self) -> u64 {
        self.occupied_cells
    }

    /// Marks an edge-local normalized placement with its physical clear mask.
    /// Concrete path paging must lift it before a reducer can use the cells.
    pub fn with_graph_row_transition(
        mut self,
        source_cleared_prefix: u8,
        physical_cleared_rows: u8,
    ) -> Result<Self, PlacementIdentityError> {
        let frame = crate::Pc4RowFrame::new(source_cleared_prefix)
            .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
        let shift = u32::from(source_cleared_prefix) * 10;
        if self.occupied_cells & ((1_u64 << shift) - 1) != 0 {
            return Err(PlacementIdentityError::InvalidGraphRowTransition);
        }
        frame
            .lift_physical_cells(self.occupied_cells >> shift)
            .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
        frame
            .after_clear(physical_cleared_rows)
            .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
        self.graph_row_transition = Some((source_cleared_prefix, physical_cleared_rows));
        Ok(self)
    }
}

/// Rebase each selected edge alternative independently for this concrete path.
/// Never cache a frame by graph node: different clear histories can converge.
pub(crate) fn rebase_concrete_placements(
    placements: &mut [ClearraPlacementIdentity],
) -> Result<(), PlacementIdentityError> {
    let Some(first) = placements.first() else {
        return Ok(());
    };
    let Some((prefix, _)) = first.graph_row_transition else {
        return if placements.iter().all(|p| p.graph_row_transition.is_none()) {
            Ok(())
        } else {
            Err(PlacementIdentityError::InvalidGraphRowTransition)
        };
    };
    let mut frame = crate::Pc4RowFrame::new(prefix)
        .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
    for placement in placements {
        let Some((prefix, clear_rows)) = placement.graph_row_transition else {
            return Err(PlacementIdentityError::InvalidGraphRowTransition);
        };
        if prefix != 4 - frame.remaining_rows() {
            return Err(PlacementIdentityError::InvalidGraphRowTransition);
        }
        placement.occupied_cells = frame
            .lift_physical_cells(placement.occupied_cells >> (u32::from(prefix) * 10))
            .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
        frame = frame
            .after_clear(clear_rows)
            .map_err(|_| PlacementIdentityError::InvalidGraphRowTransition)?;
        placement.graph_row_transition = None;
    }
    Ok(())
}

#[cfg(test)]
mod row_transition_tests {
    use super::*;

    fn lock(cells: u64, prefix: u8, clears: u8) -> ClearraPlacementIdentity {
        ClearraPlacementIdentity::new(Pc4GraphPiece::I, PlacementRotation::Zero, 0, 0, cells)
            .unwrap()
            .with_graph_row_transition(prefix, clears)
            .unwrap()
    }

    #[test]
    fn converging_normalized_paths_keep_distinct_original_frames() {
        let later = lock(15 << 10, 1, 0);
        let mut bottom_clear = [lock(15, 0, 1), later];
        let mut upper_clear = [lock(15 << 10, 0, 2), later];
        rebase_concrete_placements(&mut bottom_clear).unwrap();
        rebase_concrete_placements(&mut upper_clear).unwrap();
        assert_eq!(bottom_clear[1].occupied_cells(), 15 << 10);
        assert_eq!(upper_clear[1].occupied_cells(), 15);
        assert_eq!(bottom_clear[1].x(), later.x());
        assert_eq!(bottom_clear[1].y(), later.y());
        assert!(bottom_clear
            .iter()
            .all(|p| p.graph_row_transition.is_none()));
    }

    #[test]
    fn rejects_mixed_frames_and_inconsistent_clear_history() {
        let fixed =
            ClearraPlacementIdentity::new(Pc4GraphPiece::I, PlacementRotation::Zero, 0, 0, 15)
                .unwrap();
        for mut path in [
            vec![fixed, lock(15, 0, 0)],
            vec![lock(15, 0, 0), fixed],
            vec![lock(15, 0, 1), lock(15, 0, 0)],
        ] {
            assert_eq!(
                rebase_concrete_placements(&mut path),
                Err(PlacementIdentityError::InvalidGraphRowTransition)
            );
        }
        assert!(fixed.with_graph_row_transition(1, 0).is_err());
        assert!(fixed.with_graph_row_transition(0, 16).is_err());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementIdentityError {
    TetrominoAreaMismatch { occupied_cell_count: u32 },
    InvalidGraphRowTransition,
}

impl PlacementIdentityError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::InvalidGraphRowTransition => "pc4_placement_invalid_graph_row_transition",
            Self::TetrominoAreaMismatch { .. } => "pc4_placement_identity_tetromino_area_mismatch",
        }
    }
}

/// One source-field + piece -> target-field fact emitted by an independently
/// qualified graph-record parser.
///
/// Construction only binds that parser's result to its immutable snapshot and
/// profile. It does not qualify an upstream format, profile, dataset, or known
/// answer, and callers must not derive any field here from opaque record bytes
/// without a separately qualified record-layout parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualifiedPc4GraphEdge {
    target: QualifiedPc4TargetIdentity,
    source_field_id: u32,
    piece: Pc4GraphPiece,
    target_field_id: u32,
}

impl QualifiedPc4GraphEdge {
    pub fn from_qualified_record(
        target: &QualifiedPc4TargetIdentity,
        source_field_id: u32,
        piece: Pc4GraphPiece,
        target_field_id: u32,
    ) -> Self {
        Self {
            target: target.clone(),
            source_field_id,
            piece,
            target_field_id,
        }
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        self.target.snapshot()
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub const fn piece(&self) -> Pc4GraphPiece {
        self.piece
    }

    pub const fn target_field_id(&self) -> u32 {
        self.target_field_id
    }
}

/// Complete response from one rule-profile-specific placement materializer.
///
/// The redundant binding is intentional: the boundary checks every component
/// rather than trusting an adapter to have answered the requested edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationOutput {
    pub snapshot: QualifiedSnapshotIdentity,
    pub profile: Pc4RuleProfile,
    pub source_field_id: u32,
    pub piece: Pc4GraphPiece,
    pub target_field_id: u32,
    pub placements: Vec<ClearraPlacementIdentity>,
}

/// Pure adapter contract for an existing Clearra placement enumerator.
///
/// Implementations own geometry, collision, kick, and reachability semantics.
/// Returning an identity asserts that the placement is legal for the declared
/// profile and realizes exactly the requested transition; this boundary then
/// verifies the response binding and canonicalizes its stable identities.
pub trait Pc4PlacementMaterializer {
    type Error;

    fn profile(&self) -> Pc4RuleProfile;

    fn enumerate(
        &mut self,
        edge: &QualifiedPc4GraphEdge,
    ) -> Result<MaterializationOutput, Self::Error>;
}

/// Host-owned cancellation and immutable-snapshot freshness observation.
///
/// Both observations are sampled before and after enumeration. Implementations
/// should make cancellation monotonic for the duration of one call.
pub trait MaterializationGuard {
    fn is_cancelled(&self) -> bool;

    fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlacementMaterializationSemanticError {
    MaterializerProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    OutputSnapshotMismatch,
    OutputProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    OutputSourceFieldMismatch {
        expected: u32,
        actual: u32,
    },
    OutputPieceMismatch {
        expected: Pc4GraphPiece,
        actual: Pc4GraphPiece,
    },
    OutputTargetFieldMismatch {
        expected: u32,
        actual: u32,
    },
    PlacementPieceMismatch {
        expected: Pc4GraphPiece,
        actual: Pc4GraphPiece,
    },
    NoRealizations,
}

impl PlacementMaterializationSemanticError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::MaterializerProfileMismatch { .. } => {
                "pc4_placement_materializer_profile_mismatch"
            }
            Self::OutputSnapshotMismatch => "pc4_placement_output_snapshot_mismatch",
            Self::OutputProfileMismatch { .. } => "pc4_placement_output_profile_mismatch",
            Self::OutputSourceFieldMismatch { .. } => "pc4_placement_output_source_field_mismatch",
            Self::OutputPieceMismatch { .. } => "pc4_placement_output_piece_mismatch",
            Self::OutputTargetFieldMismatch { .. } => "pc4_placement_output_target_field_mismatch",
            Self::PlacementPieceMismatch { .. } => "pc4_placement_identity_piece_mismatch",
            Self::NoRealizations => "pc4_placement_transition_has_no_realizations",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlacementMaterializationError<E> {
    Cancelled,
    StaleSnapshot,
    Materializer(E),
    Semantic(PlacementMaterializationSemanticError),
}

impl<E> PlacementMaterializationError<E> {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_placement_materialization_cancelled",
            Self::StaleSnapshot => "pc4_placement_materialization_stale_snapshot",
            Self::Materializer(_) => "pc4_placement_materializer_failed",
            Self::Semantic(error) => error.reason(),
        }
    }
}

/// Enumerates and validates every concrete placement realizing one graph edge.
///
/// Distinct identities are preserved. Ordering and deduplication use only the
/// declared stable `ClearraPlacementIdentity`; no graph byte, iteration order,
/// or private materializer record participates in canonicalization.
pub fn materialize_qualified_graph_edge<M, G>(
    edge: &QualifiedPc4GraphEdge,
    materializer: &mut M,
    guard: &G,
) -> Result<Vec<ClearraPlacementIdentity>, PlacementMaterializationError<M::Error>>
where
    M: Pc4PlacementMaterializer,
    G: MaterializationGuard,
{
    check_guard(edge, guard)?;

    let materializer_profile = materializer.profile();
    if materializer_profile != edge.profile() {
        return Err(PlacementMaterializationError::Semantic(
            PlacementMaterializationSemanticError::MaterializerProfileMismatch {
                expected: edge.profile(),
                actual: materializer_profile,
            },
        ));
    }

    let output = materializer
        .enumerate(edge)
        .map_err(PlacementMaterializationError::Materializer)?;

    // Never inspect or expose materialized results after cancellation or an
    // immutable-snapshot generation change observed during enumeration.
    check_guard(edge, guard)?;
    validate_output_binding(edge, &output)?;

    let mut placements = output.placements;
    if placements.is_empty() {
        return Err(PlacementMaterializationError::Semantic(
            PlacementMaterializationSemanticError::NoRealizations,
        ));
    }
    for placement in &placements {
        if placement.piece() != edge.piece() {
            return Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::PlacementPieceMismatch {
                    expected: edge.piece(),
                    actual: placement.piece(),
                },
            ));
        }
    }

    placements.sort_unstable();
    placements.dedup();
    Ok(placements)
}

fn check_guard<E, G>(
    edge: &QualifiedPc4GraphEdge,
    guard: &G,
) -> Result<(), PlacementMaterializationError<E>>
where
    G: MaterializationGuard,
{
    if guard.is_cancelled() {
        return Err(PlacementMaterializationError::Cancelled);
    }
    if !guard.is_current_snapshot(edge.snapshot()) {
        return Err(PlacementMaterializationError::StaleSnapshot);
    }
    Ok(())
}

fn validate_output_binding<E>(
    edge: &QualifiedPc4GraphEdge,
    output: &MaterializationOutput,
) -> Result<(), PlacementMaterializationError<E>> {
    let mismatch = if output.snapshot != *edge.snapshot() {
        Some(PlacementMaterializationSemanticError::OutputSnapshotMismatch)
    } else if output.profile != edge.profile() {
        Some(
            PlacementMaterializationSemanticError::OutputProfileMismatch {
                expected: edge.profile(),
                actual: output.profile,
            },
        )
    } else if output.source_field_id != edge.source_field_id() {
        Some(
            PlacementMaterializationSemanticError::OutputSourceFieldMismatch {
                expected: edge.source_field_id(),
                actual: output.source_field_id,
            },
        )
    } else if output.piece != edge.piece() {
        Some(PlacementMaterializationSemanticError::OutputPieceMismatch {
            expected: edge.piece(),
            actual: output.piece,
        })
    } else if output.target_field_id != edge.target_field_id() {
        Some(
            PlacementMaterializationSemanticError::OutputTargetFieldMismatch {
                expected: edge.target_field_id(),
                actual: output.target_field_id,
            },
        )
    } else {
        None
    };

    match mismatch {
        Some(error) => Err(PlacementMaterializationError::Semantic(error)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::manifest::tests::{qualified_snapshot_identity, qualified_target_identity};
    use crate::Pc4TerminalUseCase;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SyntheticError {
        Rejected,
    }

    struct SyntheticMaterializer {
        declared_profile: Pc4RuleProfile,
        result: Result<MaterializationOutput, SyntheticError>,
        calls: usize,
        cancel_during_call: Option<Rc<Cell<bool>>>,
        stale_during_call: Option<Rc<Cell<bool>>>,
    }

    impl Pc4PlacementMaterializer for SyntheticMaterializer {
        type Error = SyntheticError;

        fn profile(&self) -> Pc4RuleProfile {
            self.declared_profile
        }

        fn enumerate(
            &mut self,
            _edge: &QualifiedPc4GraphEdge,
        ) -> Result<MaterializationOutput, Self::Error> {
            self.calls += 1;
            if let Some(cancelled) = &self.cancel_during_call {
                cancelled.set(true);
            }
            if let Some(stale) = &self.stale_during_call {
                stale.set(true);
            }
            self.result.clone()
        }
    }

    struct SyntheticGuard {
        cancelled: Rc<Cell<bool>>,
        stale: Rc<Cell<bool>>,
    }

    impl MaterializationGuard for SyntheticGuard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, _expected: &QualifiedSnapshotIdentity) -> bool {
            !self.stale.get()
        }
    }

    fn edge(profile: Pc4RuleProfile) -> QualifiedPc4GraphEdge {
        let target = qualified_target_identity(
            "generation-materializer-a",
            "synthetic-materializer-manifest-a",
            profile,
            Pc4TerminalUseCase::PcSearch,
            4,
        );
        QualifiedPc4GraphEdge::from_qualified_record(&target, 11, Pc4GraphPiece::T, 17)
    }

    fn placement(
        rotation: PlacementRotation,
        x: u16,
        occupied_cells: u64,
    ) -> ClearraPlacementIdentity {
        ClearraPlacementIdentity::new(Pc4GraphPiece::T, rotation, x, 2, occupied_cells)
            .expect("synthetic tetromino placement")
    }

    fn output(
        edge: &QualifiedPc4GraphEdge,
        placements: Vec<ClearraPlacementIdentity>,
    ) -> MaterializationOutput {
        MaterializationOutput {
            snapshot: edge.snapshot().clone(),
            profile: edge.profile(),
            source_field_id: edge.source_field_id(),
            piece: edge.piece(),
            target_field_id: edge.target_field_id(),
            placements,
        }
    }

    fn synthetic_materializer(
        edge: &QualifiedPc4GraphEdge,
        result: Result<MaterializationOutput, SyntheticError>,
    ) -> SyntheticMaterializer {
        SyntheticMaterializer {
            declared_profile: edge.profile(),
            result,
            calls: 0,
            cancel_during_call: None,
            stale_during_call: None,
        }
    }

    fn guard() -> SyntheticGuard {
        SyntheticGuard {
            cancelled: Rc::new(Cell::new(false)),
            stale: Rc::new(Cell::new(false)),
        }
    }

    #[test]
    fn all_five_profiles_remain_distinct_materializer_requests() {
        for profile in Pc4RuleProfile::ALL {
            let edge = edge(profile);
            let expected = placement(PlacementRotation::Zero, 3, 0b1111);
            let mut materializer = synthetic_materializer(&edge, Ok(output(&edge, vec![expected])));
            assert_eq!(
                materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
                Ok(vec![expected]),
                "{}",
                profile.as_str()
            );
            assert_eq!(materializer.calls, 1);
        }
    }

    #[test]
    fn multiple_realizations_are_sorted_and_deduped_only_by_stable_identity() {
        let edge = edge(Pc4RuleProfile::SrsPlus);
        let first = placement(PlacementRotation::Zero, 1, 0b0000_1111);
        let second = placement(PlacementRotation::Right, 1, 0b1111_0000);
        let third = placement(PlacementRotation::Right, 2, 0b1111_0000_0000);
        let mut materializer =
            synthetic_materializer(&edge, Ok(output(&edge, vec![third, second, first, second])));

        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Ok(vec![first, second, third])
        );
    }

    #[test]
    fn zero_realizations_is_a_typed_semantic_failure() {
        let edge = edge(Pc4RuleProfile::Srs);
        let mut materializer = synthetic_materializer(&edge, Ok(output(&edge, Vec::new())));

        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::NoRealizations
            ))
        );
    }

    #[test]
    fn wrong_source_piece_target_and_profile_bindings_are_rejected() {
        let edge = edge(Pc4RuleProfile::Jstris180);
        let valid = placement(PlacementRotation::Two, 4, 0xf000);

        let mut wrong_snapshot = output(&edge, vec![valid]);
        wrong_snapshot.snapshot = qualified_snapshot_identity(
            "generation-materializer-a",
            "synthetic-materializer-manifest-b",
        );
        assert_eq!(
            wrong_snapshot.snapshot.snapshot_identity(),
            edge.snapshot().snapshot_identity()
        );
        assert_ne!(
            wrong_snapshot.snapshot.manifest_content_identity(),
            edge.snapshot().manifest_content_identity()
        );
        let mut materializer = synthetic_materializer(&edge, Ok(wrong_snapshot));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::OutputSnapshotMismatch
            ))
        );

        let mut wrong_source = output(&edge, vec![valid]);
        wrong_source.source_field_id += 1;
        let mut materializer = synthetic_materializer(&edge, Ok(wrong_source));
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::OutputSourceFieldMismatch { .. }
            ))
        ));

        let mut wrong_piece = output(&edge, vec![valid]);
        wrong_piece.piece = Pc4GraphPiece::L;
        let mut materializer = synthetic_materializer(&edge, Ok(wrong_piece));
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::OutputPieceMismatch { .. }
            ))
        ));

        let mut wrong_target = output(&edge, vec![valid]);
        wrong_target.target_field_id += 1;
        let mut materializer = synthetic_materializer(&edge, Ok(wrong_target));
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::OutputTargetFieldMismatch { .. }
            ))
        ));

        let mut wrong_profile = output(&edge, vec![valid]);
        wrong_profile.profile = Pc4RuleProfile::SrsX;
        let mut materializer = synthetic_materializer(&edge, Ok(wrong_profile));
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::OutputProfileMismatch { .. }
            ))
        ));

        let mut wrong_declared_profile =
            synthetic_materializer(&edge, Ok(output(&edge, vec![valid])));
        wrong_declared_profile.declared_profile = Pc4RuleProfile::Srs;
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut wrong_declared_profile, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::MaterializerProfileMismatch { .. }
            ))
        ));

        let wrong_identity_piece =
            ClearraPlacementIdentity::new(Pc4GraphPiece::L, PlacementRotation::Two, 4, 2, 0xf000)
                .expect("synthetic L placement");
        let mut materializer =
            synthetic_materializer(&edge, Ok(output(&edge, vec![wrong_identity_piece])));
        assert!(matches!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Semantic(
                PlacementMaterializationSemanticError::PlacementPieceMismatch { .. }
            ))
        ));
    }

    #[test]
    fn cancellation_and_stale_snapshot_before_call_do_not_invoke_materializer() {
        let edge = edge(Pc4RuleProfile::NoKick);
        let valid = placement(PlacementRotation::Left, 5, 0xf0000);

        let cancelled_guard = guard();
        cancelled_guard.cancelled.set(true);
        let mut cancelled = synthetic_materializer(&edge, Ok(output(&edge, vec![valid])));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut cancelled, &cancelled_guard),
            Err(PlacementMaterializationError::Cancelled)
        );
        assert_eq!(cancelled.calls, 0);

        let stale_guard = guard();
        stale_guard.stale.set(true);
        let mut stale = synthetic_materializer(&edge, Ok(output(&edge, vec![valid])));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut stale, &stale_guard),
            Err(PlacementMaterializationError::StaleSnapshot)
        );
        assert_eq!(stale.calls, 0);
    }

    #[test]
    fn cancellation_or_snapshot_change_during_call_cannot_leak_results() {
        let edge = edge(Pc4RuleProfile::SrsX);
        let valid = placement(PlacementRotation::Right, 6, 0xf00000);

        let cancelled_guard = guard();
        let mut cancelled = synthetic_materializer(&edge, Ok(output(&edge, vec![valid])));
        cancelled.cancel_during_call = Some(Rc::clone(&cancelled_guard.cancelled));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut cancelled, &cancelled_guard),
            Err(PlacementMaterializationError::Cancelled)
        );

        let stale_guard = guard();
        let mut stale = synthetic_materializer(&edge, Ok(output(&edge, vec![valid])));
        stale.stale_during_call = Some(Rc::clone(&stale_guard.stale));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut stale, &stale_guard),
            Err(PlacementMaterializationError::StaleSnapshot)
        );
    }

    #[test]
    fn materializer_failure_remains_typed_and_placement_identity_checks_area() {
        let edge = edge(Pc4RuleProfile::Srs);
        let mut materializer = synthetic_materializer(&edge, Err(SyntheticError::Rejected));
        assert_eq!(
            materialize_qualified_graph_edge(&edge, &mut materializer, &guard()),
            Err(PlacementMaterializationError::Materializer(
                SyntheticError::Rejected
            ))
        );
        assert_eq!(
            ClearraPlacementIdentity::new(Pc4GraphPiece::T, PlacementRotation::Zero, 0, 0, 0b111,),
            Err(PlacementIdentityError::TetrominoAreaMismatch {
                occupied_cell_count: 3
            })
        );
    }
}
