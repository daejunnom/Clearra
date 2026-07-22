use clearra_coverage::pattern::pattern_id::PatternId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedScoreCell {
    candidate_id: usize,
    pattern_id: PatternId,
    trace_identity: String,
    score: u64,
    attack: u32,
    accuracy_level: String,
}

impl MaterializedScoreCell {
    pub fn new(
        candidate_id: usize,
        pattern_id: PatternId,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
        accuracy_level: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
            accuracy_level: accuracy_level.into(),
        }
    }

    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }

    pub fn pattern_id(&self) -> PatternId {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn score(&self) -> u64 {
        self.score
    }

    pub fn attack(&self) -> u32 {
        self.attack
    }

    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedScoreMatrix {
    pattern_count: usize,
    cells: Vec<MaterializedScoreCell>,
    profile_id: String,
    accuracy_level: String,
    complete: bool,
}

impl MaterializedScoreMatrix {
    pub fn new(
        pattern_count: usize,
        cells: Vec<MaterializedScoreCell>,
        profile_id: impl Into<String>,
        accuracy_level: impl Into<String>,
        complete: bool,
    ) -> Self {
        Self {
            pattern_count,
            cells,
            profile_id: profile_id.into(),
            accuracy_level: accuracy_level.into(),
            complete,
        }
    }

    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn cells(&self) -> &[MaterializedScoreCell] {
        &self.cells
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }

    pub fn complete(&self) -> bool {
        self.complete
    }
}
