use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    continuation_token_v1::{parse_opening_v1, parse_scenario_v1},
    opening_continuation_token::{encode_opening_continuation, parse_opening_v2},
    pc_scenario_query::PcScenarioQuery,
    scenario_continuation_token::{
        encode_scenario_continuation, encode_scenario_replay, parse_scenario_continuation_v2,
        parse_scenario_replay_v2,
    },
    OpeningPcSearchQuery,
};

pub use super::continuation_token_error::PcContinuationTokenError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcContinuationToken {
    Opening(OpeningPcSearchQuery),
    Scenario(PcScenarioQuery),
    ScenarioReplay(PcScenarioQuery),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcContinuationTokenCodec;

impl PcContinuationTokenCodec {
    pub fn encode_opening_continuation(
        query: &OpeningPcSearchQuery,
        hold_piece: Option<PieceKind>,
        pieces: &[PieceKind],
    ) -> String {
        encode_opening_continuation(query, hold_piece, pieces)
    }
}
impl PcContinuationTokenCodec {
    pub fn encode_scenario_continuation(
        query: &PcScenarioQuery,
    ) -> Result<String, PcContinuationTokenError> {
        encode_scenario_continuation(query)
    }
}
impl PcContinuationTokenCodec {
    pub fn encode_scenario_replay(
        query: &PcScenarioQuery,
    ) -> Result<String, PcContinuationTokenError> {
        encode_scenario_replay(query)
    }
}
impl PcContinuationTokenCodec {
    pub fn encode_scenario_query(
        query: &PcScenarioQuery,
    ) -> Result<String, PcContinuationTokenError> {
        Self::encode_scenario_continuation(query)
    }
}
impl PcContinuationTokenCodec {
    pub fn parse(token: &str) -> Result<PcContinuationToken, PcContinuationTokenError> {
        if token.starts_with("pc2:") {
            return parse_opening_v2(token).map(PcContinuationToken::Opening);
        }
        if token.starts_with("pc1:") {
            return parse_opening_v1(token).map(PcContinuationToken::Opening);
        }
        if token.starts_with("sc2:") {
            return parse_scenario_continuation_v2(token).map(PcContinuationToken::Scenario);
        }
        if token.starts_with("sr2:") {
            return parse_scenario_replay_v2(token).map(PcContinuationToken::ScenarioReplay);
        }
        if token.starts_with("sc1:") {
            return parse_scenario_v1(token).map(PcContinuationToken::ScenarioReplay);
        }
        Err(PcContinuationTokenError::new(
            "continuation token must start with pc2:, pc1:, sc2:, sr2:, or sc1:",
        ))
    }
}

#[cfg(test)]
#[path = "pc_continuation_token_tests.rs"]
mod tests;
