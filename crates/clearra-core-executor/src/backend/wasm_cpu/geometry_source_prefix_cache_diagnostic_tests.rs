//! Test-only semantic-key trace; no cache is installed in the producer.
//!
//! The real DP always runs. A bounded chronological trace is replayed only
//! after collection, so hypothetical hits cannot remove candidates or change
//! the observed stream. Distinct counts describe each recorded branch sample,
//! never the complete Full6 search or a union across branches.

use std::{
    cmp::Ordering,
    mem::size_of,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{piece_index, GeometryCatalog, SourcePrefixDecision, SourcePrefixProfile};
use crate::resource::ExecutionMemoryBound;

const VALID: u8 = 0x80;
const NEGATIVE: u8 = 0x10;
const ROOT_FAST: u8 = 0x20;
const DEPTH_MASK: u8 = 0x0f;

#[derive(Clone, Debug, Eq, PartialEq)]
enum DiagnosticError {
    Cancelled,
    Admission,
    Allocation,
    UnsupportedCatalog,
    ForeignCatalog,
    InvalidRow,
    InconsistentDecision,
    Geometry(&'static str),
}

#[derive(Default)]
struct Control {
    execution: ExecutionControl,
    deadline: Option<Instant>,
}

impl Control {
    fn check(&self) -> Result<(), DiagnosticError> {
        if self.execution.is_cancelled() || self.deadline.is_some_and(|end| Instant::now() >= end) {
            Err(DiagnosticError::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn admit(retained: u128, additional: u128, cap: u128) -> Result<(), DiagnosticError> {
    ExecutionMemoryBound::for_test_cap(cap)
        .ensure(retained, additional)
        .map_err(|_| DiagnosticError::Admission)
}

fn allocated_vec<T>(
    entries: usize,
    retained: u128,
    cap: u128,
    control: &Control,
) -> Result<Vec<T>, DiagnosticError> {
    control.check()?;
    let requested = (entries as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(DiagnosticError::Admission)?;
    admit(retained, requested, cap)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(entries)
        .map_err(|_| DiagnosticError::Allocation)?;
    admit(
        retained,
        values.capacity() as u128 * size_of::<T>() as u128,
        cap,
    )?;
    Ok(values)
}

#[derive(Debug)]
struct KeyCapability {
    // Weak keeps the exact immutable owner alive as an identity, without
    // acquiring ownership of its catalog payload. A digest is not authority.
    catalog: Weak<GeometryCatalog>,
    row_tokens: Vec<u16>,
    distinct_tokens: usize,
}

impl KeyCapability {
    fn compile(
        catalog: &Arc<GeometryCatalog>,
        retained: u128,
        cap: u128,
        control: &Control,
    ) -> Result<Arc<Self>, DiagnosticError> {
        control.check()?;
        if catalog.width() != 10
            || catalog.height() != 6
            || catalog.initial_board() != 0
            || catalog.required_cells() != (1_u64 << 60) - 1
        {
            return Err(DiagnosticError::UnsupportedCatalog);
        }
        let owner_bytes = (size_of::<Self>() + 2 * size_of::<usize>()) as u128;
        let mut row_tokens = allocated_vec(
            catalog.skeleton_count(),
            retained
                .checked_add(owner_bytes)
                .ok_or(DiagnosticError::Admission)?,
            cap,
            control,
        )?;
        let mut seen = [false; 7 * 64];
        for row in 0..catalog.skeleton_count() {
            if row & 255 == 0 {
                control.check()?;
            }
            let row_id = u32::try_from(row).map_err(|_| DiagnosticError::UnsupportedCatalog)?;
            let occupied = catalog.skeleton_occupied_rows(row_id);
            if occupied == 0 || occupied & !63 != 0 {
                return Err(DiagnosticError::UnsupportedCatalog);
            }
            let first = occupied.trailing_zeros();
            let last = u16::BITS - 1 - occupied.leading_zeros();
            let gaps = (((1_u16 << (last - first + 1)) - 1) << first) & !occupied;
            let poses = catalog.realizations(row_id);
            if poses.is_empty() || poses.iter().any(|pose| pose.required_deleted_rows != gaps) {
                return Err(DiagnosticError::UnsupportedCatalog);
            }
            // Authenticate the actual callback used by the DP as well as
            // every realization; no selected representative grants a key.
            if (0_u16..64).any(|deleted| {
                catalog.realization_requirement_is_satisfied(row_id, deleted)
                    != (gaps & !deleted == 0)
            }) {
                return Err(DiagnosticError::UnsupportedCatalog);
            }
            let token = (piece_index(catalog.skeleton(row_id).piece) as u16) << 6 | occupied;
            seen[usize::from(token)] = true;
            row_tokens.push(token);
        }
        let result = Arc::new(Self {
            catalog: Arc::downgrade(catalog),
            row_tokens,
            distinct_tokens: seen.into_iter().filter(|seen| *seen).count(),
        });
        admit(retained, result.retained_bytes(), cap)?;
        Ok(result)
    }

    fn retained_bytes(&self) -> u128 {
        (size_of::<Self>() + 2 * size_of::<usize>() + self.row_tokens.capacity() * size_of::<u16>())
            as u128
    }

    fn record(
        &self,
        catalog: &GeometryCatalog,
        rows: &[u32],
        decision: SourcePrefixDecision,
    ) -> Result<TraceRecord, DiagnosticError> {
        if !std::ptr::eq(self.catalog.as_ptr(), catalog) {
            return Err(DiagnosticError::ForeignCatalog);
        }
        if rows.len() > 15 {
            return Err(DiagnosticError::InvalidRow);
        }
        let mut record = TraceRecord {
            flags: VALID
                | rows.len() as u8
                | if decision.proved_infeasible {
                    NEGATIVE
                } else {
                    0
                }
                | if decision.root_fast_pass {
                    ROOT_FAST
                } else {
                    0
                },
            visited: decision.visited_row_states,
            ..TraceRecord::default()
        };
        for (slot, &row) in record.key.iter_mut().zip(rows) {
            *slot = *self
                .row_tokens
                .get(row as usize)
                .ok_or(DiagnosticError::InvalidRow)?;
        }
        record.key[..rows.len()].sort_unstable();
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TraceRecord {
    key: [u16; 15],
    flags: u8,
    visited: u8,
}

impl TraceRecord {
    fn depth(self) -> usize {
        usize::from(self.flags & DEPTH_MASK)
    }

    fn negative(self) -> bool {
        self.flags & NEGATIVE != 0
    }

    fn root_fast(self) -> bool {
        self.flags & ROOT_FAST != 0
    }

    fn key_cmp(&self, other: &Self) -> Ordering {
        self.depth()
            .cmp(&other.depth())
            .then_with(|| self.key.cmp(&other.key))
    }

    fn same_decision(self, other: Self) -> bool {
        self.negative() == other.negative()
            && self.root_fast() == other.root_fast()
            && self.visited == other.visited
    }

    fn hash(self) -> u64 {
        // Only a slot selector. Equality always compares every key word.
        self.key[..self.depth()].iter().fold(
            0xcbf2_9ce4_8422_2325 ^ self.depth() as u64,
            |hash, token| (hash ^ u64::from(*token)).wrapping_mul(0x0000_0100_0000_01b3),
        )
    }
}

#[derive(Debug)]
pub(super) struct PrefixTraceObserver {
    capability: Arc<KeyCapability>,
    records: Vec<TraceRecord>,
    limit: usize,
    after_limit: u64,
    error: Option<DiagnosticError>,
}

impl PrefixTraceObserver {
    fn new(
        capability: Arc<KeyCapability>,
        limit: usize,
        retained: u128,
        cap: u128,
        control: &Control,
    ) -> Result<Self, DiagnosticError> {
        let records = allocated_vec(limit, retained, cap, control)?;
        Ok(Self {
            capability,
            records,
            limit,
            after_limit: 0,
            error: None,
        })
    }

    pub(super) fn observe(
        &mut self,
        catalog: &GeometryCatalog,
        rows: &[u32],
        decision: SourcePrefixDecision,
    ) {
        if self.error.is_some() {
            return;
        }
        if self.records.len() >= self.limit {
            self.after_limit += 1;
            return;
        }
        match self.capability.record(catalog, rows, decision) {
            Ok(record) => self.records.push(record),
            Err(error) => self.error = Some(error),
        }
    }

    fn trace_bytes(&self) -> u128 {
        self.records.capacity() as u128 * size_of::<TraceRecord>() as u128
    }
}

#[derive(Clone, Copy, Debug)]
enum ReplayPolicy {
    AllChecks,
    ExcludeRootFast,
}

#[derive(Clone, Copy, Debug, Default)]
struct DepthStats {
    accesses: u64,
    bypassed_root_fast: u64,
    hits: u64,
    unknown_hits: u64,
    negative_hits: u64,
    misses: u64,
    collisions: u64,
    hit_row_states: u64,
}

fn replay(
    records: &[TraceRecord],
    slot_count: usize,
    policy: ReplayPolicy,
    retained: u128,
    cap: u128,
    control: &Control,
) -> Result<[DepthStats; 16], DiagnosticError> {
    if !slot_count.is_power_of_two() {
        return Err(DiagnosticError::Admission);
    }
    let mut slots = allocated_vec(slot_count, retained, cap, control)?;
    slots.resize(slot_count, TraceRecord::default());
    let mut stats = [DepthStats::default(); 16];
    for (index, &record) in records.iter().enumerate() {
        if index & 4095 == 0 {
            control.check()?;
        }
        let depth = &mut stats[record.depth()];
        if matches!(policy, ReplayPolicy::ExcludeRootFast) && record.root_fast() {
            depth.bypassed_root_fast += 1;
            continue;
        }
        depth.accesses += 1;
        let slot = &mut slots[record.hash() as usize & (slot_count - 1)];
        if slot.flags & VALID != 0 && slot.key_cmp(&record) == Ordering::Equal {
            if !slot.same_decision(record) {
                return Err(DiagnosticError::InconsistentDecision);
            }
            depth.hits += 1;
            depth.negative_hits += u64::from(record.negative());
            depth.unknown_hits += u64::from(!record.negative());
            depth.hit_row_states += u64::from(record.visited);
        } else {
            depth.misses += 1;
            depth.collisions += u64::from(slot.flags & VALID != 0);
            *slot = record;
        }
    }
    control.check()?;
    Ok(stats)
}

#[derive(Clone, Copy, Debug, Default)]
struct DistinctStats {
    records: u64,
    unique: u64,
    negative_unique: u64,
}

// In-place heapsort permits bounded cancellation polling without a second
// trace allocation or an uninterruptible sort of a user-sized sample.
fn distinct(
    records: &mut [TraceRecord],
    control: &Control,
) -> Result<[DistinctStats; 16], DiagnosticError> {
    fn sift(
        records: &mut [TraceRecord],
        mut root: usize,
        end: usize,
        steps: &mut usize,
        control: &Control,
    ) -> Result<(), DiagnosticError> {
        loop {
            *steps = steps.wrapping_add(1);
            if *steps & 4095 == 0 {
                control.check()?;
            }
            let left = root * 2 + 1;
            if left >= end {
                return Ok(());
            }
            let child =
                if left + 1 < end && records[left].key_cmp(&records[left + 1]) == Ordering::Less {
                    left + 1
                } else {
                    left
                };
            if records[root].key_cmp(&records[child]) != Ordering::Less {
                return Ok(());
            }
            records.swap(root, child);
            root = child;
        }
    }
    control.check()?;
    let mut steps = 0;
    for root in (0..records.len() / 2).rev() {
        sift(records, root, records.len(), &mut steps, control)?;
    }
    for end in (1..records.len()).rev() {
        records.swap(0, end);
        sift(records, 0, end, &mut steps, control)?;
    }
    let mut stats = [DistinctStats::default(); 16];
    let mut previous: Option<TraceRecord> = None;
    for (index, &record) in records.iter().enumerate() {
        if index & 4095 == 0 {
            control.check()?;
        }
        let depth = &mut stats[record.depth()];
        depth.records += 1;
        if let Some(prior) = previous.filter(|prior| prior.key_cmp(&record) == Ordering::Equal) {
            if !prior.same_decision(record) {
                return Err(DiagnosticError::InconsistentDecision);
            }
        } else {
            depth.unique += 1;
            depth.negative_unique += u64::from(record.negative());
        }
        previous = Some(record);
    }
    control.check()?;
    Ok(stats)
}

fn test_catalog() -> Arc<GeometryCatalog> {
    Arc::new(
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, (1_u64 << 60) - 1)
            .unwrap(),
    )
}

fn test_prefixes(catalog: &GeometryCatalog, shift: u8) -> [Vec<u32>; 3] {
    use clearra_core_domain::piece::piece_kind::PieceKind::{I, O, T};
    let fast = vec![catalog.skeleton_id(I, 15_u64 << shift).unwrap()];
    let slow: Vec<_> = [0, 3, 6]
        .map(|x| {
            let x = x + shift;
            catalog
                .skeleton_id(O, (3_u64 << x) | (3_u64 << (20 + x)))
                .unwrap()
        })
        .into();
    let mut negative = slow.clone();
    for x in [0, 4] {
        let x = x + shift;
        negative.push(
            catalog
                .skeleton_id(T, (7_u64 << (10 + x)) | (2_u64 << (30 + x)))
                .unwrap(),
        );
    }
    [fast, slow, negative]
}

#[test]
fn source_prefix_cache_diagnostic_keys_preserve_actual_decisions_and_owner() {
    assert_eq!(size_of::<TraceRecord>(), 32);
    let catalog = test_catalog();
    let control = Control::default();
    let capability = KeyCapability::compile(&catalog, 0, u128::MAX, &control).unwrap();
    assert_eq!(capability.distinct_tokens, 211);
    let mut profile = SourcePrefixProfile {
        trace_observer: Some(
            PrefixTraceObserver::new(Arc::clone(&capability), 6, 0, u128::MAX, &control).unwrap(),
        ),
        ..SourcePrefixProfile::default()
    };
    let prefixes = test_prefixes(&catalog, 0);
    let translated = test_prefixes(&catalog, 1);
    let targets: Vec<_> = (0..7)
        .map(|extra| {
            let mut counts = [2; 7];
            counts[extra] += 1;
            super::TargetGroup {
                key: clearra_supply::pattern_universe::PieceMultisetKey::from_counts(counts),
                pattern_index_id: extra as u32,
                possible_patterns: Arc::new(
                    clearra_coverage::pattern::pattern_bitset::PatternBitSet::new(1),
                ),
                pattern_index: None,
            }
        })
        .collect();
    let request = super::FullDrawPrefixRequest {
        prefix_max: [2; 7],
        suffix_min: [1; 7],
        two_bag_plus_one: false,
    }
    .for_targets(&targets)
    .unwrap();
    for (index, (rows, other)) in prefixes.iter().zip(translated).enumerate() {
        let original = super::selected_source_prefix_decision(&catalog, rows);
        assert_eq!(original.proved_infeasible, index == 2);
        assert_eq!(original.root_fast_pass, index == 0);
        assert_eq!(
            profile.stalls(request, &targets, &catalog, rows),
            original.proved_infeasible
        );
        let reversed: Vec<_> = other.into_iter().rev().collect();
        assert_eq!(
            profile.stalls(request, &targets, &catalog, &reversed),
            original.proved_infeasible
        );
    }
    let observer = profile.trace_observer.take().unwrap();
    assert!(observer.error.is_none());
    for pair in observer.records.chunks_exact(2) {
        assert_eq!(pair[0].key_cmp(&pair[1]), Ordering::Equal);
        assert!(pair[0].same_decision(pair[1]));
    }
    let foreign = Arc::new((*catalog).clone());
    assert_eq!(
        capability.record(&foreign, &prefixes[0], SourcePrefixDecision::default()),
        Err(DiagnosticError::ForeignCatalog)
    );
    let unsupported = Arc::new(
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap(),
    );
    assert!(matches!(
        KeyCapability::compile(&unsupported, 0, u128::MAX, &control),
        Err(DiagnosticError::UnsupportedCatalog)
    ));
}

#[test]
fn source_prefix_cache_diagnostic_replay_checks_both_hit_kinds_collisions_and_depths() {
    let catalog = test_catalog();
    let control = Control::default();
    let capability = KeyCapability::compile(&catalog, 0, u128::MAX, &control).unwrap();
    let mut records = Vec::new();
    for rows in test_prefixes(&catalog, 0) {
        let record = capability
            .record(
                &catalog,
                &rows,
                super::selected_source_prefix_decision(&catalog, &rows),
            )
            .unwrap();
        records.extend([record, record]);
    }
    let stats = replay(&records, 1, ReplayPolicy::AllChecks, 0, u128::MAX, &control).unwrap();
    assert_eq!(stats.iter().map(|stats| stats.hits).sum::<u64>(), 3);
    assert_eq!(stats.iter().map(|stats| stats.unknown_hits).sum::<u64>(), 2);
    assert_eq!(
        stats.iter().map(|stats| stats.negative_hits).sum::<u64>(),
        1
    );
    assert_eq!(stats.iter().map(|stats| stats.collisions).sum::<u64>(), 2);
    let excluded = replay(
        &records,
        1,
        ReplayPolicy::ExcludeRootFast,
        0,
        u128::MAX,
        &control,
    )
    .unwrap();
    assert_eq!(excluded[1].bypassed_root_fast, 2);
    assert_eq!(excluded[1].accesses, 0);
    assert_eq!(excluded[3].unknown_hits, 1);
    assert_eq!(excluded[5].negative_hits, 1);
    // A fresh replay is a cold branch: no cache state crosses owner/branch reset.
    let fresh = replay(
        &records[..1],
        1,
        ReplayPolicy::AllChecks,
        0,
        u128::MAX,
        &control,
    )
    .unwrap();
    assert_eq!(fresh[1].misses, 1);
    records.reverse();
    let unique = distinct(&mut records, &control).unwrap();
    for depth in [1, 3, 5] {
        assert_eq!(unique[depth].records, 2);
        assert_eq!(unique[depth].unique, 1);
    }
    assert_eq!(unique[5].negative_unique, 1);
    let mut disagreement = records[0];
    disagreement.flags ^= NEGATIVE;
    assert!(matches!(
        replay(
            &[records[0], disagreement],
            1,
            ReplayPolicy::AllChecks,
            0,
            u128::MAX,
            &control
        ),
        Err(DiagnosticError::InconsistentDecision)
    ));
}

#[test]
fn source_prefix_cache_diagnostic_admits_actual_capacity_and_keeps_cancellation_incomplete() {
    let control = Control::default();
    let values = allocated_vec::<TraceRecord>(17, 29, u128::MAX, &control).unwrap();
    let required = 29 + values.capacity() as u128 * size_of::<TraceRecord>() as u128;
    drop(values);
    assert!(allocated_vec::<TraceRecord>(17, 29, required, &control).is_ok());
    assert!(matches!(
        allocated_vec::<TraceRecord>(17, 29, required - 1, &control),
        Err(DiagnosticError::Admission)
    ));
    assert!(matches!(
        allocated_vec::<TraceRecord>(usize::MAX, 0, 64, &control),
        Err(DiagnosticError::Admission)
    ));
    let token = clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
    token.handle().cancel();
    let cancelled = Control {
        execution: ExecutionControl::new(token),
        deadline: None,
    };
    assert!(matches!(
        replay(&[], 1, ReplayPolicy::AllChecks, 0, 32, &cancelled),
        Err(DiagnosticError::Cancelled)
    ));
    assert!(matches!(
        distinct(&mut [], &cancelled),
        Err(DiagnosticError::Cancelled)
    ));
    let expired = Control {
        execution: ExecutionControl::default(),
        deadline: Some(Instant::now()),
    };
    assert!(matches!(
        allocated_vec::<TraceRecord>(1, 0, 32, &expired),
        Err(DiagnosticError::Cancelled)
    ));
}

#[cfg(feature = "parallel")]
struct PreparationProgress {
    deadline: Instant,
    cancel: clearra_core_domain::execution_cancellation::ExecutionCancellationHandle,
    last_report: std::sync::Mutex<Instant>,
}

#[cfg(feature = "parallel")]
impl clearra_core_domain::execution_cancellation::ProgressSink for PreparationProgress {
    fn report(&self, progress: clearra_core_domain::execution_cancellation::ExecutionProgress) {
        let now = Instant::now();
        if now >= self.deadline {
            self.cancel.cancel();
        }
        let mut last = self.last_report.lock().unwrap();
        if now.duration_since(*last) >= Duration::from_secs(5)
            || progress.stage == "geometry-candidate-family"
        {
            println!(
                "source_prefix_cache_prepare stage={} completed={} total={:?}",
                progress.stage, progress.completed, progress.total
            );
            *last = now;
        }
    }
}

#[cfg(feature = "parallel")]
fn full6_problem() -> clearra_problem::SearchProblem {
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
    };
    use clearra_supply::{
        queue::queue_pattern_expression::QueuePatternExpression, QueueObservationPolicy,
    };

    // Same real ScenarioPc factory and full-draw authority as the Full6
    // harness/count diagnostic. No explicit 177M-queue materialization.
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::pattern_expression(QueuePatternExpression::parse("P1P7P7", 0).unwrap()),
        PieceWindow::new(15),
    )
    .with_rule(clearra_rules::profile::builtin_rules::srs_plus())
    .with_exact_pieces(Some(15))
    .with_supply_window_size(SupplyWindowSize::new(15))
    .with_min_remaining_queue(0)
    .with_hold_piece(None)
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false)
            .with_max_patterns(0),
    );
    clearra_problem::ProblemCompiler::compile_scenario_pc(&query).unwrap()
}

#[cfg(feature = "parallel")]
fn plan_retained_bytes(
    plan: &super::super::ParallelGeometryPlan,
    fixed_bytes: u128,
) -> Result<u128, DiagnosticError> {
    use super::super::{FamilyEnumerator, GeometrySearch, GeometrySolutionFamily};
    // At least two live partitions preserve shared Arc ownership here. The
    // shared family/table and target backing are counted once, while each
    // enumerator contributes only its mutable traversal stack.
    let mut retained = fixed_bytes
        .checked_add(plan.shared_family_bytes as u128)
        .and_then(|bytes| bytes.checked_add(size_of::<GeometrySolutionFamily>() as u128))
        .and_then(|bytes| bytes.checked_add(plan.group_pattern_index_bytes as u128))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of_val(plan.targets.as_ref()) as u128))
        .and_then(|bytes| {
            bytes
                .checked_add(plan.searches.capacity() as u128 * size_of::<GeometrySearch>() as u128)
        })
        // Arc headers for the shared family and target slice.
        .and_then(|bytes| bytes.checked_add((4 * size_of::<usize>()) as u128))
        .ok_or(DiagnosticError::Admission)?;
    for search in &plan.searches {
        let enumerator = search
            .enumerator
            .as_ref()
            .ok_or(DiagnosticError::Geometry("partition enumerator missing"))?;
        retained = retained
            .checked_add(FamilyEnumerator::retained_bytes(enumerator) as u128)
            .ok_or(DiagnosticError::Admission)?;
    }
    Ok(retained)
}

#[cfg(feature = "parallel")]
fn run_full6_trace(
    sample_limit: usize,
    cap: u128,
    control: &Control,
) -> Result<(), DiagnosticError> {
    use super::super::{
        FamilyEnumerationAdvance, GeometrySearch, ParallelGeometryPreparation,
        PHYSICAL_PREFIX_ENUMERATION_WORK_QUANTUM,
    };
    use clearra_core_domain::piece::piece_kind::PieceKind;

    const PARTITION_WORKERS: usize = 9;
    control.check()?;
    let problem = full6_problem();
    let universe = problem.piece_source().materialized_universe().unwrap();
    let layout = universe
        .complete_factorized_layout()
        .expect("complete original factorized source");
    assert_eq!(universe.pattern_count(), 177_811_200);
    assert_eq!(universe.total_possible_pattern_count(), 177_811_200);
    assert_eq!(
        (layout.full_sequence_len(), layout.visible_sequence_len()),
        (15, 15)
    );
    assert_eq!(layout.segment_count(), 3);
    assert!(layout.segments().zip([1, 7, 7]).all(|(segment, draws)| {
        segment.draw_count() == draws && segment.choices() == PieceKind::STANDARD_TETROMINOES
    }));
    assert!(super::FullDrawPrefixRequest::compile(&problem).is_some());
    let catalog = Arc::new(GeometryCatalog::compile(&problem).unwrap());
    let multiset = universe
        .symbolic_factorized_full_draw_family_for_execution(
            15,
            problem.initial_hold(),
            true,
            crate::backend::wasm_cpu::packing_hold_projection(&problem),
        )
        .unwrap()
        .unwrap();
    let mut geometry =
        GeometrySearch::new(universe, &multiset, catalog.required_cells(), false).unwrap();
    geometry.enable_physical_prefix_pruning();
    geometry.enable_full_draw_source_prefix_pruning(&problem);
    assert!(geometry.source_prefix_pruning_enabled());
    drop(multiset);

    let source_bytes = problem
        .checked_pc_family_pointee_retained_bytes()
        .ok_or(DiagnosticError::Admission)?;
    let fixed_bytes = source_bytes
        .checked_add(catalog.retained_bytes() as u128)
        .and_then(|bytes| {
            bytes.checked_add(
                (size_of::<GeometryCatalog>()
                    + 2 * size_of::<usize>()
                    + size_of::<clearra_problem::SearchProblem>()) as u128,
            )
        })
        .ok_or(DiagnosticError::Admission)?;
    admit(fixed_bytes, geometry.retained_bytes() as u128, cap)?;
    let started = Instant::now();
    // Preserve the ordinary unbounded compiler, its construction-index
    // release, path-count table, row-order preparation, and canonical split.
    // The runtime manager owns native preparation's process-memory peak.
    let prepared = geometry
        .prepare_parallel(
            &catalog,
            &control.execution,
            PARTITION_WORKERS,
            |_, workers| workers,
        )
        .map_err(|error| {
            if matches!(error, super::super::WasmExactSearchError::Cancelled) {
                DiagnosticError::Cancelled
            } else {
                DiagnosticError::Geometry(error.reason())
            }
        })?;
    control.check()?;
    let ParallelGeometryPreparation::Ready {
        mut plan,
        balanced_workers,
    } = prepared
    else {
        return Err(DiagnosticError::Geometry(
            "canonical partitioning unavailable",
        ));
    };
    assert_eq!(balanced_workers, PARTITION_WORKERS);
    assert_eq!(plan.searches.len(), PARTITION_WORKERS * 8);
    let raw: u128 = plan
        .searches
        .iter()
        .map(|search| search.candidate_family_count().unwrap())
        .sum();
    assert_eq!(raw, 247_794_216_021_372);
    let retained = plan_retained_bytes(&plan, fixed_bytes)?;
    admit(retained, 0, cap)?;
    let capability = KeyCapability::compile(&catalog, retained, cap, control)?;
    let fixed_bytes = fixed_bytes
        .checked_add(capability.retained_bytes())
        .ok_or(DiagnosticError::Admission)?;
    println!(
        "source_prefix_cache_family raw_paths={raw} partitions={} prepare_seconds={:.6} accounted_retained_bytes={} catalog_skeletons={} semantic_tokens={}",
        plan.searches.len(),
        started.elapsed().as_secs_f64(),
        plan_retained_bytes(&plan, fixed_bytes)?,
        catalog.skeleton_count(),
        capability.distinct_tokens,
    );

    // Private caches would have no inter-worker ordering dependency. Sample
    // the first nine canonical partitions in the existing dispatch priority,
    // traversing each stream scalar and leaving product worker policy alone.
    let mut indices: [usize; PARTITION_WORKERS * 8] = std::array::from_fn(|index| index);
    indices.sort_unstable_by(|&left, &right| {
        plan.searches[right]
            .parallel_priority()
            .cmp(&plan.searches[left].parallel_priority())
            .then_with(|| left.cmp(&right))
    });
    for (dispatch_rank, &canonical_index) in indices.iter().take(PARTITION_WORKERS).enumerate() {
        control.check()?;
        let retained = plan_retained_bytes(&plan, fixed_bytes)?
            .checked_add(std::mem::size_of_val(&indices) as u128)
            .ok_or(DiagnosticError::Admission)?;
        let observer = PrefixTraceObserver::new(
            Arc::clone(&capability),
            sample_limit,
            retained,
            cap,
            control,
        )?;
        let trace_bytes = observer.trace_bytes();
        plan.searches[canonical_index]
            .enumerator
            .as_mut()
            .unwrap()
            .source_prefix_profile
            .trace_observer = Some(observer);
        let started = Instant::now();
        let mut advances = 0_u64;
        let mut emitted = 0_u64;
        let mut branch_exhausted = false;
        let mut last_report = Instant::now();
        loop {
            control.check()?;
            // Call the same resumable enumerator without dropping it on
            // completion, so its final trace can always be recovered.
            let enumerator = plan.searches[canonical_index].enumerator.as_mut().unwrap();
            let advance = enumerator
                .next_candidate_step(&catalog, true, PHYSICAL_PREFIX_ENUMERATION_WORK_QUANTUM)
                .map_err(|_| DiagnosticError::Geometry("traversal incomplete"))?;
            advances += 1;
            match advance {
                FamilyEnumerationAdvance::Pending => {}
                FamilyEnumerationAdvance::Candidate(_) => emitted += 1,
                FamilyEnumerationAdvance::Complete => branch_exhausted = true,
            }
            let observer = enumerator
                .source_prefix_profile
                .trace_observer
                .as_ref()
                .unwrap();
            if let Some(error) = &observer.error {
                return Err(error.clone());
            }
            if observer.records.len() == sample_limit || branch_exhausted {
                break;
            }
            if advances & 1023 == 0 {
                admit(plan_retained_bytes(&plan, fixed_bytes)?, trace_bytes, cap)?;
                if last_report.elapsed() >= Duration::from_secs(5) {
                    let observer = plan.searches[canonical_index]
                        .enumerator
                        .as_ref()
                        .unwrap()
                        .source_prefix_profile
                        .trace_observer
                        .as_ref()
                        .unwrap();
                    println!(
                        "source_prefix_cache_collect branch={canonical_index} sampled={} emitted={emitted} advances={advances}",
                        observer.records.len()
                    );
                    last_report = Instant::now();
                }
            }
        }
        let mut observer = plan.searches[canonical_index]
            .enumerator
            .as_mut()
            .unwrap()
            .source_prefix_profile
            .trace_observer
            .take()
            .unwrap();
        let retained = plan_retained_bytes(&plan, fixed_bytes)?
            .checked_add(trace_bytes)
            .and_then(|bytes| bytes.checked_add(std::mem::size_of_val(&indices) as u128))
            .ok_or(DiagnosticError::Admission)?;
        admit(retained, 0, cap)?;
        println!(
            "source_prefix_cache_trace branch={canonical_index} dispatch_rank={dispatch_rank} sampled={} trace_capacity={} trace_bytes={trace_bytes} after_sample_in_final_quantum={} emitted={emitted} advances={advances} branch_exhausted={branch_exhausted} capture_seconds={:.6} exhaustive_search_complete=false",
            observer.records.len(),
            observer.records.capacity(),
            observer.after_limit,
            started.elapsed().as_secs_f64(),
        );
        for slots in [4096, 16_384, 65_536] {
            for policy in [ReplayPolicy::AllChecks, ReplayPolicy::ExcludeRootFast] {
                let started = Instant::now();
                let stats = replay(&observer.records, slots, policy, retained, cap, control)?;
                for (depth, stats) in stats.into_iter().enumerate() {
                    if stats.accesses == 0 && stats.bypassed_root_fast == 0 {
                        continue;
                    }
                    println!(
                        "source_prefix_cache_replay branch={canonical_index} slots={slots} policy={policy:?} depth={depth} accesses={} bypassed_root_fast={} hits={} unknown_hits={} negative_hits={} misses={} collisions={} hit_row_states={} replay_seconds={:.6}",
                        stats.accesses, stats.bypassed_root_fast, stats.hits,
                        stats.unknown_hits, stats.negative_hits, stats.misses,
                        stats.collisions, stats.hit_row_states, started.elapsed().as_secs_f64(),
                    );
                }
            }
        }
        for (depth, stats) in distinct(&mut observer.records, control)?
            .into_iter()
            .enumerate()
        {
            if stats.records != 0 {
                println!(
                    "source_prefix_cache_distinct branch={canonical_index} depth={depth} sampled={} unique_keys={} unknown_unique={} negative_unique={}",
                    stats.records, stats.unique, stats.unique - stats.negative_unique,
                    stats.negative_unique,
                );
            }
        }
        // No cache or trace crosses a branch. The next sample starts cold.
        drop(observer);
    }
    control.check()?;
    Ok(())
}

/// Run only through the existing runtime manager. These explicit limits
/// govern a diagnostic trace, never a product generation/solution budget.
#[cfg(feature = "parallel")]
#[test]
#[ignore = "supervised diagnostic; set CLEARRA_SOURCE_PREFIX_CACHE_SAMPLES, CLEARRA_SOURCE_PREFIX_CACHE_SECONDS, and CLEARRA_SOURCE_PREFIX_CACHE_MEMORY_BYTES"]
fn full6l_source_prefix_cache_trace_diagnostic() {
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
    let limit = |name: &str| {
        let value = std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} is required for this ignored diagnostic"))
            .parse::<u64>()
            .expect("positive diagnostic limit");
        assert_ne!(value, 0, "{name} must be positive");
        value
    };
    let samples = usize::try_from(limit("CLEARRA_SOURCE_PREFIX_CACHE_SAMPLES"))
        .expect("sample count fits this diagnostic host");
    let seconds = limit("CLEARRA_SOURCE_PREFIX_CACHE_SECONDS");
    let cap = u128::from(limit("CLEARRA_SOURCE_PREFIX_CACHE_MEMORY_BYTES"));
    let started = Instant::now();
    let deadline = started
        .checked_add(Duration::from_secs(seconds))
        .expect("finite deadline");
    let token = ExecutionCancellationToken::new();
    let progress = Arc::new(PreparationProgress {
        deadline,
        cancel: token.handle(),
        last_report: std::sync::Mutex::new(started),
    });
    let control = Control {
        execution: ExecutionControl::new(token).with_progress_sink(progress),
        deadline: Some(deadline),
    };
    println!(
        "source_prefix_cache_diagnostic scalar_threads=1 canonical_partition_workers=9 sampled_branches=9 samples_per_branch={samples} checked_memory_cap_bytes={cap} deadline_seconds={seconds} preparation_peak_owner=runtime-supervisor product_workers_unchanged=true producer_cache_enabled=false"
    );
    match run_full6_trace(samples, cap, &control) {
        Ok(()) => println!(
            "source_prefix_cache_final sample_complete=true exhaustive_search_complete=false seconds={:.6}",
            started.elapsed().as_secs_f64()
        ),
        Err(error) => {
            println!(
                "source_prefix_cache_final sample_complete=false exhaustive_search_complete=false error={error:?} seconds={:.6}",
                started.elapsed().as_secs_f64()
            );
            panic!("incomplete cache diagnostic: {error:?}");
        }
    }
}
