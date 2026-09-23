//! Independent audit of the *candidate* local-relation binary.
//!
//! This is compiled only for tests and qualification tools. A successful
//! audit checks each stored source board and its reached-source collision
//! dependency closure; it is not a completeness receipt for all boards,
//! profiles, entry sets or the final product pack.

use crate::conditioned_local_pack::LocalRelationCandidatePack;
use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::reachability_reference::{
    reference_local_dependency_mask, reference_local_relation, ReferenceReachabilityEntryPose,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::KickTableProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationCandidateAuditError {
    InvalidReferenceQuery { record: usize },
    DependencyMaskMismatch { record: usize },
    RelationMismatch { record: usize },
}

/// Check every stored record against the primitive BFS and independently
/// reconstructed collision-read closure. From the fixed entries, the source
/// board's reached poses are stable under any change outside their closure:
/// every outgoing translation and every ordered kick candidate retains its
/// collision result, so an unreached pose cannot become reached without a
/// changed edge out of the reached set. This still leaves global entry
/// reachability and exits/re-entry to the exact composer.
pub fn audit_candidate_local_relation_pack(
    candidate: &LocalRelationCandidatePack,
) -> Result<usize, LocalRelationCandidateAuditError> {
    for (index, record) in candidate.records().iter().enumerate() {
        audit_local_relation_record(record, index)?;
    }
    Ok(candidate.record_count())
}

fn audit_local_relation_record(
    record: &ExactConditionedLocalRelation,
    index: usize,
) -> Result<(), LocalRelationCandidateAuditError> {
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
    let mask = reference.reached_source_dependency_mask;
    if mask != record.dependency_mask || record.dependency_occupancy != record.board & mask {
        return Err(LocalRelationCandidateAuditError::DependencyMaskMismatch { record: index });
    }
    let exits: Vec<_> = reference
        .exits
        .iter()
        .map(|pose| ConditionedReachabilityEntryPose {
            rotation: pose.rotation,
            x: pose.x,
            y: pose.y,
        })
        .collect();
    if reference.grounded_lock_anchors != record.grounded_lock_anchors || exits != record.exits {
        return Err(LocalRelationCandidateAuditError::RelationMismatch { record: index });
    }
    Ok(())
}

/// Qualification-only accumulator. Each new exact record is independently
/// audited once before it can contribute a coverage cube. The final encoded
/// pack must still be reloaded and audited as a whole; this saves repeated
/// serialization/audit work during counterexample-guided generation only.
pub struct AuditedLocalRelationRecordSet {
    profile: KickTableProfileId,
    records: Vec<ExactConditionedLocalRelation>,
}

impl AuditedLocalRelationRecordSet {
    pub fn new(profile: KickTableProfileId) -> Self {
        Self {
            profile,
            records: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        record: ExactConditionedLocalRelation,
    ) -> Result<(), LocalRelationCandidateAuditError> {
        let index = self.records.len();
        if record.kick_profile != self.profile {
            return Err(LocalRelationCandidateAuditError::InvalidReferenceQuery { record: index });
        }
        audit_local_relation_record(&record, index)?;
        self.records.push(record);
        Ok(())
    }

    pub fn prove_context_coverage(
        &self,
        domain: LocalRelationCoverageDomain<'_>,
        max_nodes: u32,
    ) -> Result<LocalRelationCoverageResult, LocalRelationCoverageError> {
        prove_context_coverage_after_audit(&self.records, self.profile, domain, max_nodes)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn into_records(self) -> Vec<ExactConditionedLocalRelation> {
        self.records
    }
}

/// A process-local proof token. Only the independent primitive audit can
/// construct it, so many declared coverage domains share one record audit.
pub struct AuditedLocalRelationCandidatePack<'a> {
    candidate: &'a LocalRelationCandidatePack,
}

pub fn audited_local_relation_candidate_pack(
    candidate: &LocalRelationCandidatePack,
) -> Result<AuditedLocalRelationCandidatePack<'_>, LocalRelationCandidateAuditError> {
    audit_candidate_local_relation_pack(candidate)?;
    Ok(AuditedLocalRelationCandidatePack { candidate })
}

impl AuditedLocalRelationCandidatePack<'_> {
    pub fn prove_context_coverage(
        &self,
        domain: LocalRelationCoverageDomain<'_>,
        max_nodes: u32,
    ) -> Result<LocalRelationCoverageResult, LocalRelationCoverageError> {
        prove_context_coverage_after_audit(
            self.candidate.records(),
            self.candidate.binding().kick_profile,
            domain,
            max_nodes,
        )
    }
}

/// A *single* context and a declared occupancy subdomain. The fixed mask and
/// value may restrict the board family further; source-entry footprint cells
/// are always added as fixed-empty conditions by the proof. This is not a
/// claim that every board, entry set, or profile has been covered.
pub struct LocalRelationCoverageDomain<'a> {
    pub width: u8,
    pub height: u8,
    pub frame: LocalRelationRowFrame,
    pub piece: PieceKind,
    pub profile: KickTableProfileId,
    pub window: ConditionedPoseWindow,
    pub entries: &'a [ConditionedReachabilityEntryPose],
    pub fixed_mask: u64,
    pub fixed_occupancy: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationCoverageError {
    InvalidDomain,
    InvalidCandidate(LocalRelationCandidateAuditError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationCoverageResult {
    Complete {
        effective_fixed_mask: u64,
        effective_fixed_occupancy: u64,
        context_records: usize,
        visited_nodes: u32,
    },
    Uncovered {
        counterexample_board: u64,
        visited_nodes: u32,
    },
    Inconclusive {
        visited_nodes: u32,
    },
}

#[derive(Clone, Copy)]
struct OccupancyCube {
    mask: u64,
    value: u64,
}

enum CoverageStep {
    Covered,
    Uncovered(u64),
    Inconclusive,
}

/// Prove that every board in the declared context/subdomain hits an audited
/// exact relation record. This is a bounded Shannon expansion over occupancy
/// bits, not sampling. A counterexample or exhausted proof budget cannot be
/// promoted to product authority. Callers must separately qualify the scope's
/// usefulness, whole-pack identity, resource limits, and signed generation.
pub fn prove_candidate_local_relation_context_coverage(
    candidate: &LocalRelationCandidatePack,
    domain: LocalRelationCoverageDomain<'_>,
    max_nodes: u32,
) -> Result<LocalRelationCoverageResult, LocalRelationCoverageError> {
    audited_local_relation_candidate_pack(candidate)
        .map_err(LocalRelationCoverageError::InvalidCandidate)?
        .prove_context_coverage(domain, max_nodes)
}

fn prove_context_coverage_after_audit(
    records: &[ExactConditionedLocalRelation],
    profile: KickTableProfileId,
    domain: LocalRelationCoverageDomain<'_>,
    max_nodes: u32,
) -> Result<LocalRelationCoverageResult, LocalRelationCoverageError> {
    if !(1..=1_000_000).contains(&max_nodes)
        || domain.width != 10
        || !(1..=6).contains(&domain.height)
        || domain.frame.target_height() != domain.height
        || profile != domain.profile
        || domain.entries.is_empty()
        || domain.entries.len() > 640
        || domain
            .entries
            .windows(2)
            .any(|pair| pose_key(&pair[0]) >= pose_key(&pair[1]))
        || reference_local_dependency_mask(
            domain.width,
            domain.height,
            domain.piece,
            domain.profile,
            domain.window,
        )
        .is_none()
    {
        return Err(LocalRelationCoverageError::InvalidDomain);
    }
    let physical_bits = u32::from(domain.width) * u32::from(domain.frame.surviving_rows());
    let board_mask = (1_u64 << physical_bits) - 1;
    let target_bits = u32::from(domain.width) * u32::from(domain.height);
    let absent_physical_rows = ((1_u64 << target_bits) - 1) & !board_mask;
    if domain.fixed_mask & !board_mask != 0 || domain.fixed_occupancy & !domain.fixed_mask != 0 {
        return Err(LocalRelationCoverageError::InvalidDomain);
    }
    let entry_empty_mask =
        entry_footprint_mask(&domain).ok_or(LocalRelationCoverageError::InvalidDomain)?;
    if domain.fixed_occupancy & entry_empty_mask != 0 {
        return Err(LocalRelationCoverageError::InvalidDomain);
    }
    let cubes = records
        .iter()
        .filter(|record| {
            record.width == domain.width
                && record.height == domain.height
                && record.row_frame == domain.frame
                && record.piece == domain.piece
                && record.kick_profile == domain.profile
                && record.window == domain.window
                && record.entries == domain.entries
        })
        .map(|record| OccupancyCube {
            mask: record.dependency_mask,
            value: record.dependency_occupancy,
        })
        .collect::<Vec<_>>();
    let initial = (0..cubes.len()).collect::<Vec<_>>();
    let effective_mask = domain.fixed_mask | entry_empty_mask | absent_physical_rows;
    let mut visited_nodes = 0;
    let proof = prove_cube_cover(
        &cubes,
        &initial,
        OccupancyCube {
            mask: effective_mask,
            value: domain.fixed_occupancy,
        },
        max_nodes,
        &mut visited_nodes,
    );
    Ok(match proof {
        CoverageStep::Covered => LocalRelationCoverageResult::Complete {
            effective_fixed_mask: effective_mask,
            effective_fixed_occupancy: domain.fixed_occupancy,
            context_records: cubes.len(),
            visited_nodes,
        },
        CoverageStep::Uncovered(counterexample_board) => LocalRelationCoverageResult::Uncovered {
            counterexample_board,
            visited_nodes,
        },
        CoverageStep::Inconclusive => LocalRelationCoverageResult::Inconclusive { visited_nodes },
    })
}

fn pose_key(pose: &ConditionedReachabilityEntryPose) -> (u8, i8, i8) {
    (pose.rotation.quarter_turns(), pose.x, pose.y)
}

fn entry_footprint_mask(domain: &LocalRelationCoverageDomain<'_>) -> Option<u64> {
    let shape = standard_tetromino_registry().get(domain.piece)?;
    let mut mask = 0_u64;
    for entry in domain.entries {
        if entry.x < domain.window.min_x
            || entry.x > domain.window.max_x
            || entry.y < domain.window.min_y
            || entry.y > domain.window.max_y
        {
            return None;
        }
        for cell in shape.shape(entry.rotation).cells() {
            let x = i16::from(entry.x) + i16::from(cell.x());
            let y = i16::from(entry.y) + i16::from(cell.y());
            if x < 0 || x >= i16::from(domain.width) || y < 0 {
                return None;
            }
            if y < i16::from(domain.frame.surviving_rows()) {
                mask |= 1_u64 << (u32::from(domain.width) * y as u32 + x as u32);
            }
        }
    }
    Some(mask)
}

fn prove_cube_cover(
    cubes: &[OccupancyCube],
    candidates: &[usize],
    current: OccupancyCube,
    max_nodes: u32,
    visited_nodes: &mut u32,
) -> CoverageStep {
    if *visited_nodes >= max_nodes {
        return CoverageStep::Inconclusive;
    }
    *visited_nodes += 1;
    let mut possible = Vec::new();
    // Candidate generation permits 65,536 records; u16 would wrap on a
    // shared split bit precisely at that bound and invalidate the proof.
    let mut frequency = [0_usize; 60];
    for &index in candidates {
        let cube = cubes[index];
        if (current.value ^ cube.value) & current.mask & cube.mask != 0 {
            continue;
        }
        if cube.mask & !current.mask == 0 {
            return CoverageStep::Covered;
        }
        possible.push(index);
        let mut undecided = cube.mask & !current.mask;
        while undecided != 0 {
            let bit = undecided.trailing_zeros() as usize;
            frequency[bit] += 1;
            undecided &= undecided - 1;
        }
    }
    let Some((bit, &count)) = frequency
        .iter()
        .enumerate()
        .max_by_key(|&(bit, count)| (*count, core::cmp::Reverse(bit)))
    else {
        return CoverageStep::Uncovered(current.value);
    };
    if count == 0 {
        return CoverageStep::Uncovered(current.value);
    }
    let split = 1_u64 << bit;
    let zero = OccupancyCube {
        mask: current.mask | split,
        value: current.value,
    };
    match prove_cube_cover(cubes, &possible, zero, max_nodes, visited_nodes) {
        CoverageStep::Covered => {}
        other => return other,
    }
    prove_cube_cover(
        cubes,
        &possible,
        OccupancyCube {
            mask: zero.mask,
            value: zero.value | split,
        },
        max_nodes,
        visited_nodes,
    )
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::KickTableProfileId;

    use super::{
        audit_candidate_local_relation_pack, audited_local_relation_candidate_pack,
        entry_footprint_mask, prove_candidate_local_relation_context_coverage, prove_cube_cover,
        AuditedLocalRelationRecordSet, CoverageStep, LocalRelationCandidateAuditError,
        LocalRelationCoverageDomain, LocalRelationCoverageError, LocalRelationCoverageResult,
        OccupancyCube,
    };
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
    fn incremental_audit_has_the_same_coverage_as_final_pack_audit() {
        let profile = KickTableProfileId::NoKick;
        let record = fixture(profile);
        let pack = load(profile, record.clone());
        let domain = || LocalRelationCoverageDomain {
            width: record.width,
            height: record.height,
            frame: record.row_frame,
            piece: record.piece,
            profile,
            window: record.window,
            entries: &record.entries,
            fixed_mask: (1_u64 << 40) - 1,
            fixed_occupancy: 0,
        };
        let mut incremental = AuditedLocalRelationRecordSet::new(profile);
        assert!(incremental.is_empty());
        incremental.push(record.clone()).unwrap();
        assert_eq!(incremental.len(), 1);
        assert_eq!(
            incremental.prove_context_coverage(domain(), 100),
            audited_local_relation_candidate_pack(&pack)
                .unwrap()
                .prove_context_coverage(domain(), 100),
        );
        let mut false_dependency = record.clone();
        false_dependency.dependency_mask ^= 1;
        assert!(matches!(
            incremental.push(false_dependency),
            Err(LocalRelationCandidateAuditError::DependencyMaskMismatch { .. })
        ));
        assert_eq!(incremental.len(), 1);
        assert!(matches!(
            incremental.push(fixture(KickTableProfileId::SrsPlus)),
            Err(LocalRelationCandidateAuditError::InvalidReferenceQuery { .. })
        ));
        assert_eq!(incremental.into_records(), vec![record]);
    }

    #[test]
    fn symbolic_cube_cover_distinguishes_exhaustive_incomplete_and_budget_limited() {
        let halves = [
            OccupancyCube { mask: 1, value: 0 },
            OccupancyCube { mask: 1, value: 1 },
        ];
        let domain = OccupancyCube { mask: 0, value: 0 };
        let mut visited = 0;
        assert!(matches!(
            prove_cube_cover(&halves, &[0, 1], domain, 8, &mut visited),
            CoverageStep::Covered
        ));
        let mut visited = 0;
        assert!(matches!(
            prove_cube_cover(&halves[..1], &[0], domain, 8, &mut visited),
            CoverageStep::Uncovered(1)
        ));
        let mut visited = 0;
        assert!(matches!(
            prove_cube_cover(&halves, &[0, 1], domain, 1, &mut visited),
            CoverageStep::Inconclusive
        ));
    }

    #[test]
    fn symbolic_cover_matches_exhaustive_eight_bit_truth_tables() {
        let mut state = 0x62df_9937_u32;
        let mut random = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        };
        for _ in 0..256 {
            let domain_mask = u64::from(random() as u8);
            let domain = OccupancyCube {
                mask: domain_mask,
                value: u64::from(random() as u8) & domain_mask,
            };
            let cubes = (0..1 + random() as usize % 6)
                .map(|_| {
                    let mask = u64::from(random() as u8);
                    OccupancyCube {
                        mask,
                        value: u64::from(random() as u8) & mask,
                    }
                })
                .collect::<Vec<_>>();
            let expected = (0..=255_u64)
                .filter(|board| board & domain.mask == domain.value)
                .all(|board| cubes.iter().any(|cube| board & cube.mask == cube.value));
            let mut visited = 0;
            let proof = prove_cube_cover(
                &cubes,
                &(0..cubes.len()).collect::<Vec<_>>(),
                domain,
                1024,
                &mut visited,
            );
            match proof {
                CoverageStep::Covered => assert!(expected),
                CoverageStep::Uncovered(board) => {
                    assert!(!expected);
                    assert_eq!(board & domain.mask, domain.value);
                    assert!(!cubes.iter().any(|cube| board & cube.mask == cube.value));
                }
                CoverageStep::Inconclusive => panic!("eight-bit proof exceeded its budget"),
            }
        }
    }

    #[test]
    fn symbolic_cover_split_frequency_does_not_wrap_at_candidate_limit() {
        let cubes = vec![OccupancyCube { mask: 1, value: 0 }; 65_536];
        let candidates = (0..cubes.len()).collect::<Vec<_>>();
        let mut visited = 0;
        assert!(matches!(
            prove_cube_cover(
                &cubes,
                &candidates,
                OccupancyCube { mask: 0, value: 0 },
                4,
                &mut visited,
            ),
            CoverageStep::Uncovered(1)
        ));
    }

    #[test]
    fn context_coverage_is_only_for_its_declared_occupancy_family() {
        let profile = KickTableProfileId::NoKick;
        let record = fixture(profile);
        assert_ne!(record.dependency_mask, 0);
        let narrow_mask = record.dependency_mask;
        let narrow_value = record.dependency_occupancy;
        let candidate = load(profile, record);
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        }];
        let domain = |fixed_mask, fixed_occupancy| LocalRelationCoverageDomain {
            width: 10,
            height: 4,
            frame: LocalRelationRowFrame::new(4, 0).unwrap(),
            piece: PieceKind::T,
            profile,
            window: ConditionedPoseWindow {
                min_x: 4,
                max_x: 4,
                min_y: 4,
                max_y: 4,
            },
            entries: &entries,
            fixed_mask,
            fixed_occupancy,
        };
        assert!(matches!(
            prove_candidate_local_relation_context_coverage(
                &candidate,
                domain(narrow_mask, narrow_value),
                1024,
            ),
            Ok(LocalRelationCoverageResult::Complete {
                context_records: 1,
                ..
            })
        ));
        assert!(matches!(
            prove_candidate_local_relation_context_coverage(&candidate, domain(0, 0), 1024),
            Ok(LocalRelationCoverageResult::Uncovered { .. })
        ));
        assert_eq!(
            prove_candidate_local_relation_context_coverage(&candidate, domain(0, 1), 1024,),
            Err(LocalRelationCoverageError::InvalidDomain)
        );
    }

    #[test]
    fn entry_footprint_is_fixed_empty_before_coverage_proof() {
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 0,
        }];
        let domain = LocalRelationCoverageDomain {
            width: 10,
            height: 4,
            frame: LocalRelationRowFrame::new(4, 0).unwrap(),
            piece: PieceKind::O,
            profile: KickTableProfileId::NoKick,
            window: ConditionedPoseWindow {
                min_x: 4,
                max_x: 4,
                min_y: 0,
                max_y: 0,
            },
            entries: &entries,
            fixed_mask: 0,
            fixed_occupancy: 0,
        };
        assert_ne!(entry_footprint_mask(&domain), Some(0));
        assert!(entry_footprint_mask(&domain).is_some());
    }

    #[test]
    fn cleared_physical_rows_are_fixed_empty_in_the_symbolic_domain() {
        let profile = KickTableProfileId::NoKick;
        let frame = LocalRelationRowFrame::new(4, 1 << 2).unwrap();
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 4,
            max_y: 4,
        };
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        }];
        let record = derive_exact_conditioned_local_relation_with_frame(
            10,
            4,
            0,
            frame,
            PieceKind::T,
            profile,
            window,
            &entries,
        )
        .unwrap();
        let physically_present_mask = (1_u64 << 30) - 1;
        let fixed_mask = record.dependency_mask() & physically_present_mask;
        let candidate = load(profile, record);
        let result = prove_candidate_local_relation_context_coverage(
            &candidate,
            LocalRelationCoverageDomain {
                width: 10,
                height: 4,
                frame,
                piece: PieceKind::T,
                profile,
                window,
                entries: &entries,
                fixed_mask,
                fixed_occupancy: 0,
            },
            1024,
        )
        .unwrap();
        assert!(matches!(
            result,
            LocalRelationCoverageResult::Complete {
                effective_fixed_mask,
                ..
            } if effective_fixed_mask & !physically_present_mask == ((1_u64 << 40) - 1) & !physically_present_mask
        ));
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
    fn local_locks_and_first_exits_compose_to_the_exact_global_entry_result() {
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for height in 1..=6_u8 {
                for piece in [PieceKind::T, PieceKind::J, PieceKind::I] {
                    for board in [0_u64, 1, 0b1001] {
                        let entry = ConditionedReachabilityEntryPose {
                            rotation: RotationState::Zero,
                            x: 4,
                            y: height as i8,
                        };
                        let window = ConditionedPoseWindow {
                            min_x: 4,
                            max_x: 4,
                            min_y: height as i8,
                            max_y: height as i8,
                        };
                        let relation = derive_exact_conditioned_local_relation(
                            10,
                            height,
                            board,
                            piece,
                            profile,
                            window,
                            &[entry],
                        )
                        .expect("entry remains placeable on the selected board");
                        let expected = crate::backend::exact_entry_lock_anchors(
                            10,
                            height,
                            board,
                            piece,
                            profile,
                            &[entry],
                        );
                        assert_eq!(
                            relation.compose_exact_global_lock_anchors_for_board(board),
                            expected,
                            "{profile:?} {height}L {piece:?} board={board:#x}"
                        );
                    }
                }
            }
        }
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
    fn reached_source_dependency_cube_preserves_every_one_row_board() {
        let profile = KickTableProfileId::NoKick;
        let piece = PieceKind::T;
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 0,
            max_y: 1,
        };
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 1,
        }];
        let static_mask = crate::reachability_reference::reference_local_dependency_mask(
            10, 1, piece, profile, window,
        )
        .unwrap();
        let record = (0..1_u64 << 10)
            .filter_map(|board| {
                derive_exact_conditioned_local_relation(
                    10, 1, board, piece, profile, window, &entries,
                )
            })
            .find(|record| record.dependency_mask() != static_mask)
            .expect("a blocked source removes at least one static dependency");
        assert_eq!(record.dependency_mask() & !static_mask, 0);
        assert_eq!(
            audit_candidate_local_relation_pack(&load(profile, record.clone())),
            Ok(1)
        );
        let mut matched_boards = 0;
        for board in 0..1_u64 << 10 {
            if board & record.dependency_mask() != record.dependency_occupancy {
                continue;
            }
            let matching = derive_exact_conditioned_local_relation(
                10, 1, board, piece, profile, window, &entries,
            )
            .expect("matching dependency occupancy keeps the entry placeable");
            assert_eq!(
                (matching.grounded_lock_anchors, matching.exits),
                (record.grounded_lock_anchors, record.exits.clone()),
                "board={board:#x} source={:#x}",
                record.board
            );
            matched_boards += 1;
        }
        assert!(matched_boards > 1);
    }

    #[test]
    fn ordered_180_kicks_keep_the_reached_source_cube_exact() {
        let profile = KickTableProfileId::SrsX;
        let piece = PieceKind::J;
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 0,
            max_y: 1,
        };
        let entries = [ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 1,
        }];
        let static_mask = crate::reachability_reference::reference_local_dependency_mask(
            10, 1, piece, profile, window,
        )
        .unwrap();
        let record = (0..1_u64 << 10)
            .filter_map(|board| {
                derive_exact_conditioned_local_relation(
                    10, 1, board, piece, profile, window, &entries,
                )
            })
            .find(|record| record.dependency_mask() != static_mask)
            .expect("a blocked source removes a static SRS-X dependency");
        assert_eq!(
            audit_candidate_local_relation_pack(&load(profile, record.clone())),
            Ok(1)
        );
        let mut matched_boards = 0;
        for board in 0..1_u64 << 10 {
            if board & record.dependency_mask() != record.dependency_occupancy {
                continue;
            }
            let matching = derive_exact_conditioned_local_relation(
                10, 1, board, piece, profile, window, &entries,
            )
            .expect("the entry remains placeable under the cube");
            assert_eq!(matching.grounded_lock_anchors, record.grounded_lock_anchors);
            assert_eq!(matching.exits, record.exits, "board={board:#x}");
            matched_boards += 1;
        }
        assert!(matched_boards > 1);
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
