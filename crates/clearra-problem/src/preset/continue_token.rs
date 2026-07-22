use clearra_pc_graph::request::{OpeningPcSearchQuery, PcContinuationToken, PcScenarioQuery};

#[derive(Clone, Debug, PartialEq)]
pub enum ContinuationPreset {
    Opening(OpeningPcSearchQuery),
    Scenario(PcScenarioQuery),
}

impl ContinuationPreset {
    pub fn from_token(token: &PcContinuationToken) -> Self {
        match token {
            PcContinuationToken::Opening(query) => Self::Opening(query.clone()),
            PcContinuationToken::Scenario(query) | PcContinuationToken::ScenarioReplay(query) => {
                Self::Scenario(query.clone())
            }
        }
    }
}
