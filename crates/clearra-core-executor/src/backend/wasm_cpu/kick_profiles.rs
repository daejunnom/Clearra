use std::sync::OnceLock;

use clearra_problem::SearchProblem;
use clearra_rules::{
    kicks::{KickTableProfile, KickTableProfileId, NoKick, SrsKicks},
    profile::replay_encoding::{replay_kick_profile_id, replay_rule_profile_id},
};

pub(super) fn builtin_kick_profile(
    profile_id: KickTableProfileId,
) -> Option<&'static KickTableProfile> {
    match profile_id {
        KickTableProfileId::Srs90 => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::profile))
        }
        KickTableProfileId::SrsPlus => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::srs_plus_profile))
        }
        KickTableProfileId::SrsX => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::srs_x_profile))
        }
        KickTableProfileId::Jstris180 => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(SrsKicks::jstris_180_profile))
        }
        KickTableProfileId::NoKick => {
            static PROFILE: OnceLock<KickTableProfile> = OnceLock::new();
            Some(PROFILE.get_or_init(NoKick::profile))
        }
        KickTableProfileId::Asc
        | KickTableProfileId::Ars
        | KickTableProfileId::Imported
        | KickTableProfileId::Custom => None,
    }
}

pub(super) fn replay_profile_ids(problem: &SearchProblem) -> (u64, u64) {
    let profile = problem.kick_profile();
    (
        replay_kick_profile_id(profile.profile_id()),
        replay_rule_profile_id(profile.source_rule()),
    )
}
