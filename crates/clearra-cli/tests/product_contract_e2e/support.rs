use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::{
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    reducer::coverage_probability_reducer::CoverageProbabilityReducer,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use serde_json::Value;

#[cfg(not(feature = "webgpu-search"))]
pub(super) fn assert_same_pc(cpu: &Value, other: &Value) {
    for field in [
        "normalized_unique_solution_count",
        "normalized_solution_set_hash",
        "covered_pattern_count",
        "coverage_probability",
        "count_complete",
        "probability_complete",
    ] {
        let expected = cpu["summary"]
            .get(field)
            .unwrap_or_else(|| panic!("missing {field}"));
        assert!(!expected.is_null(), "{field}");
        assert_eq!(other["summary"][field], *expected, "{field}");
    }
}

pub(super) fn assert_fixture_probability(
    fixture: &Value,
    row_kind: CoverageRowKind,
    row_identity: &str,
) {
    let pattern_count = fixture["pattern_count"].as_u64().expect("pattern count") as usize;
    let universe_id = PatternUniverseId::new(fixture["pattern_universe_id"].as_u64().unwrap_or(1));
    let model_id =
        PatternWeightModelId::new(fixture["pattern_weight_model_id"].as_u64().unwrap_or(1));
    let rows = fixture["rows"]
        .as_array()
        .expect("coverage rows")
        .iter()
        .map(|row| {
            let coverage = PatternBitSet::from_patterns(
                pattern_count,
                row["patterns"]
                    .as_array()
                    .expect("covered patterns")
                    .iter()
                    .map(|index| PatternId::new(index.as_u64().expect("pattern ordinal") as usize)),
            )
            .expect("valid coverage bitset");
            CoverageRow::new_with_piece_source(
                row[row_identity].as_u64().expect("candidate identity"),
                row_kind.clone(),
                1,
                universe_id,
                model_id,
                coverage,
            )
        })
        .collect();
    let matrix =
        TypedCoverageMatrix::from_rows(row_kind, universe_id, model_id, pattern_count, rows)
            .expect("typed coverage matrix");
    let weights = if fixture["weights"] == "uniform" {
        WeightedPatternSet::uniform(pattern_count).expect("uniform pattern weights")
    } else {
        WeightedPatternSet::new(
            fixture["weights"]
                .as_array()
                .expect("explicit weights")
                .iter()
                .map(|weight| {
                    ProbabilityValue::new(weight.as_f64().expect("weight")).expect("valid weight")
                })
                .collect(),
        )
        .expect("pattern weights")
    };
    let result = CoverageProbabilityReducer::family_probability(&matrix, &weights)
        .expect("execute the real union-probability reducer");
    assert_eq!(
        result.covered_patterns().count_ones() as u64,
        fixture["expected"]["covered_pattern_count"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        result.probability().get(),
        fixture["expected"]["probability"].as_f64().unwrap()
    );
}
