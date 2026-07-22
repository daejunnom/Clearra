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
