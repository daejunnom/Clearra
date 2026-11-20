//! Stable profile numbers for persisted replay and scoring evidence.
//!
//! These are not enum ordinals, worker-message opcodes, or a definition of
//! the C ABI. They match the originally emitted C descriptor numbers for
//! compatibility with existing evidence, without linking Rust producers to
//! the descriptor adapter.

use crate::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

pub const fn replay_rule_profile_id(id: RuleProfileId) -> u64 {
    match id {
        RuleProfileId::SrsPlus => 1,
        RuleProfileId::Srs => 2,
        RuleProfileId::SrsX => 3,
        RuleProfileId::Asc => 4,
        RuleProfileId::Ars => 5,
        RuleProfileId::NoKick => 6,
        RuleProfileId::Jstris180 => 7,
        RuleProfileId::Custom => 255,
    }
}

pub const fn replay_kick_profile_id(id: KickTableProfileId) -> u64 {
    match id {
        KickTableProfileId::Srs90 => 1,
        KickTableProfileId::NoKick => 2,
        KickTableProfileId::SrsPlus => 3,
        KickTableProfileId::SrsX => 4,
        KickTableProfileId::Asc => 5,
        KickTableProfileId::Ars => 6,
        KickTableProfileId::Imported => 7,
        KickTableProfileId::Jstris180 => 8,
        KickTableProfileId::Custom => 255,
    }
}
