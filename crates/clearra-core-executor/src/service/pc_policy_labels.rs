use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::SearchProblem;
use clearra_supply::hold::hold_slot::HoldSlot;
pub(crate) fn objective_name(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::All => "all",
        ObjectiveKind::Unique => "unique",
        ObjectiveKind::MinimumCover => "min-cover",
    }
}

pub(crate) fn objective_execution_name(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::All => "all-traces",
        ObjectiveKind::Unique => "unique-canonical-traces",
        ObjectiveKind::MinimumCover => "minimum-cover-coverage-matrix",
    }
}

pub(crate) fn count_policy_name(policy: PcCountPolicy) -> &'static str {
    match policy {
        PcCountPolicy::FirstSolution => "first-solution",
        PcCountPolicy::CountAll => "count-all",
        PcCountPolicy::CountUnique => "count-unique",
    }
}

pub(crate) fn hold_slot_name(slot: HoldSlot) -> String {
    slot.piece()
        .map(|piece| piece.as_ascii().to_string())
        .unwrap_or_else(|| "none".to_owned())
}

pub(crate) fn pattern_count(problem: &SearchProblem) -> usize {
    problem
        .piece_source()
        .materialized_universe()
        .map_or(0, |universe| universe.pattern_count())
}
