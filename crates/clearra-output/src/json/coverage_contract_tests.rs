use super::*;
use crate::json::JsonWriter;

#[test]
fn probability_result_contract_exposes_pattern_bitset_union_basis() {
    let result = ProbabilityResultContract::new(
        PatternUniverseId::new(7),
        PatternWeightModelId::new(9),
        4,
        3,
        0.75,
        true,
        4,
        Some(4),
        1.0,
        false,
        None,
    );

    let rendered =
        JsonWriter::write_value_for_test(&CoverageContractJson::probability_result(&result));

    assert!(rendered.contains("\"probability_basis\":\"PatternBitSet union\""));
    assert!(rendered.contains("\"covered_pattern_count\":3"));
    assert!(rendered.contains("\"coverage_probability\":0.75"));
    assert!(rendered.contains("\"score_does_not_modify_probability\":true"));
}

#[test]
fn observed_truncated_universe_is_not_renormalized() {
    let result = ProbabilityResultContract::new(
        PatternUniverseId::new(7),
        PatternWeightModelId::new(9),
        16,
        8,
        0.5,
        false,
        12,
        None,
        0.875,
        false,
        Some("observed_queue_truncated".to_owned()),
    );

    let rendered =
        JsonWriter::write_value_for_test(&CoverageContractJson::probability_result(&result));

    assert!(rendered.contains("\"probability_complete\":false"));
    assert!(rendered.contains("\"materialized_probability_mass\":0.875"));
    assert!(rendered.contains("\"renormalized\":false"));
    assert!(!rendered.contains("\"renormalized\":true"));
}
