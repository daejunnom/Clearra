use crate::{
    compile::compile_error::ProblemCompileError,
    search_problem::{SearchProblem, SearchProblemPreset},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingProblemKind {
    OpeningPc,
    ScenarioPc,
    Setup,
    Build,
}

impl PackingProblemKind {
    pub fn as_u32(self) -> u32 {
        match self {
            Self::OpeningPc => 1,
            Self::ScenarioPc => 2,
            Self::Setup => 3,
            Self::Build => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingProblemSpec {
    kind: PackingProblemKind,
    max_pieces: u16,
    flags: u16,
}

impl PackingProblemSpec {
    pub fn new(kind: PackingProblemKind, max_pieces: u16, flags: u16) -> Self {
        Self {
            kind,
            max_pieces,
            flags,
        }
    }
}
impl PackingProblemSpec {
    pub fn kind(self) -> PackingProblemKind {
        self.kind
    }
}
impl PackingProblemSpec {
    pub fn max_pieces(self) -> u16 {
        self.max_pieces
    }
}
impl PackingProblemSpec {
    pub fn flags(self) -> u16 {
        self.flags
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackingProblemCompiler;

impl PackingProblemCompiler {
    pub fn compile(problem: &SearchProblem) -> Result<PackingProblemSpec, ProblemCompileError> {
        let max_pieces = u16::try_from(problem.piece_window().max_pieces()).map_err(|_| {
            ProblemCompileError::PackingPieceWindowTooLarge {
                max_pieces: problem.piece_window().max_pieces(),
            }
        })?;

        let kind = match problem.preset() {
            SearchProblemPreset::OpeningPc => PackingProblemKind::OpeningPc,
            SearchProblemPreset::ScenarioPc => PackingProblemKind::ScenarioPc,
            SearchProblemPreset::Setup => PackingProblemKind::Setup,
            SearchProblemPreset::Build => PackingProblemKind::Build,
        };

        Ok(PackingProblemSpec::new(kind, max_pieces, 0))
    }
}

#[cfg(test)]
#[path = "packing_problem_compiler_tests.rs"]
mod tests;
