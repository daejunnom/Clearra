//! Native entry point for the spin engine's shared compute-budget driver.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_spin_structure_search::{
    SpinStructureError, SpinStructureQuery, SpinStructureReport, SpinStructureSearcher,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeSpinStructureError {
    Search(SpinStructureError),
    Cancelled,
}

impl NativeSpinStructureError {
    pub(crate) const fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Cancelled | Self::Search(SpinStructureError::Cancelled)
        )
    }

    pub(crate) const fn reason(&self) -> &'static str {
        match self {
            Self::Search(SpinStructureError::Cancelled) | Self::Cancelled => {
                "spin_structure_cancelled"
            }
            Self::Search(SpinStructureError::ParallelExecutionFailed(reason)) => reason,
            Self::Search(_) => "spin_structure_invalid_request",
        }
    }
}

impl From<SpinStructureError> for NativeSpinStructureError {
    fn from(value: SpinStructureError) -> Self {
        Self::Search(value)
    }
}

pub(crate) fn run_native_spin_structure_search(
    query: SpinStructureQuery,
    requested_workers: usize,
    control: &ExecutionControl,
) -> Result<SpinStructureReport, NativeSpinStructureError> {
    if control.is_cancelled() {
        return Err(NativeSpinStructureError::Cancelled);
    }
    let control = control
        .clone()
        .with_parallelism_if_unbound(requested_workers.max(1));
    SpinStructureSearcher::run_with_control(query, &control).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionProgress, ProgressSink},
        piece::piece_kind::PieceKind,
    };
    use clearra_spin_structure_search::{
        PieceInventory, SpinLineRequirement, SpinStructureMode, StructureBoard,
    };
    use std::sync::{Arc, Mutex};

    fn fixture_query() -> SpinStructureQuery {
        let inventory = PieceInventory::from_pieces([PieceKind::T]).expect("inventory");
        let mut query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query.max_placements = Some(1);
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });
        query
    }

    #[derive(Default)]
    struct RecordingProgressSink(Mutex<Vec<ExecutionProgress>>);

    impl ProgressSink for RecordingProgressSink {
        fn report(&self, progress: ExecutionProgress) {
            self.0.lock().expect("progress lock").push(progress);
        }
    }

    #[test]
    fn native_worker_ready_merge_is_exact_and_deterministic() {
        let query = fixture_query();
        let serial =
            run_native_spin_structure_search(query.clone(), 1, &ExecutionControl::default())
                .expect("serial structure search");
        let parallel_a =
            run_native_spin_structure_search(query.clone(), 4, &ExecutionControl::default())
                .expect("parallel structure search");
        let parallel_b = run_native_spin_structure_search(query, 4, &ExecutionControl::default())
            .expect("repeated parallel structure search");

        assert_eq!(serial.regular, parallel_a.regular);
        assert_eq!(serial.mini, parallel_a.mini);
        assert_eq!(serial.minimum_placements, parallel_a.minimum_placements);
        assert_eq!(serial.layers, parallel_a.layers);
        assert_eq!(serial.stages, parallel_a.stages);
        assert_eq!(serial.complete, parallel_a.complete);
        assert_eq!(serial.query, parallel_a.query);
        assert_eq!(parallel_a.regular, parallel_b.regular);
        assert_eq!(parallel_a.mini, parallel_b.mini);
        assert_eq!(parallel_a.minimum_placements, parallel_b.minimum_placements);
        assert_eq!(parallel_a.layers, parallel_b.layers);
        assert_eq!(parallel_a.stages, parallel_b.stages);
        assert_eq!(parallel_a.complete, parallel_b.complete);
        assert_eq!(parallel_a.query, parallel_b.query);
        assert_eq!(serial.workers_used(), 1);
        assert_eq!(parallel_a.workers_used(), 4);
    }

    #[test]
    fn native_worker_ready_path_reports_progress_and_honors_pre_cancellation() {
        let sink = Arc::new(RecordingProgressSink::default());
        let control = ExecutionControl::default().with_progress_sink(sink.clone());
        run_native_spin_structure_search(fixture_query(), 4, &control)
            .expect("parallel structure search");
        assert!(sink
            .0
            .lock()
            .expect("progress lock")
            .iter()
            .any(|progress| progress.stage == "spin-structure"));

        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let error = run_native_spin_structure_search(
            fixture_query(),
            4,
            &ExecutionControl::new(cancellation),
        )
        .expect_err("pre-cancelled structure search");
        assert!(error.is_cancelled());
    }
}
