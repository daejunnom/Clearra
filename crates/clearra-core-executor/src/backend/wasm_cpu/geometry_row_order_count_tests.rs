//! Test-only exact count of paths with a nonempty common row-order mask.
//!
//! For a leaf mask M in the meet closure, sum_{Q subset M} w(Q) = [M != 0].
//! Counting paths admitting every order in a fixed Q therefore permits ordinary
//! scalar DAG addition/product; summing the signed counts counts each accepted
//! leaf once. Summing 24 independent order counts would count leaves repeatedly.
//! This diagnostic never contributes a product result or completion authority.

use std::{
    collections::BTreeSet,
    mem::size_of,
    time::{Duration, Instant},
};

use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    pattern_universe::materialized_pattern_universe::UniformCompactPatternSource,
    queue::queue_pattern_expression::QueuePatternExpression, QueueObservationPolicy,
};

use super::{
    row_order::{RowOrderCapability, ROW_ORDER_ALL},
    CompileAdvance, FamilyCompiler, FamilyNodeKind, GeometryCatalog, GeometrySearch,
    GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID,
};

#[derive(Debug, Eq, PartialEq)]
enum CountError {
    Cancelled,
    Allocation,
    Admission,
    Overflow(u32),
    InvalidReference(u32),
    InvalidDomain,
}

struct Control {
    execution: ExecutionControl,
    deadline: Option<Instant>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            execution: ExecutionControl::default(),
            deadline: None,
        }
    }
}

impl Control {
    fn check(&self) -> Result<(), CountError> {
        if self.execution.is_cancelled() || self.deadline.is_some_and(|end| Instant::now() >= end) {
            Err(CountError::Cancelled)
        } else {
            Ok(())
        }
    }
}

struct MaskDomain {
    row_masks: Vec<u32>,
    classes: Vec<u32>,
    weights: Vec<(u32, i32)>,
}

fn meet_closure(generators: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let generators: BTreeSet<_> = generators.into_iter().collect();
    let mut seen = BTreeSet::from([ROW_ORDER_ALL]);
    let mut pending = vec![ROW_ORDER_ALL];
    let mut cursor = 0;
    while cursor < pending.len() {
        for &generator in &generators {
            let next = pending[cursor] & generator;
            if seen.insert(next) {
                pending.push(next);
            }
        }
        cursor += 1;
    }
    seen.into_iter().collect()
}

impl MaskDomain {
    fn from_catalog(catalog: &GeometryCatalog) -> Self {
        let capability = RowOrderCapability::try_compile(catalog).expect("actual Full6 capability");
        let row_masks: Vec<_> = (0..catalog.skeleton_count() as u32)
            .map(|row| capability.row_mask(row).unwrap())
            .collect();
        let actual: BTreeSet<_> = row_masks.iter().copied().collect();
        // Independent geometry/rank model authenticates the whole supported
        // domain, rather than trusting a hard-coded list of 111 bit masks.
        let abstract_masks: BTreeSet<_> = (1_u16..64)
            .filter(|occupied| occupied.count_ones() <= 4)
            .map(reference_gap_mask)
            .collect();
        assert_eq!(actual, abstract_masks);
        assert_eq!(actual.len(), 23);
        for row in 0..catalog.skeleton_count() as u32 {
            let occupied = catalog.skeleton_occupied_rows(row);
            let first = occupied.trailing_zeros();
            let last = u16::BITS - 1 - occupied.leading_zeros();
            let gaps = (((1_u16 << (last - first + 1)) - 1) << first) & !occupied;
            assert!(catalog
                .realizations(row)
                .iter()
                .all(|pose| pose.required_deleted_rows == gaps));
            assert_eq!(row_masks[row as usize], reference_gap_mask(occupied));
        }
        let classes = meet_closure(actual);
        assert_eq!(classes, meet_closure(abstract_masks));
        assert_eq!(classes.len(), 111);
        assert!(row_masks
            .iter()
            .all(|mask| classes.binary_search(mask).is_ok()));
        let mut ordered: Vec<_> = classes.iter().copied().filter(|mask| *mask != 0).collect();
        ordered.sort_unstable_by_key(|mask| (mask.count_ones(), *mask));
        let mut weights: Vec<(u32, i32)> = Vec::new();
        for mask in ordered {
            let prior: i32 = weights
                .iter()
                .filter(|(q, _)| q & mask == *q)
                .map(|(_, w)| w)
                .sum();
            let weight = 1 - prior;
            assert!((-1..=1).contains(&weight));
            if weight != 0 {
                weights.push((mask, weight));
            }
        }
        assert_eq!(weights.len(), 75);
        assert_eq!(weights.iter().filter(|(_, w)| *w == 1).count(), 38);
        assert_eq!(weights.iter().filter(|(_, w)| *w == -1).count(), 37);
        for &mask in &classes {
            let value: i32 = weights
                .iter()
                .filter(|(q, _)| q & mask == *q)
                .map(|(_, w)| w)
                .sum();
            assert_eq!(value, i32::from(mask != 0), "mask={mask:#08x}");
        }
        Self {
            row_masks,
            classes,
            weights,
        }
    }

    fn retained_bytes(&self) -> u128 {
        (self.row_masks.capacity() * size_of::<u32>()
            + self.classes.capacity() * size_of::<u32>()
            + self.weights.capacity() * size_of::<(u32, i32)>()) as u128
    }
}

/// Independent lexicographic rank enumeration, using gaps between occupied rows.
fn reference_gap_mask(occupied: u16) -> u32 {
    let first = occupied.trailing_zeros();
    let last = u16::BITS - 1 - occupied.leading_zeros();
    let required = (((1_u16 << (last - first + 1)) - 1) << first) & !occupied;
    let mut mask = 0;
    let mut bit = 0;
    for a in 1..=4 {
        for b in 1..=4 {
            for c in 1..=4 {
                for d in 1..=4 {
                    if [a == b, a == c, a == d, b == c, b == d, c == d].contains(&true) {
                        continue;
                    }
                    let mut rank = [0; 6];
                    for (index, row) in [a, b, c, d].into_iter().enumerate() {
                        rank[row] = index;
                    }
                    if (1..=4).all(|r| {
                        required & (1 << r) == 0
                            || (1..=4).all(|o| occupied & (1 << o) == 0 || rank[r] < rank[o])
                    }) {
                        mask |= 1 << bit;
                    }
                    bit += 1;
                }
            }
        }
    }
    assert_eq!(bit, 24);
    mask
}

trait CountWord: Copy {
    const ZERO: Self;
    const ONE: Self;
    fn add(self, rhs: Self) -> Option<Self>;
    fn multiply(self, rhs: Self) -> Option<Self>;
    fn wide(self) -> u128;
    fn from_mask(mask: u32) -> Self;
}

macro_rules! count_word {
    ($word:ty) => {
        impl CountWord for $word {
            const ZERO: Self = 0;
            const ONE: Self = 1;
            fn add(self, rhs: Self) -> Option<Self> {
                self.checked_add(rhs)
            }
            fn multiply(self, rhs: Self) -> Option<Self> {
                self.checked_mul(rhs)
            }
            fn wide(self) -> u128 {
                self as u128
            }
            fn from_mask(mask: u32) -> Self {
                Self::from(mask)
            }
        }
    };
}
count_word!(u64);
count_word!(u128);

fn allocate_counts<T: CountWord>(
    entries: usize,
    retained: u128,
    cap: u128,
    control: &Control,
) -> Result<Vec<T>, CountError> {
    control.check()?;
    let bound = crate::resource::ExecutionMemoryBound::for_test_cap(cap);
    let requested = (entries as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(CountError::Admission)?;
    bound
        .ensure(retained, requested)
        .map_err(|_| CountError::Admission)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(entries)
        .map_err(|_| CountError::Allocation)?;
    let actual = (values.capacity() as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(CountError::Admission)?;
    bound
        .ensure(retained, actual)
        .map_err(|_| CountError::Admission)?;
    while values.len() < entries {
        control.check()?;
        values.resize(entries.min(values.len().saturating_add(65_536)), T::ZERO);
    }
    Ok(values)
}

fn raw_counts(
    family: &GeometrySolutionFamily,
    retained: u128,
    cap: u128,
    control: &Control,
) -> Result<Vec<u128>, CountError> {
    let mut counts =
        allocate_counts::<u128>(family.node_count() as usize + 2, retained, cap, control)?;
    counts[FAMILY_EMPTY as usize] = 1;
    for reference in 2..family.node_count() + 2 {
        if reference & 4095 == 0 {
            control.check()?;
        }
        counts[reference as usize] = family
            .path_count_for_reference(reference, &counts)
            .ok_or(CountError::Overflow(reference))?;
    }
    control.check()?;
    Ok(counts)
}

fn all_nodes_fit_u64(raw: &[u128], control: &Control) -> Result<bool, CountError> {
    control.check()?;
    let mut fits = true;
    for (index, &value) in raw.iter().enumerate() {
        if index & 4095 == 0 {
            control.check()?;
        }
        fits &= value <= u128::from(u64::MAX);
    }
    control.check()?;
    Ok(fits)
}

#[derive(Debug, Default, Eq, PartialEq)]
struct ExistentialNodeCounts {
    zero: u64,
    all: u64,
    other: u64,
}

impl ExistentialNodeCounts {
    fn record(&mut self, mask: u32) {
        match mask {
            0 => self.zero += 1,
            ROW_ORDER_ALL => self.all += 1,
            _ => self.other += 1,
        }
    }
}

/// Reuse the selected count scratch for existential masks, then let the first
/// signed pass overwrite it. Counts exclude the two sentinel references and
/// include unreachable retained nodes, just like family.node_count(). This is
/// a shape diagnostic, not a claim that deleting zero nodes alone reclaims all
/// their storage (Union simplification and interning may change the final DAG).
fn existential_masks<T: CountWord>(
    family: &GeometrySolutionFamily,
    row_masks: &[u32],
    values: &mut [T],
    control: &Control,
) -> Result<ExistentialNodeCounts, CountError> {
    control.check()?;
    if values.len() != family.node_count() as usize + 2 {
        return Err(CountError::InvalidDomain);
    }
    values[FAMILY_INVALID as usize] = T::from_mask(0);
    values[FAMILY_EMPTY as usize] = T::from_mask(ROW_ORDER_ALL);
    let mut counts = ExistentialNodeCounts::default();
    for reference in 2..family.node_count() + 2 {
        if reference & 4095 == 0 {
            control.check()?;
        }
        let node = family
            .node(reference)
            .ok_or(CountError::InvalidReference(reference))?;
        if node.left >= reference || node.right >= reference {
            return Err(CountError::InvalidReference(reference));
        }
        // Each child was overwritten by THIS pass before its parent; the
        // induction from the sentinels keeps every stored value within 24 bits.
        let left = values[node.left as usize].wide() as u32;
        let mask = match node.kind {
            FamilyNodeKind::Append => {
                let allowed = *row_masks
                    .get(node.row_id as usize)
                    .ok_or(CountError::InvalidDomain)?;
                if allowed & !ROW_ORDER_ALL != 0 {
                    return Err(CountError::InvalidDomain);
                }
                allowed & left
            }
            FamilyNodeKind::Union => left | values[node.right as usize].wide() as u32,
            FamilyNodeKind::Product => left & values[node.right as usize].wide() as u32,
        };
        values[reference as usize] = T::from_mask(mask);
        counts.record(mask);
    }
    control.check()?;
    Ok(counts)
}

fn pass<T: CountWord>(
    family: &GeometrySolutionFamily,
    root: u32,
    row_masks: &[u32],
    q: u32,
    values: &mut [T],
    control: &Control,
) -> Result<u128, CountError> {
    control.check()?;
    if q == 0 || q & !ROW_ORDER_ALL != 0 || values.len() != family.node_count() as usize + 2 {
        return Err(CountError::InvalidDomain);
    }
    values[FAMILY_INVALID as usize] = T::ZERO;
    values[FAMILY_EMPTY as usize] = T::ONE;
    for reference in 2..family.node_count() + 2 {
        if reference & 4095 == 0 {
            control.check()?;
        }
        let node = family
            .node(reference)
            .ok_or(CountError::InvalidReference(reference))?;
        if node.left >= reference || node.right >= reference {
            return Err(CountError::InvalidReference(reference));
        }
        let left = values[node.left as usize];
        values[reference as usize] = match node.kind {
            FamilyNodeKind::Append => {
                let allowed = *row_masks
                    .get(node.row_id as usize)
                    .ok_or(CountError::InvalidDomain)?;
                if allowed & q == q {
                    left
                } else {
                    T::ZERO
                }
            }
            FamilyNodeKind::Union => left
                .add(values[node.right as usize])
                .ok_or(CountError::Overflow(reference))?,
            FamilyNodeKind::Product => left
                .multiply(values[node.right as usize])
                .ok_or(CountError::Overflow(reference))?,
        };
    }
    control.check()?;
    values
        .get(root as usize)
        .map(|value| value.wide())
        .ok_or(CountError::InvalidReference(root))
}

fn signed_total<T: CountWord>(
    family: &GeometrySolutionFamily,
    root: u32,
    domain: &MaskDomain,
    values: &mut [T],
    control: &Control,
    progress: bool,
) -> Result<u128, CountError> {
    if progress {
        let started = Instant::now();
        let counts = existential_masks(family, &domain.row_masks, values, control)?;
        let root_mask = values
            .get(root as usize)
            .ok_or(CountError::InvalidReference(root))?
            .wide();
        println!("row_order_count_existential nodes={} zero={} all={} other={} sentinels_excluded=true root_mask={root_mask:#08x} reused_scratch=true seconds={:.6}",
            family.node_count(), counts.zero, counts.all, counts.other, started.elapsed().as_secs_f64());
    }
    let mut positive = 0_u128;
    let mut negative = 0_u128;
    for (index, &(q, weight)) in domain.weights.iter().enumerate() {
        let started = Instant::now();
        let count = pass(family, root, &domain.row_masks, q, values, control)?;
        let accumulator = match weight {
            1 => &mut positive,
            -1 => &mut negative,
            _ => return Err(CountError::InvalidDomain),
        };
        *accumulator = accumulator
            .checked_add(count)
            .ok_or(CountError::Overflow(root))?;
        if progress {
            println!("row_order_count_pass completed={} total={} q={q:#08x} weight={weight} count={count} seconds={:.6}",
                index + 1, domain.weights.len(), started.elapsed().as_secs_f64());
        }
    }
    control.check()?;
    positive
        .checked_sub(negative)
        .ok_or(CountError::Overflow(root))
}

fn full6_catalog() -> GeometryCatalog {
    GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, (1_u64 << 60) - 1).unwrap()
}

fn brute_leaf_masks(family: &GeometrySolutionFamily, root: u32, rows: &[u32]) -> Vec<u32> {
    if root == FAMILY_INVALID {
        return vec![];
    }
    if root == FAMILY_EMPTY {
        return vec![ROW_ORDER_ALL];
    }
    let node = family.node(root).unwrap();
    let left = brute_leaf_masks(family, node.left, rows);
    match node.kind {
        FamilyNodeKind::Append => left
            .into_iter()
            .map(|mask| mask & rows[node.row_id as usize])
            .collect(),
        FamilyNodeKind::Union => left
            .into_iter()
            .chain(brute_leaf_masks(family, node.right, rows))
            .collect(),
        FamilyNodeKind::Product => {
            let right = brute_leaf_masks(family, node.right, rows);
            left.into_iter()
                .flat_map(|a| right.iter().map(move |b| a & b))
                .collect()
        }
    }
}

#[test]
fn row_order_count_actual_domain_has_111_classes_and_75_signed_weights() {
    let domain = MaskDomain::from_catalog(&full6_catalog());
    assert_eq!(domain.classes.first(), Some(&0));
    // Every possible selected prefix stays in this finite meet domain.
    for &a in &domain.classes {
        for &b in &domain.classes {
            assert!(domain.classes.binary_search(&(a & b)).is_ok());
        }
    }
}

#[test]
fn row_order_count_signed_dag_matches_explicit_leaves_and_same_q_products() {
    let domain = MaskDomain::from_catalog(&full6_catalog());
    let representatives: Vec<_> = domain
        .row_masks
        .iter()
        .copied()
        .enumerate()
        .fold(std::collections::BTreeMap::new(), |mut map, (id, mask)| {
            map.entry(mask).or_insert(id as u32);
            map
        })
        .into_iter()
        .collect();
    let (a, b) = representatives
        .iter()
        .find_map(|&(ma, a)| {
            representatives
                .iter()
                .find(|&&(mb, _)| ma != 0 && mb != 0 && ma & mb == 0)
                .map(|&(_, b)| (a, b))
        })
        .unwrap();
    let ordinary = domain
        .row_masks
        .iter()
        .position(|mask| *mask == ROW_ORDER_ALL)
        .unwrap() as u32;
    let mut family = GeometrySolutionFamily::new();
    let left = family.append(a, FAMILY_EMPTY).unwrap();
    let right = family.append(b, FAMILY_EMPTY).unwrap();
    let incompatible = family.product(left, right).unwrap();
    let alternatives = family.union(left, right).unwrap();
    let shared_product = family.product(alternatives, alternatives).unwrap();
    let continuation = family.append(ordinary, shared_product).unwrap();
    let root = family.union(continuation, incompatible).unwrap();
    // Retained orphan with A==ALL authenticates classification of all nodes,
    // rather than only nodes reachable from the selected root.
    family.append(ordinary, FAMILY_EMPTY).unwrap();
    let control = Control::default();
    let raw = raw_counts(&family, 0, u128::MAX, &control).unwrap();
    assert_eq!(raw, family.path_count_table().unwrap());
    let mut scratch = vec![0_u64; raw.len()];
    let observed = existential_masks(&family, &domain.row_masks, &mut scratch, &control).unwrap();
    let mut wide_masks = raw.clone();
    assert_eq!(
        existential_masks(&family, &domain.row_masks, &mut wide_masks, &control).unwrap(),
        observed
    );
    let mut expected = ExistentialNodeCounts::default();
    for reference in 0..family.node_count() + 2 {
        let mask = brute_leaf_masks(&family, reference, &domain.row_masks)
            .into_iter()
            .fold(0, |union, leaf| union | leaf);
        assert_eq!(scratch[reference as usize], u64::from(mask));
        assert_eq!(wide_masks[reference as usize], u128::from(mask));
        if reference >= 2 {
            expected.record(mask);
        }
    }
    assert_eq!(observed, expected);
    assert!(observed.zero > 0 && observed.all > 0 && observed.other > 0);
    assert_eq!(
        observed.zero + observed.all + observed.other,
        u64::from(family.node_count())
    );
    for reference in 0..family.node_count() + 2 {
        let leaves = brute_leaf_masks(&family, reference, &domain.row_masks);
        assert_eq!(leaves.len() as u128, raw[reference as usize]);
        for &(q, _) in &domain.weights {
            assert_eq!(
                pass(
                    &family,
                    reference,
                    &domain.row_masks,
                    q,
                    &mut scratch,
                    &control
                )
                .unwrap(),
                leaves.iter().filter(|mask| **mask & q == q).count() as u128
            );
        }
        assert_eq!(
            signed_total(&family, reference, &domain, &mut scratch, &control, false).unwrap(),
            leaves.iter().filter(|mask| **mask != 0).count() as u128
        );
    }
    assert_eq!(
        signed_total(
            &family,
            incompatible,
            &domain,
            &mut scratch,
            &control,
            false
        ),
        Ok(0)
    );
    assert_eq!(
        signed_total(
            &family,
            shared_product,
            &domain,
            &mut scratch,
            &control,
            false
        ),
        Ok(2)
    );
    assert_eq!(
        signed_total(&family, root, &domain, &mut scratch, &control, false),
        Ok(2)
    );
    let leaves = brute_leaf_masks(&family, root, &domain.row_masks);
    assert!(leaves.iter().map(|mask| mask.count_ones()).sum::<u32>() > 2);
}

#[test]
fn row_order_count_checked_width_fallback_admission_and_cancellation() {
    let control = Control::default();
    let mut family = GeometrySolutionFamily::new();
    let a = family.append(0, FAMILY_EMPTY).unwrap();
    let b = family.append(1, FAMILY_EMPTY).unwrap();
    let mut root = family.union(a, b).unwrap();
    for _ in 0..6 {
        root = family.product(root, root).unwrap();
    }
    let raw = raw_counts(&family, 0, u128::MAX, &control).unwrap();
    assert_eq!(raw[root as usize], 1_u128 << 64);
    assert!(!all_nodes_fit_u64(&raw, &control).unwrap());
    // Even a small root cannot authenticate an overflowing, unreachable node.
    assert_eq!(raw[a as usize], 1);
    let mut narrow = vec![0_u64; raw.len()];
    assert_eq!(
        pass(&family, root, &[ROW_ORDER_ALL; 2], 1, &mut narrow, &control),
        Err(CountError::Overflow(root))
    );
    let mut wide = raw;
    assert_eq!(
        pass(&family, root, &[ROW_ORDER_ALL; 2], 1, &mut wide, &control),
        Ok(1_u128 << 64)
    );
    root = family.product(root, root).unwrap();
    assert_eq!(
        raw_counts(&family, 0, u128::MAX, &control),
        Err(CountError::Overflow(root))
    );
    wide.push(0);
    assert_eq!(
        pass(&family, root, &[ROW_ORDER_ALL; 2], 1, &mut wide, &control),
        Err(CountError::Overflow(root))
    );

    let entries = 17;
    let coexisting_raw_and_family = 911;
    let exact = coexisting_raw_and_family + entries as u128 * size_of::<u64>() as u128;
    assert_eq!(
        allocate_counts::<u64>(entries, coexisting_raw_and_family, exact - 1, &control),
        Err(CountError::Admission)
    );
    assert_eq!(
        allocate_counts::<u64>(entries, coexisting_raw_and_family, exact, &control)
            .unwrap()
            .len(),
        entries
    );
    assert_eq!(
        allocate_counts::<u128>(2, u128::MAX, u128::MAX, &control),
        Err(CountError::Admission)
    );
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    let cancelled = Control {
        execution: ExecutionControl::new(token),
        deadline: None,
    };
    assert_eq!(
        allocate_counts::<u64>(2, 0, 100, &cancelled),
        Err(CountError::Cancelled)
    );
    assert_eq!(
        pass(&family, root, &[ROW_ORDER_ALL; 2], 1, &mut wide, &cancelled),
        Err(CountError::Cancelled)
    );
    assert_eq!(
        all_nodes_fit_u64(&wide, &cancelled),
        Err(CountError::Cancelled)
    );
    assert_eq!(
        existential_masks(&family, &[ROW_ORDER_ALL; 2], &mut wide, &cancelled),
        Err(CountError::Cancelled)
    );
    let expired = Control {
        execution: ExecutionControl::default(),
        deadline: Some(Instant::now()),
    };
    assert_eq!(
        raw_counts(&family, 0, u128::MAX, &expired),
        Err(CountError::Cancelled)
    );
}

/// Requires clearra-manage runtime supervision and explicit test-only limits.
/// This scalar diagnostic does not select/reduce any product worker policy.
#[test]
#[ignore = "supervised Full6 diagnostic; set CLEARRA_ROW_ORDER_COUNT_MEMORY_BYTES and CLEARRA_ROW_ORDER_COUNT_SECONDS"]
fn full6l_row_order_compatible_leaf_count_diagnostic() {
    let limit = |name: &str| {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} is required for this ignored diagnostic"))
            .parse::<u64>()
            .expect("positive diagnostic limit")
    };
    let cap = u128::from(limit("CLEARRA_ROW_ORDER_COUNT_MEMORY_BYTES"));
    let seconds = limit("CLEARRA_ROW_ORDER_COUNT_SECONDS");
    assert!(cap > 0 && seconds > 0);
    let started = Instant::now();
    let control = Control {
        execution: ExecutionControl::default(),
        deadline: Some(
            started
                .checked_add(Duration::from_secs(seconds))
                .expect("finite deadline"),
        ),
    };
    println!("row_order_count_diagnostic scalar_threads=1 product_workers_unchanged=true memory_cap_bytes={cap} deadline_seconds={seconds}");
    let result = run_full6_diagnostic(cap, &control);
    match result {
        Ok((raw, compatible)) => println!("row_order_count_final complete=true raw_paths={raw} compatible_paths={compatible} rejected_paths={} seconds={:.6}",
            raw.checked_sub(compatible).expect("compatible subset"), started.elapsed().as_secs_f64()),
        Err(error) => {
            println!("row_order_count_final complete=false error={error:?} seconds={:.6}", started.elapsed().as_secs_f64());
            panic!("incomplete diagnostic: {error:?}");
        }
    }
}

fn run_full6_diagnostic(cap: u128, control: &Control) -> Result<(u128, u128), CountError> {
    control.check()?;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::pattern_expression(QueuePatternExpression::parse("P1P7P7", 0).unwrap()),
        PieceWindow::new(15),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(15))
    .with_supply_window_size(SupplyWindowSize::new(15))
    .with_min_remaining_queue(0)
    .with_hold_piece(None)
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false)
            .with_max_patterns(0),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    drop(query);
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert!(problem.piece_source().complete() && universe.complete());
    assert_eq!(universe.pattern_count(), 177_811_200);
    assert_eq!(universe.total_possible_pattern_count(), 177_811_200);
    assert_eq!(problem.initial_hold().cursor(), 0);
    assert_eq!(problem.initial_hold().hold_piece(), None);
    assert_eq!(problem.initial_hold().bag_epoch(), 0);
    assert_eq!(problem.initial_hold().bag_remainder_key(), 0);
    let Some(UniformCompactPatternSource::FactorizedExpression(shape)) =
        universe.uniform_compact_source()
    else {
        panic!("original factorized source required");
    };
    assert_eq!(
        (shape.full_sequence_len(), shape.visible_sequence_len()),
        (15, 15)
    );
    assert!(shape
        .atoms()
        .map(|atom| (atom.draw_count(), atom.variant_count()))
        .eq([(1, 7), (7, 5040), (7, 5040)]));
    assert!(shape
        .atoms()
        .all(|atom| atom.choices() == PieceKind::STANDARD_TETROMINOES));
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    let multiset = universe
        .symbolic_factorized_full_draw_family_for_execution(
            15,
            problem.initial_hold(),
            true,
            crate::backend::wasm_cpu::packing_hold_projection(&problem),
        )
        .unwrap()
        .unwrap();
    assert_eq!(multiset.len(), 7);
    let actual_inventory: BTreeSet<_> = multiset
        .groups()
        .iter()
        .map(|group| group.key().counts())
        .collect();
    let expected_inventory: BTreeSet<_> = (0..7)
        .map(|piece| {
            let mut counts = [2; 7];
            counts[piece] += 1;
            counts
        })
        .collect();
    assert_eq!(actual_inventory, expected_inventory);
    drop(actual_inventory);
    drop(expected_inventory);
    let mut search =
        GeometrySearch::new(universe, &multiset, catalog.required_cells(), false).unwrap();
    let mut compiler = search.compiler.take().unwrap();
    drop(search);
    drop(multiset);
    drop(problem);
    let mut last_report = Instant::now();
    let mut advances = 0_u64;
    loop {
        control.check()?;
        match compiler.advance(&catalog) {
            CompileAdvance::Complete => break,
            CompileAdvance::ResourceIncomplete => return Err(CountError::Allocation),
            CompileAdvance::Pending => {}
        }
        advances += 1;
        if advances & 4095 == 0 && last_report.elapsed() >= Duration::from_secs(5) {
            println!("row_order_count_compile advances={advances} expanded={} nodes={} retained_bytes={}",
                compiler.expanded_nodes, compiler.family.node_count(), compiler.retained_bytes());
            last_report = Instant::now();
        }
    }
    // Consume this mutable owner and release its construction index exactly as
    // the unbounded enumerator handoff does; do not allocate a row-order table.
    let (family, root, released) = consume_completed_compiler(compiler);
    let domain = MaskDomain::from_catalog(&catalog);
    let retained = family.retained_bytes() as u128
        + catalog.retained_bytes() as u128
        + domain.retained_bytes();
    println!("row_order_count_family nodes={} root={root} family_bytes={} catalog_bytes={} domain_bytes={} index_released_bytes={released}",
        family.node_count(), family.retained_bytes(), catalog.retained_bytes(), domain.retained_bytes());
    let raw = raw_counts(&family, retained, cap, control)?;
    let raw_total = raw[root as usize];
    assert_eq!(raw_total, 247_794_216_021_372, "same exhaustive raw family");
    let raw_bytes = raw.capacity() as u128 * size_of::<u128>() as u128;
    let fits = all_nodes_fit_u64(&raw, control)?;
    println!("row_order_count_raw paths={raw_total} entries={} capacity={} bytes={raw_bytes} all_nodes_fit_u64={fits}", raw.len(), raw.capacity());
    let compatible = if fits {
        // Authenticate ALL raw nodes (including unreachable nodes) before any
        // narrowing. Admission includes both arrays while allocating the new one.
        let mut scratch = allocate_counts::<u64>(
            raw.len(),
            retained
                .checked_add(raw_bytes)
                .ok_or(CountError::Admission)?,
            cap,
            control,
        )?;
        println!(
            "row_order_count_scratch bits=64 capacity={} bytes={} conversion_coexisting_bytes={}",
            scratch.capacity(),
            scratch.capacity() as u128 * size_of::<u64>() as u128,
            retained + raw_bytes + scratch.capacity() as u128 * size_of::<u64>() as u128
        );
        drop(raw);
        signed_total(&family, root, &domain, &mut scratch, control, true)?
    } else {
        // Reuse the admitted u128 allocation, never silently truncate counts.
        let mut scratch = raw;
        println!(
            "row_order_count_scratch bits=128 capacity={} bytes={raw_bytes} fallback=true",
            scratch.capacity()
        );
        signed_total(&family, root, &domain, &mut scratch, control, true)?
    };
    if compatible > raw_total {
        return Err(CountError::InvalidDomain);
    }
    Ok((raw_total, compatible))
}

fn consume_completed_compiler(compiler: FamilyCompiler) -> (GeometrySolutionFamily, u32, usize) {
    let FamilyCompiler {
        mut family,
        root_family,
        ..
    } = compiler;
    let released = family.release_construction_index();
    // The helper boundary drops ALL remaining compiler fields (memo, prefix
    // arrays and target membership) before the caller admits count scratch.
    (family, root_family, released)
}
