//! Owner, admission and resumable enumeration boundaries for the optional
//! row-order table. Mathematical/capability differentials live separately.

use super::*;

fn prepare(
    enumerator: &mut FamilyEnumerator,
    catalog: &GeometryCatalog,
    limit: Option<u128>,
    quantum: usize,
) -> usize {
    let mut pending = 0;
    while !enumerator.prepare_row_order_step(catalog, limit, quantum) {
        assert!(enumerator.row_order_table.is_none());
        pending += 1;
        assert!(
            pending < 100_000,
            "finite test family must finish preparation"
        );
    }
    pending
}

#[test]
fn physical_row_order_builder_preserves_owner_and_exact_admission_falls_back() {
    let catalog = full_catalog();
    let mut measured = enumerator(&catalog, true);
    let initial = measured.retained_bytes();
    let original_nodes = (2..measured.family.node_count() + 2)
        .map(|reference| measured.family.node(reference).unwrap())
        .collect::<Vec<_>>();
    assert!(prepare(&mut measured, &catalog, None, 1) > 0);
    let table_bytes = measured.row_order_table.as_ref().unwrap().retained_bytes();
    assert_eq!(Arc::strong_count(&measured.family), 1);
    assert_eq!(measured.retained_bytes(), initial + table_bytes);
    assert_eq!(
        (2..measured.family.node_count() + 2)
            .map(|reference| measured.family.node(reference).unwrap())
            .collect::<Vec<_>>(),
        original_nodes,
    );

    for one_under in [false, true] {
        let mut actual = enumerator(&catalog, true);
        let base = actual.retained_bytes();
        let cap = (base + table_bytes) as u128 - u128::from(one_under);
        prepare(&mut actual, &catalog, Some(cap), 1);
        assert_eq!(actual.row_order_table.is_some(), !one_under);
        assert!(actual.retained_bytes() as u128 <= cap);
        let (survivors, _) = collect_steps(&mut actual, &catalog, 1, false);
        assert_eq!(survivors.len(), 1);
        assert_eq!(actual.prefix_profile.emitted_candidates, 1);
        assert_eq!(
            actual.prefix_profile.row_order_pruned_subtrees > 0,
            !one_under
        );
        assert_eq!(actual.prefix_profile.pruned_subtrees > 0, one_under);
    }
}

#[test]
fn physical_row_order_future_pruning_preserves_order_and_raw_specialization() {
    let catalog = full_catalog();
    let mut raw = enumerator(&catalog, false);
    let mut raw_steps = enumerator(&catalog, false);
    let mut unbounded = Vec::new();
    while let Some(candidate) = raw.next_candidate(&catalog).unwrap() {
        unbounded.push(candidate_record(candidate));
    }
    let (stepped, pending) = collect_steps(&mut raw_steps, &catalog, 1, true);
    assert_eq!(unbounded, stepped);
    assert!(pending > 0);
    assert_eq!(
        raw.prefix_profile.emitted_candidates,
        unbounded.len() as u64
    );
    assert_eq!(
        raw_steps.prefix_profile.emitted_candidates,
        unbounded.len() as u64
    );

    let mut closure = enumerator(&catalog, true);
    let (expected, _) = collect_steps(&mut closure, &catalog, usize::MAX, true);
    for quantum in [1, 2, 8, usize::MAX] {
        let mut actual = enumerator(&catalog, true);
        prepare(&mut actual, &catalog, None, 1);
        let raw_count = actual.family.path_count(actual.tasks[0].family);
        let (survivors, _) = collect_steps(&mut actual, &catalog, quantum, true);
        assert_eq!(survivors, expected);
        assert_eq!(raw_count, Some(unbounded.len() as u128));
        assert!(actual.prefix_profile.row_order_pruned_subtrees > 0);
        assert_eq!(actual.prefix_profile.checks, 0);
        assert_eq!(
            actual.prefix_profile.emitted_candidates,
            survivors.len() as u64
        );
    }
}

#[cfg(feature = "parallel")]
#[test]
fn physical_row_order_canonical_seeds_share_table_once_and_validate_leaf_prefix() {
    let catalog = full_catalog();
    let mut actual = enumerator(&catalog, true);
    let mut reference = enumerator(&catalog, true);
    let (expected, _) = collect_steps(&mut reference, &catalog, usize::MAX, true);
    prepare(&mut actual, &catalog, None, 1);
    let table_bytes = actual.row_order_table.as_ref().unwrap().retained_bytes();
    let family_bytes = actual.family.retained_bytes();
    let counts = actual.family.path_count_table().unwrap();
    let (mut parts, shared) = actual.split_parallel_enumerators(8, counts, true).unwrap();
    assert!(parts.len() > 1);
    assert_eq!(shared, family_bytes + table_bytes);
    for part in &parts {
        assert_eq!(
            part.retained_bytes(),
            part.tasks.capacity() * core::mem::size_of::<TraversalTask>(),
        );
        assert!(Arc::ptr_eq(
            part.row_order_table.as_ref().unwrap(),
            parts[0].row_order_table.as_ref().unwrap(),
        ));
    }
    let mut merged = Vec::new();
    for part in &mut parts {
        merged.extend(collect_steps(part, &catalog, 1, true).0);
    }
    assert_eq!(merged, expected);

    let (cycle, _) = cycle_and_ordinary_rows(&catalog);
    let (mut leaf, _) = FamilyEnumerator::from_completed_family(
        i_targets(2),
        GeometrySolutionFamily::new(),
        FAMILY_EMPTY,
        2,
        None,
    );
    leaf.rows[..2].copy_from_slice(&cycle);
    leaf.tasks[0].depth = 2;
    leaf.enable_physical_prefix_pruning();
    prepare(&mut leaf, &catalog, None, 1);
    assert!(matches!(
        leaf.next_candidate_step(&catalog, true, 1).unwrap(),
        FamilyEnumerationAdvance::Complete,
    ));
    assert_eq!(leaf.prefix_profile.row_order_pruned_subtrees, 1);
    assert_eq!(leaf.prefix_profile.emitted_candidates, 0);
}

#[test]
fn physical_row_order_foreign_table_and_partial_builder_are_unavailable() {
    let catalog = full_catalog();
    let mut source = enumerator(&catalog, true);
    let mut destination = enumerator(&catalog, true);
    let builder = FamilyRowOrderBuilder::try_new(&catalog, &source.family, None).unwrap();
    assert!(builder.finish().is_none());
    let mut foreign_builder =
        FamilyRowOrderBuilder::try_new(&catalog, &source.family, None).unwrap();
    assert_eq!(
        foreign_builder.advance(&destination.family, 1),
        RowOrderBuildAdvance::Unavailable,
    );
    prepare(&mut source, &catalog, None, 1);
    destination.row_order_table = source.row_order_table.as_ref().map(Arc::clone);
    destination.row_order_attempted = true;
    assert!(!destination
        .row_order_table
        .as_ref()
        .unwrap()
        .matches(&catalog, &destination.family));
    let (survivors, _) = collect_steps(&mut destination, &catalog, 1, true);
    assert_eq!(survivors.len(), 1);
    assert!(destination.row_order_table.is_none());
    assert_eq!(destination.prefix_profile.row_order_pruned_subtrees, 0);
    assert!(destination.prefix_profile.pruned_subtrees > 0);
}

#[cfg(feature = "parallel")]
#[test]
fn physical_row_order_preparation_cancellation_is_not_domain_completion() {
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress, ProgressSink,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CancelOnRowOrder {
        handle: ExecutionCancellationHandle,
        reports: AtomicUsize,
    }
    impl ProgressSink for CancelOnRowOrder {
        fn report(&self, progress: ExecutionProgress) {
            if progress.stage == "geometry-row-order" {
                assert!(progress.completed > 0);
                assert!(progress.completed < progress.total.unwrap());
                self.reports.fetch_add(1, Ordering::Relaxed);
                self.handle.cancel();
            }
        }
    }

    let catalog = full_catalog();
    let (family, root) = product_fixture(&catalog, ROW_ORDER_PREPARATION_WORK_QUANTUM * 2, false);
    let (enumerator, raw_count) =
        FamilyEnumerator::from_completed_family(i_targets(3), family, root, 3, None);
    let mut search = GeometrySearch::placeholder();
    search.enumerator = Some(enumerator);
    search.candidate_family_count = raw_count;
    search.enable_physical_prefix_pruning();
    let token = ExecutionCancellationToken::new();
    let sink = Arc::new(CancelOnRowOrder {
        handle: token.handle(),
        reports: AtomicUsize::new(0),
    });
    let control = ExecutionControl::new(token).with_progress_sink(sink.clone());
    assert!(matches!(
        search.prepare_parallel(&catalog, &control, 9, |_, requested| requested),
        Err(WasmExactSearchError::Cancelled),
    ));
    assert_eq!(sink.reports.load(Ordering::Relaxed), 1);
}
