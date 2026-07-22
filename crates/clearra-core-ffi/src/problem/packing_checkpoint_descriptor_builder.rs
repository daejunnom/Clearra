use clearra_problem::{SearchProblem, SearchProblemPreset};

use super::{CCheckpointSpec, CPackingProblem};

pub(crate) fn problem_kind(preset: SearchProblemPreset) -> u32 {
    match preset {
        SearchProblemPreset::OpeningPc => CPackingProblem::OPENING_PC,
        SearchProblemPreset::ScenarioPc => CPackingProblem::SCENARIO_PC,
        SearchProblemPreset::Setup => CPackingProblem::SETUP,
        SearchProblemPreset::Build => CPackingProblem::BUILD,
    }
}

pub(crate) fn checkpoint_spec(problem: &SearchProblem) -> CCheckpointSpec {
    CCheckpointSpec {
        label_count: problem.labels().len().min(u16::MAX as usize) as u16,
        checkpoint_count: problem
            .checkpoint_schedule()
            .map(|schedule| schedule.checkpoint_count().min(u16::MAX as usize) as u16)
            .unwrap_or(0),
        partition_count: problem
            .checkpoint_schedule()
            .map(|schedule| schedule.partitions().len().min(u16::MAX as usize) as u16)
            .unwrap_or(0),
        reserved: 0,
    }
}
