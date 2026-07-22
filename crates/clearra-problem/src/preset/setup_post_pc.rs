use crate::{
    preset::SetupPreset,
    query::{ScenarioQuery, SetupSearchQuery},
};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupPostPcPreset {
    setup: SetupPreset,
}

impl SetupPostPcPreset {
    pub fn from_query(query: SetupSearchQuery) -> Self {
        Self {
            setup: SetupPreset::from_query(query),
        }
    }
}
impl SetupPostPcPreset {
    pub fn query(&self) -> &SetupSearchQuery {
        self.setup.query()
    }
}
impl SetupPostPcPreset {
    pub fn into_scenario_query(self) -> ScenarioQuery {
        self.setup.into_scenario_query()
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;
    use crate::query::{SetupHoldPolicy, SetupQueueInput};

    #[test]
    fn setup_post_pc_preset_reuses_scenario_preset_lowering() {
        let setup = SetupSearchQuery::default()
            .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
            ])))
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T));
        let scenario = SetupPostPcPreset::from_query(setup).into_scenario_query();

        assert_eq!(scenario.source().as_str(), "setup-preset");
        assert_eq!(scenario.goal().as_str(), "clear-to-empty");
        assert_eq!(
            scenario.core_query().hold_state().piece(),
            Some(PieceKind::T)
        );
    }
}
