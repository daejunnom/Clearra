use clearra_spin::SpinAwardClass;

use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent},
    profile::ScoreModelId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScoreModelTable {
    line_clear_scores: [u64; 5],
    t_spin_scores: [u64; 5],
    t_spin_mini_scores: [u64; 5],
    perfect_clear_score: PerfectClearScore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerfectClearScore {
    Additive(u64),
    ReplaceAction(u64),
}

impl ScoreModelTable {
    pub(crate) fn for_model(model: ScoreModelId) -> Option<Self> {
        match model {
            ScoreModelId::Disabled => None,
            ScoreModelId::Guideline => Some(GuidelineScoreTable::TABLE),
            ScoreModelId::JstrisUltra => Some(JstrisUltraScoreTable::TABLE),
            ScoreModelId::Tetrio => Some(TetrioScoreTable::TABLE),
        }
    }
}
impl ScoreModelTable {
    pub(crate) fn score_clear(self, clear: ClearEvent) -> u64 {
        self.apply_perfect_clear(self.line_score(clear.lines()), clear)
    }
}
impl ScoreModelTable {
    pub(crate) fn score_event(self, event: ScoreEvent) -> u64 {
        let base = event.spin().map_or_else(
            || self.line_score(event.clear().lines()),
            |spin| {
                if spin.is_mini() {
                    self.t_spin_mini_score(spin.lines())
                } else {
                    self.t_spin_score(spin.lines())
                }
            },
        );
        self.apply_perfect_clear(base, event.clear())
    }
}
impl ScoreModelTable {
    pub(crate) fn score_award_class(self, award_class: SpinAwardClass, lines: u8) -> u64 {
        match award_class {
            SpinAwardClass::None => self.line_score(lines),
            SpinAwardClass::Mini | SpinAwardClass::AllMini => self.t_spin_mini_score(lines),
            SpinAwardClass::Regular | SpinAwardClass::AllSpin | SpinAwardClass::Special => {
                self.t_spin_score(lines)
            }
            SpinAwardClass::Unknown => 0,
        }
    }
}
impl ScoreModelTable {
    fn line_score(self, lines: u8) -> u64 {
        self.line_clear_scores[line_index(lines)]
    }
}
impl ScoreModelTable {
    fn t_spin_score(self, lines: u8) -> u64 {
        self.t_spin_scores[line_index(lines)]
    }
}
impl ScoreModelTable {
    fn t_spin_mini_score(self, lines: u8) -> u64 {
        self.t_spin_mini_scores[line_index(lines)]
    }
}
impl ScoreModelTable {
    fn apply_perfect_clear(self, action_score: u64, clear: ClearEvent) -> u64 {
        if !clear.is_perfect_clear() {
            return action_score;
        }
        match self.perfect_clear_score {
            PerfectClearScore::Additive(score) => action_score.saturating_add(score),
            PerfectClearScore::ReplaceAction(score) => score,
        }
    }
}

fn line_index(lines: u8) -> usize {
    usize::from(lines.min(4))
}

pub(crate) struct GuidelineScoreTable;

impl GuidelineScoreTable {
    pub(crate) const TABLE: ScoreModelTable = ScoreModelTable {
        line_clear_scores: [0, 100, 300, 500, 800],
        t_spin_scores: [400, 800, 1200, 1600, 1600],
        t_spin_mini_scores: [100, 200, 400, 800, 800],
        perfect_clear_score: PerfectClearScore::Additive(3500),
    };
}

pub(crate) struct JstrisUltraScoreTable;

impl JstrisUltraScoreTable {
    pub(crate) const TABLE: ScoreModelTable = ScoreModelTable {
        line_clear_scores: [0, 100, 300, 500, 800],
        t_spin_scores: [400, 800, 1200, 1600, 1600],
        t_spin_mini_scores: [100, 200, 400, 800, 800],
        perfect_clear_score: PerfectClearScore::Additive(3000),
    };
}

pub(crate) struct TetrioScoreTable;

impl TetrioScoreTable {
    #[allow(dead_code)]
    pub(crate) const SOURCE_NOTE: &'static str =
        "TETR.IO score table: quad=800, all clear=3500; drop score is handled by DropScorePolicy::HardDrop2SoftDrop1";

    pub(crate) const TABLE: ScoreModelTable = ScoreModelTable {
        line_clear_scores: [0, 100, 300, 500, 800],
        t_spin_scores: [400, 800, 1200, 1600, 2600],
        t_spin_mini_scores: [100, 200, 400, 800, 1600],
        perfect_clear_score: PerfectClearScore::ReplaceAction(3500),
    };
}

#[cfg(test)]
#[path = "score_table_tests.rs"]
mod tests;
