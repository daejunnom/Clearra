//! Producer-bound necessary supply membership for compact PC replay families.
//!
//! A negative bit proves that this original queue cannot supply the graph's
//! piece multiset. A positive bit makes no reachability or replay-count claim.
//! Each multiset retains the producer's shared pattern backing, never a new
//! candidate-by-pattern matrix.
use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};
use clearra_replay::ExactScoringExecutionBatch;
use clearra_rules::profile::replay_encoding::{replay_kick_profile_id, replay_rule_profile_id};
use clearra_supply::pattern_universe::PieceMultisetKey;
use sha2::{Digest, Sha256};

use crate::pc_chance_coverage_evidence::{PcChanceCoverageEvidence, PcChanceProblemEvidence};

#[path = "pc_path_supply_refinement.rs"]
mod refinement;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcPathSupplyEvidence {
    problem: PcChanceProblemEvidence,
    source_sha256: [u8; 32],
    groups: Vec<(PieceMultisetKey, PatternBitSet)>,
}

impl PcPathSupplyEvidence {
    pub(crate) fn checked_creation_future_bytes(
        problem: &SearchProblem,
        group_count: usize,
        refine_symbolic_membership: bool,
    ) -> Option<u128> {
        PcChanceCoverageEvidence::checked_pc_family_creation_future_bytes(problem, 0)?
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
            .checked_add(if refine_symbolic_membership {
                refinement::checked_future_bytes(problem, group_count)?
            } else {
                0
            })?
            .checked_add(
                (group_count as u128)
                    .checked_mul(
                        core::mem::size_of::<(PieceMultisetKey, PatternBitSet)>() as u128
                    )?,
            )
    }

    /// Only the executed Core producer calls this with its private target
    /// groups; public coverage rows or wire fields cannot construct this proof.
    pub(crate) fn from_executed_source<'a>(
        problem: &SearchProblem,
        batch: &ExactScoringExecutionBatch,
        targets: impl ExactSizeIterator<Item = (PieceMultisetKey, &'a PatternBitSet)>,
    ) -> Result<Self, &'static str> {
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or("pc_path_supply_universe_missing")?;
        let kick = problem.kick_profile();
        if !path_contract(problem)
            || !batch.complete()
            || !universe.complete()
            || batch.layout().width() != 10
            || batch.layout().height() != problem.visible_height()
            || batch.initial_occupied() != problem.initial_board().occupied_words()[0]
            || batch.initial_cursor() != problem.initial_hold().cursor()
            || batch.initial_hold() != problem.initial_hold().hold_piece()
            || batch.hold_enabled() != problem.supply().hold_enabled()
            || batch.projects_unplaced_lookahead() != problem.supply().projects_unplaced_lookahead()
            || batch.projects_standard_bag_lookahead()
                != problem.supply().projects_standard_bag_lookahead()
            || batch.kick_table_id() != replay_kick_profile_id(kick.profile_id())
            || batch.rule_profile_id() != replay_rule_profile_id(kick.source_rule())
            || batch.patterns().len() != universe.pattern_count()
            || batch
                .patterns()
                .iter()
                .enumerate()
                .any(|(index, pattern)| pattern.as_slice() != universe.sequence_at(index).as_ref())
            || batch.graphs().iter().enumerate().any(|(index, graph)| {
                graph.candidate_id() != index as u64 + 1
                    || graph.identity().initial_board_mask() != batch.initial_occupied()
            })
            || batch
                .graphs()
                .windows(2)
                .any(|pair| pair[0].identity() >= pair[1].identity())
        {
            return Err("pc_path_supply_source_mismatch");
        }
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(targets.len())
            .map_err(|_| "pc_path_supply_storage_unavailable")?;
        for (key, patterns) in targets {
            if patterns.pattern_count() != universe.pattern_count() {
                return Err("pc_path_supply_universe_mismatch");
            }
            // PatternBitSet::clone shares immutable dense/sparse backing.
            groups.push((key, patterns.clone()));
        }
        groups.sort_unstable_by_key(|(key, _)| *key);
        if groups.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || batch.graphs().iter().any(|graph| {
                groups
                    .binary_search_by_key(&multiset(graph.identity()), |(key, _)| *key)
                    .is_err()
            })
        {
            return Err("pc_path_supply_group_mismatch");
        }
        Ok(Self {
            problem: PcChanceProblemEvidence::from_pc_path_search_problem(problem)
                .map_err(|_| "pc_path_supply_problem_mismatch")?,
            source_sha256: source_sha256(batch),
            groups,
        })
    }

    /// Validate once against the original query and entire canonical source,
    /// before using negative bits in a manifest or a worker window.
    pub fn matches_source(
        &self,
        problem: &SearchProblem,
        batches: &[ExactScoringExecutionBatch],
    ) -> bool {
        path_contract(problem)
            && self.problem.matches_search_problem(problem)
            && batches.len() == 1
            && batches[0].complete()
            && self.source_sha256 == source_sha256(&batches[0])
    }

    /// `None` is unknown and must not prune a cell. A present row uses the
    /// unchanged original pattern ordinals, including unsuccessful queues.
    pub fn patterns_for_identity(
        &self,
        identity: StandardBoard64TilingIdentity,
    ) -> Option<&PatternBitSet> {
        self.groups
            .binary_search_by_key(&multiset(identity), |(key, _)| *key)
            .ok()
            .map(|index| &self.groups[index].1)
    }

    pub(crate) fn pattern_bitsets(&self) -> impl Iterator<Item = &PatternBitSet> {
        self.groups.iter().map(|(_, bits)| bits)
    }

    /// Includes the Arc allocation itself; shared pattern storage is counted
    /// by Core's pointer-exact result-wide storage inventory.
    pub(crate) fn checked_shared_non_pattern_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
            .checked_add(self.problem.checked_storage_retained_bytes()?)?
            .checked_add(
                (self.groups.capacity() as u128)
                    .checked_mul(
                        core::mem::size_of::<(PieceMultisetKey, PatternBitSet)>() as u128
                    )?,
            )
    }
}

fn path_contract(problem: &SearchProblem) -> bool {
    problem
        .pc_chance_evidence_policy()
        .retains_pc_path_v2_evidence()
        && matches!(
            problem.preset(),
            SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc
        )
        && problem.goal().as_str() == "clear-to-empty"
        && problem.initial_board().width() == 10
        && (1..=6).contains(&problem.visible_height())
        && problem.objective().kind() == ObjectiveKind::All
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && !problem.objective().score().requested()
        && !problem
            .queue_observation_policy()
            .requires_observation_policy()
}

fn multiset(identity: StandardBoard64TilingIdentity) -> PieceMultisetKey {
    PieceMultisetKey::from_pieces((0..identity.placement_count()).map(|index| {
        identity
            .placement(index)
            .expect("identity placement count is bounded")
            .piece()
    }))
}

fn source_sha256(batch: &ExactScoringExecutionBatch) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"clearra-pc-path-supply-membership-v1\0");
    hash.update(batch.layout().width().to_le_bytes());
    hash.update(batch.layout().height().to_le_bytes());
    hash.update(batch.initial_occupied().to_le_bytes());
    hash.update(batch.initial_cursor().to_le_bytes());
    hash.update([
        batch
            .initial_hold()
            .map_or(0, |piece| piece.as_ascii() as u8),
        u8::from(batch.hold_enabled()),
        u8::from(batch.projects_unplaced_lookahead()),
        u8::from(batch.projects_standard_bag_lookahead()),
    ]);
    hash.update(batch.kick_table_id().to_le_bytes());
    hash.update(batch.rule_profile_id().to_le_bytes());
    hash.update((batch.patterns().len() as u64).to_le_bytes());
    for pattern in batch.patterns() {
        hash.update((pattern.len() as u64).to_le_bytes());
        for piece in pattern {
            hash.update([piece.as_ascii() as u8]);
        }
    }
    hash.update((batch.graphs().len() as u64).to_le_bytes());
    for graph in batch.graphs() {
        hash.update(graph.candidate_id().to_le_bytes());
        let identity = graph.identity();
        hash.update(identity.initial_board_mask().to_le_bytes());
        hash.update((identity.placement_count() as u64).to_le_bytes());
        for index in 0..identity.placement_count() {
            let placement = identity
                .placement(index)
                .expect("identity placement count is bounded");
            hash.update([placement.piece().as_ascii() as u8]);
            hash.update(placement.cells_mask().to_le_bytes());
        }
    }
    hash.finalize().into()
}
