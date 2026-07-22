use crate::operation::operation::OperationId;

use super::tiling_variant::TilingVariantId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildVariantId(u32);

impl BuildVariantId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl BuildVariantId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationSetKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldDecision {
    UseCurrent,
    SwapHeld,
    StoreCurrentThenUseNext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineClearEvent {
    pub deleted_row_mask: u16,
    pub cleared_lines: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReachabilityEvidence {
    pub reachable: bool,
    pub kick_evidence_complete: bool,
}

impl ReachabilityEvidence {
    pub const fn confirmed() -> Self {
        Self {
            reachable: true,
            kick_evidence_complete: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildVariant {
    pub build_variant_id: BuildVariantId,
    pub tiling_variant_id: TilingVariantId,
    pub operation_order: Vec<OperationId>,
    pub hold_decisions: Vec<HoldDecision>,
    pub consumed_pattern_id: PatternId,
    pub line_clear_events: Vec<LineClearEvent>,
    pub reachability_evidence: ReachabilityEvidence,
}

impl BuildVariant {
    pub fn new(
        build_variant_id: BuildVariantId,
        tiling_variant_id: TilingVariantId,
        operation_order: Vec<OperationId>,
        hold_decisions: Vec<HoldDecision>,
        consumed_pattern_id: PatternId,
        line_clear_events: Vec<LineClearEvent>,
        reachability_evidence: ReachabilityEvidence,
    ) -> Self {
        Self {
            build_variant_id,
            tiling_variant_id,
            operation_order,
            hold_decisions,
            consumed_pattern_id,
            line_clear_events,
            reachability_evidence,
        }
    }
}
impl BuildVariant {
    pub fn can_source_coverage_row(&self) -> bool {
        self.reachability_evidence.reachable
            && self.operation_order.len() == self.hold_decisions.len()
    }
}
