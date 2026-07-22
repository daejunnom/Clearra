use clearra_core_domain::{
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_geometry::layout::board64_layout::Board64Layout;

use crate::{
    event::{KickEvidenceEvent, MovementEvidenceEvent, TraceCompleteness},
    ownership::{ColoredCellOwnership, ColoredCellOwnershipError},
    replay::{replay_event::ReplayEvent, replay_event_builder::replay_events_from_trace},
    trace::{
        HoldDecision, SolutionTrace, SolutionTraceBuilder, SolutionTraceBuilderError,
        TraceCanonicalKey,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildVariantOperation {
    operation_id: Option<OperationId>,
    piece: PieceKind,
    rotation: RotationState,
    x: u16,
    y: u16,
    expected_mask: Option<u64>,
    expected_cleared_row_mask: Option<u16>,
}

impl BuildVariantOperation {
    pub fn new(piece: PieceKind, rotation: RotationState, x: u16, y: u16) -> Self {
        Self {
            operation_id: None,
            piece,
            rotation,
            x,
            y,
            expected_mask: None,
            expected_cleared_row_mask: None,
        }
    }
}
impl BuildVariantOperation {
    pub fn with_operation_id(mut self, operation_id: OperationId) -> Self {
        self.operation_id = Some(operation_id);
        self
    }
}
impl BuildVariantOperation {
    pub fn with_mask(mut self, expected_mask: u64) -> Self {
        self.expected_mask = Some(expected_mask);
        self
    }
}
impl BuildVariantOperation {
    pub fn with_cleared_row_mask(mut self, expected_cleared_row_mask: u16) -> Self {
        self.expected_cleared_row_mask = Some(expected_cleared_row_mask);
        self
    }
}
impl BuildVariantOperation {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl BuildVariantOperation {
    pub fn rotation(self) -> RotationState {
        self.rotation
    }
}
impl BuildVariantOperation {
    pub fn x(self) -> u16 {
        self.x
    }
}
impl BuildVariantOperation {
    pub fn y(self) -> u16 {
        self.y
    }
}
impl BuildVariantOperation {
    pub fn expected_mask(self) -> Option<u64> {
        self.expected_mask
    }
}
impl BuildVariantOperation {
    pub fn expected_cleared_row_mask(self) -> Option<u16> {
        self.expected_cleared_row_mask
    }
}
impl BuildVariantOperation {
    pub fn operation_id(self) -> Option<OperationId> {
        self.operation_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildVariantReplayInput {
    variant_id: String,
    layout: Board64Layout,
    initial_occupied: u64,
    operations: Vec<BuildVariantOperation>,
    representative_order: Vec<usize>,
    hold_decisions: Vec<HoldDecision>,
    representative: bool,
    sample: bool,
    kick_evidence: Vec<KickEvidenceEvent>,
    movement_evidence: Vec<MovementEvidenceEvent>,
    trace_completeness: TraceCompleteness,
}

impl BuildVariantReplayInput {
    pub fn new(
        variant_id: impl Into<String>,
        layout: Board64Layout,
        initial_occupied: u64,
        operations: Vec<BuildVariantOperation>,
    ) -> Self {
        let representative_order = (0..operations.len()).collect();
        let hold_decisions = vec![HoldDecision::None; operations.len()];
        Self {
            variant_id: variant_id.into(),
            layout,
            initial_occupied,
            operations,
            representative_order,
            hold_decisions,
            representative: true,
            sample: true,
            kick_evidence: Vec::new(),
            movement_evidence: Vec::new(),
            trace_completeness: TraceCompleteness::Complete,
        }
    }
}
impl BuildVariantReplayInput {
    pub fn with_representative_order(mut self, representative_order: Vec<usize>) -> Self {
        self.representative_order = representative_order;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn with_hold_decisions(mut self, hold_decisions: Vec<HoldDecision>) -> Self {
        self.hold_decisions = hold_decisions;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn with_trace_marker(mut self, representative: bool, sample: bool) -> Self {
        self.representative = representative;
        self.sample = sample;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn with_kick_evidence(mut self, kick_evidence: Vec<KickEvidenceEvent>) -> Self {
        self.kick_evidence = kick_evidence;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn with_movement_evidence(mut self, movement_evidence: Vec<MovementEvidenceEvent>) -> Self {
        self.movement_evidence = movement_evidence;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn with_trace_completeness(mut self, trace_completeness: TraceCompleteness) -> Self {
        self.trace_completeness = trace_completeness;
        self
    }
}
impl BuildVariantReplayInput {
    pub fn variant_id(&self) -> &str {
        &self.variant_id
    }
}
impl BuildVariantReplayInput {
    pub fn layout(&self) -> Board64Layout {
        self.layout
    }
}
impl BuildVariantReplayInput {
    pub fn initial_occupied(&self) -> u64 {
        self.initial_occupied
    }
}
impl BuildVariantReplayInput {
    pub fn operations(&self) -> &[BuildVariantOperation] {
        &self.operations
    }
}
impl BuildVariantReplayInput {
    pub fn representative_order(&self) -> &[usize] {
        &self.representative_order
    }
}
impl BuildVariantReplayInput {
    pub fn hold_decisions(&self) -> &[HoldDecision] {
        &self.hold_decisions
    }
}
impl BuildVariantReplayInput {
    pub fn representative(&self) -> bool {
        self.representative
    }
}
impl BuildVariantReplayInput {
    pub fn sample(&self) -> bool {
        self.sample
    }
}
impl BuildVariantReplayInput {
    pub fn kick_evidence(&self) -> &[KickEvidenceEvent] {
        &self.kick_evidence
    }
}
impl BuildVariantReplayInput {
    pub fn movement_evidence(&self) -> &[MovementEvidenceEvent] {
        &self.movement_evidence
    }
}
impl BuildVariantReplayInput {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayTrace {
    variant_id: String,
    solution_trace: SolutionTrace,
    events: Vec<ReplayEvent>,
    colored_cell_ownership: ColoredCellOwnership,
    representative: bool,
    sample: bool,
}

impl ReplayTrace {
    pub fn new(
        variant_id: impl Into<String>,
        solution_trace: SolutionTrace,
        events: Vec<ReplayEvent>,
        colored_cell_ownership: ColoredCellOwnership,
        representative: bool,
        sample: bool,
    ) -> Self {
        Self {
            variant_id: variant_id.into(),
            solution_trace,
            events,
            colored_cell_ownership,
            representative,
            sample,
        }
    }
}
impl ReplayTrace {
    pub fn variant_id(&self) -> &str {
        &self.variant_id
    }
}
impl ReplayTrace {
    pub fn solution_trace(&self) -> &SolutionTrace {
        &self.solution_trace
    }
}
impl ReplayTrace {
    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }
}
impl ReplayTrace {
    pub fn colored_cell_ownership(&self) -> &ColoredCellOwnership {
        &self.colored_cell_ownership
    }
}
impl ReplayTrace {
    pub fn representative(&self) -> bool {
        self.representative
    }
}
impl ReplayTrace {
    pub fn sample(&self) -> bool {
        self.sample
    }
}
impl ReplayTrace {
    pub fn trace_steps(&self) -> usize {
        self.solution_trace.len()
    }
}
impl ReplayTrace {
    pub fn canonical_key(&self) -> String {
        TraceCanonicalKey::from_trace(&self.solution_trace).stable_key()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplayEngine;

impl ReplayEngine {
    pub fn build_variant_to_trace(
        input: &BuildVariantReplayInput,
    ) -> Result<ReplayTrace, ReplayEngineError> {
        Self::build_variant_to_trace_inner(input, None)
    }
}
impl ReplayEngine {
    pub fn build_variant_to_trace_with_budget(
        input: &BuildVariantReplayInput,
        budget: ReplayTraceBufferBudget,
    ) -> Result<ReplayTrace, ReplayEngineError> {
        Self::build_variant_to_trace_inner(input, Some(budget))
    }
}
impl ReplayEngine {
    fn build_variant_to_trace_inner(
        input: &BuildVariantReplayInput,
        budget: Option<ReplayTraceBufferBudget>,
    ) -> Result<ReplayTrace, ReplayEngineError> {
        let solution_trace = SolutionTraceBuilder::new(
            input.layout(),
            input.initial_occupied(),
            input.operations().to_vec(),
            input.representative_order().to_vec(),
        )
        .map_err(ReplayEngineError::TraceBuilder)?
        .with_hold_decisions(input.hold_decisions().to_vec())
        .build()
        .map_err(ReplayEngineError::TraceBuilder)?;
        let colored_cell_ownership = ColoredCellOwnership::from_trace(&solution_trace)
            .map_err(ReplayEngineError::ColoredCellOwnership)?;
        let events = replay_events_from_trace(
            &solution_trace,
            input.representative(),
            input.sample(),
            input.kick_evidence(),
            input.movement_evidence(),
            input.trace_completeness(),
        );
        if let Some(budget) = budget {
            if events.len() > budget.max_events() {
                return Err(ReplayEngineError::ReplayTraceBufferBudgetExceeded {
                    event_count: events.len(),
                    max_events: budget.max_events(),
                });
            }
        }

        Ok(ReplayTrace::new(
            input.variant_id(),
            solution_trace,
            events,
            colored_cell_ownership,
            input.representative(),
            input.sample(),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayTraceBufferBudget {
    max_events: usize,
}

impl ReplayTraceBufferBudget {
    pub const fn new(max_events: usize) -> Self {
        Self { max_events }
    }
}
impl ReplayTraceBufferBudget {
    pub const fn max_events(self) -> usize {
        self.max_events
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayEngineError {
    TraceBuilder(SolutionTraceBuilderError),
    ColoredCellOwnership(ColoredCellOwnershipError),
    ReplayTraceBufferBudgetExceeded {
        event_count: usize,
        max_events: usize,
    },
}
