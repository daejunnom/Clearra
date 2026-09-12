use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::{
    materialize_qualified_graph_edge, ClearraPlacementIdentity, FixedQueueGraphPath,
    MaterializationGuard, Pc4PlacementMaterializer, Pc4RuleProfile, PlacementMaterializationError,
    QualifiedSnapshotIdentity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcretePathMaterializationBudgetKind {
    GraphEdges,
    AlternativesPerEdge,
    TotalStoredAlternatives,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcretePathMaterializationBudgets {
    graph_edges: NonZeroUsize,
    alternatives_per_edge: NonZeroUsize,
    total_stored_alternatives: NonZeroUsize,
    page_solutions: NonZeroUsize,
}

impl ConcretePathMaterializationBudgets {
    pub const fn new(
        graph_edges: NonZeroUsize,
        alternatives_per_edge: NonZeroUsize,
        total_stored_alternatives: NonZeroUsize,
        page_solutions: NonZeroUsize,
    ) -> Self {
        Self {
            graph_edges,
            alternatives_per_edge,
            total_stored_alternatives,
            page_solutions,
        }
    }

    pub const fn graph_edges(self) -> usize {
        self.graph_edges.get()
    }

    pub const fn alternatives_per_edge(self) -> usize {
        self.alternatives_per_edge.get()
    }

    pub const fn total_stored_alternatives(self) -> usize {
        self.total_stored_alternatives.get()
    }

    pub const fn page_solutions(self) -> usize {
        self.page_solutions.get()
    }
}

pub struct FixedQueuePathMaterializationRequest<'a> {
    snapshot: &'a QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    graph_path: &'a FixedQueueGraphPath,
    budgets: ConcretePathMaterializationBudgets,
}

impl<'a> FixedQueuePathMaterializationRequest<'a> {
    pub const fn new(
        snapshot: &'a QualifiedSnapshotIdentity,
        profile: Pc4RuleProfile,
        graph_path: &'a FixedQueueGraphPath,
        budgets: ConcretePathMaterializationBudgets,
    ) -> Self {
        Self {
            snapshot,
            profile,
            graph_path,
            budgets,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConcretePathMaterializationSemanticError {
    EdgeSnapshotMismatch {
        edge_index: usize,
    },
    EdgeProfileMismatch {
        edge_index: usize,
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    EdgeSourceMismatch {
        edge_index: usize,
        expected: u32,
        actual: u32,
    },
}

impl ConcretePathMaterializationSemanticError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::EdgeSnapshotMismatch { .. } => "pc4_concrete_path_edge_snapshot_mismatch",
            Self::EdgeProfileMismatch { .. } => "pc4_concrete_path_edge_profile_mismatch",
            Self::EdgeSourceMismatch { .. } => "pc4_concrete_path_edge_source_mismatch",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConcretePathMaterializationError<E> {
    Cancelled,
    StaleSnapshot,
    BudgetExceeded {
        kind: ConcretePathMaterializationBudgetKind,
        edge_index: Option<usize>,
        limit: usize,
        actual: usize,
    },
    Semantic(ConcretePathMaterializationSemanticError),
    Edge {
        edge_index: usize,
        source: PlacementMaterializationError<E>,
    },
}

impl<E> ConcretePathMaterializationError<E> {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_concrete_path_materialization_cancelled",
            Self::StaleSnapshot => "pc4_concrete_path_materialization_stale_snapshot",
            Self::BudgetExceeded { .. } => "pc4_concrete_path_materialization_budget_exceeded",
            Self::Semantic(error) => error.reason(),
            Self::Edge { source, .. } => source.reason(),
        }
    }
}

impl<E> fmt::Display for ConcretePathMaterializationError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// A prepared concrete family for one qualified graph path.
///
/// Every edge is materialized exactly once, but the Cartesian product of those
/// alternatives is never allocated. Callers page that product through a cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueConcretePathFamily {
    snapshot: QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    start_field_id: u32,
    terminal_field_id: u32,
    target_field_ids: Vec<u32>,
    alternatives: Vec<Vec<ClearraPlacementIdentity>>,
    maximum_page_solutions: usize,
    cursor_token: Arc<()>,
}

impl FixedQueueConcretePathFamily {
    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        &self.snapshot
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn start_field_id(&self) -> u32 {
        self.start_field_id
    }

    pub const fn terminal_field_id(&self) -> u32 {
        self.terminal_field_id
    }

    pub fn graph_edge_count(&self) -> usize {
        self.alternatives.len()
    }

    pub fn cursor(&self) -> FixedQueueConcretePathCursor {
        FixedQueueConcretePathCursor {
            family_token: Arc::clone(&self.cursor_token),
            next_indices: vec![0; self.alternatives.len()],
            exhausted: false,
        }
    }

    /// Materializes at most `limit` concrete paths in deterministic mixed-radix
    /// order. The last graph edge changes fastest. A zero-edge terminal path
    /// yields exactly one empty concrete path.
    pub fn next_page<G>(
        &self,
        cursor: &mut FixedQueueConcretePathCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Vec<FixedQueueConcretePath>, ConcretePathPageError>
    where
        G: MaterializationGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(ConcretePathPageError::CursorMismatch);
        }
        if limit.get() > self.maximum_page_solutions {
            return Err(ConcretePathPageError::PageLimitExceeded {
                limit: self.maximum_page_solutions,
                requested: limit.get(),
            });
        }
        self.check_page_guard(guard)?;
        if cursor.exhausted {
            return Ok(Vec::new());
        }

        let mut page = Vec::with_capacity(limit.get());
        let mut next_indices = cursor.next_indices.clone();
        let mut exhausted = cursor.exhausted;
        while page.len() < limit.get() && !exhausted {
            self.check_page_guard(guard)?;
            let placements = self
                .alternatives
                .iter()
                .zip(&next_indices)
                .map(|(alternatives, &index)| alternatives[index])
                .collect();
            page.push(FixedQueueConcretePath {
                start_field_id: self.start_field_id,
                terminal_field_id: self.terminal_field_id,
                target_field_ids: self.target_field_ids.clone(),
                placements,
            });
            advance_cursor(&self.alternatives, &mut next_indices, &mut exhausted);
        }
        self.check_page_guard(guard)?;
        cursor.next_indices = next_indices;
        cursor.exhausted = exhausted;
        Ok(page)
    }

    fn check_page_guard<G>(&self, guard: &G) -> Result<(), ConcretePathPageError>
    where
        G: MaterializationGuard,
    {
        if guard.is_cancelled() {
            return Err(ConcretePathPageError::Cancelled);
        }
        if !guard.is_current_snapshot(&self.snapshot) {
            return Err(ConcretePathPageError::StaleSnapshot);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FixedQueueConcretePathCursor {
    family_token: Arc<()>,
    next_indices: Vec<usize>,
    exhausted: bool,
}

impl FixedQueueConcretePathCursor {
    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueConcretePath {
    start_field_id: u32,
    terminal_field_id: u32,
    target_field_ids: Vec<u32>,
    placements: Vec<ClearraPlacementIdentity>,
}

impl FixedQueueConcretePath {
    pub const fn start_field_id(&self) -> u32 {
        self.start_field_id
    }

    pub const fn terminal_field_id(&self) -> u32 {
        self.terminal_field_id
    }

    /// Internal graph-state provenance in edge order. Product presenters must
    /// not expose these opaque IDs as user-facing field notation.
    pub fn target_field_ids(&self) -> &[u32] {
        &self.target_field_ids
    }

    pub fn placements(&self) -> &[ClearraPlacementIdentity] {
        &self.placements
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcretePathPageError {
    Cancelled,
    StaleSnapshot,
    CursorMismatch,
    PageLimitExceeded { limit: usize, requested: usize },
}

impl ConcretePathPageError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_concrete_path_page_cancelled",
            Self::StaleSnapshot => "pc4_concrete_path_page_stale_snapshot",
            Self::CursorMismatch => "pc4_concrete_path_page_cursor_mismatch",
            Self::PageLimitExceeded { .. } => "pc4_concrete_path_page_limit_exceeded",
        }
    }
}

/// Materializes the alternatives for each edge of one already qualified graph
/// path without expanding their Cartesian product.
pub fn prepare_fixed_queue_concrete_family<M, G>(
    request: FixedQueuePathMaterializationRequest<'_>,
    materializer: &mut M,
    guard: &G,
) -> Result<FixedQueueConcretePathFamily, ConcretePathMaterializationError<M::Error>>
where
    M: Pc4PlacementMaterializer,
    G: MaterializationGuard,
{
    check_materialization_guard(request.snapshot, guard)?;
    let edges = request.graph_path.edges();
    if edges.len() > request.budgets.graph_edges() {
        return Err(ConcretePathMaterializationError::BudgetExceeded {
            kind: ConcretePathMaterializationBudgetKind::GraphEdges,
            edge_index: None,
            limit: request.budgets.graph_edges(),
            actual: edges.len(),
        });
    }

    let mut expected_source = request.graph_path.start_field_id();
    let mut total_stored_alternatives = 0usize;
    let mut target_field_ids = Vec::with_capacity(edges.len());
    let mut alternatives = Vec::with_capacity(edges.len());
    for (edge_index, edge) in edges.iter().enumerate() {
        if edge.snapshot() != request.snapshot {
            return Err(ConcretePathMaterializationError::Semantic(
                ConcretePathMaterializationSemanticError::EdgeSnapshotMismatch { edge_index },
            ));
        }
        if edge.profile() != request.profile {
            return Err(ConcretePathMaterializationError::Semantic(
                ConcretePathMaterializationSemanticError::EdgeProfileMismatch {
                    edge_index,
                    expected: request.profile,
                    actual: edge.profile(),
                },
            ));
        }
        if edge.source_field_id() != expected_source {
            return Err(ConcretePathMaterializationError::Semantic(
                ConcretePathMaterializationSemanticError::EdgeSourceMismatch {
                    edge_index,
                    expected: expected_source,
                    actual: edge.source_field_id(),
                },
            ));
        }

        let edge_alternatives = materialize_qualified_graph_edge(edge, materializer, guard)
            .map_err(|source| ConcretePathMaterializationError::Edge { edge_index, source })?;
        if edge_alternatives.len() > request.budgets.alternatives_per_edge() {
            return Err(ConcretePathMaterializationError::BudgetExceeded {
                kind: ConcretePathMaterializationBudgetKind::AlternativesPerEdge,
                edge_index: Some(edge_index),
                limit: request.budgets.alternatives_per_edge(),
                actual: edge_alternatives.len(),
            });
        }
        total_stored_alternatives =
            total_stored_alternatives.saturating_add(edge_alternatives.len());
        if total_stored_alternatives > request.budgets.total_stored_alternatives() {
            return Err(ConcretePathMaterializationError::BudgetExceeded {
                kind: ConcretePathMaterializationBudgetKind::TotalStoredAlternatives,
                edge_index: Some(edge_index),
                limit: request.budgets.total_stored_alternatives(),
                actual: total_stored_alternatives,
            });
        }
        expected_source = edge.target_field_id();
        target_field_ids.push(edge.target_field_id());
        alternatives.push(edge_alternatives);
    }

    check_materialization_guard(request.snapshot, guard)?;

    Ok(FixedQueueConcretePathFamily {
        snapshot: request.snapshot.clone(),
        profile: request.profile,
        start_field_id: request.graph_path.start_field_id(),
        terminal_field_id: request.graph_path.terminal_field_id(),
        target_field_ids,
        alternatives,
        maximum_page_solutions: request.budgets.page_solutions(),
        cursor_token: Arc::new(()),
    })
}

fn check_materialization_guard<E, G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), ConcretePathMaterializationError<E>>
where
    G: MaterializationGuard,
{
    if guard.is_cancelled() {
        return Err(ConcretePathMaterializationError::Cancelled);
    }
    if !guard.is_current_snapshot(snapshot) {
        return Err(ConcretePathMaterializationError::StaleSnapshot);
    }
    Ok(())
}

fn advance_cursor(
    alternatives: &[Vec<ClearraPlacementIdentity>],
    next_indices: &mut [usize],
    exhausted: &mut bool,
) {
    if alternatives.is_empty() {
        *exhausted = true;
        return;
    }
    for edge_index in (0..alternatives.len()).rev() {
        next_indices[edge_index] += 1;
        if next_indices[edge_index] < alternatives[edge_index].len() {
            return;
        }
        next_indices[edge_index] = 0;
    }
    *exhausted = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        manifest::tests::qualified_snapshot_identity, MaterializationOutput, Pc4GraphPiece,
        PlacementRotation, QualifiedPc4GraphEdge,
    };
    use std::cell::Cell;

    fn snapshot_identity(generation: &str) -> QualifiedSnapshotIdentity {
        qualified_snapshot_identity(generation, format!("synthetic-lazy-manifest:{generation}"))
    }

    fn placement(
        piece: Pc4GraphPiece,
        rotation: PlacementRotation,
        x: u16,
        occupied_cells: u64,
    ) -> ClearraPlacementIdentity {
        ClearraPlacementIdentity::new(piece, rotation, x, 0, occupied_cells).expect("placement")
    }

    struct SyntheticMaterializer {
        profile: Pc4RuleProfile,
    }

    impl Pc4PlacementMaterializer for SyntheticMaterializer {
        type Error = &'static str;

        fn profile(&self) -> Pc4RuleProfile {
            self.profile
        }

        fn enumerate(
            &mut self,
            edge: &QualifiedPc4GraphEdge,
        ) -> Result<MaterializationOutput, Self::Error> {
            let placements = match edge.piece() {
                Pc4GraphPiece::I => vec![
                    placement(Pc4GraphPiece::I, PlacementRotation::Zero, 0, 0x0f),
                    placement(Pc4GraphPiece::I, PlacementRotation::Right, 1, 0x1111),
                ],
                Pc4GraphPiece::O => vec![
                    placement(Pc4GraphPiece::O, PlacementRotation::Zero, 0, 0x33),
                    placement(Pc4GraphPiece::O, PlacementRotation::Right, 1, 0x66),
                    placement(Pc4GraphPiece::O, PlacementRotation::Two, 2, 0xcc),
                ],
                _ => return Err("unexpected piece"),
            };
            Ok(MaterializationOutput {
                snapshot: edge.snapshot().clone(),
                profile: edge.profile(),
                source_field_id: edge.source_field_id(),
                piece: edge.piece(),
                target_field_id: edge.target_field_id(),
                placements,
            })
        }
    }

    struct Guard {
        current: QualifiedSnapshotIdentity,
        cancelled: bool,
    }

    impl MaterializationGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            self.current == *expected
        }
    }

    struct CancelAfterChecks {
        current: QualifiedSnapshotIdentity,
        checks: Cell<usize>,
        allowed_checks: usize,
    }

    impl MaterializationGuard for CancelAfterChecks {
        fn is_cancelled(&self) -> bool {
            let checks = self.checks.get();
            self.checks.set(checks + 1);
            checks >= self.allowed_checks
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            self.current == *expected
        }
    }

    fn budgets() -> ConcretePathMaterializationBudgets {
        ConcretePathMaterializationBudgets::new(
            NonZeroUsize::new(16).expect("non-zero"),
            NonZeroUsize::new(16).expect("non-zero"),
            NonZeroUsize::new(64).expect("non-zero"),
            NonZeroUsize::new(100).expect("non-zero"),
        )
    }

    fn two_edge_path(snapshot: &QualifiedSnapshotIdentity) -> FixedQueueGraphPath {
        FixedQueueGraphPath::from_test_edges(
            10,
            vec![
                QualifiedPc4GraphEdge::from_qualified_record(
                    snapshot.clone(),
                    Pc4RuleProfile::Srs,
                    10,
                    Pc4GraphPiece::I,
                    11,
                ),
                QualifiedPc4GraphEdge::from_qualified_record(
                    snapshot.clone(),
                    Pc4RuleProfile::Srs,
                    11,
                    Pc4GraphPiece::O,
                    12,
                ),
            ],
        )
    }

    #[test]
    fn concrete_product_is_paged_without_eagerly_allocating_all_paths() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = two_edge_path(&snapshot);
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &snapshot,
                Pc4RuleProfile::Srs,
                &graph_path,
                budgets(),
            ),
            &mut materializer,
            &guard,
        )
        .expect("prepared family");

        let mut cursor = family.cursor();
        let first = family
            .next_page(&mut cursor, NonZeroUsize::new(2).expect("non-zero"), &guard)
            .expect("first page");
        assert_eq!(first.len(), 2);
        assert!(!cursor.is_exhausted());
        assert_eq!(first[0].target_field_ids(), &[11, 12]);
        assert_eq!(
            first
                .iter()
                .map(|path| {
                    path.placements()
                        .iter()
                        .map(|placement| placement.x())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            vec![vec![0, 0], vec![0, 1]]
        );

        let second = family
            .next_page(&mut cursor, NonZeroUsize::new(8).expect("non-zero"), &guard)
            .expect("second page");
        assert_eq!(second.len(), 4);
        assert!(cursor.is_exhausted());
        assert_eq!(second.last().expect("last").placements()[0].x(), 1);
        assert_eq!(second.last().expect("last").placements()[1].x(), 2);
        assert!(family
            .next_page(&mut cursor, NonZeroUsize::new(1).expect("non-zero"), &guard,)
            .expect("exhausted page")
            .is_empty());
    }

    #[test]
    fn zero_edge_terminal_path_has_one_empty_concrete_realization() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = FixedQueueGraphPath::from_test_edges(10, Vec::new());
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &snapshot,
                Pc4RuleProfile::Srs,
                &graph_path,
                budgets(),
            ),
            &mut materializer,
            &guard,
        )
        .expect("empty family");
        let mut cursor = family.cursor();
        let page = family
            .next_page(&mut cursor, NonZeroUsize::new(4).expect("non-zero"), &guard)
            .expect("page");
        assert_eq!(page.len(), 1);
        assert!(page[0].placements().is_empty());
        assert!(page[0].target_field_ids().is_empty());
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn zero_edge_preparation_still_checks_cancellation_and_snapshot_freshness() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = FixedQueueGraphPath::from_test_edges(10, Vec::new());
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let cancelled = Guard {
            current: snapshot.clone(),
            cancelled: true,
        };
        assert_eq!(
            prepare_fixed_queue_concrete_family(
                FixedQueuePathMaterializationRequest::new(
                    &snapshot,
                    Pc4RuleProfile::Srs,
                    &graph_path,
                    budgets(),
                ),
                &mut materializer,
                &cancelled,
            ),
            Err(ConcretePathMaterializationError::Cancelled)
        );
        let stale = Guard {
            current: snapshot_identity("generation-b"),
            cancelled: false,
        };
        assert_eq!(
            prepare_fixed_queue_concrete_family(
                FixedQueuePathMaterializationRequest::new(
                    &snapshot,
                    Pc4RuleProfile::Srs,
                    &graph_path,
                    budgets(),
                ),
                &mut materializer,
                &stale,
            ),
            Err(ConcretePathMaterializationError::StaleSnapshot)
        );
    }

    #[test]
    fn same_snapshot_labels_with_different_manifest_content_fail_closed() {
        let snapshot = snapshot_identity("generation-a");
        let differently_qualified = qualified_snapshot_identity(
            "generation-a",
            "synthetic-lazy-manifest:different-content",
        );
        assert_eq!(
            snapshot.snapshot_identity(),
            differently_qualified.snapshot_identity()
        );
        assert_ne!(
            snapshot.manifest_content_identity(),
            differently_qualified.manifest_content_identity()
        );

        let graph_path = two_edge_path(&differently_qualified);
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        assert_eq!(
            prepare_fixed_queue_concrete_family(
                FixedQueuePathMaterializationRequest::new(
                    &snapshot,
                    Pc4RuleProfile::Srs,
                    &graph_path,
                    budgets(),
                ),
                &mut materializer,
                &guard,
            ),
            Err(ConcretePathMaterializationError::Semantic(
                ConcretePathMaterializationSemanticError::EdgeSnapshotMismatch { edge_index: 0 }
            ))
        );

        let empty_path = FixedQueueGraphPath::from_test_edges(10, Vec::new());
        let stale_guard = Guard {
            current: differently_qualified,
            cancelled: false,
        };
        assert_eq!(
            prepare_fixed_queue_concrete_family(
                FixedQueuePathMaterializationRequest::new(
                    &snapshot,
                    Pc4RuleProfile::Srs,
                    &empty_path,
                    budgets(),
                ),
                &mut materializer,
                &stale_guard,
            ),
            Err(ConcretePathMaterializationError::StaleSnapshot)
        );
    }

    #[test]
    fn cursor_is_bound_to_one_family_and_pages_fail_closed_on_staleness() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = two_edge_path(&snapshot);
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &snapshot,
                Pc4RuleProfile::Srs,
                &graph_path,
                budgets(),
            ),
            &mut materializer,
            &guard,
        )
        .expect("family");
        let other_snapshot = snapshot_identity("generation-b");
        let other_graph_path = two_edge_path(&other_snapshot);
        let other_guard = Guard {
            current: other_snapshot.clone(),
            cancelled: false,
        };
        let other_family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &other_snapshot,
                Pc4RuleProfile::Srs,
                &other_graph_path,
                budgets(),
            ),
            &mut materializer,
            &other_guard,
        )
        .expect("other family");

        let mut cursor = family.cursor();
        assert_eq!(
            other_family.next_page(
                &mut cursor,
                NonZeroUsize::new(1).expect("non-zero"),
                &other_guard,
            ),
            Err(ConcretePathPageError::CursorMismatch)
        );
        let stale_guard = Guard {
            current: other_snapshot,
            cancelled: false,
        };
        assert_eq!(
            family.next_page(
                &mut cursor,
                NonZeroUsize::new(1).expect("non-zero"),
                &stale_guard,
            ),
            Err(ConcretePathPageError::StaleSnapshot)
        );
    }

    #[test]
    fn alternative_memory_budgets_fail_before_product_expansion() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = two_edge_path(&snapshot);
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let tight = ConcretePathMaterializationBudgets::new(
            NonZeroUsize::new(2).expect("non-zero"),
            NonZeroUsize::new(2).expect("non-zero"),
            NonZeroUsize::new(4).expect("non-zero"),
            NonZeroUsize::new(100).expect("non-zero"),
        );
        assert_eq!(
            prepare_fixed_queue_concrete_family(
                FixedQueuePathMaterializationRequest::new(
                    &snapshot,
                    Pc4RuleProfile::Srs,
                    &graph_path,
                    tight,
                ),
                &mut materializer,
                &guard,
            ),
            Err(ConcretePathMaterializationError::BudgetExceeded {
                kind: ConcretePathMaterializationBudgetKind::AlternativesPerEdge,
                edge_index: Some(1),
                limit: 2,
                actual: 3,
            })
        );
    }

    #[test]
    fn cancelled_page_does_not_advance_cursor_and_page_size_is_bounded() {
        let snapshot = snapshot_identity("generation-a");
        let graph_path = two_edge_path(&snapshot);
        let guard = Guard {
            current: snapshot.clone(),
            cancelled: false,
        };
        let mut materializer = SyntheticMaterializer {
            profile: Pc4RuleProfile::Srs,
        };
        let family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                &snapshot,
                Pc4RuleProfile::Srs,
                &graph_path,
                ConcretePathMaterializationBudgets::new(
                    NonZeroUsize::new(16).expect("non-zero"),
                    NonZeroUsize::new(16).expect("non-zero"),
                    NonZeroUsize::new(64).expect("non-zero"),
                    NonZeroUsize::new(2).expect("non-zero"),
                ),
            ),
            &mut materializer,
            &guard,
        )
        .expect("family");
        let mut cursor = family.cursor();
        assert_eq!(
            family.next_page(&mut cursor, NonZeroUsize::new(3).expect("non-zero"), &guard,),
            Err(ConcretePathPageError::PageLimitExceeded {
                limit: 2,
                requested: 3,
            })
        );

        let cancelling_guard = CancelAfterChecks {
            current: snapshot,
            checks: Cell::new(0),
            allowed_checks: 2,
        };
        assert_eq!(
            family.next_page(
                &mut cursor,
                NonZeroUsize::new(2).expect("non-zero"),
                &cancelling_guard,
            ),
            Err(ConcretePathPageError::Cancelled)
        );
        assert_eq!(cursor.next_indices, vec![0, 0]);
        assert!(!cursor.is_exhausted());

        let recovery_guard = Guard {
            current: cancelling_guard.current.clone(),
            cancelled: false,
        };
        let page = family
            .next_page(
                &mut cursor,
                NonZeroUsize::new(1).expect("non-zero"),
                &recovery_guard,
            )
            .expect("retry from unchanged cursor");
        assert_eq!(
            page[0]
                .placements()
                .iter()
                .map(|placement| placement.x())
                .collect::<Vec<_>>(),
            vec![0, 0]
        );
    }
}
