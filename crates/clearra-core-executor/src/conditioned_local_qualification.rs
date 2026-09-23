//! Independent audit of the *candidate* local-relation binary.
//!
//! This is compiled only for tests and qualification tools. A successful
//! audit checks each stored source board and its conservative collision
//! dependency closure; it is not a completeness receipt for all boards,
//! profiles, entry sets or the final product pack.

use crate::conditioned_local_pack::LocalRelationCandidatePack;
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::reachability_reference::{
    reference_local_dependency_mask, reference_local_relation, ReferenceReachabilityEntryPose,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationCandidateAuditError {
    InvalidReferenceQuery { record: usize },
    DependencyMaskMismatch { record: usize },
    RelationMismatch { record: usize },
}

/// Check every stored record against the primitive BFS and independently
/// reconstructed collision-read closure. Collision outcomes inside a fixed
/// pose window depend only on this closure: it includes all statically valid
/// sources, translation/grounding targets and *every* ordered kick candidate,
/// including sources that are blocked or currently unreachable. This still
/// leaves global entry reachability and exits/re-entry to the exact composer.
pub fn audit_candidate_local_relation_pack(
    candidate: &LocalRelationCandidatePack,
) -> Result<usize, LocalRelationCandidateAuditError> {
    for (index, record) in candidate.records().iter().enumerate() {
        let Some(mask) = reference_local_dependency_mask(
            record.width,
            record.height,
            record.piece,
            record.kick_profile,
            record.window,
        ) else {
            return Err(LocalRelationCandidateAuditError::InvalidReferenceQuery { record: index });
        };
        if mask != record.dependency_mask || record.dependency_occupancy != record.board & mask {
            return Err(LocalRelationCandidateAuditError::DependencyMaskMismatch { record: index });
        }
        let entries: Vec<_> = record
            .entries
            .iter()
            .map(|pose| ReferenceReachabilityEntryPose {
                rotation: pose.rotation,
                x: pose.x,
                y: pose.y,
            })
            .collect();
        let Some(reference) = reference_local_relation(
            record.width,
            record.height,
            record.board,
            record.piece,
            record.kick_profile,
            record.window,
            &entries,
        ) else {
            return Err(LocalRelationCandidateAuditError::InvalidReferenceQuery { record: index });
        };
        let exits: Vec<_> = reference
            .exits
            .iter()
            .map(|pose| ConditionedReachabilityEntryPose {
                rotation: pose.rotation,
                x: pose.x,
                y: pose.y,
            })
            .collect();
        if reference.grounded_lock_anchors != record.grounded_lock_anchors || exits != record.exits
        {
            return Err(LocalRelationCandidateAuditError::RelationMismatch { record: index });
        }
    }
    Ok(candidate.record_count())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::KickTableProfileId;

    use super::{audit_candidate_local_relation_pack, LocalRelationCandidateAuditError};
    use crate::conditioned_local_pack::{
        built_in_local_relation_binding, encode_local_relation_candidate_pack,
        load_local_relation_candidate_pack,
    };
    use crate::conditioned_local_relation::{
        derive_exact_conditioned_local_relation,
        derive_exact_conditioned_local_relation_with_frame, ConditionedPoseWindow,
        ExactConditionedLocalRelation, LocalRelationRowFrame,
    };
    use crate::conditioned_reachability::ConditionedReachabilityEntryPose;

    fn fixture(profile: KickTableProfileId) -> ExactConditionedLocalRelation {
        derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::T,
            profile,
            ConditionedPoseWindow {
                min_x: 4,
                max_x: 4,
                min_y: 4,
                max_y: 4,
            },
            &[ConditionedReachabilityEntryPose {
                rotation: RotationState::Zero,
                x: 4,
                y: 4,
            }],
        )
        .expect("source-board candidate")
    }

    fn load(
        profile: KickTableProfileId,
        record: ExactConditionedLocalRelation,
    ) -> crate::conditioned_local_pack::LocalRelationCandidatePack {
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        load_local_relation_candidate_pack(&bytes, binding, None).unwrap()
    }

    #[test]
    fn five_profiles_have_matching_primitive_relation_and_collision_closure() {
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let candidate = load(profile, fixture(profile));
            assert_eq!(audit_candidate_local_relation_pack(&candidate), Ok(1));
        }
    }

    #[test]
    fn deleted_original_row_frame_preserves_the_independent_physical_relation() {
        let profile = KickTableProfileId::SrsPlus;
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 4,
            max_y: 4,
        };
        let entry = ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        };
        let frame = LocalRelationRowFrame::new(4, 1 << 2).unwrap();
        let record = derive_exact_conditioned_local_relation_with_frame(
            10,
            4,
            0,
            frame,
            PieceKind::T,
            profile,
            window,
            &[entry],
        )
        .expect("same physical board, nontrivial original-row correspondence");
        assert_eq!(
            audit_candidate_local_relation_pack(&load(profile, record)),
            Ok(1)
        );
        assert!(derive_exact_conditioned_local_relation_with_frame(
            10,
            4,
            1_u64 << 30,
            frame,
            PieceKind::T,
            profile,
            window,
            &[entry],
        )
        .is_none());
    }

    #[test]
    fn every_piece_and_supported_height_has_a_stable_collision_dependency() {
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for piece in PieceKind::STANDARD_TETROMINOES {
                for height in 1..=6_u8 {
                    let window = ConditionedPoseWindow {
                        min_x: 4,
                        max_x: 4,
                        min_y: height as i8,
                        max_y: height as i8,
                    };
                    let entry = ConditionedReachabilityEntryPose {
                        rotation: RotationState::Zero,
                        x: 4,
                        y: height as i8,
                    };
                    let original = derive_exact_conditioned_local_relation(
                        10,
                        height,
                        0,
                        piece,
                        profile,
                        window,
                        &[entry],
                    )
                    .expect("empty-board entry");
                    let candidate = load(profile, original.clone());
                    assert_eq!(
                        audit_candidate_local_relation_pack(&candidate),
                        Ok(1),
                        "{profile:?} {piece:?} {height}L"
                    );
                    let board_bits = 10_u32 * u32::from(height);
                    if let Some(bit) =
                        (0..board_bits).find(|bit| original.dependency_mask & (1_u64 << *bit) == 0)
                    {
                        let altered = derive_exact_conditioned_local_relation(
                            10,
                            height,
                            1_u64 << bit,
                            piece,
                            profile,
                            window,
                            &[entry],
                        )
                        .expect("outside-dependency obstacle preserves entry");
                        assert_eq!(
                            altered.grounded_lock_anchors, original.grounded_lock_anchors,
                            "{profile:?} {piece:?} {height}L bit={bit}"
                        );
                        assert_eq!(
                            altered.exits, original.exits,
                            "{profile:?} {piece:?} {height}L bit={bit}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn format_integrity_cannot_authorize_a_missing_dependency_or_false_lock() {
        let profile = KickTableProfileId::SrsPlus;
        let mut missing_dependency = fixture(profile);
        let bit = missing_dependency.dependency_mask.trailing_zeros();
        assert!(bit < 64);
        missing_dependency.dependency_mask &= !(1_u64 << bit);
        let candidate = load(profile, missing_dependency);
        assert_eq!(
            audit_candidate_local_relation_pack(&candidate),
            Err(LocalRelationCandidateAuditError::DependencyMaskMismatch { record: 0 })
        );

        let mut false_lock = fixture(profile);
        false_lock.grounded_lock_anchors[0] ^= 1;
        let candidate = load(profile, false_lock);
        assert_eq!(
            audit_candidate_local_relation_pack(&candidate),
            Err(LocalRelationCandidateAuditError::RelationMismatch { record: 0 })
        );
    }
}
