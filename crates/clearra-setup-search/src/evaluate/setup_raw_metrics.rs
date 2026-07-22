use std::collections::BTreeSet;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_supply::bag::bag_boundary::standard_7_bag_observed_boundary_report;

use crate::{
    evaluate::PostPcEvaluation,
    query::{SetupQueueInput, SetupSearchQuery},
    variant::build_variant::BuildVariant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Requires180Evidence {
    NotModeled,
    Known { required: bool },
}

impl Requires180Evidence {
    pub fn known(required: bool) -> Self {
        Self::Known { required }
    }
}
impl Requires180Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotModeled => "not-modeled",
            Self::Known { required: true } => "required",
            Self::Known { required: false } => "not-required",
        }
    }
}
impl Requires180Evidence {
    pub fn is_modeled(self) -> bool {
        matches!(self, Self::Known { .. })
    }
}
impl Requires180Evidence {
    pub fn requires_180(self) -> bool {
        matches!(self, Self::Known { required: true })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleProfileEvidence {
    NotModeled,
    DefaultMvpRule { post_pc_rule: RuleProfileId },
    Explicit(RuleProfileId),
}

impl RuleProfileEvidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotModeled => "not-modeled",
            Self::DefaultMvpRule { .. } => "default-rule-profile",
            Self::Explicit(rule) => rule.as_str(),
        }
    }
}
impl RuleProfileEvidence {
    pub fn post_pc_rule_profile(self) -> Option<RuleProfileId> {
        match self {
            Self::DefaultMvpRule { post_pc_rule } | Self::Explicit(post_pc_rule) => {
                Some(post_pc_rule)
            }
            Self::NotModeled => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRawMetrics {
    shape_family_count: usize,
    tiling_variant_count: usize,
    build_variant_count: usize,
    queue_prefix: Vec<PieceKind>,
    hold_required: bool,
    hold_piece: Option<PieceKind>,
    bag_boundary_offsets: Vec<usize>,
    bag_boundary_ambiguous: bool,
    requires_180: Requires180Evidence,
    rule_profile_evidence: RuleProfileEvidence,
    post_pc_evaluation: PostPcEvaluation,
}

impl SetupRawMetrics {
    pub fn new(
        shape_family_count: usize,
        tiling_variant_count: usize,
        build_variant_count: usize,
    ) -> Self {
        Self {
            shape_family_count,
            tiling_variant_count,
            build_variant_count,
            queue_prefix: Vec::new(),
            hold_required: false,
            hold_piece: None,
            bag_boundary_offsets: Vec::new(),
            bag_boundary_ambiguous: false,
            requires_180: Requires180Evidence::NotModeled,
            rule_profile_evidence: RuleProfileEvidence::NotModeled,
            post_pc_evaluation: PostPcEvaluation::Unsupported {
                reason: "post-PC evaluation has not been attached to this setup raw metrics",
            },
        }
    }
}
impl SetupRawMetrics {
    pub fn from_query(
        query: &SetupSearchQuery,
        shape_family_count: usize,
        tiling_variant_count: usize,
        build_variants: &[BuildVariant],
        requires_180: Requires180Evidence,
        rule_profile_evidence: RuleProfileEvidence,
        post_pc_evaluation: PostPcEvaluation,
    ) -> Self {
        let mut summary = Self::new(
            shape_family_count,
            tiling_variant_count,
            build_variants.len(),
        );
        summary.queue_prefix = queue_prefix(query);
        summary.bag_boundary_offsets = bag_boundary_offsets(query.queue());
        summary.bag_boundary_ambiguous = summary.bag_boundary_offsets.len() != 1;
        summary.requires_180 = requires_180;
        summary.rule_profile_evidence = rule_profile_evidence;
        summary.post_pc_evaluation = post_pc_evaluation;

        let hold_requirements = build_variants
            .iter()
            .filter_map(|variant| variant.identity().hold_requirement())
            .collect::<BTreeSet<_>>();
        summary.hold_required = !hold_requirements.is_empty();
        summary.hold_piece = if hold_requirements.len() == 1 {
            hold_requirements.iter().next().copied()
        } else {
            None
        };

        summary
    }
}
impl SetupRawMetrics {
    pub fn with_post_pc_evaluation(mut self, post_pc_evaluation: PostPcEvaluation) -> Self {
        self.post_pc_evaluation = post_pc_evaluation;
        self
    }
}
impl SetupRawMetrics {
    pub fn shape_family_count(&self) -> usize {
        self.shape_family_count
    }
}
impl SetupRawMetrics {
    pub fn tiling_variant_count(&self) -> usize {
        self.tiling_variant_count
    }
}
impl SetupRawMetrics {
    pub fn build_variant_count(&self) -> usize {
        self.build_variant_count
    }
}
impl SetupRawMetrics {
    pub fn queue_prefix(&self) -> &[PieceKind] {
        &self.queue_prefix
    }
}
impl SetupRawMetrics {
    pub fn queue_prefix_len(&self) -> usize {
        self.queue_prefix.len()
    }
}
impl SetupRawMetrics {
    pub fn hold_required(&self) -> bool {
        self.hold_required
    }
}
impl SetupRawMetrics {
    pub fn hold_piece(&self) -> Option<PieceKind> {
        self.hold_piece
    }
}
impl SetupRawMetrics {
    pub fn bag_boundary_offsets(&self) -> &[usize] {
        &self.bag_boundary_offsets
    }
}
impl SetupRawMetrics {
    pub fn bag_boundary_ambiguous(&self) -> bool {
        self.bag_boundary_ambiguous
    }
}
impl SetupRawMetrics {
    pub fn requires_180(&self) -> bool {
        self.requires_180.requires_180()
    }
}
impl SetupRawMetrics {
    pub fn requires_180_evidence(&self) -> Requires180Evidence {
        self.requires_180
    }
}
impl SetupRawMetrics {
    pub fn rule_profile_evidence(&self) -> RuleProfileEvidence {
        self.rule_profile_evidence
    }
}
impl SetupRawMetrics {
    pub fn post_pc_rule_profile(&self) -> Option<RuleProfileId> {
        self.rule_profile_evidence.post_pc_rule_profile()
    }
}
impl SetupRawMetrics {
    pub fn post_pc_evaluation(&self) -> &PostPcEvaluation {
        &self.post_pc_evaluation
    }
}
impl SetupRawMetrics {
    pub fn post_pc_solution_found(&self) -> bool {
        self.post_pc_evaluation.solution_found()
    }
}

fn queue_prefix(query: &SetupSearchQuery) -> Vec<PieceKind> {
    queue_pieces(query.queue())
        .into_iter()
        .take(query.piece_budget().max_piece_count() as usize)
        .collect()
}

fn bag_boundary_offsets(queue: &SetupQueueInput) -> Vec<usize> {
    match queue {
        SetupQueueInput::BagAlignedPattern(pattern) => {
            if pattern.is_empty() {
                Vec::new()
            } else {
                vec![0]
            }
        }
        SetupQueueInput::FixedSequence(_) | SetupQueueInput::Observed(_) => {
            standard_7_bag_observed_boundary_report(&queue_pieces(queue))
                .candidates()
                .iter()
                .map(|candidate| candidate.initial_offset())
                .collect()
        }
    }
}

fn queue_pieces(queue: &SetupQueueInput) -> Vec<PieceKind> {
    match queue {
        SetupQueueInput::FixedSequence(sequence) => sequence.pieces().to_vec(),
        SetupQueueInput::BagAlignedPattern(pattern) => pattern.pieces().to_vec(),
        SetupQueueInput::Observed(queue) => queue.pieces().to_vec(),
    }
}

#[cfg(test)]
#[path = "setup_raw_metrics_tests.rs"]
mod tests;
