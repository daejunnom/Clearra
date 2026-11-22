use clearra_problem::SearchProblem;
use clearra_supply::hold::hold_policy::HoldPolicy;

use super::{piece_index, GeometryCatalog, TargetGroup, TwoBagPlusOneInventory};
use crate::performance::{profile_sample_scale, ExecutorSearchStage, SearchStageSpan};

#[cfg(test)]
#[path = "geometry_source_prefix_cache_diagnostic_tests.rs"]
mod cache_diagnostic_tests;

#[cfg(test)]
#[path = "geometry_source_capacity_tests.rs"]
mod capacity_tests;

const FULL_DRAW_COUNT: usize = 15;
const PREFIX_LOCK_COUNT: u8 = 7;
// Optional pruning must not add an unbounded target loop to every prefix.
// Larger domains retain the original physical producer and exact verifier.
const MAX_PREFIX_TARGETS: usize = 8;

/// A request-local permission, passed only through this search's owned geometry
/// and its frozen partitions. It carries no positive success authority. The
/// caller separately admits the physical-PC result/output contract.
#[derive(Clone, Copy, Debug)]
pub(super) struct FullDrawPrefixRequest {
    prefix_max: [u8; 7],
    suffix_min: [u8; 7],
    two_bag_plus_one: bool,
}

impl FullDrawPrefixRequest {
    pub(super) fn compile(problem: &SearchProblem) -> Option<Self> {
        let initial = problem.initial_hold();
        let hold_enabled = problem.supply().hold_enabled();
        if cfg!(target_family = "wasm")
            || usize::BITS < 64
            || problem.exact_pieces() != Some(FULL_DRAW_COUNT)
            || problem.supply().source_sequence_length() != FULL_DRAW_COUNT
            || problem.supply().projects_standard_bag_lookahead()
            || initial.cursor() != 0
            || !initial.hold_empty()
            || initial.hold_piece().is_some()
            || initial.hold_policy()
                != if hold_enabled {
                    HoldPolicy::Allowed
                } else {
                    HoldPolicy::Forbidden
                }
            || initial.bag_epoch() != 0
            || initial.bag_remainder_key() != 0
            || initial.source_kind() != problem.piece_source().kind()
            || initial.piece_source_id() != problem.piece_source().id()
        {
            return None;
        }
        let universe = problem.piece_source().materialized_universe()?;
        if !universe.complete()
            || universe.truncation_reason().is_some()
            || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        {
            return None;
        }

        // Empty Hold consumes at most n + 1 original draws for n locks. With
        // Hold disabled it consumes exactly n. Every original piece is placed
        // in this finite full-draw request, so target inventory also bounds
        // the prefix by subtracting the suffix's necessary inventory.
        let prefix_draws = usize::from(PREFIX_LOCK_COUNT) + usize::from(hold_enabled);
        let mut prefix_max = [0_u8; 7];
        let mut suffix_min = [0_u8; 7];
        if let Some(layout) = universe.complete_factorized_layout() {
            if layout.full_sequence_len() != FULL_DRAW_COUNT
                || layout.visible_sequence_len() != FULL_DRAW_COUNT
            {
                return None;
            }
            for segment in layout.segments() {
                let before_cut = prefix_draws
                    .saturating_sub(segment.draw_start())
                    .min(segment.draw_count());
                let after_cut = segment.draw_count() - before_cut;
                for &piece in segment.choices() {
                    let kind = piece_index(piece);
                    prefix_max[kind] += u8::from(before_cut != 0);
                    // A kind is unavoidable only when every choice remains
                    // to be drawn after this cut. Partial atoms may omit it.
                    suffix_min[kind] += u8::from(after_cut == segment.choices().len());
                }
            }
        } else if universe.pattern_count() == 1 && universe.sequence_len_at(0) == FULL_DRAW_COUNT {
            // Read at most one short actual queue. Never expand an arbitrary
            // explicit universe just to prepare this optional negative cut.
            let sequence = universe.sequence_at(0);
            for (position, &piece) in sequence.iter().enumerate() {
                let counts = if position < prefix_draws {
                    &mut prefix_max
                } else {
                    &mut suffix_min
                };
                counts[piece_index(piece)] += 1;
            }
        } else {
            return None;
        }
        Some(Self {
            prefix_max,
            suffix_min,
            two_bag_plus_one: false,
        })
    }

    pub(super) fn for_targets(mut self, targets: &[TargetGroup]) -> Option<Self> {
        if targets.is_empty()
            || targets.len() > MAX_PREFIX_TARGETS
            || targets.iter().any(|target| {
                target
                    .key
                    .counts()
                    .into_iter()
                    .map(usize::from)
                    .sum::<usize>()
                    != FULL_DRAW_COUNT
            })
        {
            return None;
        }
        // Authenticate the existing O(7) specialization once, from actual
        // caps and the complete target domain. No source text or label is
        // authority, and no seven-target loop is added to the hot path.
        self.two_bag_plus_one = self.prefix_max == [2; 7]
            && self.suffix_min == [1; 7]
            && TwoBagPlusOneInventory::matches_targets(targets);
        Some(self)
    }

    fn target_caps(self, target: [u8; 7]) -> Option<[u8; 7]> {
        let mut caps = [0_u8; 7];
        for kind in 0..7 {
            caps[kind] =
                self.prefix_max[kind].min(target[kind].checked_sub(self.suffix_min[kind])?);
        }
        Some(caps)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SourcePrefixDecision {
    pub(super) proved_infeasible: bool,
    pub(super) visited_row_states: u8,
    pub(super) root_fast_pass: bool,
}

#[derive(Default)]
struct SourcePrefixEvaluation {
    proved_infeasible: bool,
    visited_row_states: u64,
    root_fast_pass: bool,
}

#[cfg(test)]
pub(super) fn selected_source_prefix_stalls(catalog: &GeometryCatalog, rows: &[u32]) -> bool {
    selected_source_prefix_decision(catalog, rows).proved_infeasible
}

fn selected_source_prefix_decision(
    catalog: &GeometryCatalog,
    rows: &[u32],
) -> SourcePrefixDecision {
    if catalog.width() != 10
        || catalog.height() != 6
        || catalog.initial_board() != 0
        || catalog.required_cells() != (1_u64 << 60) - 1
        || rows.len() > 15
    {
        return SourcePrefixDecision::default();
    }
    let mut kind_operations = [0_u16; 7];
    let mut row_contributors = [0_u16; 6];
    for (operation, &row_id) in rows.iter().enumerate() {
        let Some(skeleton) = catalog.try_skeleton(row_id) else {
            return SourcePrefixDecision::default();
        };
        let bit = 1_u16 << operation;
        kind_operations[piece_index(skeleton.piece)] |= bit;
        let mut occupied = catalog.skeleton_occupied_rows(row_id);
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            row_contributors[row] |= bit;
        }
    }
    selected_source_prefix_relaxation(
        7,
        kind_operations,
        &row_contributors,
        |operation, deleted| catalog.realization_requirement_is_satisfied(rows[operation], deleted),
    )
}

fn selected_capacity_source_prefix_decision(
    request: FullDrawPrefixRequest,
    targets: &[TargetGroup],
    catalog: &GeometryCatalog,
    rows: &[u32],
) -> SourcePrefixEvaluation {
    if catalog.width() != 10
        || catalog.height() != 6
        || catalog.initial_board() != 0
        || catalog.required_cells() != (1_u64 << 60) - 1
        || rows.len() > FULL_DRAW_COUNT
        || targets.is_empty()
        || targets.len() > MAX_PREFIX_TARGETS
    {
        return SourcePrefixEvaluation::default();
    }
    let mut kind_operations = [0_u16; 7];
    let mut row_contributors = [0_u16; 6];
    for (operation, &row_id) in rows.iter().enumerate() {
        let Some(skeleton) = catalog.try_skeleton(row_id) else {
            return SourcePrefixEvaluation::default();
        };
        let bit = 1_u16 << operation;
        kind_operations[piece_index(skeleton.piece)] |= bit;
        let mut occupied = catalog.skeleton_occupied_rows(row_id);
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            row_contributors[row] |= bit;
        }
    }

    let mut visited_row_states = 0_u64;
    for target in targets {
        let counts = target.key.counts();
        if kind_operations
            .iter()
            .zip(counts)
            .any(|(&operations, count)| operations.count_ones() > u32::from(count))
        {
            continue;
        }
        let Some(caps) = request.target_caps(counts) else {
            // The complete target cannot supply the suffix's necessary kind.
            continue;
        };
        let decision = selected_capacity_prefix_relaxation(
            PREFIX_LOCK_COUNT,
            counts,
            caps,
            kind_operations,
            &row_contributors,
            |operation, deleted| {
                catalog.realization_requirement_is_satisfied(rows[operation], deleted)
            },
        );
        let root_fast_pass = visited_row_states == 0 && decision.root_fast_pass;
        visited_row_states += u64::from(decision.visited_row_states);
        if !decision.proved_infeasible {
            return SourcePrefixEvaluation {
                proved_infeasible: false,
                visited_row_states,
                root_fast_pass,
            };
        }
    }
    SourcePrefixEvaluation {
        proved_infeasible: true,
        visited_row_states,
        root_fast_pass: false,
    }
}

/// Negative-only relaxation for one actual target inventory T and prefix cap C.
/// At row deletion state D, unavailable selected operations have counts U(D).
/// Every future operation is optimistically ready, so at most T-U operations
/// can be used. Fewer than n capped available operations cannot supply n locks.
///
/// Row transitions keep only the selected mandatory contributors, relaxing
/// all future contributors away. Each actual prefix is included: positive
/// realization requirements are monotone and a simultaneous clear may be
/// decomposed into single-row additions. A surviving state is only Unknown;
/// exact physical reachability and the original queue/Hold product still run.
fn selected_capacity_prefix_relaxation(
    prefix_locks: u8,
    target: [u8; 7],
    caps: [u8; 7],
    kind_operations: [u16; 7],
    row_contributors: &[u16],
    mut requirement_is_satisfied: impl FnMut(usize, u16) -> bool,
) -> SourcePrefixDecision {
    let total_count = target.into_iter().map(u32::from).sum::<u32>();
    if prefix_locks == 0
        || total_count > FULL_DRAW_COUNT as u32
        || u32::from(prefix_locks) > total_count
        || row_contributors.len() > 6
        || caps.iter().zip(target).any(|(&cap, count)| cap > count)
    {
        return SourcePrefixDecision::default();
    }
    let mut all_operations = 0_u16;
    for &operations in &kind_operations {
        if operations & all_operations != 0 {
            return SourcePrefixDecision::default();
        }
        all_operations |= operations;
    }
    let operation_count = all_operations.count_ones();
    if operation_count > FULL_DRAW_COUNT as u32
        || all_operations != ((1_u32 << operation_count) - 1) as u16
        || row_contributors
            .iter()
            .any(|contributors| contributors & !all_operations != 0)
    {
        return SourcePrefixDecision::default();
    }
    if kind_operations
        .iter()
        .zip(target)
        .any(|(&operations, count)| operations.count_ones() > u32::from(count))
    {
        return SourcePrefixDecision {
            proved_infeasible: true,
            ..SourcePrefixDecision::default()
        };
    }

    let mut reachable = 1_u64;
    let mut visited_row_states = 0_u8;
    for deleted in 0..1_usize << row_contributors.len() {
        if reachable & (1_u64 << deleted) == 0 {
            continue;
        }
        visited_row_states += 1;
        let mut available = 0_u16;
        for operation in 0..operation_count as usize {
            if requirement_is_satisfied(operation, deleted as u16) {
                available |= 1 << operation;
            }
        }
        let unavailable = all_operations & !available;
        let capped_available = kind_operations
            .iter()
            .zip(target)
            .zip(caps)
            .map(|((&operations, count), cap)| {
                (u32::from(count) - (operations & unavailable).count_ones()).min(u32::from(cap))
            })
            .sum::<u32>();
        if capped_available >= u32::from(prefix_locks) {
            return SourcePrefixDecision {
                proved_infeasible: false,
                visited_row_states,
                root_fast_pass: deleted == 0,
            };
        }

        let mut mandatory = 0_u16;
        for (row, &contributors) in row_contributors.iter().enumerate() {
            if deleted & (1 << row) != 0 {
                mandatory |= contributors;
            }
        }
        for (row, &contributors) in row_contributors.iter().enumerate() {
            if deleted & (1 << row) != 0 || contributors & !available != 0 {
                continue;
            }
            let next_mandatory = mandatory | contributors;
            if next_mandatory.count_ones() > u32::from(prefix_locks)
                || kind_operations.iter().zip(caps).any(|(&operations, cap)| {
                    (operations & next_mandatory).count_ones() > u32::from(cap)
                })
            {
                continue;
            }
            reachable |= 1_u64 << (deleted | (1 << row));
        }
    }
    SourcePrefixDecision {
        proved_infeasible: true,
        visited_row_states,
        root_fast_pass: false,
    }
}

/// Relax every not-yet-selected operation to be immediately available and to
/// contribute no cost to deleting a row. For selected S, u_k is its kind count,
/// and a_k(D) counts its operations with SOME positive realization ready at D.
/// For a feasible extra kind p, unknown future inventory is R=2+e_p-u, while
/// the first n locks have cap B=1+e_p. Therefore
///
///   sum min(a+R,B) = n+1 - sum max(u-a-1,0).
///
/// The availability deficit is independent of p. A deficit at most one is an
/// Unknown/pass, never buildability. Otherwise retain each p under which row
/// contributors fit B and at most n locks. Every actual completion's first n
/// locks is included in this relaxation. Exhausting it proves ALL completions
/// of this selected prefix impossible, without consulting a residual memo.
pub(super) fn selected_source_prefix_relaxation(
    bag_size: u8,
    kind_operations: [u16; 7],
    row_contributors: &[u16],
    mut requirement_is_satisfied: impl FnMut(usize, u16) -> bool,
) -> SourcePrefixDecision {
    if !(1..=7).contains(&bag_size) || row_contributors.len() > 6 {
        return SourcePrefixDecision::default();
    }
    let mut all_operations = 0_u16;
    let mut eligible_extra = (1_u8 << bag_size) - 1;
    for (kind, &operations) in kind_operations.iter().enumerate() {
        if operations & all_operations != 0 || (kind >= usize::from(bag_size) && operations != 0) {
            return SourcePrefixDecision::default();
        }
        all_operations |= operations;
        match operations.count_ones() {
            0..=2 => {}
            3 => eligible_extra &= 1 << kind,
            _ => eligible_extra = 0,
        }
    }
    let operation_count = all_operations.count_ones();
    if operation_count > u32::from(bag_size) * 2 + 1
        || all_operations != ((1_u32 << operation_count) - 1) as u16
        || row_contributors
            .iter()
            .any(|contributors| contributors & !all_operations != 0)
    {
        return SourcePrefixDecision::default();
    }
    if eligible_extra == 0 {
        return SourcePrefixDecision {
            proved_infeasible: true,
            ..SourcePrefixDecision::default()
        };
    }

    // Numeric D order is topological: adding a row always increases D. Keep
    // the full 64-state relaxation; the complete-tiling 22-state bound does
    // not apply after future row contributors have been removed for free.
    let mut reachable_extra = [0_u8; 64];
    reachable_extra[0] = eligible_extra;
    let mut visited_row_states = 0_u8;
    let state_count = 1_usize << row_contributors.len();
    for deleted in 0..state_count {
        let extra = reachable_extra[deleted];
        if extra == 0 {
            continue;
        }
        visited_row_states += 1;
        let mut available = 0_u16;
        for operation in 0..operation_count as usize {
            if requirement_is_satisfied(operation, deleted as u16) {
                available |= 1 << operation;
            }
        }
        let unavailable = all_operations & !available;
        let unavailable_kinds = kind_operations
            .iter()
            .filter(|&&kind| kind & unavailable != 0)
            .count() as u32;
        if unavailable.count_ones() - unavailable_kinds <= 1 {
            return SourcePrefixDecision {
                root_fast_pass: deleted == 0,
                visited_row_states,
                proved_infeasible: false,
            };
        }

        // OR, not row-wise addition: one operation may span several rows.
        let mut mandatory = 0_u16;
        for (row, &contributors) in row_contributors.iter().enumerate() {
            if deleted & (1 << row) != 0 {
                mandatory |= contributors;
            }
        }
        for (row, &contributors) in row_contributors.iter().enumerate() {
            if deleted & (1 << row) != 0 || contributors & !available != 0 {
                continue;
            }
            let next_mandatory = mandatory | contributors;
            if next_mandatory.count_ones() > u32::from(bag_size) {
                continue;
            }
            let mut next_extra = extra;
            for (kind, &operations) in kind_operations.iter().enumerate() {
                match (operations & next_mandatory).count_ones() {
                    0..=1 => {}
                    2 => next_extra &= 1 << kind,
                    _ => {
                        next_extra = 0;
                        break;
                    }
                }
            }
            reachable_extra[deleted | (1 << row)] |= next_extra;
        }
    }
    SourcePrefixDecision {
        proved_infeasible: true,
        visited_row_states,
        root_fast_pass: false,
    }
}

#[derive(Debug, Default)]
pub(super) struct SourcePrefixProfile {
    pub(super) checks: u64,
    pub(super) pruned_subtrees: u64,
    pub(super) visited_row_states: u64,
    pub(super) root_fast_passes: u64,
    // The ignored diagnostic records decisions without changing traversal or
    // skipping a DP. This field and its observer call do not exist in products.
    #[cfg(test)]
    trace_observer: Option<cache_diagnostic_tests::PrefixTraceObserver>,
}

impl SourcePrefixProfile {
    pub(super) fn stalls(
        &mut self,
        request: FullDrawPrefixRequest,
        targets: &[TargetGroup],
        catalog: &GeometryCatalog,
        rows: &[u32],
    ) -> bool {
        self.checks = self.checks.saturating_add(1);
        let stage = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometrySourcePrefix,
            profile_sample_scale(self.checks as usize),
        );
        let evaluation = if request.two_bag_plus_one {
            let decision = selected_source_prefix_decision(catalog, rows);
            #[cfg(test)]
            if let Some(observer) = self.trace_observer.as_mut() {
                // This diagnostic's historical key is valid only for the
                // authenticated two-bag-plus-one target/cap specialization.
                observer.observe(catalog, rows, decision);
            }
            SourcePrefixEvaluation {
                proved_infeasible: decision.proved_infeasible,
                visited_row_states: u64::from(decision.visited_row_states),
                root_fast_pass: decision.root_fast_pass,
            }
        } else {
            selected_capacity_source_prefix_decision(request, targets, catalog, rows)
        };
        stage.finish(0);
        self.visited_row_states = self
            .visited_row_states
            .saturating_add(evaluation.visited_row_states);
        self.root_fast_passes = self
            .root_fast_passes
            .saturating_add(u64::from(evaluation.root_fast_pass));
        self.pruned_subtrees = self
            .pruned_subtrees
            .saturating_add(u64::from(evaluation.proved_infeasible));
        evaluation.proved_infeasible
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl Drop for SourcePrefixProfile {
    fn drop(&mut self) {
        for (stage, count) in [
            (ExecutorSearchStage::WasmGeometrySourcePrefix, self.checks),
            (
                ExecutorSearchStage::WasmGeometrySourcePrefixPrunedSubtrees,
                self.pruned_subtrees,
            ),
            (
                ExecutorSearchStage::WasmGeometrySourcePrefixRowStates,
                self.visited_row_states,
            ),
            (
                ExecutorSearchStage::WasmGeometrySourcePrefixRootFastPasses,
                self.root_fast_passes,
            ),
        ] {
            SearchStageSpan::record_exact_work(stage, count);
        }
    }
}
