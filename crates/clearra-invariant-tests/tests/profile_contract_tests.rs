use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_profiles::{
    bag::{bag_profile::BagProfileId, standard_7bag::standard_7_bag_profile},
    board::{
        board_profile::BoardProfileId,
        standard10::{standard_10_analysis_size, standard_10_board_profile, STANDARD_10_WIDTH},
    },
    pieces::{
        piece_set_profile::PieceSetProfileId,
        standard_tetrominoes::standard_tetromino_piece_set_profile,
    },
    search::search_defaults::SearchDefaults,
};

#[test]
fn profile_ids_expose_stable_canonical_strings() {
    assert_eq!(BoardProfileId::Standard10.as_str(), "standard-10");
    assert_eq!(BagProfileId::Standard7Bag.as_str(), "standard-7-bag");
    assert_eq!(
        PieceSetProfileId::StandardTetrominoes.as_str(),
        "standard-tetrominoes"
    );
}

#[test]
fn standard_10_profile_has_expected_dimensions() {
    let profile = standard_10_board_profile();

    assert_eq!(profile.size().width(), STANDARD_10_WIDTH);
    assert_eq!(profile.size().height(), 20);
    assert!(profile.is_standard_10());
}

#[test]
fn standard_analysis_size_uses_requested_line_count() {
    let size = standard_10_analysis_size(6).expect("6-line analysis board is valid");

    assert_eq!(size.width(), STANDARD_10_WIDTH);
    assert_eq!(size.height(), 6);
}

#[test]
fn standard_bag_has_one_of_each_standard_piece() {
    let profile = standard_7_bag_profile();

    assert_eq!(profile.bag_size(), 7);
    assert_eq!(
        profile.piece_set_id(),
        PieceSetProfileId::StandardTetrominoes
    );
    assert_eq!(profile.pieces_per_bag(), &PieceKind::STANDARD_TETROMINOES);
    assert_eq!(profile.entries().len(), 7);
    assert!(profile
        .entries()
        .iter()
        .all(|entry| entry.multiplicity() == 1 && entry.weight() == 1));
    assert_eq!(profile.multiplicity_for(PieceKind::I), 1);
    assert_eq!(profile.total_weight(), 7);
}

#[test]
fn standard_piece_set_contains_seven_unique_pieces() {
    let profile = standard_tetromino_piece_set_profile();

    assert_eq!(profile.len(), 7);
    assert_eq!(profile.pieces(), &PieceKind::STANDARD_TETROMINOES);
}

#[test]
fn mvp1_defaults_expose_runtime_budget_values() {
    let defaults = SearchDefaults::MVP1;

    assert_eq!(defaults.setup_max_results(), 256);
    assert_eq!(defaults.build_max_patterns(), 4096);
    assert_eq!(defaults.scenario_retained_trace_limit(), 64);
    assert_eq!(defaults.max_nodes(), 0);
    assert_eq!(defaults.max_seconds(), 0);
    assert_eq!(defaults.execution_max_frontier_states(), 0);
    assert_eq!(defaults.execution_max_candidates(), 0);
    assert_eq!(defaults.execution_max_patterns(), 0);
    assert_eq!(defaults.execution_max_memory_mib(), None);
}
