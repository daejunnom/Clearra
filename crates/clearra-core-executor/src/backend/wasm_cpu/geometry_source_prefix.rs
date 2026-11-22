use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::SearchProblem;
use clearra_supply::{hold::hold_policy::HoldPolicy, piece_source::PieceSourceKind};

use super::{piece_index, GeometryCatalog, TargetGroup, TwoBagPlusOneInventory};
use crate::performance::{profile_sample_scale, ExecutorSearchStage, SearchStageSpan};

#[cfg(test)]
#[path = "geometry_source_prefix_cache_diagnostic_tests.rs"]
mod cache_diagnostic_tests;

/// A request-local permission, passed only through this search's owned geometry
/// and its frozen partitions. It carries no positive success authority. The
/// caller separately admits the physical-PC result/output contract.
#[derive(Clone, Copy, Debug)]
pub(super) struct FullDrawPrefixRequest(());

impl FullDrawPrefixRequest {
    pub(super) fn compile(problem: &SearchProblem) -> Option<Self> {
        let initial = problem.initial_hold();
        if cfg!(target_family = "wasm")
            || usize::BITS < 64
            || problem.exact_pieces() != Some(15)
            || !problem.supply().hold_enabled()
            || problem.supply().source_sequence_length() != 15
            || problem.supply().projects_standard_bag_lookahead()
            || initial.cursor() != 0
            || !initial.hold_empty()
            || initial.hold_piece().is_some()
            || initial.hold_policy() != HoldPolicy::Allowed
            || initial.bag_epoch() != 0
            || initial.bag_remainder_key() != 0
            || initial.source_kind() != PieceSourceKind::MaterializedPatternUniverse
            || initial.piece_source_id() != problem.piece_source().id()
        {
            return None;
        }
        let layout = problem
            .piece_source()
            .materialized_universe()?
            .complete_factorized_layout()?;
        (layout.full_sequence_len() == 15
            && layout.visible_sequence_len() == 15
            && layout.segment_count() == 3
            && layout.segments().zip([1, 7, 7]).all(|(segment, draws)| {
                segment.draw_count() == draws
                    && segment.choices() == PieceKind::STANDARD_TETROMINOES
            }))
        .then_some(Self(()))
    }

    pub(super) fn supports_targets(self, targets: &[TargetGroup]) -> bool {
        TwoBagPlusOneInventory::matches_targets(targets)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SourcePrefixDecision {
    pub(super) proved_infeasible: bool,
    pub(super) visited_row_states: u8,
    pub(super) root_fast_pass: bool,
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
    pub(super) fn stalls(&mut self, catalog: &GeometryCatalog, rows: &[u32]) -> bool {
        self.checks = self.checks.saturating_add(1);
        let stage = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometrySourcePrefix,
            profile_sample_scale(self.checks as usize),
        );
        let decision = selected_source_prefix_decision(catalog, rows);
        stage.finish(0);
        #[cfg(test)]
        if let Some(observer) = self.trace_observer.as_mut() {
            observer.observe(catalog, rows, decision);
        }
        self.visited_row_states = self
            .visited_row_states
            .saturating_add(u64::from(decision.visited_row_states));
        self.root_fast_passes = self
            .root_fast_passes
            .saturating_add(u64::from(decision.root_fast_pass));
        self.pruned_subtrees = self
            .pruned_subtrees
            .saturating_add(u64::from(decision.proved_infeasible));
        decision.proved_infeasible
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
