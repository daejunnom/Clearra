use std::sync::OnceLock;

use clearra_core_ffi::rules::{kick_profile_code, rule_profile_code};
use clearra_problem::SearchProblem;
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId, NoKick, SrsKicks};

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
        u64::from(kick_profile_code(profile.profile_id())),
        u64::from(rule_profile_code(profile.source_rule())),
    )
}
