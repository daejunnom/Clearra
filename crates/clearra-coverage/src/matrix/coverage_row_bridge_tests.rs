use clearra_core_domain::probability::probability_value::ProbabilityValue;

use crate::{
    matrix::{
        coverage_matrix::CoverageMatrix,
        coverage_row::CoverageRow,
        coverage_row_bridge::{
            coverage_row_from_raw_words, coverage_row_from_raw_words_with_identity,
            coverage_row_from_raw_words_with_identity_and_piece_source, CoverageRowBridgeError,
        },
    },
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    probability::union_probability::union_probability,
    row::coverage_row_kind::CoverageRowKind,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

fn raw_single_pattern(candidate_id: u64, pattern_count: usize, pattern_id: usize) -> CoverageRow {
    let mut words = [0_u64; 16];
    words[pattern_id / 64] |= 1_u64 << (pattern_id % 64);
    coverage_row_from_raw_words(
        candidate_id,
        pattern_count,
        pattern_count.div_ceil(64),
        &words,
    )
    .expect("raw coverage row")
}

#[test]
fn raw_c_coverage_row_words_can_be_read_by_rust_coverage() {
    let row = raw_single_pattern(77, 8, 3);

    assert_eq!(row.candidate_id(), 77);
    assert!(row.patterns().contains(PatternId::new(3)));
}

#[test]
fn identity_aware_bridge_rejects_default_zero_universe_identity() {
    let words = [1_u64; 16];

    assert_eq!(
        coverage_row_from_raw_words_with_identity_and_piece_source(
            1,
            CoverageRowKind::Build,
            11,
            0,
            9,
            8,
            1,
            &words
        ),
        Err(CoverageRowBridgeError::MissingPatternUniverseIdentity)
    );
}

#[test]
fn identity_aware_bridge_rejects_default_zero_weight_model_identity() {
    let words = [1_u64; 16];

    assert_eq!(
        coverage_row_from_raw_words_with_identity_and_piece_source(
            1,
            CoverageRowKind::Build,
            11,
            7,
            0,
            8,
            1,
            &words
        ),
        Err(CoverageRowBridgeError::MissingPatternWeightModelIdentity)
    );
}

#[test]
fn identity_aware_bridge_rejects_default_zero_piece_source_identity() {
    let words = [1_u64; 16];

    assert_eq!(
        coverage_row_from_raw_words_with_identity(1, CoverageRowKind::Build, 7, 9, 8, 1, &words),
        Err(CoverageRowBridgeError::MissingPieceSourceIdentity)
    );
    assert_eq!(
        coverage_row_from_raw_words_with_identity_and_piece_source(
            1,
            CoverageRowKind::Build,
            0,
            7,
            9,
            8,
            1,
            &words
        ),
        Err(CoverageRowBridgeError::MissingPieceSourceIdentity)
    );
}

#[test]
fn identity_aware_bridge_reads_bits_when_identity_is_explicit() {
    let mut words = [0_u64; 16];
    words[0] = 1_u64 << 3;

    let row = coverage_row_from_raw_words_with_identity_and_piece_source(
        77,
        CoverageRowKind::Build,
        11,
        7,
        9,
        8,
        1,
        &words,
    )
    .expect("coverage row");

    assert_eq!(row.candidate_id(), 77);
    assert_eq!(row.piece_source_id(), 11);
    assert_eq!(row.row_kind(), &CoverageRowKind::Build);
    assert_eq!(row.pattern_universe_id(), PatternUniverseId::new(7));
    assert_eq!(row.pattern_weight_model_id(), PatternWeightModelId::new(9));
    assert!(row.coverage_bits().contains(PatternId::new(3)));
}

#[test]
fn identity_aware_bridge_preserves_piece_source_id() {
    let mut words = [0_u64; 16];
    words[0] = 1_u64 << 3;

    let row = coverage_row_from_raw_words_with_identity_and_piece_source(
        77,
        CoverageRowKind::Build,
        123,
        7,
        9,
        8,
        1,
        &words,
    )
    .expect("coverage row");

    assert_eq!(row.candidate_id(), 77);
    assert_eq!(row.piece_source_id(), 123);
    assert!(row.coverage_bits().contains(PatternId::new(3)));
}

#[test]
fn pattern_bitset_universe_is_checked_after_raw_bridge() {
    let first = raw_single_pattern(1, 4, 0);
    let second = raw_single_pattern(2, 5, 1);

    assert!(first.patterns().union(second.patterns()).is_err());
}

#[test]
fn rejects_tail_bits_outside_c_pattern_universe() {
    let words = [1_u64 << 9; 16];

    assert_eq!(
        coverage_row_from_raw_words(1, 5, 1, &words),
        Err(CoverageRowBridgeError::TailBitsOutsidePatternUniverse)
    );
}

#[test]
fn coverage_row_candidate_id_is_stable_across_reads() {
    let mut words = [0_u64; 16];
    words[0] = 1_u64 << 2;

    let first = coverage_row_from_raw_words(0xabc, 8, 1, &words).expect("first");
    let second = coverage_row_from_raw_words(0xabc, 8, 1, &words).expect("second");

    assert_eq!(first.candidate_id(), second.candidate_id());
    assert_eq!(first.candidate_id(), 0xabc);
}

#[test]
fn or_union_works_for_rows_read_from_raw_c_words() {
    let rows = vec![raw_single_pattern(1, 8, 1), raw_single_pattern(2, 8, 6)];
    let matrix = CoverageMatrix::from_rows(8, rows).expect("matrix");

    let union = matrix.union_all();

    assert!(union.contains(PatternId::new(1)));
    assert!(union.contains(PatternId::new(6)));
    assert_eq!(union.count_ones(), 2);
}

#[test]
fn union_probability_from_bridge_rows_never_exceeds_one() {
    let rows = vec![
        CoverageRow::new(
            1,
            PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)]).expect("first"),
        ),
        CoverageRow::new(
            2,
            PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(2)])
                .expect("second"),
        ),
    ];
    let matrix = CoverageMatrix::from_rows(4, rows).expect("matrix");
    let weights = crate::pattern::weighted_pattern_set::WeightedPatternSet::new(vec![
        ProbabilityValue::new(0.25).expect("weight"),
        ProbabilityValue::new(0.25).expect("weight"),
        ProbabilityValue::new(0.25).expect("weight"),
        ProbabilityValue::new(0.25).expect("weight"),
    ])
    .expect("weights");

    let probability = union_probability(&matrix.union_all(), &weights).expect("probability");

    assert!(probability.get() <= 1.0);
    assert_eq!(probability.get(), 0.75);
}
