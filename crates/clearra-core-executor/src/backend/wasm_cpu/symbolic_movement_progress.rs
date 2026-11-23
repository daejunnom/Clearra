//! Coarse movement diagnostics, independent for each concurrently built piece.
//!
//! Events contain only static labels and integers. The observer owns clocks,
//! formatting and RSS sampling; these values never authorize completeness.

use clearra_core_domain::execution_cancellation::{
    ExecutionControl, ExecutionProgress, ProgressSink,
};
use clearra_core_domain::piece::piece_kind::PieceKind;

use super::symbolic_boolean::BooleanArena;

#[derive(Clone, Copy)]
pub(super) enum MovementPhase {
    Free = 1,
    Edges = 2,
    Propagation = 3,
    Locks = 4,
    LegalRows = 5,
    Published = 6,
    Consuming = 7,
    Released = 8,
}

#[derive(Clone, Copy)]
pub(super) enum MovementMetric {
    Phase = 0,
    PrivateNodes = 1,
    PrivateBytes = 2,
    ReleasedBuilderBytes = 3,
}

const STAGES: [[&str; 4]; 7] = [
    [
        "symbolic_movement_i_phase",
        "symbolic_movement_i_private_nodes",
        "symbolic_movement_i_private_bytes",
        "symbolic_movement_i_released_builder_bytes",
    ],
    [
        "symbolic_movement_o_phase",
        "symbolic_movement_o_private_nodes",
        "symbolic_movement_o_private_bytes",
        "symbolic_movement_o_released_builder_bytes",
    ],
    [
        "symbolic_movement_t_phase",
        "symbolic_movement_t_private_nodes",
        "symbolic_movement_t_private_bytes",
        "symbolic_movement_t_released_builder_bytes",
    ],
    [
        "symbolic_movement_s_phase",
        "symbolic_movement_s_private_nodes",
        "symbolic_movement_s_private_bytes",
        "symbolic_movement_s_released_builder_bytes",
    ],
    [
        "symbolic_movement_z_phase",
        "symbolic_movement_z_private_nodes",
        "symbolic_movement_z_private_bytes",
        "symbolic_movement_z_released_builder_bytes",
    ],
    [
        "symbolic_movement_j_phase",
        "symbolic_movement_j_private_nodes",
        "symbolic_movement_j_private_bytes",
        "symbolic_movement_j_released_builder_bytes",
    ],
    [
        "symbolic_movement_l_phase",
        "symbolic_movement_l_private_nodes",
        "symbolic_movement_l_private_bytes",
        "symbolic_movement_l_released_builder_bytes",
    ],
];

pub(super) fn report_metric(
    sink: &dyn ProgressSink,
    piece: PieceKind,
    metric: MovementMetric,
    value: u64,
) {
    let index = match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    };
    sink.report(ExecutionProgress::new(
        STAGES[index][metric as usize],
        value,
        None,
    ));
}

pub(super) fn report_phase(
    control: &ExecutionControl,
    piece: PieceKind,
    phase: MovementPhase,
    arena: &BooleanArena,
) {
    let Some(sink) = &control.progress_sink else {
        return;
    };
    report_metric(
        sink.as_ref(),
        piece,
        MovementMetric::PrivateNodes,
        arena.private_node_count() as u64,
    );
    report_metric(
        sink.as_ref(),
        piece,
        MovementMetric::PrivateBytes,
        arena.private_retained_bytes() as u64,
    );
    report_metric(sink.as_ref(), piece, MovementMetric::Phase, phase as u64);
}

pub(super) fn report_released(control: &ExecutionControl, piece: PieceKind) {
    let Some(sink) = &control.progress_sink else {
        return;
    };
    report_metric(sink.as_ref(), piece, MovementMetric::PrivateNodes, 0);
    report_metric(sink.as_ref(), piece, MovementMetric::PrivateBytes, 0);
    report_metric(
        sink.as_ref(),
        piece,
        MovementMetric::Phase,
        MovementPhase::Released as u64,
    );
}
