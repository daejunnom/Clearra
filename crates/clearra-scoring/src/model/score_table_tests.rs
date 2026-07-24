use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent, spin_event::SpinEvent},
    profile::ScoreModelId,
    state::combo_state::ComboState,
};

use super::*;

#[test]
fn score_model_tables_are_profile_specific() {
    let clear = ClearEvent::new(4, true);

    let guideline = ScoreModelTable::for_model(ScoreModelId::Guideline)
        .expect("guideline table")
        .score_clear(clear);
    let jstris = ScoreModelTable::for_model(ScoreModelId::JstrisUltra)
        .expect("jstris table")
        .score_clear(clear);
    let tetrio = ScoreModelTable::for_model(ScoreModelId::Tetrio)
        .expect("tetrio table")
        .score_clear(clear);

    assert_ne!(guideline, jstris);
    assert_ne!(jstris, tetrio);
}

#[test]
fn score_model_tables_score_t_spins_separately_from_line_clears() {
    let table = ScoreModelTable::for_model(ScoreModelId::Guideline).expect("table");
    let double = ScoreEvent::new(
        0,
        ClearEvent::new(2, false),
        None,
        ComboState::default(),
        ComboState::default(),
        false,
        false,
    );
    let t_spin_double = ScoreEvent::new(
        0,
        ClearEvent::new(2, false),
        Some(SpinEvent::new('T', false, 2)),
        ComboState::default(),
        ComboState::default(),
        false,
        false,
    );

    assert!(table.score_event(t_spin_double) > table.score_event(double));
}

#[test]
fn guideline_level_one_perfect_clear_table_is_line_specific() {
    let table = ScoreModelTable::for_model(ScoreModelId::Guideline).expect("guideline table");

    assert!(GuidelineScoreTable::SOURCE_NOTE.contains("PC bonuses 800/1200/1800/2000"));
    assert_eq!(table.score_clear(ClearEvent::new(1, true)), 900);
    assert_eq!(table.score_clear(ClearEvent::new(2, true)), 1500);
    assert_eq!(table.score_clear(ClearEvent::new(3, true)), 2300);
    assert_eq!(table.score_clear(ClearEvent::new(4, true)), 2800);
}

#[test]
fn guideline_back_to_back_tetris_pc_adjusts_action_and_pc_bonus_separately() {
    let table = ScoreModelTable::for_model(ScoreModelId::Guideline).expect("guideline table");
    let event = ScoreEvent::new(
        0,
        ClearEvent::new(4, true),
        None,
        ComboState::default(),
        ComboState::default(),
        true,
        true,
    );

    assert_eq!(
        table.score_event_with_b2b(event, crate::profile::B2BPolicy::multiplier(3, 2, 0)),
        4400
    );
}

#[test]
fn jstris_ultra_scores_mini_double_and_b2b_by_its_profile_table() {
    let table = ScoreModelTable::for_model(ScoreModelId::JstrisUltra).expect("jstris table");
    let mini_double = ScoreEvent::new(
        0,
        ClearEvent::new(2, false),
        Some(SpinEvent::new('T', true, 2)),
        ComboState::default(),
        ComboState::default(),
        true,
        true,
    );

    assert!(JstrisUltraScoreTable::SOURCE_NOTE.contains("1.5x B2B"));
    assert_eq!(
        table.score_event_with_b2b(mini_double, crate::profile::B2BPolicy::multiplier(3, 2, 0)),
        1800
    );
}

#[test]
fn jstris_ultra_does_not_apply_b2b_multiplier_to_pc_bonus() {
    let table = ScoreModelTable::for_model(ScoreModelId::JstrisUltra).expect("jstris table");
    let event = ScoreEvent::new(
        0,
        ClearEvent::new(4, true),
        None,
        ComboState::default(),
        ComboState::default(),
        true,
        true,
    );

    assert_eq!(
        table.score_event_with_b2b(event, crate::profile::B2BPolicy::multiplier(3, 2, 0)),
        4200
    );
}

#[test]
fn score_profile_covers_all_award_classes() {
    let table = ScoreModelTable::for_model(ScoreModelId::Guideline).expect("table");

    for award_class in SpinAwardClass::ALL {
        let _ = table.score_award_class(award_class, 2);
    }
}

#[test]
fn tetrio_score_table_matches_source_pinned_values() {
    let table = ScoreModelTable::for_model(ScoreModelId::Tetrio).expect("tetrio table");

    assert!(TetrioScoreTable::SOURCE_NOTE.contains("quad=800"));
    assert_eq!(table.line_score(1), 100);
    assert_eq!(table.line_score(2), 300);
    assert_eq!(table.line_score(3), 500);
    assert_eq!(table.line_score(4), 800);
    assert_eq!(table.t_spin_score(0), 400);
    assert_eq!(table.t_spin_score(1), 800);
    assert_eq!(table.t_spin_score(2), 1200);
    assert_eq!(table.t_spin_score(3), 1600);
    assert_eq!(table.t_spin_score(4), 2600);
    assert_eq!(table.t_spin_mini_score(0), 100);
    assert_eq!(table.t_spin_mini_score(1), 200);
    assert_eq!(table.t_spin_mini_score(2), 400);
    assert_eq!(table.t_spin_mini_score(3), 800);
    assert_eq!(table.t_spin_mini_score(4), 1600);
    assert_eq!(table.score_clear(ClearEvent::new(4, true)), 3500);
}
