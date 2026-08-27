use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_scoring::{
    builtin::{
        guideline_pc_score_with_spin_profile, jstris_ultra_pc_score_with_spin_profile,
        tetrio_pc_score_with_spin_profile,
    },
    profile::{
        AttackModelId, B2BPolicy, ComboPolicy, DropScorePolicy, LevelPolicy, ScoreModelId,
        ScoreProfile, SpinProfile, SpinProfileId, TraceRequirement,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreProfileMemoryProjection {
    pub id_storage_bytes: u128,
    pub display_name_storage_bytes: u128,
    pub required_memory_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreProfileMemoryReport {
    pub projection: ScoreProfileMemoryProjection,
    pub retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreProfileMemoryGuardError {
    ProjectionOverflow,
    LimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed,
}

pub(crate) fn score_profile(policy: ScoreObjectivePolicy) -> ScoreProfile {
    let spin_profile = spin_profile_id(policy.spin_profile());
    match policy.profile() {
        ScoreProfileSelection::Guideline => guideline_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::JstrisUltra => jstris_ultra_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::Tetrio => tetrio_pc_score_with_spin_profile(spin_profile),
    }
}

pub fn checked_score_profile_memory_projection(
    policy: ScoreObjectivePolicy,
) -> Option<ScoreProfileMemoryProjection> {
    let spin = spin_profile_id(policy.spin_profile()).as_str();
    let (id_prefix, display_prefix) = profile_text_prefixes(policy.profile());
    let id_storage_bytes = (id_prefix.len() as u128).checked_add(spin.len() as u128)?;
    let display_name_storage_bytes = (display_prefix.len() as u128)
        .checked_add(spin.len() as u128)?
        .checked_add(1)?;
    Some(ScoreProfileMemoryProjection {
        id_storage_bytes,
        display_name_storage_bytes,
        required_memory_bytes: id_storage_bytes.checked_add(display_name_storage_bytes)?,
    })
}

pub fn score_profile_with_memory_guard(
    policy: ScoreObjectivePolicy,
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(ScoreProfile, ScoreProfileMemoryReport), ScoreProfileMemoryGuardError> {
    let projection = checked_score_profile_memory_projection(policy)
        .ok_or(ScoreProfileMemoryGuardError::ProjectionOverflow)?;
    let required_memory_bytes = already_retained_bytes
        .checked_add(projection.required_memory_bytes)
        .ok_or(ScoreProfileMemoryGuardError::ProjectionOverflow)?;
    if required_memory_bytes > max_memory_bytes {
        return Err(ScoreProfileMemoryGuardError::LimitExceeded {
            required_memory_bytes,
            max_memory_bytes,
        });
    }
    let spin_profile = spin_profile_id(policy.spin_profile());
    let spin = spin_profile.as_str();
    let (id_prefix, display_prefix) = profile_text_prefixes(policy.profile());
    let id = try_join_profile_text(id_prefix, spin, "")?;
    let display_name = try_join_profile_text(display_prefix, spin, ")")?;
    let retained_bytes = (id.capacity() as u128)
        .checked_add(display_name.capacity() as u128)
        .ok_or(ScoreProfileMemoryGuardError::ProjectionOverflow)?;
    let actual_required_memory_bytes = already_retained_bytes
        .checked_add(retained_bytes)
        .ok_or(ScoreProfileMemoryGuardError::ProjectionOverflow)?;
    if actual_required_memory_bytes > max_memory_bytes {
        return Err(ScoreProfileMemoryGuardError::LimitExceeded {
            required_memory_bytes: actual_required_memory_bytes,
            max_memory_bytes,
        });
    }
    let profile = match policy.profile() {
        ScoreProfileSelection::Guideline => ScoreProfile::new(id, display_name)
            .with_score_model(ScoreModelId::Guideline)
            .with_spin_profile(SpinProfile::builtin(spin_profile))
            .with_combo_policy(ComboPolicy::linear(50, 0))
            .with_b2b_policy(B2BPolicy::multiplier(3, 2, 0))
            .with_level_policy(LevelPolicy::FixedLevelOne)
            .with_drop_score_policy(DropScorePolicy::Disabled)
            .with_trace_requirement(TraceRequirement::PlacementTrace),
        ScoreProfileSelection::JstrisUltra => ScoreProfile::new(id, display_name)
            .with_score_model(ScoreModelId::JstrisUltra)
            .with_spin_profile(SpinProfile::builtin(spin_profile))
            .with_combo_policy(ComboPolicy::linear(50, 0))
            .with_b2b_policy(B2BPolicy::multiplier(3, 2, 0))
            .with_drop_score_policy(DropScorePolicy::Disabled)
            .with_trace_requirement(TraceRequirement::PlacementTrace),
        ScoreProfileSelection::Tetrio => ScoreProfile::new(id, display_name)
            .with_score_model(ScoreModelId::Tetrio)
            .with_attack_model(AttackModelId::Tetrio)
            .with_spin_profile(SpinProfile::builtin(spin_profile))
            .with_combo_policy(ComboPolicy::linear(50, 1))
            .with_b2b_policy(B2BPolicy::multiplier(3, 2, 1))
            .with_drop_score_policy(DropScorePolicy::Disabled)
            .with_trace_requirement(TraceRequirement::PlacementTrace)
            // The authoritative TETR.IO PC builtin starts from the normal
            // TETR.IO profile and then deliberately disables attack output.
            .with_attack_model(AttackModelId::Disabled),
    };
    Ok((
        profile,
        ScoreProfileMemoryReport {
            projection,
            retained_bytes,
        },
    ))
}

fn profile_text_prefixes(profile: ScoreProfileSelection) -> (&'static str, &'static str) {
    match profile {
        ScoreProfileSelection::Guideline => {
            ("guideline-pc-", "Guideline-compatible Level 1 PC scoring (")
        }
        ScoreProfileSelection::JstrisUltra => ("jstris-ultra-pc-", "Jstris Ultra PC scoring ("),
        ScoreProfileSelection::Tetrio => ("tetrio-pc-", "TETR.IO PC scoring ("),
    }
}

fn try_join_profile_text(
    prefix: &str,
    middle: &str,
    suffix: &str,
) -> Result<String, ScoreProfileMemoryGuardError> {
    let capacity = prefix
        .len()
        .checked_add(middle.len())
        .and_then(|length| length.checked_add(suffix.len()))
        .ok_or(ScoreProfileMemoryGuardError::ProjectionOverflow)?;
    let mut output = String::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| ScoreProfileMemoryGuardError::AllocationFailed)?;
    output.push_str(prefix);
    output.push_str(middle);
    output.push_str(suffix);
    Ok(output)
}

pub(crate) const fn spin_profile_id(profile: SpinProfileSelection) -> SpinProfileId {
    match profile {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

pub(crate) fn score_profile_matches_policy(
    profile: &ScoreProfile,
    policy: ScoreObjectivePolicy,
) -> bool {
    let spin = spin_profile_id(policy.spin_profile()).as_str();
    let (id_prefix, _) = profile_text_prefixes(policy.profile());
    profile
        .id()
        .strip_prefix(id_prefix)
        .is_some_and(|suffix| suffix == spin)
}

#[cfg(test)]
mod tests {
    use clearra_objectives::policy::score_objective_policy::{
        ScoreProfileSelection, SpinProfileSelection,
    };

    use super::*;

    #[test]
    fn guarded_profiles_match_builtin_authority_and_enforce_exact_cap() {
        for profile in [
            ScoreProfileSelection::Guideline,
            ScoreProfileSelection::JstrisUltra,
            ScoreProfileSelection::Tetrio,
        ] {
            for spin in [
                SpinProfileSelection::TSpins,
                SpinProfileSelection::TSpinsPlus,
                SpinProfileSelection::AllSpin,
                SpinProfileSelection::AllSpinPlus,
                SpinProfileSelection::AllMini,
                SpinProfileSelection::AllMiniPlus,
            ] {
                let policy = ScoreObjectivePolicy::summary()
                    .with_profile(profile)
                    .with_spin_profile(spin);
                let projection =
                    checked_score_profile_memory_projection(policy).expect("profile projection");
                let (guarded, report) =
                    score_profile_with_memory_guard(policy, 0, projection.required_memory_bytes)
                        .expect("exact profile cap");
                assert_eq!(guarded, score_profile(policy));
                assert_eq!(report.projection, projection);
                assert!(report.retained_bytes <= projection.required_memory_bytes);

                assert_eq!(
                    score_profile_with_memory_guard(
                        policy,
                        0,
                        projection.required_memory_bytes - 1,
                    ),
                    Err(ScoreProfileMemoryGuardError::LimitExceeded {
                        required_memory_bytes: projection.required_memory_bytes,
                        max_memory_bytes: projection.required_memory_bytes - 1,
                    })
                );
            }
        }
    }
}
