//! Producer-owned complete physical PC-path source, never score or Build authority.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, objective::objective_kind::ObjectiveKind,
};
use clearra_core_ffi::rules::{kick_profile_code, rule_profile_code};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};
use clearra_replay::FullHeightExecutionBatch;
use sha2::{Digest, Sha256};

use crate::pc_chance_coverage_evidence::{PcChanceCoverageEvidence, PcChanceProblemEvidence};

/// Read-only exact input and complete canonical family binding. Only Core's
/// executed PC producer may construct/attach it; public result fields, a
/// representative trace, or a scoring batch cannot grant this authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcFullHeightReplayEvidence {
    problem: PcChanceProblemEvidence,
    batch: FullHeightExecutionBatch,
    source_keys_sha256: [u8; 32],
    source_key_count: usize,
}

impl PcFullHeightReplayEvidence {
    pub(crate) fn checked_creation_future_bytes(problem: &SearchProblem) -> Option<u128> {
        // The physical batch moves from the producer (no second graph clone).
        // Admit the independently owned normalized problem snapshot before it
        // is constructed, using the same exhaustive owner inventory as chance.
        PcChanceCoverageEvidence::checked_pc_family_creation_future_bytes(problem, 0)?
            .checked_add(core::mem::size_of::<Self>() as u128)
    }

    pub(crate) fn from_executed_problem(
        problem: &SearchProblem,
        keys: &[String],
        batch: FullHeightExecutionBatch,
    ) -> Result<Self, &'static str> {
        let execution = batch.execution();
        if !path_contract(problem)
            || !execution.complete()
            || keys.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("extended_pc_replay_source_contract_mismatch");
        }
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or("extended_pc_replay_source_universe_missing")?;
        let kick = problem.kick_profile();
        if !universe.complete()
            || batch.height() != problem.visible_height() as u8
            || batch.initial() != Board256Mask::from_words(problem.initial_board().occupied_words())
            || execution.initial_cursor() != problem.initial_hold().cursor()
            || execution.initial_hold() != problem.initial_hold().hold_piece()
            || execution.hold_enabled() != problem.supply().hold_enabled()
            || execution.projects_unplaced_lookahead()
                != problem.supply().projects_unplaced_lookahead()
            || execution.projects_standard_bag_lookahead()
                != problem.supply().projects_standard_bag_lookahead()
            || execution.kick_table_id() != u64::from(kick_profile_code(kick.profile_id()))
            || execution.rule_profile_id() != u64::from(rule_profile_code(kick.source_rule()))
            || execution.kick_table_id() == 0
            || execution.rule_profile_id() == 0
            || execution.patterns().len() != universe.pattern_count()
            || execution
                .patterns()
                .iter()
                .enumerate()
                .any(|(index, pattern)| pattern.as_slice() != universe.sequence_at(index).as_ref())
        {
            return Err("extended_pc_replay_source_snapshot_mismatch");
        }
        // Graphs were rebound after the complete family was sorted. Multiple
        // physical realizations may share a key, but no canonical member may
        // disappear or be replaced by a foreign graph. Check without allocating
        // another dictionary/mask and preserve the producer's sorted order.
        let mut next_key = 0_usize;
        let mut previous_id = None;
        for graph in execution.graphs() {
            let index = usize::try_from(graph.candidate_id())
                .ok()
                .and_then(|id| id.checked_sub(1))
                .ok_or("extended_pc_replay_source_candidate_invalid")?;
            if keys
                .get(index)
                .is_none_or(|key| key != graph.candidate_key())
            {
                return Err("extended_pc_replay_source_candidate_mismatch");
            }
            if previous_id != Some(graph.candidate_id()) {
                if index != next_key {
                    return Err("extended_pc_replay_source_family_incomplete");
                }
                next_key += 1;
                previous_id = Some(graph.candidate_id());
            }
        }
        if next_key != keys.len() {
            return Err("extended_pc_replay_source_family_incomplete");
        }
        let snapshot = PcChanceProblemEvidence::from_pc_path_search_problem(problem)
            .map_err(|_| "extended_pc_replay_source_problem_mismatch")?;
        Ok(Self {
            problem: snapshot,
            batch,
            source_keys_sha256: source_keys_sha256(keys),
            source_key_count: keys.len(),
        })
    }

    /// Exhaustive normalized input comparison plus exact canonical dictionary
    /// binding. Public Core key setters cannot relabel a genuine path source.
    pub fn matches_result_source(&self, problem: &SearchProblem, keys: &[String]) -> bool {
        path_contract(problem)
            && self.problem.matches_search_problem(problem)
            && self.batch.execution().complete()
            && self.matches_source_keys(keys)
    }

    pub fn matches_source_keys(&self, keys: &[String]) -> bool {
        self.source_key_count == keys.len()
            && keys.windows(2).all(|pair| pair[0] < pair[1])
            && self.source_keys_sha256 == source_keys_sha256(keys)
    }

    pub fn batch(&self) -> &FullHeightExecutionBatch {
        &self.batch
    }

    pub const fn source_key_count(&self) -> usize {
        self.source_key_count
    }

    pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
        self.problem
            .checked_storage_retained_bytes()?
            .checked_add(self.batch.checked_nested_retained_bytes()?)
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
        && (7..=24).contains(&problem.visible_height())
        && problem.objective().kind() == ObjectiveKind::All
        && problem.count_policy() == PcCountPolicy::CountAll
        && problem.output_policy() == SearchOutputPolicy::Trace
        && !problem.objective().score().requested()
        && !problem.objective().execution_constraints().requested()
        && !problem
            .queue_observation_policy()
            .requires_observation_policy()
}

fn source_keys_sha256(keys: &[String]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"clearra-pc-full-height-replay-source-v1\0");
    hash.update((keys.len() as u64).to_le_bytes());
    for key in keys {
        hash.update((key.len() as u64).to_le_bytes());
        hash.update(key.as_bytes());
    }
    hash.finalize().into()
}
