use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{
    goal::SearchGoal, PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem, SearchProblemKind,
    SearchProblemPreset,
};
use clearra_supply::{
    hold::hold_policy::HoldPolicy, piece_source::PieceSourceKind, QueueObservationPolicy,
};

use super::{catalog::GeometryCatalog, geometry::GeometryCandidate};

/// Negative-only prefix bound for one candidate and its actual finite source.
///
/// In P1P7P7 with empty initial Hold and all 15 original pieces placed, the
/// candidate inventory [2; 7] + e_p identifies the independent first draw p.
/// Its first seven locks consume at most the first eight original draws, whose
/// inventory is [1; 7] + e_p. A prefix exceeding that inventory cannot occur
/// under any original queue or Hold choices. The exact source product still
/// decides every surviving physical path, its original ordinals and witness.
///
/// This value is compiled afresh beside the candidate's graph from the actual
/// complete source backing. It is never cached by universe ID or inventory,
/// and cannot be transferred to a different source, candidate or operation
/// order. The masks use this candidate's operation indices, not catalog IDs.
#[derive(Debug)]
pub(super) struct FullDrawSourcePrefix {
    kind_operations: [u16; 7],
    extra_kind: u8,
    first_bag_len: u8,
    #[cfg(feature = "placement-dependency-pc")]
    source_identity: Option<super::standard_bag_coverage::StandardBagRequestIdentity>,
}

/// Cumulative work observations for this workspace, not completion authority.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SourcePrefixTemporalMetrics {
    pub checks: usize,
    pub root_fast_passes: usize,
    pub visited_row_states: usize,
    pub negative_candidates: usize,
}

impl SourcePrefixTemporalMetrics {
    pub(super) fn record(&mut self, result: TemporalPrefixResult) {
        self.checks = self.checks.saturating_add(1);
        self.root_fast_passes = self
            .root_fast_passes
            .saturating_add(usize::from(result.root_fast_pass));
        self.visited_row_states = self
            .visited_row_states
            .saturating_add(usize::from(result.visited_row_states));
        self.negative_candidates = self
            .negative_candidates
            .saturating_add(usize::from(result.proved_infeasible));
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TemporalPrefixResult {
    pub proved_infeasible: bool,
    pub visited_row_states: u8,
    pub root_fast_pass: bool,
}

impl FullDrawSourcePrefix {
    pub(super) fn compile(
        problem: &SearchProblem,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
    ) -> Option<Self> {
        #[cfg(test)]
        if TEST_DISABLED.get() {
            return None;
        }
        let initial = problem.initial_hold();
        if cfg!(target_family = "wasm")
            || usize::BITS < 64
            || !matches!(
                (problem.preset(), problem.problem_kind()),
                (SearchProblemPreset::OpeningPc, SearchProblemKind::OpeningPc)
                    | (
                        SearchProblemPreset::ScenarioPc,
                        SearchProblemKind::ScenarioPc
                    )
            )
            || !matches!(problem.search_goal(), SearchGoal::ClearToEmpty)
            || catalog.width() != 10
            || catalog.height() != 6
            || catalog.initial_board() != 0
            || catalog.required_cells() != (1_u64 << 60) - 1
            || problem.exact_pieces() != Some(15)
            || candidate.row_ids().len() != 15
            || problem.objective() != ObjectivePolicy::unique()
            || problem.count_policy() != PcCountPolicy::CountUnique
            || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
            || problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::Disabled
            || problem.output_policy() != SearchOutputPolicy::Trace
            || problem.solution_probability_policy().requested()
            || problem.build_replay_evidence_requested()
            || problem.allowed_colored_solution_identities().is_some()
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
        let universe = problem.piece_source().materialized_universe()?;
        let layout = universe.complete_factorized_layout()?;
        if layout.full_sequence_len() != 15
            || layout.visible_sequence_len() != 15
            || layout.segment_count() != 3
            || !layout.segments().zip([1, 7, 7]).all(|(segment, draws)| {
                segment.draw_count() == draws
                    && segment.choices() == PieceKind::STANDARD_TETROMINOES
            })
        {
            return None;
        }

        // Finite factorized EOF may release the retained source piece once.
        // projects_unplaced_lookahead therefore stays supported: it does not
        // admit an unseen replacement piece into the 15-lock inventory.
        let mut kind_operations = [0_u16; 7];
        for (operation, &row) in candidate.row_ids().iter().enumerate() {
            let kind = piece_index(catalog.skeleton(row).piece);
            kind_operations[kind] |= 1_u16 << operation;
        }
        let prefix = Self::from_kind_operations(kind_operations, 7)?;
        #[cfg(feature = "placement-dependency-pc")]
        let prefix = {
            let mut prefix = prefix;
            prefix.source_identity = Some(
                super::standard_bag_coverage::StandardBagRequestIdentity::for_universe(
                    universe,
                    problem.initial_hold(),
                    problem.supply().hold_enabled(),
                    problem.supply().projects_unplaced_lookahead(),
                ),
            );
            prefix
        };
        Some(prefix)
    }

    // The smaller alphabets are exercised against the canonical finite Hold
    // automaton in tests. Product admission above only uses the actual seven
    // standard kinds and complete P1P7P7 source.
    fn from_kind_operations(kind_operations: [u16; 7], bag_size: u8) -> Option<Self> {
        if !(1..=7).contains(&bag_size) {
            return None;
        }
        let operation_count = u32::from(bag_size) * 2 + 1;
        let mut occupied = 0_u16;
        let mut extra_kind = None;
        for (kind, &operations) in kind_operations.iter().enumerate() {
            if occupied & operations != 0 {
                return None;
            }
            occupied |= operations;
            match (kind < usize::from(bag_size), operations.count_ones()) {
                (true, 2) | (false, 0) => {}
                (true, 3) if extra_kind.is_none() => extra_kind = Some(kind as u8),
                _ => return None,
            }
        }
        if occupied != (1_u16 << operation_count) - 1 {
            return None;
        }
        Some(Self {
            kind_operations,
            extra_kind: extra_kind?,
            first_bag_len: bag_size,
            #[cfg(feature = "placement-dependency-pc")]
            source_identity: None,
        })
    }

    /// Available only after actual request/candidate compilation, never from
    /// the smaller-alphabet inventory helper used by abstract tests.
    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn full_draw_source_identity(
        &self,
    ) -> Option<(super::standard_bag_coverage::StandardBagRequestIdentity, u8)> {
        self.source_identity
            .map(|identity| (identity, self.extra_kind))
    }

    /// A mask to intersect with the graph's existing unplaced-operation mask.
    /// Passing this test never establishes physical or source reachability.
    pub(super) fn permitted_operations(&self, subset: u16) -> u16 {
        if subset.count_ones() >= u32::from(self.first_bag_len) {
            return u16::MAX;
        }
        let mut permitted = u16::MAX;
        for (kind, &operations) in self.kind_operations.iter().enumerate() {
            let limit = 1 + u32::from(kind == usize::from(self.extra_kind));
            if (subset & operations).count_ones() >= limit {
                permitted &= !operations;
            }
        }
        permitted
    }

    /// Check only necessary positive ILC requirements and first-bag capacity.
    /// The source owner and operation order are the same as in `compile` above;
    /// the caller lends its existing projection's original-row contributors.
    pub(super) fn temporal_relaxation(
        &self,
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        row_contributors: &[u16],
    ) -> TemporalPrefixResult {
        if catalog.height() != 6
            || row_contributors.len() != 6
            || candidate.row_ids().len() != usize::from(self.first_bag_len) * 2 + 1
        {
            return TemporalPrefixResult::default();
        }
        temporal_prefix_relaxation(self, row_contributors, |operation, deleted_rows| {
            // This query includes every realization's positive requirement
            // alternative. Forbidden rows, collision, grounding and SRS are
            // deliberately relaxed, so a pass carries no positive authority.
            catalog
                .realization_requirement_is_satisfied(candidate.row_ids()[operation], deleted_rows)
        })
    }

    fn first_bag_capped_count(&self, operations: u16) -> u32 {
        self.kind_operations
            .iter()
            .enumerate()
            .map(|(kind, &kind_operations)| {
                let limit = 1 + u32::from(kind == usize::from(self.extra_kind));
                (operations & kind_operations).count_ones().min(limit)
            })
            .sum()
    }
}

/// Exhaust a six-row relaxation before building the physical subset graph.
///
/// A(D) contains operations with some positive requirement satisfied by the
/// deleted rows D. C(D) is the OR of the operation contributors of those rows.
/// Row r may be added when C(r) is in A(D) and C(D+r) fits the first-bag cap.
/// Each actual simultaneous clear can be added one row at a time: positive
/// requirements are monotone, and every intermediate C(D) stays within the
/// actual placed prefix. Before the seventh lock, all first-seven operations
/// therefore belong to A(D) at some reachable D and fit the cap. Exhausting D
/// without seven capped available operations is a negative proof only.
fn temporal_prefix_relaxation(
    prefix: &FullDrawSourcePrefix,
    row_contributors: &[u16],
    mut requirement_is_satisfied: impl FnMut(usize, u16) -> bool,
) -> TemporalPrefixResult {
    let operation_count = usize::from(prefix.first_bag_len) * 2 + 1;
    let all_operations = (1_u16 << operation_count) - 1;
    if row_contributors.len() > 6
        || row_contributors
            .iter()
            .any(|contributors| contributors & !all_operations != 0)
    {
        return TemporalPrefixResult::default();
    }
    let mut available_at = |deleted_rows| {
        let mut available = 0_u16;
        for operation in 0..operation_count {
            if requirement_is_satisfied(operation, deleted_rows) {
                available |= 1_u16 << operation;
            }
        }
        available
    };
    let required_count = u32::from(prefix.first_bag_len);
    let root_available = available_at(0);
    if prefix.first_bag_capped_count(root_available) >= required_count {
        return TemporalPrefixResult {
            root_fast_pass: true,
            visited_row_states: 1,
            ..TemporalPrefixResult::default()
        };
    }

    let all_rows = (1_u16 << row_contributors.len()) - 1;
    // Bit D denotes one of at most 64 original-row deletion states. These two
    // stack scalars replace a candidate-sized subset array or heap work queue.
    let mut pending = 1_u64;
    let mut seen = 1_u64;
    let mut visited_row_states = 0_u8;
    while pending != 0 {
        let deleted_rows = pending.trailing_zeros() as u16;
        pending &= pending - 1;
        visited_row_states += 1;
        let available = if deleted_rows == 0 {
            root_available
        } else {
            available_at(deleted_rows)
        };
        if prefix.first_bag_capped_count(available) >= required_count {
            return TemporalPrefixResult {
                visited_row_states,
                ..TemporalPrefixResult::default()
            };
        }
        // An operation spanning multiple deleted rows contributes once.
        let mut mandatory = 0_u16;
        let mut selected_rows = deleted_rows;
        while selected_rows != 0 {
            let row = selected_rows.trailing_zeros() as usize;
            selected_rows &= selected_rows - 1;
            mandatory |= row_contributors[row];
        }
        let mut remaining_rows = all_rows & !deleted_rows;
        while remaining_rows != 0 {
            let row = remaining_rows.trailing_zeros() as usize;
            remaining_rows &= remaining_rows - 1;
            let contributors = row_contributors[row];
            if contributors & !available != 0 {
                continue;
            }
            let next_mandatory = mandatory | contributors;
            let mandatory_count = next_mandatory.count_ones();
            if mandatory_count > required_count
                || prefix.first_bag_capped_count(next_mandatory) != mandatory_count
            {
                continue;
            }
            let next = deleted_rows | (1_u16 << row);
            let next_bit = 1_u64 << next;
            if seen & next_bit == 0 {
                seen |= next_bit;
                pending |= next_bit;
            }
        }
    }
    TemporalPrefixResult {
        proved_infeasible: true,
        visited_row_states,
        root_fast_pass: false,
    }
}

fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[cfg(test)]
thread_local! {
    static TEST_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn with_disabled<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_DISABLED.set(self.0);
        }
    }
    let _restore = Restore(TEST_DISABLED.replace(true));
    run()
}

#[cfg(test)]
#[path = "source_prefix_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "source_prefix_temporal_tests.rs"]
mod temporal_tests;
