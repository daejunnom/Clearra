use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{
    CoreExecutionResult, CorePathStep, SetupCandidateReport, SetupFinderReport,
    SetupHoldConditionReport,
};
use clearra_problem::{
    compile_setup_search_conditions, SetupCandidatePriority, SetupCycleResetBorrowPolicy,
    SetupPathDetail, SetupQueueInput, SetupSearchQuery,
};

pub(crate) fn query(priority: SetupCandidatePriority) -> SetupSearchQuery {
    SetupSearchQuery::default().with_candidate_priority(priority)
}

pub(crate) fn core_result(query: &SetupSearchQuery) -> CoreExecutionResult {
    let condition = compile_setup_search_conditions(query)
        .expect("compile Setup ranked-family fixture condition")
        .remove(0);
    let setup_id = SetupPathDetail::setup_id_for(1, 0, 1).expect("canonical Setup fixture id");
    let candidate = SetupCandidateReport::new(
        setup_id,
        1,
        1,
        1,
        1,
        1,
        "1.0".to_owned(),
        "1.0".to_owned(),
        "1.0".to_owned(),
        vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
    );
    let report = SetupFinderReport::new(
        query.search_mode(),
        query.queue_observation_policy(),
        query.residue().cycle().unwrap_or_default(),
        pieces_string(query.residue().pieces()),
        queue_based_pieces(query.queue()),
        pieces_string(query.next_cycle_remaining_pieces().unwrap_or(&[])),
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
        "1".to_owned(),
        1,
        true,
        vec![SetupHoldConditionReport::new(
            condition.condition_id().to_owned(),
            condition.initial_hold(),
            condition.pattern_expression().to_owned(),
            1,
            1,
            false,
            true,
            vec![candidate],
        )],
    );
    let candidate_count = report
        .hold_conditions()
        .iter()
        .map(|condition| condition.candidates().len())
        .sum::<usize>();
    let set_hash = setup_candidate_set_hash(&report);
    CoreExecutionResult::new(
        vec![
            ("status".to_owned(), "setup-finder-complete".to_owned()),
            ("count_complete".to_owned(), "true".to_owned()),
            ("probability_complete".to_owned(), "true".to_owned()),
            ("resource_truncated".to_owned(), "false".to_owned()),
            ("resource_truncation_reason".to_owned(), "none".to_owned()),
            (
                "setup_coverage_semantics".to_owned(),
                query
                    .queue_observation_policy()
                    .coverage_semantics()
                    .to_owned(),
            ),
            (
                "queue_knowledge".to_owned(),
                query.queue_observation_policy().keyword().to_owned(),
            ),
            (
                "visible_piece_count".to_owned(),
                query
                    .queue_observation_policy()
                    .visible_piece_count()
                    .map_or_else(|| "all".to_owned(), |count| count.to_string()),
            ),
            (
                "setup_search_mode".to_owned(),
                query.search_mode().keyword().to_owned(),
            ),
            (
                "remaining_pieces".to_owned(),
                report.remaining_pieces().to_owned(),
            ),
            (
                "queue_based_pieces".to_owned(),
                report.queue_based_pieces().to_owned(),
            ),
            (
                "next_cycle_remaining_pieces".to_owned(),
                report.next_cycle_remaining_pieces().to_owned(),
            ),
            ("setup_cycle".to_owned(), report.cycle().to_string()),
            (
                "setup_candidate_priority".to_owned(),
                query.candidate_priority().keyword().to_owned(),
            ),
            (
                "setup_length_preference".to_owned(),
                query.length_preference().keyword().to_owned(),
            ),
            (
                "geometry_candidate_family_count".to_owned(),
                report.geometry_family_count().to_owned(),
            ),
            (
                "partial_build_node_count".to_owned(),
                report.partial_build_node_count().to_string(),
            ),
            (
                "tablebase_requested".to_owned(),
                query.tablebase_requested().to_string(),
            ),
            (
                "normalized_solution_key_algorithm".to_owned(),
                "clearra-setup-candidate-key-v2-exact-partial-state".to_owned(),
            ),
            (
                "normalized_solution_set_hash_algorithm".to_owned(),
                "clearra-setup-candidate-set-fnv64-v1".to_owned(),
            ),
            (
                "unique_solution_count".to_owned(),
                candidate_count.to_string(),
            ),
            (
                "normalized_unique_solution_count".to_owned(),
                candidate_count.to_string(),
            ),
            (
                "solution_found".to_owned(),
                (candidate_count != 0).to_string(),
            ),
            ("normalized_solution_set_hash".to_owned(), set_hash.clone()),
            ("actual_normalized_solution_set_hash".to_owned(), set_hash),
        ],
        Vec::new(),
    )
    .with_setup_finder_report(report)
}

fn setup_candidate_set_hash(report: &SetupFinderReport) -> String {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut conditions = report.hold_conditions().iter().collect::<Vec<_>>();
    conditions.sort_unstable_by(|left, right| left.condition_id().cmp(right.condition_id()));
    let mut hash = FNV_OFFSET;
    for condition in conditions {
        for candidate in condition.candidates() {
            for byte in condition
                .condition_id()
                .bytes()
                .chain(core::iter::once(b'|'))
            {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            for shift in (0..10).rev() {
                let nibble = ((candidate.board_mask() >> (shift * 4)) & 0x0f) as u8;
                let byte = if nibble < 10 {
                    b'0' + nibble
                } else {
                    b'a' + nibble - 10
                };
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("css1:{hash:016x}")
}

fn queue_based_pieces(queue: &SetupQueueInput) -> String {
    queue
        .as_fixed_sequence()
        .map(|queue| pieces_string(queue.pieces()))
        .unwrap_or_default()
}

fn pieces_string(pieces: &[PieceKind]) -> String {
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}
