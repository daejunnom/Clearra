use super::*;

fn objective_piece_source(pattern_count: u16, weights: WeightedPatternSet) -> PieceSource {
    let provenance = SupplyProvenance::new(
        "test-bag",
        "standard-tetrominoes",
        None,
        BagBoundaryEvidence::FixedBoundary,
        false,
        false,
    )
    .expect("provenance");
    let weight_values = (0..usize::from(pattern_count))
        .map(|index| weights.weight(PatternId::new(index)).expect("weight"))
        .collect();
    let universe = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(101),
        PatternWeightModelId::new(202),
        vec![vec![PieceKind::I]; usize::from(pattern_count)],
        weight_values,
        u128::from(pattern_count),
        true,
        None,
    )
    .expect("objective universe");
    PieceSource::materialized_pattern_universe(universe, provenance)
}
fn objective_aggregate(
    source: &PieceSource,
    pattern_count: usize,
    candidate_id: u64,
    patterns: impl IntoIterator<Item = usize>,
) -> CandidateExecutionAggregate {
    let coverage =
        PatternBitSet::from_patterns(pattern_count, patterns.into_iter().map(PatternId::new))
            .expect("coverage");
    CandidateExecutionAggregate::new(
        CoverageRow::new_with_piece_source(
            candidate_id,
            CoverageRowKind::Build,
            source.id().get(),
            source.pattern_universe_id().expect("universe id"),
            source.pattern_weight_model_id().expect("weight model id"),
            coverage,
        ),
        Vec::new(),
        Some(format!("candidate:{candidate_id}")),
    )
}
fn objective_identity(source: &PieceSource) -> CoverageUniverseIdentity {
    CoverageUniverseIdentity {
        piece_source_id: source.id().get(),
        pattern_universe_id: source.pattern_universe_id().expect("universe id").get(),
        pattern_weight_model_id: source
            .pattern_weight_model_id()
            .expect("weight model id")
            .get(),
    }
}

mod case_objective_trace_key_uses_candidate_and_trace_identity {
    use super::*;
    use clearra_coverage::{
        pattern::pattern_bitset::PatternBitSet, row::coverage_row::CoverageRow,
    };

    #[test]
    fn objective_trace_key_uses_candidate_and_trace_identity() {
        let first = CNativeBuildVariantView {
            candidate_id: 9,
            build_variant_id: 1,
            canonical_operation_set_id: 9,
            operation_set_hash: 0xbeef,
            coverage_pattern_id: 3,
            ..Default::default()
        };
        let second = CNativeBuildVariantView {
            build_variant_id: 2,
            ..first
        };

        let first_owned = owned_build_variant(first);
        let second_owned = owned_build_variant(second);
        let first_key = trace_key_for_build_variant(&first_owned, 9);
        let second_key = trace_key_for_build_variant(&second_owned, 9);

        assert_eq!(first_key, "bvk2:0000000000000009:00000003:0000000000000001");
        assert_eq!(
            second_key,
            "bvk2:0000000000000009:00000003:0000000000000002"
        );
        assert_ne!(first_key, second_key);

        let other_candidate = CNativeBuildVariantView {
            candidate_id: 10,
            build_variant_id: 1,
            canonical_operation_set_id: 10,
            operation_set_hash: 0xbeef,
            coverage_pattern_id: 0,
            ..Default::default()
        };
        let row = CoverageRow::new_with_piece_source(
            9,
            CoverageRowKind::Build,
            1,
            PatternUniverseId::new(2),
            PatternWeightModelId::new(3),
            PatternBitSet::from_patterns(4, [PatternId::new(3)]).expect("coverage"),
        );
        let reordered = [owned_build_variant(other_candidate), first_owned];

        let aggregates = aggregate_candidate_executions(&reordered, &[row]).expect("aggregate");
        assert_eq!(aggregates.len(), 1);
        assert_eq!(
            aggregates[0].stable_key(),
            "bvk2:0000000000000009:00000003:0000000000000001"
        );
    }
}

mod case_minimum_cover_requires_all_requested_patterns {
    use super::*;

    #[test]
    fn minimum_cover_requires_all_requested_patterns() {
        let source = objective_piece_source(2, WeightedPatternSet::uniform(2).expect("weights"));
        let aggregates = [
            objective_aggregate(&source, 2, 11, [0]),
            objective_aggregate(&source, 2, 22, [1]),
        ];
        let outcome = reduce_objectives(
            &source,
            &aggregates,
            2,
            objective_identity(&source),
            ScenarioPackingWitness::solved(0, 2, 0),
            2,
            true,
        )
        .expect("objective reduction");
        let (result, complete, reason) = outcome.into_parts();
        let result = result.expect("objective result");

        assert!(complete);
        assert_eq!(reason, None);
        assert!(result.minimum_cover().is_complete());
        assert_eq!(result.minimum_cover().row_indices().len(), 2);
    }
}

mod case_minimum_cover_does_not_succeed_by_covering_pattern_zero_only {
    use super::*;

    #[test]
    fn minimum_cover_does_not_succeed_by_covering_pattern_zero_only() {
        let source = objective_piece_source(2, WeightedPatternSet::uniform(2).expect("weights"));
        let aggregates = [objective_aggregate(&source, 2, 11, [0])];
        let outcome = reduce_objectives(
            &source,
            &aggregates,
            2,
            objective_identity(&source),
            ScenarioPackingWitness::solved(0, 1, 0),
            1,
            true,
        )
        .expect("objective reduction");
        let (result, complete, reason) = outcome.into_parts();
        let result = result.expect("objective result");

        assert!(complete);
        assert_eq!(reason, None);
        assert!(!result.minimum_cover().is_complete());
        assert_eq!(result.coverage().covered_patterns().count_ones(), 1);
        assert_eq!(result.coverage().probability().get(), 0.5);
    }
}

mod case_objective_uses_nonuniform_pattern_weights {
    use super::*;

    #[test]
    fn objective_uses_nonuniform_pattern_weights() {
        let weights = WeightedPatternSet::new(vec![
            ProbabilityValue::new(0.8).expect("weight"),
            ProbabilityValue::new(0.2).expect("weight"),
        ])
        .expect("weights");
        let source = objective_piece_source(2, weights);
        let aggregates = [objective_aggregate(&source, 2, 11, [0])];
        let outcome = reduce_objectives(
            &source,
            &aggregates,
            2,
            objective_identity(&source),
            ScenarioPackingWitness::solved(0, 1, 0),
            1,
            true,
        )
        .expect("objective reduction");
        let (result, complete, _) = outcome.into_parts();

        assert!(complete);
        assert_eq!(
            result
                .expect("objective result")
                .coverage()
                .probability()
                .get(),
            0.8
        );
    }
}

mod case_objective_aggregate_links_rows_by_candidate_id_not_position {
    use super::*;

    #[test]
    fn objective_aggregate_links_rows_by_candidate_id_not_position() {
        let first = owned_build_variant(CNativeBuildVariantView {
            candidate_id: 11,
            build_variant_id: 1,
            operation_set_hash: 0xaaaa,
            ..Default::default()
        });
        let second = owned_build_variant(CNativeBuildVariantView {
            candidate_id: 22,
            build_variant_id: 2,
            operation_set_hash: 0xaaaa,
            ..Default::default()
        });
        let source = objective_piece_source(2, WeightedPatternSet::uniform(2).expect("weights"));
        let rows = [
            objective_aggregate(&source, 2, 22, [1])
                .coverage_row()
                .clone(),
            objective_aggregate(&source, 2, 11, [0])
                .coverage_row()
                .clone(),
        ];

        let aggregates =
            aggregate_candidate_executions(&[first, second], &rows).expect("aggregate");

        assert_eq!(aggregates[0].candidate_id(), 22);
        assert_eq!(aggregates[0].execution_variants()[0].candidate_id(), 22);
        assert_eq!(aggregates[1].candidate_id(), 11);
        assert_eq!(aggregates[1].execution_variants()[0].candidate_id(), 11);
    }
}
