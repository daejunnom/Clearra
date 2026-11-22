//! Exact, bounded reuse of the source-prefix cap's word masks.
//!
//! A plan depends on the complete operation-kind sequence, not merely its
//! multiset. The owner fixes n, H, and the certified kind limits. It preserves
//! the scalar predicate `popcount(S) <= H && count_kind(S) <= limit_kind`;
//! candidate-local active edges and reverse productivity remain the caller's
//! responsibility. No source or movement authority is inferred here.
//!
//! Query work is checked before lookup, construction, and storage operations.
//! On success, charge `PrefixMaskQuery::work` to the owner. On failure, the
//! change in `metrics().work` reports the work already charged. A caller must
//! separately charge `RUNTIME_WORD_WORK` before applying a nontrivial word.
//! Words with no active lanes or no possible cap rejection keep the old
//! scalar depth loop. Scanning every word unconditionally can cost more than
//! the individual rejected prefixes it replaces.

use std::collections::HashMap;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use crate::backend::wasm_cpu::{queue_observation_policy::piece_code, WasmExactSearchError};

pub(in crate::backend::wasm_cpu) const MAX_MASK_OPERATIONS: u8 = 11;
const MAX_MASK_WORDS: usize = (1 << MAX_MASK_OPERATIONS) / 64;
const MAX_CACHE_ENTRIES: usize = 512;
pub(in crate::backend::wasm_cpu) const RUNTIME_WORD_WORK: usize = 1;

// T[G][k] contains exactly the lanes l with popcount(l & G) <= k.
// This immutable 3,584-byte table has no per-request construction or heap.
const LANES_WITH_AT_MOST: [[u64; 7]; 64] = {
    let mut table = [[0_u64; 7]; 64];
    let mut group = 0_usize;
    while group < 64 {
        let mut limit = 0_usize;
        while limit <= 6 {
            let mut lane = 0_usize;
            while lane < 64 {
                if (lane & group).count_ones() as usize <= limit {
                    table[group][limit] |= 1_u64 << lane;
                }
                lane += 1;
            }
            limit += 1;
        }
        group += 1;
    }
    table
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Scope {
    operations: u8,
    horizon: u8,
    limits: [u8; 7],
}

struct Entry {
    key: u64,
    valid: [u64; MAX_MASK_WORDS],
    cap_sensitive_words: u32,
}

/// Small append-only slot ticket; no 32-word array is copied on a cache hit.
/// The scope and exact key also reject tickets from a different owner scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::backend::wasm_cpu) struct PrefixMaskPlan {
    scope: Scope,
    key: u64,
    slot: u16,
    cap_sensitive_words: u32,
}

impl PrefixMaskPlan {
    pub(in crate::backend::wasm_cpu) fn needs_filter(self, word: usize, active: u64) -> bool {
        active != 0
            && word < word_count(self.scope.operations)
            && self.cap_sensitive_words & (1_u32 << word) != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::backend::wasm_cpu) struct PrefixMaskWord {
    pub valid: u64,
    /// Invalid only because of the cap, restricted to depths <= H.
    pub cap_invalid: u64,
}

impl PrefixMaskWord {
    /// The caller first charges RUNTIME_WORD_WORK and polls cancellation.
    /// This also removes depths > H, which the old loop skipped individually.
    pub(in crate::backend::wasm_cpu) fn filter(self, active: u64) -> (u64, usize) {
        (
            active & self.valid,
            (active & self.cap_invalid).count_ones() as usize,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::backend::wasm_cpu) struct PrefixMaskMetrics {
    pub lookups: usize,
    pub hits: usize,
    pub misses: usize,
    pub full_fallbacks: usize,
    pub horizon_words: usize,
    pub group_piece_reads: usize,
    pub group_checks: usize,
    pub build_words: usize,
    pub build_group_checks: usize,
    pub storage_words: usize,
    pub insertions: usize,
    pub work: usize,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::backend::wasm_cpu) struct PrefixMaskQuery {
    /// None means the bounded cache is full; use the unchanged scalar check.
    pub plan: Option<PrefixMaskPlan>,
    pub work: usize,
    pub cache_hit: bool,
}

pub(in crate::backend::wasm_cpu) struct PrefixMaskCache {
    scope: Scope,
    // Common depth mask, stored once rather than in every sequence plan.
    horizon_words: [u64; MAX_MASK_WORDS],
    horizon_ready: bool,
    entries: Vec<Entry>,
    slots: HashMap<u64, u16>,
    metrics: PrefixMaskMetrics,
    peak_bytes: usize,
}

impl PrefixMaskCache {
    pub(in crate::backend::wasm_cpu) fn new(
        operations: u8,
        horizon: u8,
        limits: [u8; 7],
    ) -> Result<Self, WasmExactSearchError> {
        if operations == 0 || operations > MAX_MASK_OPERATIONS || horizon > operations {
            return Err(invalid("wasm_observation_prefix_mask_scope_invalid"));
        }
        Ok(Self {
            scope: Scope {
                operations,
                horizon,
                limits,
            },
            horizon_words: [0; MAX_MASK_WORDS],
            horizon_ready: false,
            entries: Vec::new(),
            slots: HashMap::new(),
            metrics: PrefixMaskMetrics::default(),
            peak_bytes: 0,
        })
    }

    /// `remaining_work` is the caller's remaining shared budget, before this
    /// query. It is not a separate or enlarged limit. Work units comprise one
    /// bounded (n <= 11) exact-key lookup, individual miss construction steps,
    /// and both the temporary initialization and retained copy of plan words.
    pub(in crate::backend::wasm_cpu) fn query(
        &mut self,
        pieces: &[PieceKind],
        remaining_work: usize,
        control: &ExecutionControl,
    ) -> Result<PrefixMaskQuery, WasmExactSearchError> {
        let mut budget = WorkBudget::new(remaining_work);
        budget.charge(1, &mut self.metrics, control)?;
        self.metrics.lookups += 1;
        if pieces.len() != usize::from(self.scope.operations) {
            return Err(invalid("wasm_observation_prefix_mask_scope_changed"));
        }
        let key = sequence_key(pieces);
        if let Some(slot) = self.slots.get(&key).copied() {
            self.metrics.hits += 1;
            return Ok(PrefixMaskQuery {
                plan: Some(self.ticket(slot)),
                work: budget.used,
                cache_hit: true,
            });
        }
        self.metrics.misses += 1;
        if self.entries.len() == MAX_CACHE_ENTRIES {
            self.metrics.full_fallbacks += 1;
            return Ok(PrefixMaskQuery {
                plan: None,
                work: budget.used,
                cache_hit: false,
            });
        }

        self.prepare_horizon(&mut budget, control)?;
        let mut groups = [0_u16; 7];
        for (operation, &piece) in pieces.iter().enumerate() {
            budget.charge(1, &mut self.metrics, control)?;
            self.metrics.group_piece_reads += 1;
            groups[usize::from(piece_code(piece) - 1)] |= 1_u16 << operation;
        }
        let mut limited = [(0_u16, 0_u8); 7];
        let mut limited_count = 0;
        for (operations, limit) in groups.into_iter().zip(self.scope.limits) {
            budget.charge(1, &mut self.metrics, control)?;
            self.metrics.group_checks += 1;
            if operations.count_ones() > u32::from(limit) {
                limited[limited_count] = (operations, limit);
                limited_count += 1;
            }
        }

        budget.charge(MAX_MASK_WORDS, &mut self.metrics, control)?;
        self.metrics.storage_words += MAX_MASK_WORDS;
        let mut valid = [0_u64; MAX_MASK_WORDS];
        let mut cap_sensitive_words = 0_u32;
        for word in 0..word_count(self.scope.operations) {
            budget.charge(1, &mut self.metrics, control)?;
            self.metrics.build_words += 1;
            let horizon = self.horizon_words[word];
            let mut retained = horizon;
            for &(group, limit) in &limited[..limited_count] {
                if retained == 0 {
                    break;
                }
                budget.charge(1, &mut self.metrics, control)?;
                self.metrics.build_group_checks += 1;
                let high_count = ((word as u16) & (group >> 6)).count_ones();
                if high_count > u32::from(limit) {
                    retained = 0;
                    break;
                }
                let remaining = u32::from(limit) - high_count;
                retained &= LANES_WITH_AT_MOST[usize::from(group & 63)][remaining.min(6) as usize];
            }
            valid[word] = retained;
            if horizon & !retained != 0 {
                cap_sensitive_words |= 1_u32 << word;
            }
        }

        // Precharge the retained array copy and metadata insertion before
        // allocating or publishing any entry. The append-only slot is stable.
        budget.charge(MAX_MASK_WORDS + 1, &mut self.metrics, control)?;
        self.metrics.storage_words += MAX_MASK_WORDS;
        self.metrics.insertions += 1;
        self.reserve_storage()?;
        let slot = self.entries.len() as u16;
        self.entries.push(Entry {
            key,
            valid,
            cap_sensitive_words,
        });
        self.slots.insert(key, slot);
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(PrefixMaskQuery {
            plan: Some(self.ticket(slot)),
            work: budget.used,
            cache_hit: false,
        })
    }

    pub(in crate::backend::wasm_cpu) fn word(
        &self,
        plan: PrefixMaskPlan,
        word: usize,
    ) -> Result<PrefixMaskWord, WasmExactSearchError> {
        if plan.scope != self.scope || word >= word_count(self.scope.operations) {
            return Err(invalid("wasm_observation_prefix_mask_scope_changed"));
        }
        let entry = self
            .entries
            .get(usize::from(plan.slot))
            .filter(|entry| entry.key == plan.key)
            .ok_or_else(|| invalid("wasm_observation_prefix_mask_plan_missing"))?;
        let valid = entry.valid[word];
        Ok(PrefixMaskWord {
            valid,
            cap_invalid: self.horizon_words[word] & !valid,
        })
    }

    pub(in crate::backend::wasm_cpu) fn metrics(&self) -> PrefixMaskMetrics {
        self.metrics
    }

    /// Heap capacity only. The common horizon row is inline in this owner;
    /// the constant lane table belongs to the executable's shared static data.
    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        self.entries.capacity() * core::mem::size_of::<Entry>() + self.index_bytes()
    }

    pub(in crate::backend::wasm_cpu) fn peak_bytes(&self) -> usize {
        self.peak_bytes
    }

    fn ticket(&self, slot: u16) -> PrefixMaskPlan {
        let entry = &self.entries[usize::from(slot)];
        PrefixMaskPlan {
            scope: self.scope,
            key: entry.key,
            slot,
            cap_sensitive_words: entry.cap_sensitive_words,
        }
    }

    fn prepare_horizon(
        &mut self,
        budget: &mut WorkBudget,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if self.horizon_ready {
            return Ok(());
        }
        let prefixes = 1_usize << self.scope.operations;
        let field = if prefixes >= 64 {
            u64::MAX
        } else {
            (1_u64 << prefixes) - 1
        };
        for word in 0..word_count(self.scope.operations) {
            budget.charge(1, &mut self.metrics, control)?;
            self.metrics.horizon_words += 1;
            self.horizon_words[word] = self
                .scope
                .horizon
                .checked_sub(word.count_ones() as u8)
                .map_or(0, |low_depth| {
                    LANES_WITH_AT_MOST[63][usize::from(low_depth.min(6))] & field
                });
        }
        self.horizon_ready = true;
        Ok(())
    }

    fn reserve_storage(&mut self) -> Result<(), WasmExactSearchError> {
        // Reserve the bound once. Subsequent appends neither move plan arrays
        // nor invalidate tickets. The peak formula also covers a future
        // smaller initial reservation with old/new buffers coexisting.
        if self.entries.capacity() < MAX_CACHE_ENTRIES {
            let old = self.entries.capacity() * core::mem::size_of::<Entry>();
            self.entries
                .try_reserve_exact(MAX_CACHE_ENTRIES - self.entries.len())
                .map_err(|_| storage())?;
            self.observe_peak(old);
        }
        if self.slots.capacity() < MAX_CACHE_ENTRIES {
            let old = self.index_bytes();
            self.slots
                .try_reserve(MAX_CACHE_ENTRIES - self.slots.len())
                .map_err(|_| storage())?;
            self.observe_peak(old);
        }
        self.observe_peak(0);
        Ok(())
    }

    fn index_bytes(&self) -> usize {
        self.slots.capacity() * (core::mem::size_of::<(u64, u16)>() + 1)
    }

    fn observe_peak(&mut self, extra: usize) {
        self.peak_bytes = self
            .peak_bytes
            .max(self.retained_bytes().saturating_add(extra));
    }
}

struct WorkBudget {
    remaining: usize,
    used: usize,
}

impl WorkBudget {
    fn new(remaining: usize) -> Self {
        Self { remaining, used: 0 }
    }

    fn charge(
        &mut self,
        amount: usize,
        metrics: &mut PrefixMaskMetrics,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let Some(total) = metrics.work.checked_add(amount) else {
            return Err(invalid("wasm_observation_physical_work_limit"));
        };
        if amount > self.remaining {
            return Err(invalid("wasm_observation_physical_work_limit"));
        }
        self.remaining -= amount;
        self.used += amount;
        metrics.work = total;
        Ok(())
    }
}

fn word_count(operations: u8) -> usize {
    (1_usize << operations).div_ceil(64)
}

fn sequence_key(pieces: &[PieceKind]) -> u64 {
    let mut key = (pieces.len() as u64) << (3 * MAX_MASK_OPERATIONS);
    for (operation, &piece) in pieces.iter().enumerate() {
        key |= u64::from(piece_code(piece)) << (3 * operation);
    }
    key
}

fn invalid(reason: &'static str) -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem(reason)
}

fn storage() -> WasmExactSearchError {
    invalid("wasm_observation_prefix_mask_storage_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(pieces: &[PieceKind], prefix: usize, horizon: u8, limits: [u8; 7]) -> (bool, bool) {
        let depth_allowed = prefix.count_ones() <= u32::from(horizon);
        let mut counts = [0_u8; 7];
        for (operation, &piece) in pieces.iter().enumerate() {
            if prefix & (1 << operation) != 0 {
                counts[usize::from(piece_code(piece) - 1)] += 1;
            }
        }
        let cap_allowed = counts
            .into_iter()
            .zip(limits)
            .all(|(count, cap)| count <= cap);
        (depth_allowed && cap_allowed, depth_allowed && !cap_allowed)
    }

    #[test]
    fn low_lane_table_is_exact_for_every_group_and_threshold() {
        for group in 0..64_usize {
            for limit in 0..=6_usize {
                for lane in 0..64_usize {
                    assert_eq!(
                        LANES_WITH_AT_MOST[group][limit] & (1_u64 << lane) != 0,
                        (lane & group).count_ones() as usize <= limit,
                    );
                }
            }
        }
    }

    #[test]
    fn prefix_word_masks_match_scalar_through_eleven_operations_and_all_horizons() {
        let control = ExecutionControl::default();
        for n in 1..=MAX_MASK_OPERATIONS {
            for layout in 0..4_usize {
                let pieces: Vec<_> = (0..usize::from(n))
                    .map(|operation| match layout {
                        0 => PieceKind::I,
                        1 => PieceKind::STANDARD_TETROMINOES[operation % 7],
                        2 => PieceKind::STANDARD_TETROMINOES[(operation / 2) % 7],
                        _ => PieceKind::STANDARD_TETROMINOES[(6 + 7 - operation % 7) % 7],
                    })
                    .collect();
                let mut held_limits = [1; 7];
                held_limits[0] = 2;
                for limits in [[1; 7], held_limits] {
                    for horizon in 0..=n {
                        let mut cache = PrefixMaskCache::new(n, horizon, limits).unwrap();
                        let query = cache.query(&pieces, usize::MAX, &control).unwrap();
                        let plan = query.plan.unwrap();
                        for prefix in 0..(1_usize << n) {
                            let word = cache.word(plan, prefix / 64).unwrap();
                            let bit = 1_u64 << (prefix % 64);
                            let expected = scalar(&pieces, prefix, horizon, limits);
                            assert_eq!(word.valid & bit != 0, expected.0);
                            assert_eq!(word.cap_invalid & bit != 0, expected.1);
                            if !plan.needs_filter(prefix / 64, bit) {
                                assert!(!expected.1);
                            }
                        }
                        assert!(cache.peak_bytes() >= cache.retained_bytes());
                    }
                }
            }
        }
    }

    #[test]
    fn equal_multisets_with_different_operation_order_have_distinct_plans() {
        use PieceKind::{I, O};
        let control = ExecutionControl::default();
        let mut cache = PrefixMaskCache::new(3, 2, [1; 7]).unwrap();
        let first = cache.query(&[I, I, O], usize::MAX, &control).unwrap();
        let second = cache.query(&[I, O, I], usize::MAX, &control).unwrap();
        assert_ne!(first.plan, second.plan);
        assert_eq!(
            cache.word(first.plan.unwrap(), 0).unwrap().valid & (1 << 3),
            0
        );
        assert_ne!(
            cache.word(second.plan.unwrap(), 0).unwrap().valid & (1 << 3),
            0
        );
        let retained = cache.retained_bytes();
        let before = cache.metrics();
        let repeated = cache.query(&[I, I, O], 1, &control).unwrap();
        assert_eq!(repeated.plan, first.plan);
        assert!(repeated.cache_hit);
        assert_eq!(repeated.work, 1);
        assert_eq!(cache.metrics().work, before.work + 1);
        assert_eq!(cache.metrics().storage_words, before.storage_words);
        assert_eq!(cache.retained_bytes(), retained);
    }

    #[test]
    fn cap_vacuous_words_keep_scalar_loop_and_masked_words_report_only_cap_rejections() {
        let control = ExecutionControl::default();
        let pieces = vec![PieceKind::I; 11];
        let mut cache = PrefixMaskCache::new(11, 2, [2; 7]).unwrap();
        let plan = cache
            .query(&pieces, usize::MAX, &control)
            .unwrap()
            .plan
            .unwrap();
        assert!((0..word_count(11)).all(|word| !plan.needs_filter(word, u64::MAX)));

        let mut cache = PrefixMaskCache::new(11, 3, [2; 7]).unwrap();
        let plan = cache
            .query(&pieces, usize::MAX, &control)
            .unwrap()
            .plan
            .unwrap();
        for word_index in 0..word_count(11) {
            let word = cache.word(plan, word_index).unwrap();
            let (retained, rejected) = word.filter(u64::MAX);
            let expected_rejected = (0..64)
                .filter(|lane| scalar(&pieces, word_index * 64 + lane, 3, [2; 7]).1)
                .count();
            assert_eq!(retained, word.valid);
            assert_eq!(rejected, expected_rejected);
            assert_eq!(plan.needs_filter(word_index, u64::MAX), rejected != 0);
            assert!(!plan.needs_filter(word_index, 0));
        }
    }

    #[test]
    fn cache_capacity_falls_back_without_rebuilding_and_keeps_old_tickets() {
        let control = ExecutionControl::default();
        let sequence = |mut value: usize| {
            std::array::from_fn::<_, 4, _>(|_| {
                let piece = PieceKind::STANDARD_TETROMINOES[value % 7];
                value /= 7;
                piece
            })
        };
        let mut cache = PrefixMaskCache::new(4, 3, [1; 7]).unwrap();
        let first = cache
            .query(&sequence(0), usize::MAX, &control)
            .unwrap()
            .plan
            .unwrap();
        for key in 1..MAX_CACHE_ENTRIES {
            assert!(cache
                .query(&sequence(key), usize::MAX, &control)
                .unwrap()
                .plan
                .is_some());
        }
        assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);
        let retained = cache.retained_bytes();
        let before = cache.metrics();
        let fallback = cache
            .query(&sequence(MAX_CACHE_ENTRIES), 1, &control)
            .unwrap();
        assert!(fallback.plan.is_none());
        assert_eq!(fallback.work, 1);
        assert_eq!(cache.metrics().build_words, before.build_words);
        assert_eq!(cache.metrics().work, before.work + 1);
        assert_eq!(cache.retained_bytes(), retained);
        assert_eq!(
            cache.query(&sequence(0), 1, &control).unwrap().plan,
            Some(first)
        );
        assert!(cache.word(first, 0).is_ok());
    }

    #[test]
    fn work_budget_and_cancellation_stop_before_unbudgeted_construction() {
        let control = ExecutionControl::default();
        let pieces = [PieceKind::I; 11];
        let mut cache = PrefixMaskCache::new(11, 5, [1; 7]).unwrap();
        assert!(matches!(
            cache.query(&pieces, 0, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(cache.metrics().work, 0);
        assert_eq!(cache.retained_bytes(), 0);
        assert!(matches!(
            cache.query(&pieces, 1, &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(cache.metrics().work, 1);
        assert!(cache.entries.is_empty());
        assert_eq!(cache.retained_bytes(), 0);

        let before = cache.metrics().work;
        assert!(cache.query(&pieces, 40, &control).is_err());
        assert!(cache.metrics().work - before <= 40);
        assert!(cache.entries.is_empty());
        let completed = cache.query(&pieces, usize::MAX, &control).unwrap();
        assert!(completed.plan.is_some());
        let before = cache.metrics();
        control.cancellation.handle().cancel();
        assert!(matches!(
            cache.query(&pieces, 1, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert_eq!(cache.metrics(), before);
    }

    #[test]
    fn horizon_limits_and_ticket_scope_cannot_cross_owners() {
        let control = ExecutionControl::default();
        let pieces = [PieceKind::I, PieceKind::O, PieceKind::I];
        let mut first = PrefixMaskCache::new(3, 2, [1; 7]).unwrap();
        let plan = first
            .query(&pieces, usize::MAX, &control)
            .unwrap()
            .plan
            .unwrap();
        for (horizon, limits) in [(3, [1; 7]), (2, [2; 7])] {
            let mut other = PrefixMaskCache::new(3, horizon, limits).unwrap();
            other.query(&pieces, usize::MAX, &control).unwrap();
            assert!(other.word(plan, 0).is_err());
        }
        assert!(first.word(plan, word_count(3)).is_err());
        assert!(first.query(&pieces[..2], usize::MAX, &control).is_err());
        assert!(PrefixMaskCache::new(0, 0, [1; 7]).is_err());
        assert!(PrefixMaskCache::new(12, 0, [1; 7]).is_err());
        assert!(PrefixMaskCache::new(3, 4, [1; 7]).is_err());
    }
}
