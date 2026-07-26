use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

use crate::problem::{
    C_BAG_STANDARD_7_BAG, C_KICK_ARS, C_KICK_ASC, C_KICK_CUSTOM, C_KICK_IMPORTED,
    C_KICK_JSTRIS_180, C_KICK_NO_KICK, C_KICK_SRS_90, C_KICK_SRS_PLUS_180, C_KICK_SRS_X, C_PIECE_I,
    C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_SET_STANDARD_TETROMINOES, C_PIECE_T,
    C_PIECE_Z, C_RULE_ARS, C_RULE_ASC, C_RULE_CUSTOM, C_RULE_JSTRIS_180, C_RULE_NO_KICK,
    C_RULE_SRS, C_RULE_SRS_PLUS, C_RULE_SRS_X, C_SPAWN_ARIKA, C_SPAWN_CUSTOM, C_SPAWN_STANDARD_10,
};

pub(crate) fn piece_code(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => C_PIECE_I,
        PieceKind::O => C_PIECE_O,
        PieceKind::T => C_PIECE_T,
        PieceKind::S => C_PIECE_S,
        PieceKind::Z => C_PIECE_Z,
        PieceKind::J => C_PIECE_J,
        PieceKind::L => C_PIECE_L,
    }
}

pub(crate) fn rotation_code(rotation: RotationState) -> u8 {
    rotation.quarter_turns()
}

pub fn rule_profile_code(id: RuleProfileId) -> u32 {
    match id {
        RuleProfileId::SrsPlus => C_RULE_SRS_PLUS,
        RuleProfileId::Srs => C_RULE_SRS,
        RuleProfileId::SrsX => C_RULE_SRS_X,
        RuleProfileId::Jstris180 => C_RULE_JSTRIS_180,
        RuleProfileId::Asc => C_RULE_ASC,
        RuleProfileId::Ars => C_RULE_ARS,
        RuleProfileId::NoKick => C_RULE_NO_KICK,
        RuleProfileId::Custom => C_RULE_CUSTOM,
    }
}

pub fn kick_profile_code(id: KickTableProfileId) -> u32 {
    match id {
        KickTableProfileId::Srs90 => C_KICK_SRS_90,
        KickTableProfileId::NoKick => C_KICK_NO_KICK,
        KickTableProfileId::SrsPlus => C_KICK_SRS_PLUS_180,
        KickTableProfileId::SrsX => C_KICK_SRS_X,
        KickTableProfileId::Jstris180 => C_KICK_JSTRIS_180,
        KickTableProfileId::Asc => C_KICK_ASC,
        KickTableProfileId::Ars => C_KICK_ARS,
        KickTableProfileId::Imported => C_KICK_IMPORTED,
        KickTableProfileId::Custom => C_KICK_CUSTOM,
    }
}

pub(crate) fn piece_set_profile_code(id: &str) -> u32 {
    match id {
        "standard-tetrominoes" => C_PIECE_SET_STANDARD_TETROMINOES,
        _ => 0,
    }
}

pub(crate) fn bag_profile_code(id: &str) -> u32 {
    match id {
        "standard-7-bag" => C_BAG_STANDARD_7_BAG,
        _ => 0,
    }
}

pub(crate) fn spawn_profile_code(id: &str) -> u32 {
    match id {
        "standard-10-spawn" => C_SPAWN_STANDARD_10,
        "arika-spawn" => C_SPAWN_ARIKA,
        _ => C_SPAWN_CUSTOM,
    }
}
