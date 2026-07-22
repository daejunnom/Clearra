use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, TilingVariantId},
    operation::operation::OperationId,
    solution::{HoldDecision, LineClearEvent, PatternId, ReachabilityEvidence},
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use crate::identity::build_identity::BuildIdentity;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildVariant {
    id: BuildVariantId,
    tiling_variant_id: TilingVariantId,
    identity: BuildIdentity,
    coverage: PatternBitSet,
    operation_order: Vec<OperationId>,
    hold_decisions: Vec<HoldDecision>,
    consumed_pattern_id: Option<PatternId>,
    line_clear_events: Vec<LineClearEvent>,
    reachability_evidence: Option<ReachabilityEvidence>,
}

impl BuildVariant {
    pub fn new(
        id: BuildVariantId,
        tiling_variant_id: TilingVariantId,
        identity: BuildIdentity,
        coverage: PatternBitSet,
    ) -> Self {
        Self {
            id,
            tiling_variant_id,
            identity,
            coverage,
            operation_order: Vec::new(),
            hold_decisions: Vec::new(),
            consumed_pattern_id: None,
            line_clear_events: Vec::new(),
            reachability_evidence: None,
        }
    }
}
impl BuildVariant {
    pub fn with_execution_interpretation(
        mut self,
        operation_order: Vec<OperationId>,
        hold_decisions: Vec<HoldDecision>,
        consumed_pattern_id: PatternId,
        line_clear_events: Vec<LineClearEvent>,
        reachability_evidence: ReachabilityEvidence,
    ) -> Self {
        self.operation_order = operation_order;
        self.hold_decisions = hold_decisions;
        self.consumed_pattern_id = Some(consumed_pattern_id);
        self.line_clear_events = line_clear_events;
        self.reachability_evidence = Some(reachability_evidence);
        self
    }
}
impl BuildVariant {
    pub fn id(&self) -> BuildVariantId {
        self.id
    }
}
impl BuildVariant {
    pub fn tiling_variant_id(&self) -> TilingVariantId {
        self.tiling_variant_id
    }
}
impl BuildVariant {
    pub fn identity(&self) -> BuildIdentity {
        self.identity
    }
}
impl BuildVariant {
    pub fn coverage(&self) -> &PatternBitSet {
        &self.coverage
    }
}
impl BuildVariant {
    pub fn operation_order(&self) -> &[OperationId] {
        &self.operation_order
    }
}
impl BuildVariant {
    pub fn hold_decisions(&self) -> &[HoldDecision] {
        &self.hold_decisions
    }
}
impl BuildVariant {
    pub fn consumed_pattern_id(&self) -> Option<PatternId> {
        self.consumed_pattern_id
    }
}
impl BuildVariant {
    pub fn line_clear_events(&self) -> &[LineClearEvent] {
        &self.line_clear_events
    }
}
impl BuildVariant {
    pub fn reachability_evidence(&self) -> Option<ReachabilityEvidence> {
        self.reachability_evidence
    }
}
impl BuildVariant {
    pub fn can_source_coverage_row(&self) -> bool {
        self.reachability_evidence
            .is_some_and(|evidence| evidence.reachable)
            && self.operation_order.len() == self.hold_decisions.len()
            && self.consumed_pattern_id.is_some()
    }
}
