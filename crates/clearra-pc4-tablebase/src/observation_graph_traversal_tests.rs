use core::{cell::Cell, num::NonZeroUsize};
use std::{collections::BTreeMap, rc::Rc};

use super::*;
use crate::manifest::tests::qualified_target_identity;
use crate::{
    FixedQueueAdjacencyQuery, FixedQueueHoldBudgets, FixedQueueHoldDecision, FixedQueueHoldState,
    FixedQueueTerminalQuery, Pc4BagProfile, Pc4BagRevealBudgets, Pc4BagState, Pc4GraphPiece,
    Pc4ObservationFrontierBudgets, Pc4ObservationFrontierRequest, Pc4RuleProfile,
    Pc4TerminalUseCase, QualifiedCompleteAdjacency, QualifiedPc4GraphEdge,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderError {
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalError {
    Rejected,
}

struct Guard {
    cancelled: Rc<Cell<bool>>,
    stale: Rc<Cell<bool>>,
}

impl FixedQueueTraversalGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    fn is_current_snapshot(&self, _expected: &QualifiedSnapshotIdentity) -> bool {
        !self.stale.get()
    }
}

struct Provider {
    target: QualifiedPc4TargetIdentity,
    graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>,
    calls: Vec<(u32, Pc4GraphPiece, usize)>,
    fail: bool,
    cancel_during_call: Option<Rc<Cell<bool>>>,
    stale_during_call: Option<Rc<Cell<bool>>>,
}

impl QualifiedCompleteAdjacencyProvider for Provider {
    type Error = ProviderError;

    fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
        self.calls
            .push((query.source_field_id(), query.piece(), query.queue_index()));
        if let Some(cancelled) = &self.cancel_during_call {
            cancelled.set(true);
        }
        if let Some(stale) = &self.stale_during_call {
            stale.set(true);
        }
        if self.fail {
            return Err(ProviderError::Rejected);
        }
        let edges = self
            .graph
            .get(&(query.source_field_id(), query.piece()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|target_field_id| {
                QualifiedPc4GraphEdge::from_qualified_record(
                    query.target(),
                    query.source_field_id(),
                    query.piece(),
                    target_field_id,
                )
            })
            .collect();
        Ok(QualifiedCompleteAdjacency::from_qualified_provider(
            query.target(),
            query.source_field_id(),
            query.piece(),
            query.queue_index(),
            edges,
        ))
    }
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("positive synthetic budget")
}

fn guard() -> Guard {
    Guard {
        cancelled: Rc::new(Cell::new(false)),
        stale: Rc::new(Cell::new(false)),
    }
}

fn target(use_case: Pc4TerminalUseCase, lines: u8) -> QualifiedPc4TargetIdentity {
    qualified_target_identity(
        "observation-graph-generation",
        "observation-graph-manifest",
        Pc4RuleProfile::Srs,
        use_case,
        lines,
    )
}

fn frontier() -> Pc4ObservationFrontierFamily {
    let bag_profile = Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("synthetic bag");
    let bag_state =
        Pc4BagState::new(bag_profile, [1, 1, 0, 0, 0, 0, 0], 3).expect("synthetic state");
    let budgets = Pc4ObservationFrontierBudgets::new(
        Pc4BagRevealBudgets::new(
            nonzero(8),
            nonzero(1_000),
            nonzero(1_000),
            nonzero(1),
            nonzero(1_000),
            nonzero(1_000),
        ),
        FixedQueueHoldBudgets::new(
            nonzero(1_000),
            nonzero(1_000),
            nonzero(1_000),
            nonzero(1_000),
        ),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(8),
        nonzero(128),
    );
    crate::prepare_pc4_observation_frontier(
        Pc4ObservationFrontierRequest::new(
            &[Pc4GraphPiece::I],
            0,
            bag_state,
            1,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
            budgets,
        ),
        &|| false,
    )
    .expect("synthetic frontier")
}

fn composite_budgets() -> Pc4ObservationGraphBudgets {
    Pc4ObservationGraphBudgets::new(
        nonzero(32),
        nonzero(1_000),
        nonzero(1_000),
        nonzero(1_000),
        nonzero(32),
        nonzero(64),
        nonzero(64),
    )
}

fn graph_budgets() -> FixedQueueTraversalBudgets {
    FixedQueueTraversalBudgets::new(nonzero(100), nonzero(100), nonzero(8), nonzero(100))
}

fn family_with(
    target: QualifiedPc4TargetIdentity,
    budgets: Pc4ObservationGraphBudgets,
    graph_page_budgets: FixedQueueTraversalPageBudgets,
) -> Pc4ObservationGraphFamily {
    prepare_pc4_observation_graph_family(
        Pc4ObservationGraphRequest::new(
            target,
            7,
            frontier(),
            TerminalDepthContract::QueueExhaustedOnly,
            graph_budgets(),
            graph_page_budgets,
            budgets,
        ),
        &guard(),
    )
    .expect("synthetic composite family")
}

fn family() -> Pc4ObservationGraphFamily {
    family_with(
        target(Pc4TerminalUseCase::PcSearch, 4),
        composite_budgets(),
        FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
    )
}

fn provider(family: &Pc4ObservationGraphFamily) -> Provider {
    Provider {
        target: family.target().clone(),
        graph: BTreeMap::from([
            ((7, Pc4GraphPiece::I), vec![11, 10, 10]),
            ((7, Pc4GraphPiece::T), vec![20]),
        ]),
        calls: Vec::new(),
        fail: false,
        cancel_during_call: None,
        stale_during_call: None,
    }
}

fn terminal(query: &FixedQueueTerminalQuery<'_>) -> Result<bool, TerminalError> {
    if query.target_lines().get() != 4 || query.use_case() != Pc4TerminalUseCase::PcSearch {
        return Err(TerminalError::Rejected);
    }
    Ok(query.queue_is_exhausted())
}

fn drain(family: &Pc4ObservationGraphFamily, page_size: usize) -> Vec<Pc4ObservationGraphPath> {
    let mut provider = provider(family);
    let mut cursor = family.cursor();
    let mut paths = Vec::new();
    while !cursor.is_exhausted() {
        let page = family
            .next_page(
                &mut cursor,
                nonzero(page_size),
                &mut provider,
                &mut terminal,
                &guard(),
            )
            .expect("synthetic composite page");
        paths.extend_from_slice(page.paths());
    }
    paths
}

fn cursor_state(
    cursor: &Pc4ObservationGraphCursor,
) -> (usize, usize, usize, usize, u128, Option<u128>, bool) {
    (
        cursor.frontier_entries_started(),
        cursor.visited_state_occurrences(),
        cursor.adjacency_queries(),
        cursor.emitted_paths(),
        cursor.frontier_next_entry_index(),
        cursor.active_frontier_entry_index(),
        cursor.is_exhausted(),
    )
}

#[test]
fn exact_probability_target_source_and_replay_evidence_survive_graph_paging() {
    let family = family();
    let paths = drain(&family, 64);

    assert_eq!(paths.len(), 6);
    assert!(paths.iter().all(|path| path.source_field_id() == 7));
    assert!(paths
        .iter()
        .all(|path| path.graph_path().start_field_id() == 7));
    assert!(paths.iter().all(|path| path.target() == family.target()));
    assert!(paths.iter().all(|path| {
        path.probability().numerator() == 1 && path.probability().denominator() == 2
    }));
    assert_eq!(
        paths
            .iter()
            .map(|path| {
                (
                    path.frontier_entry().reveal_rank(),
                    path.frontier_entry().hold_path_index(),
                    path.frontier_entry().hold_path().steps()[0].decision(),
                    path.graph_path().terminal_field_id(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (0, 0, FixedQueueHoldDecision::UseCurrent, 10),
            (0, 0, FixedQueueHoldDecision::UseCurrent, 11),
            (0, 1, FixedQueueHoldDecision::SwapHeld, 20),
            (1, 0, FixedQueueHoldDecision::UseCurrent, 10),
            (1, 0, FixedQueueHoldDecision::UseCurrent, 11),
            (1, 1, FixedQueueHoldDecision::SwapHeld, 20),
        ]
    );
    let reveal_probabilities = paths
        .iter()
        .filter(|path| {
            path.frontier_entry().hold_path_index() == 0
                && path.graph_path().terminal_field_id() == 10
        })
        .map(|path| {
            (
                path.frontier_entry().reveal_rank(),
                path.probability().numerator(),
                path.probability().denominator(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(reveal_probabilities, vec![(0, 1, 2), (1, 1, 2)]);
}

#[test]
fn canonical_result_family_is_independent_of_composite_page_size() {
    let family = family();
    assert_eq!(drain(&family, 1), drain(&family, 64));
}

#[test]
fn first_small_page_does_not_expand_later_observation_branches() {
    let family = family();
    let mut provider = provider(&family);
    let mut cursor = family.cursor();
    let page = family
        .next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard(),
        )
        .expect("first bounded page");

    assert_eq!(page.paths().len(), 1);
    assert_eq!(page.frontier_entries_started(), 1);
    assert_eq!(cursor.frontier_entries_started(), 1);
    assert_eq!(cursor.frontier_next_entry_index(), 1);
    assert_eq!(provider.calls, vec![(7, Pc4GraphPiece::I, 0)]);
}

#[test]
fn cursor_is_bound_to_the_exact_target_family() {
    let first = family();
    let second = family_with(
        target(Pc4TerminalUseCase::SetupSearch, 3),
        composite_budgets(),
        FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
    );
    let mut cursor = second.cursor();
    let before = cursor_state(&cursor);
    let mut provider = provider(&first);

    assert!(matches!(
        first.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::CursorMismatch)
    ));
    assert_eq!(cursor_state(&cursor), before);
    assert!(provider.calls.is_empty());
}

#[test]
fn provider_binding_mismatch_is_typed_and_transactional() {
    let family = family();
    let mut cursor = family.cursor();
    let before = cursor_state(&cursor);
    let mut provider = provider(&family);
    provider.target = qualified_target_identity(
        "observation-graph-generation",
        "observation-graph-manifest",
        Pc4RuleProfile::Jstris180,
        Pc4TerminalUseCase::PcSearch,
        4,
    );

    assert!(matches!(
        family.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::Traversal(
            FixedQueueTraversalPageError::Semantic(_)
        ))
    ));
    assert_eq!(cursor_state(&cursor), before);
    assert!(provider.calls.is_empty());
}

#[test]
fn provider_failure_does_not_commit_frontier_or_graph_progress() {
    let family = family();
    let mut cursor = family.cursor();
    let before = cursor_state(&cursor);
    let mut provider = provider(&family);
    provider.fail = true;

    assert_eq!(
        family.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::Traversal(
            FixedQueueTraversalPageError::Provider(ProviderError::Rejected)
        ))
    );
    assert_eq!(cursor_state(&cursor), before);
    assert_eq!(provider.calls.len(), 1);
}

#[test]
fn cancellation_and_snapshot_drift_during_provider_callback_are_transactional() {
    for cancel in [true, false] {
        let family = family();
        let mut cursor = family.cursor();
        let before = cursor_state(&cursor);
        let guard = guard();
        let mut provider = provider(&family);
        if cancel {
            provider.cancel_during_call = Some(Rc::clone(&guard.cancelled));
        } else {
            provider.stale_during_call = Some(Rc::clone(&guard.stale));
        }

        let result = family.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard,
        );
        if cancel {
            assert!(matches!(
                result,
                Err(Pc4ObservationGraphPageError::Cancelled)
            ));
        } else {
            assert!(matches!(
                result,
                Err(Pc4ObservationGraphPageError::StaleSnapshot)
            ));
        }
        assert_eq!(cursor_state(&cursor), before);
        assert_eq!(provider.calls.len(), 1);
    }
}

#[test]
fn entry_and_state_budgets_fail_without_committing_the_attempt() {
    let entry_limited = family_with(
        target(Pc4TerminalUseCase::PcSearch, 4),
        Pc4ObservationGraphBudgets::new(
            nonzero(1),
            nonzero(100),
            nonzero(100),
            nonzero(100),
            nonzero(1),
            nonzero(8),
            nonzero(8),
        ),
        FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
    );
    let mut entry_provider = provider(&entry_limited);
    let mut cursor = entry_limited.cursor();
    let first = entry_limited
        .next_page(
            &mut cursor,
            nonzero(8),
            &mut entry_provider,
            &mut terminal,
            &guard(),
        )
        .expect("first entry page");
    assert_eq!(first.paths().len(), 2);
    assert!(first.stopped_by_work_budget());
    let before = cursor_state(&cursor);
    assert!(matches!(
        entry_limited.next_page(
            &mut cursor,
            nonzero(8),
            &mut entry_provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::BudgetExceeded(
            Pc4ObservationGraphBudgetExceeded {
                kind: Pc4ObservationGraphBudgetKind::FrontierEntries,
                limit: 1,
                attempted: 2,
            }
        ))
    ));
    assert_eq!(cursor_state(&cursor), before);

    let state_limited = family_with(
        target(Pc4TerminalUseCase::PcSearch, 4),
        Pc4ObservationGraphBudgets::new(
            nonzero(8),
            nonzero(1),
            nonzero(100),
            nonzero(100),
            nonzero(8),
            nonzero(8),
            nonzero(8),
        ),
        FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
    );
    let mut provider = provider(&state_limited);
    let mut cursor = state_limited.cursor();
    let before = cursor_state(&cursor);
    assert!(matches!(
        state_limited.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::BudgetExceeded(
            Pc4ObservationGraphBudgetExceeded {
                kind: Pc4ObservationGraphBudgetKind::VisitedStateOccurrences,
                limit: 1,
                attempted: 2,
            }
        ))
    ));
    assert_eq!(cursor_state(&cursor), before);
}

#[test]
fn page_work_limits_resume_an_active_graph_branch() {
    let family = family_with(
        target(Pc4TerminalUseCase::PcSearch, 4),
        Pc4ObservationGraphBudgets::new(
            nonzero(32),
            nonzero(1_000),
            nonzero(1_000),
            nonzero(1_000),
            nonzero(1),
            nonzero(1),
            nonzero(8),
        ),
        FixedQueueTraversalPageBudgets::new(nonzero(1), nonzero(8)),
    );
    let mut provider = provider(&family);
    let mut cursor = family.cursor();
    let first = family
        .next_page(
            &mut cursor,
            nonzero(8),
            &mut provider,
            &mut terminal,
            &guard(),
        )
        .expect("bounded work page");

    assert!(first.paths().is_empty());
    assert!(first.stopped_by_work_budget());
    assert_eq!(first.graph_calls(), 1);
    assert_eq!(cursor.active_frontier_entry_index(), Some(0));

    let mut paths = Vec::new();
    while !cursor.is_exhausted() {
        let page = family
            .next_page(
                &mut cursor,
                nonzero(8),
                &mut provider,
                &mut terminal,
                &guard(),
            )
            .expect("resumed work page");
        paths.extend_from_slice(page.paths());
    }
    assert_eq!(paths.len(), 6);
}

#[test]
fn graph_dead_ends_emit_no_result_and_do_not_stall_the_frontier() {
    let family = family();
    let mut provider = provider(&family);
    provider.graph.clear();
    let mut cursor = family.cursor();
    let mut pages = 0usize;
    let mut emitted = 0usize;
    while !cursor.is_exhausted() {
        let page = family
            .next_page(
                &mut cursor,
                nonzero(8),
                &mut provider,
                &mut terminal,
                &guard(),
            )
            .expect("dead-end page");
        pages += 1;
        emitted += page.paths().len();
        assert!(pages < 16, "bounded dead ends must converge");
    }
    assert_eq!(emitted, 0);
    assert_eq!(cursor.frontier_entries_started(), 4);
}

#[test]
fn page_limit_and_initial_guard_failures_do_no_work() {
    let family = family();
    let mut cursor = family.cursor();
    let before = cursor_state(&cursor);
    let mut provider = provider(&family);
    assert!(matches!(
        family.next_page(
            &mut cursor,
            nonzero(65),
            &mut provider,
            &mut terminal,
            &guard(),
        ),
        Err(Pc4ObservationGraphPageError::PageLimitExceeded {
            limit: 64,
            attempted: 65,
        })
    ));
    assert_eq!(cursor_state(&cursor), before);
    assert!(provider.calls.is_empty());

    let cancelled_guard = guard();
    cancelled_guard.cancelled.set(true);
    assert!(matches!(
        family.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &cancelled_guard,
        ),
        Err(Pc4ObservationGraphPageError::Cancelled)
    ));
    assert_eq!(cursor_state(&cursor), before);
    assert!(provider.calls.is_empty());

    let stale_guard = guard();
    stale_guard.stale.set(true);
    assert!(matches!(
        family.next_page(
            &mut cursor,
            nonzero(1),
            &mut provider,
            &mut terminal,
            &stale_guard,
        ),
        Err(Pc4ObservationGraphPageError::StaleSnapshot)
    ));
    assert_eq!(cursor_state(&cursor), before);
    assert!(provider.calls.is_empty());
}

#[test]
fn prepare_rejects_cancelled_or_stale_target_without_graph_work() {
    let cancelled_guard = guard();
    cancelled_guard.cancelled.set(true);
    assert_eq!(
        prepare_pc4_observation_graph_family(
            Pc4ObservationGraphRequest::new(
                target(Pc4TerminalUseCase::PcSearch, 4),
                7,
                frontier(),
                TerminalDepthContract::QueueExhaustedOnly,
                graph_budgets(),
                FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
                composite_budgets(),
            ),
            &cancelled_guard,
        )
        .expect_err("cancelled prepare"),
        Pc4ObservationGraphPrepareError::Cancelled
    );

    let stale_guard = guard();
    stale_guard.stale.set(true);
    assert_eq!(
        prepare_pc4_observation_graph_family(
            Pc4ObservationGraphRequest::new(
                target(Pc4TerminalUseCase::PcSearch, 4),
                7,
                frontier(),
                TerminalDepthContract::QueueExhaustedOnly,
                graph_budgets(),
                FixedQueueTraversalPageBudgets::new(nonzero(8), nonzero(8)),
                composite_budgets(),
            ),
            &stale_guard,
        )
        .expect_err("stale prepare"),
        Pc4ObservationGraphPrepareError::StaleSnapshot
    );
}
