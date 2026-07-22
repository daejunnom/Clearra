use clearra_supply::PatternUniverseMaterializationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProblemCompileError {
    UnsupportedOpeningPreset {
        lines: u8,
    },
    UnsupportedPreset,
    PackingPieceWindowTooLarge {
        max_pieces: usize,
    },
    SupplyWindowTooShort {
        source_pieces: usize,
        required_source_pieces: usize,
    },
    SupplyWindowConflictsWithConcreteQueue {
        source_pieces: usize,
        queue_pieces: usize,
    },
    SupplyWindowShorterThanObservedQueue {
        source_pieces: usize,
        observed_pieces: usize,
    },
    PatternUniverseMaterialization(PatternUniverseMaterializationError),
    ProfileSpecificSpinTargetRequiresScoreProfile,
}
