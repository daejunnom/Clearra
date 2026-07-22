use clearra_core_ffi::{CBuildUpProblem, CBuildUpProblemBuilder, FfiProblemError};
use clearra_problem::SearchProblem;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildUpProblemLoweringError {
    Ffi(FfiProblemError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpProblemLowering;

impl BuildUpProblemLowering {
    pub fn lower(problem: &SearchProblem) -> Result<CBuildUpProblem, BuildUpProblemLoweringError> {
        CBuildUpProblemBuilder::from_search_problem(problem)
            .map_err(BuildUpProblemLoweringError::Ffi)
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
    use clearra_core_ffi::problem::C_BUILDUP_FLAG_HOLD_ENABLED;
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcQueueInput};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;

    #[test]
    fn build_up_problem_owns_piece_source_ref_and_hold_automaton() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
            ])),
        );
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

        let buildup = BuildUpProblemLowering::lower(&problem).expect("buildup");

        assert_eq!(
            buildup.piece_source.piece_source_id,
            buildup.packing.piece_source.piece_source_id
        );
        assert_eq!(
            buildup.initial_hold_automaton.piece_source_id,
            buildup.piece_source.piece_source_id
        );
        assert_ne!(buildup.piece_source.provenance_id, 0);
        assert_ne!(buildup.initial_hold_automaton.provenance_id, 0);
        assert_eq!(
            buildup.buildup_flags & C_BUILDUP_FLAG_HOLD_ENABLED,
            C_BUILDUP_FLAG_HOLD_ENABLED
        );
    }
}
