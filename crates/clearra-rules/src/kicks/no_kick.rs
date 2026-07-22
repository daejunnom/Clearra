use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::profile::rule_profile::RuleProfileId;

use super::{
    kick_table::{
        KickOffsetSequence, KickTable, KickTableEntry, KickTableProfile, KickTableProfileId,
        KickTransition,
    },
    srs_kicks::eight_direction_transitions,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoKick;

impl NoKick {
    pub fn table() -> KickTable {
        KickTable::no_kick()
    }
}
impl NoKick {
    pub fn profile() -> KickTableProfile {
        KickTableProfile::new(
            KickTableProfileId::NoKick,
            RuleProfileId::NoKick,
            PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .flat_map(|piece| {
                    eight_direction_transitions()
                        .into_iter()
                        .map(move |(from, to)| {
                            KickTableEntry::new(
                                KickTransition::new(piece, from, to),
                                KickOffsetSequence::no_kick(),
                            )
                        })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    use super::*;

    #[test]
    fn no_kick_profile_exposes_no_kick_sequence_for_every_standard_transition() {
        let profile = NoKick::profile();

        assert_eq!(profile.id(), KickTableProfileId::NoKick);
        assert_eq!(profile.source_rule(), RuleProfileId::NoKick);
        assert_eq!(
            profile.entries().len(),
            PieceKind::STANDARD_TETROMINOES.len() * eight_direction_transitions().len()
        );
        assert_eq!(
            profile
                .sequence_for(KickTransition::new(
                    PieceKind::T,
                    RotationState::Zero,
                    RotationState::Right
                ))
                .expect("T 0->R")
                .offsets(),
            KickOffsetSequence::no_kick().offsets()
        );
    }
}
