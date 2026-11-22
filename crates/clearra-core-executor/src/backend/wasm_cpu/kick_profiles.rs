use std::{collections::BTreeMap, sync::OnceLock};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_problem::SearchProblem;
use clearra_rules::{
    kicks::{normalized_kick_delta, KickTableProfile, KickTableProfileId, NoKick, SrsKicks},
    profile::replay_encoding::{replay_kick_profile_id, replay_rule_profile_id},
};

/// Normalize in a wide coordinate domain. The existing zero-offset transform
/// supplies the shape-origin correction; widening precedes adding user data.
/// In particular, the raw i8 endpoints must never wrap during normalization.
pub(super) fn normalized_kick_delta_wide(
    piece: PieceKind,
    from: RotationState,
    to: RotationState,
    dx: i8,
    dy: i8,
) -> (i16, i16) {
    let (origin_x, origin_y) = normalized_kick_delta(piece, from, to, 0, 0);
    (
        i16::from(dx) + i16::from(origin_x),
        i16::from(dy) + i16::from(origin_y),
    )
}

/// A table-derived permission for the smaller H+D sky boundary. This token
/// cannot be granted by a profile tag. Universal movement and E2 pruning do
/// not require it; without it the compiler uses the guard-complete H+D+U.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReducedSkyCeiling {
    downward: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct KickSkyBounds {
    upward: i16,
    downward: i16,
    reduced: Option<ReducedSkyCeiling>,
}

impl KickSkyBounds {
    pub(super) fn for_ordered_deltas(ordered: [&[(i16, i16)]; 12]) -> Self {
        Self::for_ordered_sequences(|slot| ordered[slot].iter().copied())
    }

    /// Revisit immutable raw or normalized sequences without allocating an
    /// owned normalized copy. This lets resource admission use the exact same
    /// sky theorem before permitting any candidate-sized allocation.
    pub(super) fn for_ordered_sequences<I: Iterator<Item = (i16, i16)>>(
        mut sequence: impl FnMut(usize) -> I,
    ) -> Self {
        let mut upward = 0_i16;
        let mut downward = 0_i16;
        for slot in 0..12 {
            for (_, dy) in sequence(slot) {
                upward = upward.max(dy);
                downward = downward.max(-dy);
            }
        }
        // For every possible last sky source H+t, an out-of-bound earlier
        // candidate must be dominated by a retained, unconditionally free
        // sky candidate before the candidate that would enter below H.
        // Equal dx gives the same horizontal validity as that low candidate.
        let reduced = (upward <= downward
            && (0..12).all(|slot| reduced_sky_prefixes_qualified(sequence(slot), downward)))
        .then_some(ReducedSkyCeiling { downward });
        Self {
            upward,
            downward,
            reduced,
        }
    }

    /// Standard normalized cells have min y=0 and no negative offsets, so
    /// anchor y>=H is collision-free sky for every board supported below H.
    /// Last-entry sources are at most H-1+D (or H for a unit descent); all
    /// their ordered candidates lie below H+D+U. All-sky reachability to a
    /// below-H pose is therefore exact even if higher sky paths are omitted.
    pub(super) fn source_ceiling(self, height: u8) -> i16 {
        i16::from(height)
            + match self.reduced {
                Some(proof) => proof.downward,
                None => self.downward + self.upward,
            }
    }

    #[cfg(test)]
    pub(super) fn uses_reduced_ceiling(self) -> bool {
        self.reduced.is_some()
    }
}

/// Check sufficient condition B by reusing prefix facts instead of rescanning
/// every earlier kick. Under U<=D, each earlier vertical delta covers either
/// t in [0,D-dy] (dy>=0) or [-dy,D] (dy<=0). For one x, their union is two
/// end intervals. The only possible uncovered t values form one middle gap.
fn reduced_sky_prefixes_qualified(
    sequence: impl Iterator<Item = (i16, i16)>,
    downward: i16,
) -> bool {
    let mut maximum_prior_y = -downward;
    let mut sky_coverage_by_x = BTreeMap::<i16, (i16, i16)>::new();
    for (dx, dy) in sequence {
        // A low destination and some earlier clipped-above-ceiling candidate
        // are possible exactly on this interval of last-entry source heights.
        let first = 0.max(downward - maximum_prior_y + 1);
        let last = downward.min(-dy - 1);
        if first <= last {
            let &(left_end, right_start) =
                sky_coverage_by_x.get(&dx).unwrap_or(&(-1, downward + 1));
            if first.max(left_end + 1) <= last.min(right_start - 1) {
                return false;
            }
        }
        maximum_prior_y = maximum_prior_y.max(dy);
        let coverage = sky_coverage_by_x.entry(dx).or_insert((-1, downward + 1));
        if dy >= 0 {
            coverage.0 = coverage.0.max(downward - dy);
        }
        if dy <= 0 {
            coverage.1 = coverage.1.min(-dy);
        }
    }
    true
}

#[cfg(test)]
mod sky_capability_tests {
    use super::KickSkyBounds;

    #[test]
    fn reused_prefix_sky_intervals_equal_the_quantified_guard_condition() {
        let offsets: Vec<_> = (-1..=1)
            .flat_map(|x| (-2..=2).map(move |y| (x, y)))
            .collect();
        for &a in &offsets {
            for &b in &offsets {
                for &c in &offsets {
                    let sequence = [a, b, c];
                    let upward = sequence.iter().map(|&(_, y)| y).max().unwrap().max(0);
                    let downward = sequence.iter().map(|&(_, y)| -y).max().unwrap().max(0);
                    let expected = upward <= downward
                        && sequence.iter().enumerate().all(|(j, &(dx, dy))| {
                            (0..=downward).all(|t| {
                                t + dy >= 0
                                    || !sequence[..j]
                                        .iter()
                                        .any(|&(_, earlier_y)| t + earlier_y > downward)
                                    || sequence[..j].iter().any(|&(earlier_x, earlier_y)| {
                                        earlier_x == dx && (0..=downward).contains(&(t + earlier_y))
                                    })
                            })
                        });
                    assert_eq!(
                        KickSkyBounds::for_ordered_deltas([&sequence; 12]).uses_reduced_ceiling(),
                        expected,
                        "ordered candidates {sequence:?}"
                    );
                }
            }
        }
    }
}

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
