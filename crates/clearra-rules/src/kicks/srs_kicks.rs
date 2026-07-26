use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::profile::rule_profile::RuleProfileId;

use super::kick_table::{
    KickOffset, KickOffsetSequence, KickTable, KickTableEntry, KickTableProfile,
    KickTableProfileId, KickTransition,
};
use super::srs_offsets::{
    jlstz_offsets, jstris_180_offsets, srs_i_offsets, srs_plus_i_180_offsets, srs_plus_i_offsets,
    srs_plus_jlstz_180_offsets, srs_x_i_180_offsets,
};

pub use super::srs_offsets::{eight_direction_transitions, one_eighty_transitions};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SrsKicks;

impl SrsKicks {
    pub fn basic_table() -> KickTable {
        KickTable::new(vec![
            KickOffset::new(0, 0),
            KickOffset::new(1, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(0, 1),
        ])
    }
}
impl SrsKicks {
    pub fn offsets(piece: PieceKind, from: RotationState, to: RotationState) -> KickTable {
        KickTable::from_sequence(Self::sequence(KickTransition::new(piece, from, to)))
    }
}
impl SrsKicks {
    pub fn sequence(transition: KickTransition) -> KickOffsetSequence {
        if transition.piece() == PieceKind::O {
            return KickOffsetSequence::no_kick();
        }

        if transition.piece() == PieceKind::I {
            return KickOffsetSequence::new(srs_i_offsets(transition.from(), transition.to()));
        }

        KickOffsetSequence::new(jlstz_offsets(transition.from(), transition.to()))
    }
}
impl SrsKicks {
    pub fn profile() -> KickTableProfile {
        KickTableProfile::new(
            KickTableProfileId::Srs90,
            RuleProfileId::Srs,
            srs_profile_entries(RuleProfileId::Srs),
        )
    }
}
impl SrsKicks {
    pub fn srs_plus_profile() -> KickTableProfile {
        KickTableProfile::new(
            KickTableProfileId::SrsPlus,
            RuleProfileId::SrsPlus,
            srs_plus_profile_entries(),
        )
    }
}
impl SrsKicks {
    pub fn srs_x_profile() -> KickTableProfile {
        KickTableProfile::new(
            KickTableProfileId::SrsX,
            RuleProfileId::SrsX,
            srs_x_profile_entries(),
        )
    }
}
impl SrsKicks {
    pub fn jstris_180_profile() -> KickTableProfile {
        KickTableProfile::new(
            KickTableProfileId::Jstris180,
            RuleProfileId::Jstris180,
            jstris_180_profile_entries(),
        )
    }
}

fn srs_profile_entries(_source_rule: RuleProfileId) -> Vec<KickTableEntry> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .flat_map(|piece| {
            eight_direction_transitions()
                .into_iter()
                .map(move |(from, to)| {
                    let transition = KickTransition::new(piece, from, to);
                    KickTableEntry::new(transition, SrsKicks::sequence(transition))
                })
        })
        .collect()
}

fn srs_plus_profile_entries() -> Vec<KickTableEntry> {
    let mut entries = PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .flat_map(|piece| {
            eight_direction_transitions()
                .into_iter()
                .map(move |(from, to)| {
                    let transition = KickTransition::new(piece, from, to);
                    let sequence = if piece == PieceKind::O {
                        KickOffsetSequence::no_kick()
                    } else if piece == PieceKind::I {
                        KickOffsetSequence::new(srs_plus_i_offsets(from, to))
                    } else {
                        KickOffsetSequence::new(jlstz_offsets(from, to))
                    };
                    KickTableEntry::new(transition, sequence)
                })
        })
        .collect::<Vec<_>>();
    entries.extend(
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .filter(|piece| *piece != PieceKind::O)
            .flat_map(|piece| {
                one_eighty_transitions().into_iter().map(move |(from, to)| {
                    KickTableEntry::new(
                        KickTransition::new(piece, from, to),
                        if piece == PieceKind::I {
                            KickOffsetSequence::new(srs_plus_i_180_offsets(from, to))
                        } else {
                            KickOffsetSequence::new(srs_plus_jlstz_180_offsets(from, to))
                        },
                    )
                })
            }),
    );
    entries
}

fn srs_x_profile_entries() -> Vec<KickTableEntry> {
    let mut entries = srs_profile_entries(RuleProfileId::SrsX);
    entries.extend(
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .filter(|piece| *piece != PieceKind::O)
            .flat_map(|piece| {
                one_eighty_transitions().into_iter().map(move |(from, to)| {
                    KickTableEntry::new(
                        KickTransition::new(piece, from, to),
                        if piece == PieceKind::I {
                            KickOffsetSequence::new(srs_x_i_180_offsets(from, to))
                        } else {
                            KickOffsetSequence::new(srs_plus_jlstz_180_offsets(from, to))
                        },
                    )
                })
            }),
    );
    entries
}

fn jstris_180_profile_entries() -> Vec<KickTableEntry> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .filter(|piece| *piece != PieceKind::O)
        .flat_map(|piece| {
            let quarter_turns = eight_direction_transitions()
                .into_iter()
                .map(move |(from, to)| {
                    let transition = KickTransition::new(piece, from, to);
                    KickTableEntry::new(transition, SrsKicks::sequence(transition))
                });
            let half_turns = one_eighty_transitions().into_iter().map(move |(from, to)| {
                KickTableEntry::new(
                    KickTransition::new(piece, from, to),
                    KickOffsetSequence::new(jstris_180_offsets(from, to)),
                )
            });
            quarter_turns.chain(half_turns)
        })
        .collect()
}

#[cfg(test)]
#[path = "srs_kicks_tests.rs"]
mod tests;
