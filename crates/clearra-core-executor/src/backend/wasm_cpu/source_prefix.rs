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
pub(super) struct FullDrawSourcePrefix {
    kind_operations: [u16; 7],
    extra_kind: u8,
    first_bag_len: u8,
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
            || problem.preset() != SearchProblemPreset::ScenarioPc
            || problem.problem_kind() != SearchProblemKind::ScenarioPc
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
            || problem.replay_trace_policy().retained_trace_limit() > 1
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
        Self::from_kind_operations(kind_operations, 7)
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
        })
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
