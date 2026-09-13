use super::*;
use crate::manifest::tests::qualified_target_identity;
use crate::Pc4TerminalUseCase;
use std::{cell::Cell, collections::BTreeMap, rc::Rc, time::Instant};

const SOURCE: u32 = 1_000;

fn target() -> QualifiedPc4TargetIdentity {
    qualified_target_identity(
        "suffix-generation",
        "suffix-manifest",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    )
}

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

#[derive(Default)]
struct Guard {
    cancelled: Rc<Cell<bool>>,
    stale: Cell<bool>,
}

impl FixedQueueTraversalGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
    fn is_current_snapshot(&self, _: &QualifiedSnapshotIdentity) -> bool {
        !self.stale.get()
    }
}

struct Terminal {
    target: QualifiedPc4TargetIdentity,
    stable: bool,
    calls: usize,
}

impl Terminal {
    fn new(stable: bool) -> Self {
        Self {
            target: target(),
            stable,
            calls: 0,
        }
    }
}

impl FixedQueueTerminalPredicate for Terminal {
    type Error = &'static str;
    fn is_terminal(&mut self, query: &FixedQueueTerminalQuery<'_>) -> Result<bool, Self::Error> {
        self.calls += 1;
        Ok(query.target() == &self.target
            && query.field_id() == self.target.terminal_field().field_id())
    }
    fn qualified_field_terminal(&self) -> Option<&QualifiedPc4TargetIdentity> {
        self.stable.then_some(&self.target)
    }
}

struct Provider {
    target: QualifiedPc4TargetIdentity,
    graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>,
    calls: usize,
    reject: bool,
    cancel_on_call: Option<Rc<Cell<bool>>>,
}

impl QualifiedCompleteAdjacencyProvider for Provider {
    type Error = &'static str;
    fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }
    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
        self.calls += 1;
        if let Some(flag) = &self.cancel_on_call {
            flag.set(true);
        }
        if self.reject {
            return Err("range-not-ready");
        }
        let edges = self
            .graph
            .get(&(query.source_field_id(), query.piece()))
            .into_iter()
            .flatten()
            .map(|&destination| {
                QualifiedPc4GraphEdge::from_qualified_record(
                    query.target(),
                    query.source_field_id(),
                    query.piece(),
                    destination,
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

fn provider(graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>) -> Provider {
    Provider {
        target: target(),
        graph,
        calls: 0,
        reject: false,
        cancel_on_call: None,
    }
}

fn family(queue: &[Pc4GraphPiece], work: usize, early: bool) -> FixedQueueTraversalFamily {
    prepare_fixed_queue_traversal_family(
        FixedQueueTraversalFamilyRequest::new(
            &target(),
            SOURCE,
            queue,
            if early {
                TerminalDepthContract::PredicateMayTerminateEarly
            } else {
                TerminalDepthContract::QueueExhaustedOnly
            },
            FixedQueueTraversalBudgets::new(nz(100_000), nz(1_000), nz(64), nz(100_000)),
            FixedQueueTraversalPageBudgets::new(nz(work), nz(64)),
        ),
        &Guard::default(),
    )
    .unwrap()
}

fn drain(
    family: &FixedQueueTraversalFamily,
    cursor: &mut FixedQueueTraversalCursor,
    provider: &mut Provider,
    terminal: &mut Terminal,
) -> Vec<FixedQueueGraphPath> {
    let mut paths = Vec::new();
    let mut pages = 0;
    while !cursor.is_exhausted() {
        let page = family
            .next_page(cursor, nz(3), provider, terminal, &Guard::default())
            .unwrap();
        assert!(page.visited_state_occurrences() <= family.page_budgets().state_occurrences());
        paths.extend_from_slice(page.paths());
        pages += 1;
        assert!(pages <= 100_000, "finite fixture did not exhaust");
    }
    paths
}

fn layered(depth: usize, live: bool) -> BTreeMap<(u32, Pc4GraphPiece), Vec<u32>> {
    let mut graph = BTreeMap::new();
    for level in 0..depth {
        let sources = if level == 0 {
            vec![SOURCE]
        } else {
            vec![2 * level as u32, 2 * level as u32 + 1]
        };
        let targets = if live && level + 1 == depth {
            vec![target().terminal_field().field_id()]
        } else {
            vec![2 * (level as u32 + 1), 2 * (level as u32 + 1) + 1]
        };
        for source in sources {
            graph.insert((source, Pc4GraphPiece::I), targets.clone());
        }
    }
    graph
}

#[test]
fn pc4_suffix_dag_removes_exponential_empty_work_without_changing_paths() {
    for work in [1, 3, 64] {
        for live in [false, true] {
            let family = family(&[Pc4GraphPiece::I; 8], work, false);
            let mut baseline = provider(layered(8, live));
            let mut candidate = provider(layered(8, live));
            let mut ordinary_cursor = family.cursor();
            let mut memo_cursor = family.cursor();
            let mut ordinary_terminal = Terminal::new(false);
            let mut memo_terminal = Terminal::new(true);
            let expected = drain(
                &family,
                &mut ordinary_cursor,
                &mut baseline,
                &mut ordinary_terminal,
            );
            let actual = drain(
                &family,
                &mut memo_cursor,
                &mut candidate,
                &mut memo_terminal,
            );
            assert_eq!(
                actual, expected,
                "canonical order, prefixes and all paths must agree"
            );
            assert_eq!(actual.len(), if live { 128 } else { 0 });
            assert_eq!(baseline.calls, 255);
            assert_eq!(candidate.calls, 15);
            assert!(ordinary_terminal.calls > 0);
            assert_eq!(memo_terminal.calls, 0);
            assert!(memo_cursor.suffix_memo_hits() > 0);
            if !live {
                assert_eq!(ordinary_cursor.visited_state_occurrences(), 511);
                assert_eq!(memo_cursor.visited_state_occurrences(), 31);
                assert_eq!(memo_cursor.empty_suffixes_skipped(), 14);
            }
        }
    }
}

#[test]
fn pc4_suffix_shares_only_the_remaining_queue_and_preserves_different_prefixes() {
    use Pc4GraphPiece::{I, J, O, T};
    let graph = BTreeMap::from([
        ((SOURCE, I), vec![2]),
        ((SOURCE, J), vec![2]),
        ((2, O), vec![3]),
        ((3, T), vec![0]),
    ]);
    let first = family(&[I, O, T], 2, false);
    let second = family(&[J, O, T], 2, false);
    let memo = FixedQueueSuffixMemo::new(&target(), 1_000);
    let mut first_cursor = first.cursor();
    let mut second_cursor = second.cursor();
    first_cursor.share_suffix_memo(&memo);
    second_cursor.share_suffix_memo(&memo);
    let mut provider = provider(graph);
    let first_paths = drain(
        &first,
        &mut first_cursor,
        &mut provider,
        &mut Terminal::new(true),
    );
    assert_eq!(provider.calls, 3);
    let second_paths = drain(
        &second,
        &mut second_cursor,
        &mut provider,
        &mut Terminal::new(true),
    );
    assert_eq!(
        provider.calls, 4,
        "the second prefix needs only its own first query"
    );
    assert_eq!(first_paths.len(), 1);
    assert_eq!(second_paths.len(), 1);
    assert_eq!(first_paths[0].edges()[0].piece(), I);
    assert_eq!(second_paths[0].edges()[0].piece(), J);
    assert_eq!(&first_paths[0].edges()[1..], &second_paths[0].edges()[1..]);
}

#[test]
fn pc4_suffix_does_not_reuse_empty_proofs_across_terminal_depth_contracts() {
    use Pc4GraphPiece::{I, O};
    let memo = FixedQueueSuffixMemo::new(&target(), 1_000);
    let mut provider = provider(BTreeMap::from([((SOURCE, I), vec![0])]));
    let exhausted_only = family(&[I, O], 1, false);
    let early = family(&[I, O], 1, true);
    let mut exhausted_cursor = exhausted_only.cursor();
    let mut early_cursor = early.cursor();
    exhausted_cursor.share_suffix_memo(&memo);
    early_cursor.share_suffix_memo(&memo);
    assert!(drain(
        &exhausted_only,
        &mut exhausted_cursor,
        &mut provider,
        &mut Terminal::new(true)
    )
    .is_empty());
    let paths = drain(
        &early,
        &mut early_cursor,
        &mut provider,
        &mut Terminal::new(true),
    );
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].consumed_pieces(), 1);
}

#[test]
fn pc4_suffix_capacity_exhaustion_repeats_work_without_dropping_solutions() {
    let family = family(&[Pc4GraphPiece::I; 4], 3, false);
    let mut cursor = family.cursor();
    cursor.share_suffix_memo(&FixedQueueSuffixMemo::new(&target(), 1));
    let mut provider = provider(layered(4, true));
    let paths = drain(
        &family,
        &mut cursor,
        &mut provider,
        &mut Terminal::new(true),
    );
    assert_eq!(paths.len(), 8);
    assert_eq!(provider.calls, 15);
    assert_eq!(cursor.suffix_memo_hits(), 0);
}

#[test]
fn pc4_suffix_work_page_and_range_failure_are_not_empty_proofs() {
    use Pc4GraphPiece::I;
    let family = family(&[I; 3], 1, false);
    let memo = FixedQueueSuffixMemo::new(&target(), 1_000);
    let mut cursor = family.cursor();
    cursor.share_suffix_memo(&memo);
    let mut provider = provider(BTreeMap::from([
        ((SOURCE, I), vec![2]),
        ((2, I), vec![3]),
        ((3, I), vec![0]),
    ]));
    let mut terminal = Terminal::new(true);
    let page = family
        .next_page(
            &mut cursor,
            nz(3),
            &mut provider,
            &mut terminal,
            &Guard::default(),
        )
        .unwrap();
    assert!(page.paths().is_empty() && page.stopped_by_work_budget() && !page.is_exhausted());
    provider.reject = true;
    assert_eq!(
        family.next_page(
            &mut cursor,
            nz(3),
            &mut provider,
            &mut terminal,
            &Guard::default()
        ),
        Err(FixedQueueTraversalPageError::Provider("range-not-ready"))
    );
    assert_eq!(cursor.visited_state_occurrences(), 1);
    provider.reject = false;
    assert_eq!(
        drain(&family, &mut cursor, &mut provider, &mut terminal).len(),
        1
    );
    let mut fresh = family.cursor();
    fresh.share_suffix_memo(&memo);
    assert_eq!(
        drain(&family, &mut fresh, &mut provider, &mut terminal).len(),
        1
    );
    assert_eq!(fresh.adjacency_queries(), 0);
}

#[test]
fn pc4_suffix_cancelled_page_discards_facts_and_warm_cache_still_checks_authority() {
    let family = family(&[Pc4GraphPiece::I], 8, false);
    let mut provider = provider(BTreeMap::from([((SOURCE, Pc4GraphPiece::I), vec![0])]));
    let guard = Guard::default();
    provider.cancel_on_call = Some(Rc::clone(&guard.cancelled));
    let mut terminal = Terminal::new(true);
    let mut cursor = family.cursor();
    assert_eq!(
        family.next_page(&mut cursor, nz(3), &mut provider, &mut terminal, &guard),
        Err(FixedQueueTraversalPageError::Cancelled)
    );
    assert_eq!(cursor.visited_state_occurrences(), 0);
    guard.cancelled.set(false);
    provider.cancel_on_call = None;
    assert_eq!(
        drain(&family, &mut cursor, &mut provider, &mut terminal).len(),
        1
    );
    assert_eq!(
        provider.calls, 2,
        "cancelled query was not published into the memo"
    );
    let mut warm = family.cursor();
    warm.share_suffix_memo(&cursor.suffix_memo);
    guard.stale.set(true);
    assert_eq!(
        family.next_page(&mut warm, nz(3), &mut provider, &mut terminal, &guard),
        Err(FixedQueueTraversalPageError::StaleSnapshot)
    );
    assert_eq!(warm.visited_state_occurrences(), 0);
    guard.stale.set(false);
    provider.target = qualified_target_identity(
        "different-generation",
        "different-manifest",
        Pc4RuleProfile::Srs,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    assert!(matches!(
        family.next_page(&mut warm, nz(3), &mut provider, &mut terminal, &guard),
        Err(FixedQueueTraversalPageError::Semantic(_))
    ));
    assert_eq!(provider.calls, 2);
}

#[test]
fn pc4_suffix_rejects_terminal_mode_changes_and_cross_profile_cache_attachment() {
    let family = family(&[Pc4GraphPiece::I; 2], 1, false);
    let mut provider = provider(layered(2, true));
    let mut cursor = family.cursor();
    family
        .next_page(
            &mut cursor,
            nz(3),
            &mut provider,
            &mut Terminal::new(true),
            &Guard::default(),
        )
        .unwrap();
    assert_eq!(
        family.next_page(
            &mut cursor,
            nz(3),
            &mut provider,
            &mut Terminal::new(false),
            &Guard::default()
        ),
        Err(FixedQueueTraversalPageError::CursorInvariantViolation)
    );
    let other = qualified_target_identity(
        "suffix-generation",
        "suffix-manifest",
        Pc4RuleProfile::SrsX,
        Pc4TerminalUseCase::PcSearch,
        4,
    );
    let mut fresh = family.cursor();
    fresh.share_suffix_memo(&FixedQueueSuffixMemo::new(&other, 1_000));
    assert_eq!(
        family.next_page(
            &mut fresh,
            nz(3),
            &mut provider,
            &mut Terminal::new(true),
            &Guard::default()
        ),
        Err(FixedQueueTraversalPageError::CursorInvariantViolation)
    );
}

#[test]
#[ignore = "explicit non-publishing suffix-DP ABBA measurement"]
fn pc4_suffix_dag_abba() {
    for live in [false, true] {
        let mut elapsed = [0_u128; 2];
        let mut queries = [0_usize; 2];
        let mut visits = [0_usize; 2];
        let mut outputs = [0_usize; 2];
        for _ in 0..4 {
            for candidate in [false, true, true, false] {
                let index = usize::from(candidate);
                let family = family(&[Pc4GraphPiece::I; 10], 64, false);
                let mut provider = provider(layered(10, live));
                let mut cursor = family.cursor();
                let started = Instant::now();
                let paths = drain(
                    &family,
                    &mut cursor,
                    &mut provider,
                    &mut Terminal::new(candidate),
                );
                elapsed[index] += started.elapsed().as_nanos();
                queries[index] += provider.calls;
                visits[index] += cursor.visited_state_occurrences();
                outputs[index] += paths.len();
            }
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(queries, [8 * 1_023, 8 * 19]);
        println!("pc4_suffix_abba live={live} repetitions_each=8 elapsed_ns={elapsed:?} adjacency_queries={queries:?} state_visits={visits:?} output_paths={outputs:?}");
    }
}
