use clearra_core_domain::ids::setup_id::{BuildVariantId, TilingVariantId};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::PcCompletionGoal;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    evaluate::{PostPcEvaluationSummary, PostPcScoreSummary, ScoreEvaluationBasis},
    identity::build_identity::BuildIdentity,
    query::SetupQueueInput,
};

use super::*;

#[test]
fn setup_raw_metrics_reports_queue_hold_boundary_rule_180_and_post_pc() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::fixed_sequence(
        FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]),
    ));
    let variants = vec![BuildVariant::new(
        BuildVariantId::new(1),
        TilingVariantId::new(2),
        BuildIdentity::new(0b1111, Some(PieceKind::T)),
        PatternBitSet::new(1),
    )];
    let post_pc = PostPcEvaluation::Evaluated(PostPcEvaluationSummary::new(
        true,
        PcCompletionGoal::ClearToEmpty,
        2,
        12,
        10,
        3,
        true,
        true,
        PostPcScoreSummary::new(
            "test-score",
            5000,
            8,
            3,
            true,
            ScoreEvaluationBasis::AllTraces,
        ),
    ));

    let summary = SetupRawMetrics::from_query(
        &query,
        1,
        2,
        &variants,
        Requires180Evidence::known(true),
        RuleProfileEvidence::Explicit(RuleProfileId::SrsX),
        post_pc,
    );

    assert_eq!(summary.shape_family_count(), 1);
    assert_eq!(summary.tiling_variant_count(), 2);
    assert_eq!(summary.build_variant_count(), 1);
    assert_eq!(
        summary.queue_prefix(),
        &[PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]
    );
    assert!(summary.hold_required());
    assert_eq!(summary.hold_piece(), Some(PieceKind::T));
    assert!(!summary.bag_boundary_offsets().is_empty());
    assert!(summary.bag_boundary_ambiguous());
    assert!(summary.requires_180());
    assert_eq!(
        summary.requires_180_evidence(),
        Requires180Evidence::Known { required: true }
    );
    assert_eq!(
        summary.rule_profile_evidence(),
        RuleProfileEvidence::Explicit(RuleProfileId::SrsX)
    );
    assert_eq!(summary.rule_profile_evidence().as_str(), "srs-x");
    assert_eq!(summary.post_pc_rule_profile(), Some(RuleProfileId::SrsX));
    assert!(summary.post_pc_solution_found());
}

#[test]
fn bag_aligned_pattern_summary_reports_zero_boundary_offset() {
    let query = SetupSearchQuery::default().with_queue(SetupQueueInput::bag_aligned_pattern(
        clearra_supply::queue::bag_aligned_pattern::BagAlignedPattern::new(vec![
            PieceKind::I,
            PieceKind::O,
        ]),
    ));

    let summary = SetupRawMetrics::from_query(
        &query,
        1,
        1,
        &[],
        Requires180Evidence::NotModeled,
        RuleProfileEvidence::NotModeled,
        PostPcEvaluation::Unsupported { reason: "test" },
    );

    assert_eq!(summary.bag_boundary_offsets(), &[0]);
    assert!(!summary.bag_boundary_ambiguous());
    assert!(!summary.hold_required());
    assert!(!summary.requires_180_evidence().is_modeled());
    assert_eq!(summary.rule_profile_evidence().as_str(), "not-modeled");
}

#[test]
fn default_setup_raw_metrics_keeps_primary_counts() {
    let summary = SetupRawMetrics::new(2, 3, 4);

    assert_eq!(summary.shape_family_count(), 2);
    assert_eq!(summary.tiling_variant_count(), 3);
    assert_eq!(summary.build_variant_count(), 4);
    assert_eq!(summary.queue_prefix_len(), 0);
    assert_eq!(
        summary.requires_180_evidence(),
        Requires180Evidence::NotModeled
    );
    assert_eq!(
        summary.rule_profile_evidence(),
        RuleProfileEvidence::NotModeled
    );
    assert_eq!(
        summary.post_pc_evaluation(),
        &PostPcEvaluation::Unsupported {
            reason: "post-PC evaluation has not been attached to this setup raw metrics"
        }
    );
}
