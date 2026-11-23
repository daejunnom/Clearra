use std::collections::BTreeSet;

use clearra_core_domain::execution_cancellation::CancellationToken;

use super::{
    GeometryDownView, GeometrySolutionFamily, ZddArena, ZddError, ZddId, ZddLimits, FAMILY_EMPTY,
};

fn arena() -> ZddArena {
    ZddArena::new(ZddLimits::default())
}

fn singleton(arena: &mut ZddArena, variables: &[u32]) -> ZddId {
    variables
        .iter()
        .rev()
        .fold(ZddId::UNIT, |child, &variable| {
            arena
                .mk(variable, ZddId::EMPTY, child)
                .expect("ordered singleton")
        })
}

fn family(arena: &mut ZddArena, sets: &[&[u32]]) -> ZddId {
    sets.iter().fold(ZddId::EMPTY, |root, variables| {
        let member = singleton(arena, variables);
        arena.union(root, member).expect("set union")
    })
}

fn members(arena: &mut ZddArena, root: ZddId) -> BTreeSet<Vec<u32>> {
    let count = arena.count(root).expect("finite fixture count");
    assert!(count <= 128, "only bounded fixture output is enumerated");
    arena
        .page(root, 0, 128)
        .expect("bounded page")
        .into_iter()
        .collect()
}

fn expected(sets: &[&[u32]]) -> BTreeSet<Vec<u32>> {
    sets.iter().map(|set| set.to_vec()).collect()
}

fn power_set(arena: &mut ZddArena, variables: &[u32]) -> ZddId {
    variables
        .iter()
        .rev()
        .fold(ZddId::UNIT, |child, &variable| {
            arena.mk(variable, child, child).expect("optional variable")
        })
}

fn geometry_pair_family() -> (GeometrySolutionFamily, u32) {
    let mut geometry = GeometrySolutionFamily::new();
    let b = geometry.append(1, FAMILY_EMPTY).expect("b");
    let c = geometry.append(2, FAMILY_EMPTY).expect("c");
    let alternatives = geometry.union(b, c).expect("choice");
    let root = geometry.append(0, alternatives).expect("shared a");
    (geometry, root)
}

#[test]
fn zero_suppression_and_canonical_union_preserve_set_meaning() {
    let mut arena = arena();
    let a = singleton(&mut arena, &[3]);
    assert_eq!(arena.mk(9, a, ZddId::EMPTY), Ok(a));
    assert_eq!(arena.union(a, a), Ok(a));
    assert_eq!(arena.count(ZddId::EMPTY), Ok(0));
    assert_eq!(arena.count(ZddId::UNIT), Ok(1));
    let optional = arena.mk(3, ZddId::UNIT, ZddId::UNIT).expect("optional");
    assert_ne!(optional, ZddId::UNIT);
    assert_eq!(members(&mut arena, optional), expected(&[&[], &[3]]));

    let forward = family(&mut arena, &[&[1, 4], &[2], &[], &[1, 4]]);
    let backward = family(&mut arena, &[&[], &[2], &[1, 4]]);
    assert_eq!(forward, backward);
    assert_eq!(arena.count(forward), Ok(3));
}

#[test]
fn binary_operations_use_absence_for_missing_variables() {
    let mut arena = arena();
    let left = family(&mut arena, &[&[], &[1], &[1, 4], &[3, 70]]);
    let right = family(&mut arena, &[&[1], &[2], &[3, 70]]);
    let union = arena.union(left, right).expect("union");
    let intersection = arena.intersection(left, right).expect("intersection");
    let difference = arena.difference(left, right).expect("difference");
    let reverse_difference = arena.difference(right, left).expect("reverse difference");
    assert_eq!(
        members(&mut arena, union),
        expected(&[&[], &[1], &[1, 4], &[2], &[3, 70]])
    );
    assert_eq!(
        members(&mut arena, intersection),
        expected(&[&[1], &[3, 70]])
    );
    assert_eq!(members(&mut arena, difference), expected(&[&[], &[1, 4]]));
    assert_eq!(members(&mut arena, reverse_difference), expected(&[&[2]]));
    assert_eq!(arena.intersection(ZddId::UNIT, right), Ok(ZddId::EMPTY));
}

#[test]
fn add_select_and_projection_merge_witnesses_without_dropping_correlations() {
    let mut arena = arena();
    let root = family(&mut arena, &[&[], &[3], &[5]]);
    let added = arena.add(root, 3).expect("add");
    assert_eq!(members(&mut arena, added), expected(&[&[3], &[3, 5]]));
    let present = arena.select(added, 5, true).expect("present");
    let absent = arena.select(added, 5, false).expect("absent");
    assert_eq!(members(&mut arena, present), expected(&[&[3, 5]]));
    assert_eq!(members(&mut arena, absent), expected(&[&[3]]));
    let removed = arena.remove(added, 5).expect("remove");
    assert_eq!(removed, absent);

    let joint = family(
        &mut arena,
        &[&[1, 100, 107], &[2, 101, 107], &[1, 101, 108]],
    );
    let projected = arena
        .project(joint, &[100, 101, 107, 108])
        .expect("source projection");
    assert_eq!(members(&mut arena, projected), expected(&[&[1], &[2]]));
    let queue_100 = arena.select(joint, 100, true).expect("one source branch");
    assert_eq!(members(&mut arena, queue_100), expected(&[&[1, 100, 107]]));
    assert_eq!(
        arena.project(joint, &[101, 100]),
        Err(ZddError::InvalidVariableOrder)
    );
}

#[test]
fn global_variables_include_the_entire_u32_domain() {
    let mut arena = arena();
    let root = singleton(&mut arena, &[70, u32::MAX]);
    assert_eq!(members(&mut arena, root), expected(&[&[70, u32::MAX]]));
    let removed = arena
        .remove(root, u32::MAX)
        .expect("remove largest variable");
    assert_eq!(members(&mut arena, removed), expected(&[&[70]]));
    let restored = arena.add(removed, u32::MAX).expect("add largest variable");
    assert_eq!(restored, root);
}

#[test]
fn exact_count_reports_overflow_and_rank_page_skips_large_subtrees() {
    let mut arena = arena();
    let variables = (0..41).collect::<Vec<_>>();
    let root = power_set(&mut arena, &variables);
    assert_eq!(arena.count(root), Ok(1_u128 << 41));
    assert_eq!(
        arena.page(root, 1_u128 << 40, 2),
        Ok(vec![vec![0], vec![0, 40]])
    );
    assert_eq!(arena.page(root, 1_u128 << 41, 2), Ok(Vec::new()));
    assert_eq!(arena.page(root, 0, 0), Ok(Vec::new()));

    let variables = (0..127).collect::<Vec<_>>();
    let fits = power_set(&mut arena, &variables);
    assert_eq!(arena.count(fits), Ok(1_u128 << 127));
    let overflow = arena.add(fits, 127).expect("disjoint variable");
    let overflow = arena.union(fits, overflow).expect("128 optional variables");
    assert_eq!(arena.count(overflow), Err(ZddError::CountOverflow));
    assert_ne!(overflow, ZddId::EMPTY);
}

#[test]
fn iterative_operations_do_not_use_the_call_stack_for_variable_depth() {
    let mut arena = arena();
    let variables = (0..4096).collect::<Vec<_>>();
    let root = singleton(&mut arena, &variables);
    let removed = arena.remove(root, 2048).expect("deep projection");
    assert_eq!(arena.count(removed), Ok(1));
    let page = arena.page(removed, 0, 1).expect("one deep member");
    assert_eq!(page[0].len(), 4095);
    assert_eq!(page[0].first(), Some(&0));
    assert_eq!(page[0].last(), Some(&4095));
    assert!(!page[0].contains(&2048));
}

#[test]
fn explicit_limits_and_cancellation_never_become_empty_families() {
    let mut node_limited = ZddArena::new(ZddLimits {
        max_nodes: 1,
        ..ZddLimits::default()
    });
    let atom = node_limited
        .mk(4, ZddId::EMPTY, ZddId::UNIT)
        .expect("first node");
    assert_eq!(
        node_limited.mk(2, ZddId::EMPTY, atom),
        Err(ZddError::NodeLimit)
    );
    assert_eq!(node_limited.count(atom), Ok(1));

    let mut memo_limited = ZddArena::new(ZddLimits {
        max_memo_entries: 1,
        ..ZddLimits::default()
    });
    let child = memo_limited.mk(1, ZddId::UNIT, ZddId::UNIT).expect("child");
    let root = memo_limited.mk(0, child, child).expect("root");
    assert_eq!(memo_limited.count(root), Err(ZddError::MemoLimit));
    assert_eq!(memo_limited.count(child), Ok(2));

    let mut work_limited = ZddArena::new(ZddLimits {
        max_work: 1,
        ..ZddLimits::default()
    });
    let atom = work_limited
        .mk(0, ZddId::EMPTY, ZddId::UNIT)
        .expect("one work step");
    assert_eq!(work_limited.count(atom), Err(ZddError::WorkLimit));
    assert!(work_limited
        .node(atom)
        .expect("prior node stays valid")
        .is_some());

    let mut cancelled = arena();
    let token = CancellationToken::new();
    cancelled.set_cancellation(token.clone());
    token.handle().cancel();
    assert_eq!(
        cancelled.union(ZddId::EMPTY, ZddId::UNIT),
        Err(ZddError::Cancelled)
    );
}

#[test]
fn explicit_compaction_reclaims_garbage_and_remaps_all_kept_roots() {
    let mut arena = arena();
    let left = family(&mut arena, &[&[1, 8], &[2, 9]]);
    let right = family(&mut arena, &[&[1, 8], &[4]]);
    let expected_left = members(&mut arena, left);
    let expected_right = members(&mut arena, right);
    for variable in 100..200 {
        arena
            .mk(variable, ZddId::UNIT, ZddId::UNIT)
            .expect("unretained node");
    }
    let before = arena.node_count();
    let mut roots = [left, right, ZddId::UNIT];
    let result = arena.compact(&mut roots).expect("compact");
    assert_eq!(result.previous_nodes, before);
    assert!(result.retained_nodes < before);
    assert_eq!(result.epoch, 1);
    assert_eq!(arena.epoch(), 1);
    assert!(result.peak_retained_bytes >= arena.retained_bytes());
    assert_eq!(members(&mut arena, roots[0]), expected_left);
    assert_eq!(members(&mut arena, roots[1]), expected_right);
    assert_eq!(roots[2], ZddId::UNIT);
    let intersection = arena
        .intersection(roots[0], roots[1])
        .expect("fresh apply cache");
    assert_eq!(members(&mut arena, intersection), expected(&[&[1, 8]]));

    let token = CancellationToken::new();
    arena.set_cancellation(token.clone());
    token.handle().cancel();
    let preserved = roots;
    let node_count = arena.node_count();
    assert_eq!(arena.compact(&mut roots), Err(ZddError::Cancelled));
    assert_eq!(roots, preserved);
    assert_eq!(arena.node_count(), node_count);
    assert_eq!(arena.epoch(), 1);
}

#[test]
fn down_filter_accepts_extendible_partial_sets_and_preserves_source_variables() {
    let (geometry, root) = geometry_pair_family();
    let mut down = GeometryDownView::new(&geometry, root, 3, ZddLimits::default()).expect("view");
    let mut arena = arena();
    let partials = power_set(&mut arena, &[0, 1, 2]);
    let joint = arena.add(partials, 100).expect("source label");
    let filtered = down
        .filter_extendible(&mut arena, joint)
        .expect("Down filter");
    assert_eq!(
        members(&mut arena, filtered),
        expected(&[
            &[100],
            &[0, 100],
            &[1, 100],
            &[2, 100],
            &[0, 1, 100],
            &[0, 2, 100]
        ])
    );

    let joint = family(&mut arena, &[&[0, 1, 100], &[0, 2, 101], &[1, 2, 101]]);
    let filtered = down
        .filter_extendible(&mut arena, joint)
        .expect("joint filter");
    assert_eq!(
        members(&mut arena, filtered),
        expected(&[&[0, 1, 100], &[0, 2, 101]])
    );
    assert!(down.metrics().derivative_misses > 0);
    let mut roots = [joint];
    arena.compact(&mut roots).expect("new ZDD epoch");
    let filtered = down
        .filter_extendible(&mut arena, roots[0])
        .expect("same geometry cache");
    assert_eq!(
        members(&mut arena, filtered),
        expected(&[&[0, 1, 100], &[0, 2, 101]])
    );
}

#[test]
fn down_product_keeps_factor_choices_and_rejects_a_violated_disjoint_contract() {
    let mut geometry = GeometrySolutionFamily::new();
    let a = geometry.append(0, FAMILY_EMPTY).expect("a");
    let b = geometry.append(1, FAMILY_EMPTY).expect("b");
    let c = geometry.append(2, FAMILY_EMPTY).expect("c");
    let d = geometry.append(3, FAMILY_EMPTY).expect("d");
    let left = geometry.union(a, b).expect("left choice");
    let right = geometry.union(c, d).expect("right choice");
    let root = geometry.product(left, right).expect("disjoint product");
    let mut down = GeometryDownView::new(&geometry, root, 4, ZddLimits::default()).expect("view");
    let mut arena = arena();
    let joint = family(&mut arena, &[&[], &[0], &[0, 1], &[0, 2], &[2, 3], &[1, 3]]);
    let filtered = down
        .filter_extendible(&mut arena, joint)
        .expect("product filter");
    assert_eq!(
        members(&mut arena, filtered),
        expected(&[&[], &[0], &[0, 2], &[1, 3]])
    );
    drop(down);

    let malformed = geometry
        .product(a, a)
        .expect("raw API allows overlapping factors");
    let mut down =
        GeometryDownView::new(&geometry, malformed, 4, ZddLimits::default()).expect("view");
    let selected = singleton(&mut arena, &[0]);
    assert_eq!(
        down.filter_extendible(&mut arena, selected),
        Err(ZddError::InvalidGeometry)
    );
}

#[test]
fn down_source_only_suffix_does_not_convert_or_walk_the_original_family() {
    let mut geometry = GeometrySolutionFamily::new();
    let mut root = FAMILY_EMPTY;
    for factor in 0..80 {
        let left = geometry.append(factor * 2, FAMILY_EMPTY).expect("left row");
        let right = geometry
            .append(factor * 2 + 1, FAMILY_EMPTY)
            .expect("right row");
        let choice = geometry.union(left, right).expect("choice");
        root = geometry
            .product(root, choice)
            .expect("independent geometry factor");
    }
    let mut down = GeometryDownView::new(&geometry, root, 160, ZddLimits::default()).expect("view");
    let mut arena = arena();
    let source = power_set(&mut arena, &[200, 201, 202]);
    assert_eq!(down.filter_extendible(&mut arena, source), Ok(source));
    assert_eq!(down.generated_node_count(), 0);
    assert_eq!(down.metrics().derivative_misses, 0);
    assert_eq!(down.metrics().filter_misses, 0);
    assert_eq!(arena.count(source), Ok(8));
}

#[test]
fn catalog_overlap_prunes_a_derivative_before_visiting_its_suffix() {
    // A 4x2 region has two horizontal I placements or two O placements.
    // Both covers are disjoint exact covers; placements from the two choices
    // overlap and cannot be combined into a third completing cover.
    let mut geometry = GeometrySolutionFamily::new();
    let first_suffix = geometry.append(1, FAMILY_EMPTY).expect("upper I");
    let second_suffix = geometry.append(3, FAMILY_EMPTY).expect("right O");
    let left = geometry.append(0, first_suffix).expect("I cover");
    let right = geometry.append(2, second_suffix).expect("O cover");
    let root = geometry.union(left, right).expect("choice");
    let cells = [0x000f, 0x00f0, 0x0033, 0x00cc];
    let mut down = GeometryDownView::new(&geometry, root, 4, ZddLimits::default()).expect("view");
    down.set_row_cells(&cells).expect("original catalog masks");
    assert_eq!(down.set_row_cells(&cells), Ok(()));
    assert_eq!(
        down.set_row_cells(&[0x000f, 0x00f0, 0x0033, 0x0f00]),
        Err(ZddError::InvalidGeometry)
    );
    let mut arena = arena();
    let joint = family(&mut arena, &[&[0], &[2], &[0, 2], &[0, 1], &[2, 3]]);
    let filtered = down
        .filter_extendible(&mut arena, joint)
        .expect("overlap pruning");
    assert_eq!(
        members(&mut arena, filtered),
        expected(&[&[0], &[2], &[0, 1], &[2, 3]])
    );
    assert!(down.metrics().overlapping_row_prunes > 0);
    down.clear_derivatives();
    assert_eq!(down.generated_node_count(), 0);
    assert_eq!(down.filter_extendible(&mut arena, joint), Ok(filtered));
}

#[test]
fn down_refusal_is_distinct_from_an_unextendible_selection() {
    let (geometry, root) = geometry_pair_family();
    let mut arena = arena();
    let selected = singleton(&mut arena, &[0, 1]);
    let mut down = GeometryDownView::new(
        &geometry,
        root,
        3,
        ZddLimits {
            max_work: 1,
            ..ZddLimits::default()
        },
    )
    .expect("limited view");
    assert_eq!(
        down.filter_extendible(&mut arena, selected),
        Err(ZddError::WorkLimit)
    );
    let mut down = GeometryDownView::new(&geometry, root, 3, ZddLimits::default()).expect("view");
    assert_eq!(down.filter_extendible(&mut arena, selected), Ok(selected));
}
