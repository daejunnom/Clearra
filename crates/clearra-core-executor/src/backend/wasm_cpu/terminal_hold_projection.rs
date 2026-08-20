use clearra_supply::pattern_universe::PatternPiecePositionIndex;

pub(super) const fn finite_terminal_release_allowed(
    sequence_len: usize,
    queue_position: usize,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    terminal_projection_consumed: bool,
    held_piece_present: bool,
    terminal_step: bool,
) -> bool {
    hold_enabled
        && projects_unplaced_lookahead
        && !terminal_projection_consumed
        && held_piece_present
        && terminal_step
        && queue_position == sequence_len
}

pub(super) fn terminal_release_pattern_word(
    pattern_index: &PatternPiecePositionIndex,
    queue_position: usize,
    word_index: usize,
    active_patterns: u64,
    projects_standard_bag_lookahead: bool,
) -> u64 {
    if queue_position != pattern_index.sequence_len() {
        return 0;
    }
    active_patterns
        & !projected_current_word(
            pattern_index,
            queue_position,
            word_index,
            projects_standard_bag_lookahead,
        )
}

pub(super) fn projected_current_word(
    pattern_index: &PatternPiecePositionIndex,
    queue_position: usize,
    word_index: usize,
    projects_standard_bag_lookahead: bool,
) -> u64 {
    (1..=7).fold(0_u64, |word, piece_code| {
        word | pattern_index.piece_word_with_projected_standard_bag_lookahead(
            queue_position,
            piece_code,
            word_index,
            projects_standard_bag_lookahead,
        )
    })
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_supply::pattern_universe::{
        MaterializedPatternUniverse, PatternPiecePositionIndex,
    };

    use super::{finite_terminal_release_allowed, terminal_release_pattern_word};

    fn pattern_index(sequence: Vec<PieceKind>) -> PatternPiecePositionIndex {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(1),
            vec![sequence],
            vec![ProbabilityValue::ONE],
            1,
            true,
            None,
        )
        .expect("test pattern universe");
        PatternPiecePositionIndex::compile(&universe).expect("test pattern index")
    }

    #[test]
    fn finite_release_requires_the_exact_terminal_state_once() {
        assert!(finite_terminal_release_allowed(
            7, 7, true, true, false, true, true
        ));
        for rejected in [
            finite_terminal_release_allowed(7, 6, true, true, false, true, true),
            finite_terminal_release_allowed(7, 7, false, true, false, true, true),
            finite_terminal_release_allowed(7, 7, true, false, false, true, true),
            finite_terminal_release_allowed(7, 7, true, true, true, true, true),
            finite_terminal_release_allowed(7, 7, true, true, false, false, true),
            finite_terminal_release_allowed(7, 7, true, true, false, true, false),
        ] {
            assert!(!rejected);
        }
    }

    #[test]
    fn an_inferred_seventh_current_blocks_terminal_release() {
        use PieceKind::{I, J, O, S, T, Z};

        let index = pattern_index(vec![I, O, T, S, Z, J]);
        assert_eq!(
            terminal_release_pattern_word(
                &index,
                index.sequence_len(),
                0,
                index.active_word(0),
                true,
            ),
            0
        );
        assert_ne!(
            terminal_release_pattern_word(
                &index,
                index.sequence_len(),
                0,
                index.active_word(0),
                false,
            ),
            0
        );
    }

    #[test]
    fn a_complete_seven_piece_source_has_no_projected_current() {
        use PieceKind::{I, J, L, O, S, T, Z};

        let index = pattern_index(vec![I, O, T, S, Z, J, L]);
        assert_eq!(
            terminal_release_pattern_word(
                &index,
                index.sequence_len() - 1,
                0,
                index.active_word(0),
                true,
            ),
            0
        );
        assert_ne!(
            terminal_release_pattern_word(
                &index,
                index.sequence_len(),
                0,
                index.active_word(0),
                true,
            ),
            0
        );
    }
}
