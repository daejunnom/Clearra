mod bag_window_compiler {
    use crate::problem::{
        CBagWindow, CPieceWindowDescriptor, CQueueView, C_QUEUE_BAG_ALIGNED_PATTERN,
    };

    pub(super) fn bag_window_from_queue(
        queue: &CQueueView,
        piece_window: &CPieceWindowDescriptor,
    ) -> CBagWindow {
        CBagWindow {
            start: if piece_window.has_exact_pieces == 1 {
                0
            } else {
                piece_window.max_pieces
            },
            len: queue.len,
            boundary_known: (queue.mode == C_QUEUE_BAG_ALIGNED_PATTERN) as u8,
            reserved: [0; 3],
        }
    }
}
mod compact_supply_descriptors {
    use crate::{
        problem::{CHoldState, CPieceMultisetWindow, CPieceWindowDescriptor, CQueueView},
        supply::{CHoldAutomatonStateDescriptor, CPieceSourceDescriptor},
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CompactSupplyDescriptors {
        queue: CQueueView,
        hold: CHoldState,
        piece_window: CPieceWindowDescriptor,
        piece_multiset_window: CPieceMultisetWindow,
        piece_source: CPieceSourceDescriptor,
        initial_hold_automaton: CHoldAutomatonStateDescriptor,
    }

    impl CompactSupplyDescriptors {
        pub fn new(
            queue: CQueueView,
            hold: CHoldState,
            piece_window: CPieceWindowDescriptor,
            piece_multiset_window: CPieceMultisetWindow,
            piece_source: CPieceSourceDescriptor,
            initial_hold_automaton: CHoldAutomatonStateDescriptor,
        ) -> Self {
            Self {
                queue,
                hold,
                piece_window,
                piece_multiset_window,
                piece_source,
                initial_hold_automaton,
            }
        }
    }
    impl CompactSupplyDescriptors {
        pub fn queue(self) -> CQueueView {
            self.queue
        }
    }
    impl CompactSupplyDescriptors {
        pub fn hold(self) -> CHoldState {
            self.hold
        }
    }
    impl CompactSupplyDescriptors {
        pub fn piece_window(self) -> CPieceWindowDescriptor {
            self.piece_window
        }
    }
    impl CompactSupplyDescriptors {
        pub fn piece_multiset_window(self) -> CPieceMultisetWindow {
            self.piece_multiset_window
        }
    }
    impl CompactSupplyDescriptors {
        pub fn piece_source(self) -> CPieceSourceDescriptor {
            self.piece_source
        }
    }
    impl CompactSupplyDescriptors {
        pub fn initial_hold_automaton(self) -> CHoldAutomatonStateDescriptor {
            self.initial_hold_automaton
        }
    }
}
mod compiler {
    use clearra_problem::SearchProblem;

    use crate::{
        problem::FfiProblemError,
        supply::{HoldAutomatonDescriptorCompiler, PieceSourceDescriptorCompiler},
    };

    use super::{
        compact_supply_descriptors::CompactSupplyDescriptors, hold_state_compiler::hold_state,
        piece_multiset_compiler::piece_multiset_window_from_queue,
        piece_window_compiler::piece_window_descriptor,
        queue_view_compiler::queue_view_for_problem,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SupplyDescriptorCompiler;

    impl SupplyDescriptorCompiler {
        pub fn compile(
            problem: &SearchProblem,
        ) -> Result<CompactSupplyDescriptors, FfiProblemError> {
            let piece_window = piece_window_descriptor(problem)?;
            let queue = queue_view_for_problem(problem)?;
            let hold = hold_state(problem);
            let piece_multiset_window = piece_multiset_window_from_queue(&queue, &piece_window);
            let piece_source = PieceSourceDescriptorCompiler::compile(problem.piece_source())
                .map_err(|_| FfiProblemError::InvalidSupplyDescriptor)?;
            let initial_hold_automaton =
                HoldAutomatonDescriptorCompiler::compile(problem.initial_hold());
            Ok(CompactSupplyDescriptors::new(
                queue,
                hold,
                piece_window,
                piece_multiset_window,
                piece_source,
                initial_hold_automaton,
            ))
        }
    }
    impl SupplyDescriptorCompiler {
        pub fn bag_window_from_queue(
            queue: &crate::problem::CQueueView,
            piece_window: &crate::problem::CPieceWindowDescriptor,
        ) -> crate::problem::CBagWindow {
            super::bag_window_compiler::bag_window_from_queue(queue, piece_window)
        }
    }
}
mod hold_state_compiler {
    use clearra_problem::SearchProblem;

    use crate::problem::{CHoldState, C_PIECE_NONE};

    use super::piece_code::piece_code;

    pub(super) fn hold_state(problem: &SearchProblem) -> CHoldState {
        CHoldState {
            enabled: problem.supply().hold_enabled() as u8,
            empty: problem.supply().hold_state().is_empty() as u8,
            piece: problem
                .supply()
                .hold_piece()
                .map(piece_code)
                .unwrap_or(C_PIECE_NONE),
            reserved: 0,
        }
    }
}
mod piece_code {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use crate::problem::{
        C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z,
    };

    pub(crate) fn piece_code(piece: PieceKind) -> u8 {
        match piece {
            PieceKind::I => C_PIECE_I,
            PieceKind::O => C_PIECE_O,
            PieceKind::T => C_PIECE_T,
            PieceKind::S => C_PIECE_S,
            PieceKind::Z => C_PIECE_Z,
            PieceKind::J => C_PIECE_J,
            PieceKind::L => C_PIECE_L,
        }
    }
}
mod piece_multiset_compiler {
    use crate::problem::{
        CPieceMultisetWindow, CPieceWindowDescriptor, CQueueView, C_PIECE_L,
        C_PIECE_MULTISET_WINDOW_CAPACITY,
    };

    pub(super) fn piece_multiset_window_from_queue(
        queue: &CQueueView,
        piece_window: &CPieceWindowDescriptor,
    ) -> CPieceMultisetWindow {
        let mut window = CPieceMultisetWindow::default();
        let count = usize::from(queue.stored_len).min(C_PIECE_MULTISET_WINDOW_CAPACITY);
        window.total_count = count as u8;
        window.exact_count = if piece_window.has_exact_pieces == 1
            && usize::from(piece_window.exact_pieces) <= count
        {
            piece_window.exact_pieces as u8
        } else {
            0
        };
        for index in 0..count {
            let piece = queue.pieces[index];
            if piece <= C_PIECE_L {
                window.counts[usize::from(piece)] =
                    window.counts[usize::from(piece)].saturating_add(1);
            }
        }
        window
    }
}
mod piece_window_compiler {
    use clearra_problem::SearchProblem;

    use crate::problem::{CPieceWindowDescriptor, FfiProblemError};

    pub(super) fn piece_window_descriptor(
        problem: &SearchProblem,
    ) -> Result<CPieceWindowDescriptor, FfiProblemError> {
        let max_pieces = to_u16(problem.piece_window().max_pieces(), |value| {
            FfiProblemError::PieceWindowTooLarge { max_pieces: value }
        })?;
        let exact_pieces = match problem.exact_pieces() {
            Some(value) => Some(to_u16(value, |exact_pieces| {
                FfiProblemError::ExactPiecesTooLarge { exact_pieces }
            })?),
            None => None,
        };
        Ok(CPieceWindowDescriptor {
            max_pieces,
            exact_pieces: exact_pieces.unwrap_or(0),
            has_exact_pieces: exact_pieces.is_some() as u8,
            reserved: [0; 3],
        })
    }

    fn to_u16(
        value: usize,
        error: impl FnOnce(usize) -> FfiProblemError,
    ) -> Result<u16, FfiProblemError> {
        u16::try_from(value).map_err(|_| error(value))
    }
}
mod queue_view_compiler {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::PcQueueInput;
    use clearra_problem::SearchProblem;

    use crate::problem::{
        CQueueView, FfiProblemError, C_PIECE_NONE, C_QUEUE_BAG_ALIGNED_PATTERN,
        C_QUEUE_FIXED_SEQUENCE, C_QUEUE_OBSERVED, C_QUEUE_VIEW_CAPACITY,
        C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN, C_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED,
    };

    use super::piece_code::piece_code;

    pub(super) fn queue_view_for_problem(
        problem: &SearchProblem,
    ) -> Result<CQueueView, FfiProblemError> {
        let view = queue_view(problem.supply().queue())?;
        reject_truncated_required_window(problem, &view)?;
        Ok(view)
    }

    fn queue_view(queue: &PcQueueInput) -> Result<CQueueView, FfiProblemError> {
        if let PcQueueInput::PatternExpression(expression) = queue {
            let sequence = expression.first_sequence();
            return queue_view_from_parts(
                C_QUEUE_BAG_ALIGNED_PATTERN,
                C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
                sequence.as_ref(),
            );
        }
        let (mode, provenance_id, pieces): (u8, u32, &[PieceKind]) = match queue {
            PcQueueInput::FixedSequence(sequence) => (
                C_QUEUE_FIXED_SEQUENCE,
                C_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
                sequence.pieces(),
            ),
            PcQueueInput::BagAlignedPattern(pattern) => (
                C_QUEUE_BAG_ALIGNED_PATTERN,
                C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
                pattern.pieces(),
            ),
            PcQueueInput::PatternExpression(_) => unreachable!("handled above"),
            PcQueueInput::Standard7Bag => (
                C_QUEUE_BAG_ALIGNED_PATTERN,
                C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
                &PieceKind::STANDARD_TETROMINOES,
            ),
            PcQueueInput::Observed(queue) => (
                C_QUEUE_OBSERVED,
                C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED,
                queue.pieces(),
            ),
        };
        queue_view_from_parts(mode, provenance_id, pieces)
    }

    fn queue_view_from_parts(
        mode: u8,
        provenance_id: u32,
        pieces: &[PieceKind],
    ) -> Result<CQueueView, FfiProblemError> {
        let len = u16::try_from(pieces.len())
            .map_err(|_| FfiProblemError::QueueTooLong { len: pieces.len() })?;
        let stored_len = pieces.len().min(C_QUEUE_VIEW_CAPACITY);
        let mut stored = [C_PIECE_NONE; C_QUEUE_VIEW_CAPACITY];
        for (index, piece) in pieces.iter().take(C_QUEUE_VIEW_CAPACITY).enumerate() {
            stored[index] = piece_code(*piece);
        }
        Ok(CQueueView {
            mode,
            truncated: (pieces.len() > C_QUEUE_VIEW_CAPACITY) as u8,
            len,
            stored_len: stored_len as u16,
            reserved: 0,
            provenance_id,
            pieces: stored,
        })
    }

    fn reject_truncated_required_window(
        problem: &SearchProblem,
        view: &CQueueView,
    ) -> Result<(), FfiProblemError> {
        if view.truncated == 0 {
            return Ok(());
        }
        let required_pieces = problem
            .exact_pieces()
            .unwrap_or_else(|| problem.piece_window().max_pieces());
        if required_pieces <= usize::from(view.stored_len) {
            return Ok(());
        }
        Err(FfiProblemError::QueueTruncatedButExactNeeded {
            len: usize::from(view.len),
            stored_len: usize::from(view.stored_len),
            required_pieces,
        })
    }
}

pub use compact_supply_descriptors::CompactSupplyDescriptors;
pub use compiler::SupplyDescriptorCompiler;
pub(crate) use piece_code::piece_code;

#[cfg(test)]
use crate::problem::{
    FfiProblemError, C_PIECE_I, C_PIECE_O, C_PIECE_T, C_QUEUE_BAG_ALIGNED_PATTERN,
    C_QUEUE_FIXED_SEQUENCE, C_QUEUE_OBSERVED, C_QUEUE_VIEW_CAPACITY,
    C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN, C_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
    C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED,
};
#[cfg(test)]
#[path = "supply_descriptor_compiler_tests.rs"]
mod supply_descriptor_compiler_tests;
