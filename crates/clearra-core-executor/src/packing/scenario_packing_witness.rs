#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScenarioPackingWitness {
    pub(crate) solution_found: bool,
    pub(crate) cleared_lines: u8,
    pub(crate) total_solution_count: usize,
    pub(crate) unique_solution_count: usize,
    pub(crate) queue_consumed: usize,
    pub(crate) placed_piece_count: usize,
}

impl ScenarioPackingWitness {
    pub(crate) fn no_solution() -> Self {
        Self {
            solution_found: false,
            cleared_lines: 0,
            total_solution_count: 0,
            unique_solution_count: 0,
            queue_consumed: 0,
            placed_piece_count: 0,
        }
    }

    pub(crate) fn solved_with_unique(
        cleared_lines: u8,
        total_solution_count: usize,
        unique_solution_count: usize,
        queue_consumed: usize,
    ) -> Self {
        Self {
            solution_found: true,
            cleared_lines,
            total_solution_count,
            unique_solution_count,
            queue_consumed,
            placed_piece_count: queue_consumed,
        }
    }

    #[cfg(test)]
    pub(crate) fn solved(
        cleared_lines: u8,
        total_solution_count: usize,
        queue_consumed: usize,
    ) -> Self {
        Self::solved_with_unique(
            cleared_lines,
            total_solution_count,
            total_solution_count,
            queue_consumed,
        )
    }
}
