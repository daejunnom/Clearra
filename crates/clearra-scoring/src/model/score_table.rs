use clearra_spin::SpinAwardClass;

use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent},
    profile::{B2BPolicy, ScoreModelId},
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
    GuidelineByLines {
        bonuses: [u64; 5],
        back_to_back_tetris_bonus: u64,
    },
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
        self.apply_perfect_clear(self.line_score(clear.lines()), clear, false)
    }
}
impl ScoreModelTable {
    #[cfg(test)]
    pub(crate) fn score_event(self, event: ScoreEvent) -> u64 {
        self.apply_perfect_clear(self.action_score(event), event.clear(), event.b2b_before())
    }

    pub(crate) fn score_event_with_b2b(self, event: ScoreEvent, b2b_policy: B2BPolicy) -> u64 {
        let action_score = self.action_score(event);
        let applies_b2b = b2b_policy.enabled() && event.b2b_before() && event.is_difficult_clear();

        match self.perfect_clear_score {
            PerfectClearScore::Additive(_) | PerfectClearScore::GuidelineByLines { .. } => {
                let adjusted_action = if applies_b2b {
                    b2b_policy.adjusted_score(action_score)
                } else {
                    action_score
                };
                self.apply_perfect_clear(adjusted_action, event.clear(), event.b2b_before())
            }
            _ => {
                let event_score =
                    self.apply_perfect_clear(action_score, event.clear(), event.b2b_before());
                if applies_b2b {
                    b2b_policy.adjusted_score(event_score)
                } else {
                    event_score
                }
            }
        }
    }

    fn action_score(self, event: ScoreEvent) -> u64 {
        event.spin().map_or_else(
            || self.line_score(event.clear().lines()),
            |spin| {
                if spin.is_mini() {
                    self.t_spin_mini_score(spin.lines())
                } else {
                    self.t_spin_score(spin.lines())
                }
            },
        )
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
    fn apply_perfect_clear(
        self,
        action_score: u64,
        clear: ClearEvent,
        back_to_back_before: bool,
    ) -> u64 {
        if !clear.is_perfect_clear() {
            return action_score;
        }
        match self.perfect_clear_score {
            PerfectClearScore::Additive(score) => action_score.saturating_add(score),
            PerfectClearScore::GuidelineByLines {
                bonuses,
                back_to_back_tetris_bonus,
            } => {
                let bonus = if clear.lines() == 4 && back_to_back_before {
                    back_to_back_tetris_bonus
                } else {
                    bonuses[line_index(clear.lines())]
                };
                action_score.saturating_add(bonus)
            }
            PerfectClearScore::ReplaceAction(score) => score,
        }
    }
}

fn line_index(lines: u8) -> usize {
    usize::from(lines.min(4))
}

pub(crate) struct GuidelineScoreTable;

impl GuidelineScoreTable {
    #[allow(dead_code)]
    pub(crate) const SOURCE_NOTE: &'static str =
        "Recent Guideline-compatible level-1 scoring: line clears 100/300/500/800; PC bonuses 800/1200/1800/2000 and 3200 for a B2B Tetris PC";

    pub(crate) const TABLE: ScoreModelTable = ScoreModelTable {
        line_clear_scores: [0, 100, 300, 500, 800],
        t_spin_scores: [400, 800, 1200, 1600, 1600],
        t_spin_mini_scores: [100, 200, 400, 800, 800],
        perfect_clear_score: PerfectClearScore::GuidelineByLines {
            bonuses: [0, 800, 1200, 1800, 2000],
            back_to_back_tetris_bonus: 3200,
        },
    };
}

pub(crate) struct JstrisUltraScoreTable;

impl JstrisUltraScoreTable {
    #[allow(dead_code)]
    pub(crate) const SOURCE_NOTE: &'static str =
        "Jstris Ultra scoring: Guideline-compatible action scores, +3000 PC, 1.5x B2B, 50x combo, and Mini T-Spin Double scored as T-Spin Double";

    pub(crate) const TABLE: ScoreModelTable = ScoreModelTable {
        line_clear_scores: [0, 100, 300, 500, 800],
        t_spin_scores: [400, 800, 1200, 1600, 1600],
        t_spin_mini_scores: [100, 200, 1200, 1600, 1600],
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
