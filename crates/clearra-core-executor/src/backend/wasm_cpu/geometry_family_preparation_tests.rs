//! Preparation scheduling only: compare existing serial table recurrences and
//! canonical traversal, without running a queue/probability search.

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use clearra_core_domain::execution_cancellation::CancellationHandle;
use clearra_core_domain::execution_cancellation::CancellationToken;
use clearra_core_domain::execution_cancellation::ExecutionProgress;
use clearra_core_domain::execution_cancellation::ProgressSink;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::resource::ResourceLease;
use clearra_core_domain::resource::ResourceLeaseCapacity;
use clearra_core_domain::resource::ResourceLeaseOwnerId;
use clearra_core_domain::resource::ResourceLeaseRequest;
use clearra_core_domain::resource::SharedResourceLeaseAuthority;
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::pattern_universe::PieceMultisetKey;

use super::super::piece_index;
use super::super::GeometrySearch;
use super::super::ParallelGeometryPreparation;
use super::*;

type CandidateSequence = Vec<(u32, StandardBoard64TilingIdentity, Vec<u32>)>;

pub(super) fn isolated_thread_memory() -> Option<ResourceLease> {
    // A private authority keeps forced-thread regressions independent of other
    // tests that legitimately reserve the whole global memory surface.
    let bytes = (SCOPED_THREAD_STACK_BYTES + 64 * 1024) as u64;
    let authority = SharedResourceLeaseAuthority::new(ResourceLeaseCapacity::new(1, bytes)?);
    authority
        .try_acquire(
            ResourceLeaseOwnerId::new(1)?,
            ResourceLeaseRequest::memory_only(bytes)?,
        )
        .ok()
}

fn catalog(height: u8) -> GeometryCatalog {
    GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        height,
        0,
        (1_u64 << (u32::from(height) * 10)) - 1,
    )
    .unwrap()
}

fn i_row(catalog: &GeometryCatalog, x: u32, rows: &[u32]) -> u32 {
    let cells = rows
        .iter()
        .fold(0, |cells, y| cells | (1_u64 << (y * 10 + x)));
    catalog.skeleton_id(PieceKind::I, cells).unwrap()
}

fn targets() -> Arc<[TargetGroup]> {
    let mut counts = [0; 7];
    counts[piece_index(PieceKind::I)] = 3;
    Arc::from([TargetGroup {
        key: PieceMultisetKey::from_counts(counts),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::all(1)),
        pattern_index: None,
    }])
}

fn fixture(catalog: &GeometryCatalog) -> (GeometrySolutionFamily, u32) {
    let mut family = GeometrySolutionFamily::new();
    let cycle_b = family
        .append(i_row(catalog, 1, &[0, 1, 3, 4]), FAMILY_EMPTY)
        .unwrap();
    let blocked = family
        .append(i_row(catalog, 0, &[0, 2, 3, 4]), cycle_b)
        .unwrap();
    let good_b = family
        .append(i_row(catalog, 3, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let good = family
        .append(i_row(catalog, 2, &[0, 1, 2, 3]), good_b)
        .unwrap();
    let choice = family.union(blocked, good).unwrap();
    let tail_a = family
        .append(i_row(catalog, 4, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let tail_b = family
        .append(i_row(catalog, 5, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let tail = family.union(tail_a, tail_b).unwrap();
    let root = family.product(choice, tail).unwrap();
    (family, root)
}

fn reference_tables(
    family: &Arc<GeometrySolutionFamily>,
    catalog: &GeometryCatalog,
) -> PreparedFamilyTables {
    let row_order =
        FamilyRowOrderBuilder::try_new(catalog, family, None).and_then(|mut builder| loop {
            match builder.advance(family, 1) {
                RowOrderBuildAdvance::Pending => {}
                RowOrderBuildAdvance::Complete => break builder.finish(),
                RowOrderBuildAdvance::Unavailable => break None,
            }
        });
    PreparedFamilyTables {
        path_counts: family.path_count_table(),
        row_order,
    }
}

fn assert_tables_equal(
    expected: &PreparedFamilyTables,
    actual: &PreparedFamilyTables,
    family: &Arc<GeometrySolutionFamily>,
    catalog: &GeometryCatalog,
) {
    assert_eq!(actual.path_counts, expected.path_counts);
    match (&expected.row_order, &actual.row_order) {
        (Some(expected), Some(actual)) => {
            assert!(actual.matches(catalog, family));
            assert_eq!(actual.retained_bytes(), expected.retained_bytes());
            for reference in 0..family.node_count() + 2 {
                assert_eq!(actual.mask(reference), expected.mask(reference));
            }
            for row in 0..catalog.skeleton_count() as u32 {
                assert_eq!(actual.row_mask(row), expected.row_mask(row));
            }
        }
        (None, None) => {}
        _ => panic!("optional-table availability changed"),
    }
}

fn prepared_enumerator(
    family: &Arc<GeometrySolutionFamily>,
    root: u32,
    tables: PreparedFamilyTables,
) -> (FamilyEnumerator, Vec<u128>) {
    let counts = tables.path_counts.unwrap();
    let mut enumerator = FamilyEnumerator::from_shared_completed_family(
        targets(),
        Arc::clone(family),
        root,
        3,
        Some(counts[root as usize]),
    );
    enumerator.row_order_attempted = true;
    enumerator.row_order_table = tables.row_order.map(Arc::new);
    enumerator.enable_physical_prefix_pruning();
    (enumerator, counts)
}

fn collect(enumerator: &mut FamilyEnumerator, catalog: &GeometryCatalog) -> CandidateSequence {
    let mut candidates = Vec::new();
    while let Some(candidate) = enumerator.next_candidate(catalog).unwrap() {
        candidates.push((
            candidate.target_index,
            candidate.identity,
            candidate.row_ids().to_vec(),
        ));
    }
    candidates
}

#[test]
fn family_preparation_overlap_preserves_all_tables_and_canonical_partition_order() {
    let catalog = catalog(6);
    let (family, root) = fixture(&catalog);
    let family = Arc::new(family);
    let nodes = (2..family.node_count() + 2)
        .map(|id| family.node(id).unwrap())
        .collect::<Vec<_>>();
    let expected = reference_tables(&family, &catalog);
    assert_eq!(expected.path_counts.as_ref().unwrap()[root as usize], 4);
    let (mut reference, _) = prepared_enumerator(&family, root, expected);
    let expected_candidates = collect(&mut reference, &catalog);
    assert_eq!(expected_candidates.len(), 2);
    assert!(reference.prefix_profile.row_order_pruned_subtrees > 0);

    for dispatch in [
        PreparationDispatch::ScopedWithIsolatedMemory,
        PreparationDispatch::SpawnUnavailable,
        PreparationDispatch::MemoryUnavailable,
    ] {
        let owners = Arc::strong_count(&family);
        let actual =
            prepare_tables(&family, &catalog, &ExecutionControl::default(), dispatch).unwrap();
        assert_tables_equal(
            &reference_tables(&family, &catalog),
            &actual,
            &family,
            &catalog,
        );
        assert_eq!(
            Arc::strong_count(&family),
            owners,
            "table uses only a Weak family binding"
        );
        let (mut serial, _) = prepared_enumerator(&family, root, actual);
        assert_eq!(collect(&mut serial, &catalog), expected_candidates);

        let tables =
            prepare_tables(&family, &catalog, &ExecutionControl::default(), dispatch).unwrap();
        let (parallel, counts) = prepared_enumerator(&family, root, tables);
        let (mut shards, _) = parallel
            .split_parallel_enumerators(4, counts, true)
            .unwrap();
        assert!(shards.len() > 1);
        let actual_candidates = shards
            .iter_mut()
            .flat_map(|shard| collect(shard, &catalog))
            .collect::<Vec<_>>();
        assert_eq!(actual_candidates, expected_candidates);
    }
    assert_eq!(
        (2..family.node_count() + 2)
            .map(|id| family.node(id).unwrap())
            .collect::<Vec<_>>(),
        nodes
    );
}

#[test]
fn family_preparation_keeps_overflow_and_optional_table_unavailability_distinct() {
    let catalog = catalog(6);
    let mut family = GeometrySolutionFamily::new();
    let a = family
        .append(i_row(&catalog, 0, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let b = family
        .append(i_row(&catalog, 1, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let mut overflow = family.union(a, b).unwrap();
    for _ in 0..7 {
        overflow = family.product(overflow, overflow).unwrap();
    }
    let family = Arc::new(family);
    assert!(
        family.path_count(a).is_none(),
        "existing whole-table overflow includes unreachable nodes"
    );
    for dispatch in [
        PreparationDispatch::Sequential,
        PreparationDispatch::ScopedWithIsolatedMemory,
        PreparationDispatch::SpawnUnavailable,
        PreparationDispatch::MemoryUnavailable,
    ] {
        let actual =
            prepare_tables(&family, &catalog, &ExecutionControl::default(), dispatch).unwrap();
        assert!(actual.path_counts.is_none());
        assert!(actual.row_order.is_some());
        assert_tables_equal(
            &reference_tables(&family, &catalog),
            &actual,
            &family,
            &catalog,
        );
    }

    let unsupported_catalog = self::catalog(4);
    let mut small = GeometrySolutionFamily::new();
    let root = small
        .append(i_row(&unsupported_catalog, 0, &[0, 1, 2, 3]), FAMILY_EMPTY)
        .unwrap();
    let small = Arc::new(small);
    let actual = prepare_tables(
        &small,
        &unsupported_catalog,
        &ExecutionControl::default(),
        PreparationDispatch::Scoped,
    )
    .unwrap();
    assert_eq!(actual.path_counts.as_ref().unwrap()[root as usize], 1);
    assert!(
        actual.row_order.is_none(),
        "unsupported table is an optional fallback, not count failure"
    );
}

struct CancelOnRowOrder {
    handle: CancellationHandle,
    reports: AtomicUsize,
    reporter: Mutex<Option<thread::ThreadId>>,
}

impl ProgressSink for CancelOnRowOrder {
    fn report(&self, progress: ExecutionProgress) {
        if progress.stage == "geometry-row-order" {
            self.reports.fetch_add(1, Ordering::Relaxed);
            *self.reporter.lock().unwrap() = Some(thread::current().id());
            self.handle.cancel();
        }
    }
}

#[test]
fn family_preparation_cancellation_joins_worker_and_never_publishes_partial_tables() {
    let catalog = catalog(6);
    let (family, _) = fixture(&catalog);
    let family = Arc::new(family);
    for dispatch in [
        PreparationDispatch::ScopedWithIsolatedMemory,
        PreparationDispatch::SpawnUnavailable,
    ] {
        let token = CancellationToken::new();
        token.handle().cancel();
        let control = ExecutionControl::new(token);
        assert!(matches!(
            prepare_tables(&family, &catalog, &control, dispatch),
            Err(WasmExactSearchError::Cancelled)
        ));

        let token = CancellationToken::new();
        let sink = Arc::new(CancelOnRowOrder {
            handle: token.handle(),
            reports: AtomicUsize::new(0),
            reporter: Mutex::new(None),
        });
        let control = ExecutionControl::new(token).with_progress_sink(sink.clone());
        assert!(matches!(
            prepare_tables(&family, &catalog, &control, dispatch),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(sink.reports.load(Ordering::Relaxed), 1);
        let reporter = sink.reporter.lock().unwrap().unwrap();
        assert_eq!(
            reporter == thread::current().id(),
            matches!(dispatch, PreparationDispatch::SpawnUnavailable)
        );
        assert_eq!(Arc::strong_count(&family), 1);
        assert_eq!(
            Arc::weak_count(&family),
            0,
            "joined cancellation drops the unfinished table owner"
        );
    }
}

#[test]
fn family_preparation_policy_keeps_small_single_worker_and_bounded_requests_serial() {
    assert!(!overlap_is_worthwhile(
        MIN_PARALLEL_FAMILY_NODES - 1,
        9,
        None
    ));
    assert!(!overlap_is_worthwhile(MIN_PARALLEL_FAMILY_NODES, 1, None));
    assert!(!overlap_is_worthwhile(MIN_PARALLEL_FAMILY_NODES, 0, None));
    assert!(overlap_is_worthwhile(MIN_PARALLEL_FAMILY_NODES, 2, None));
    for limit in [0, u128::MAX] {
        assert!(!overlap_is_worthwhile(
            MIN_PARALLEL_FAMILY_NODES,
            9,
            Some(limit)
        ));
    }
    let catalog = catalog(6);
    let (family, root) = fixture(&catalog);
    let mut compiler = FamilyCompiler::new(catalog.required_cells(), targets());
    compiler.family = family;
    compiler.root_family = root;
    compiler.target_depth = 3;
    assert!(!compiler.can_overlap_preparation(9));
    compiler.family.set_retained_limit_bytes(Some(u128::MAX));
    assert_eq!(compiler.family.retained_limit_bytes(), Some(u128::MAX));
    assert!(!compiler.can_overlap_preparation(9));
}

#[test]
fn family_preparation_consumed_compiler_releases_interner_and_preserves_family_owner() {
    let catalog = catalog(6);
    let (family, root) = fixture(&catalog);
    let expected_counts = family.path_count_table().unwrap();
    let nodes = (2..family.node_count() + 2)
        .map(|id| family.node(id).unwrap())
        .collect::<Vec<_>>();
    assert!(family.unbounded_interning_active());
    let before = family.retained_bytes();
    let mut compiler = FamilyCompiler::new(catalog.required_cells(), targets());
    compiler.family = family;
    compiler.root_family = root;
    compiler.target_depth = 3;
    let mut prepared = compiler
        .into_overlapped_enumerator(&catalog, &ExecutionControl::default())
        .unwrap();
    assert_eq!(prepared.candidate_count, Some(4));
    assert_eq!(prepared.path_counts, Some(expected_counts));
    let enumerator = &mut prepared.enumerator;
    assert!(!enumerator.family.unbounded_interning_active());
    assert!(enumerator.family.retained_bytes() < before);
    assert_eq!(Arc::strong_count(&enumerator.family), 1);
    assert_eq!(
        (2..enumerator.family.node_count() + 2)
            .map(|id| enumerator.family.node(id).unwrap())
            .collect::<Vec<_>>(),
        nodes
    );
    assert!(enumerator.row_order_attempted);
    assert!(!enumerator.needs_row_order_preparation());
    assert!(enumerator
        .row_order_table
        .as_ref()
        .unwrap()
        .matches(&catalog, &enumerator.family));
    enumerator.enable_physical_prefix_pruning();
    assert!(!enumerator.needs_row_order_preparation());
    assert_eq!(collect(enumerator, &catalog).len(), 2);
}

#[derive(Default)]
struct RowOrderReporter(Mutex<Option<thread::ThreadId>>);

impl ProgressSink for RowOrderReporter {
    fn report(&self, progress: ExecutionProgress) {
        if progress.stage == "geometry-row-order" {
            *self.0.lock().unwrap() = Some(thread::current().id());
        }
    }
}

#[test]
fn family_preparation_large_stage_uses_product_dispatch_without_enumerating_source_queues() {
    let catalog = catalog(6);
    let (mut family, root) = fixture(&catalog);
    // A large immutable DAG with the same four-path root. Unreachable nodes
    // exercise the real whole-table preparation threshold without increasing
    // candidate or source enumeration. Every node still has a valid row ID.
    let ordinary = i_row(&catalog, 6, &[0, 1, 2, 3]);
    let mut tail = FAMILY_EMPTY;
    while family.node_count() < MIN_PARALLEL_FAMILY_NODES {
        tail = family.append(ordinary, tail).unwrap();
    }
    let mut compiler = FamilyCompiler::new(catalog.required_cells(), targets());
    compiler.family = family;
    compiler.root_family = root;
    compiler.target_depth = 3;
    compiler.stack.clear();
    assert!(compiler.can_overlap_preparation(2));
    compiler.resource_authoritative = true;
    assert!(!compiler.can_overlap_preparation(2));
    compiler.resource_authoritative = false;

    let reporter = Arc::new(RowOrderReporter::default());
    let control = ExecutionControl::default().with_progress_sink(reporter.clone());
    let mut search = GeometrySearch::placeholder();
    search.compiler = Some(compiler);
    search.enable_physical_prefix_pruning();
    let prepared = search
        .prepare_parallel(&catalog, &control, 2, |count, requested| {
            assert_eq!(count, 4);
            assert_eq!(requested, 2);
            1 // Inspect the prepared owner before any solver jobs are dispatched.
        })
        .unwrap();
    let ParallelGeometryPreparation::Serial { mut geometry, .. } = prepared else {
        panic!("test requests the prepared serial owner");
    };
    // Global memory admission may legitimately select the serial fallback if
    // another test owns that surface. Forced scoped execution is independently
    // covered above with an isolated authority and an actual worker thread.
    assert!(reporter.0.lock().unwrap().is_some());
    assert_eq!(geometry.candidate_family_count(), Some(4));
    let enumerator = geometry.enumerator.as_mut().unwrap();
    assert!(!enumerator.needs_row_order_preparation());
    assert!(enumerator
        .row_order_table
        .as_ref()
        .unwrap()
        .matches(&catalog, &enumerator.family));
    assert_eq!(collect(enumerator, &catalog).len(), 2);
}
