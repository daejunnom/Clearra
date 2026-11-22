//! Qualification and ownership of an exact ordered compact kick table.
//! Numeric profile IDs describe the source, not the table's ordered identity.

use std::sync::Arc;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::{
    kicks::{KickTableEntry, KickTableProfileId, VerifiedKickTableProfile},
    profile::rule_profile::RuleProfileId,
};

#[cfg(any(test, not(target_family = "wasm")))]
use super::kick_profiles::builtin_kick_profile;
use super::{
    kick_profiles::{normalized_kick_delta_wide, KickSkyBounds},
    piece_index,
};

const SLOTS_PER_PIECE: usize = 12;
type OrderedDeltas = [Option<Box<[(i16, i16)]>>; SLOTS_PER_PIECE];
type BorrowedOrderedEntries<'a> = [Option<&'a KickTableEntry>; SLOTS_PER_PIECE];

/// Explicit limits on logical compiler allocation counts, not allocator RSS.
/// Production admission passes no implicit budget. Encoded pose/target indices
/// and checked count arithmetic are validated independently in every mode.
#[derive(Clone, Copy)]
struct CompileBudget {
    poses: usize,
    rotation_targets: usize,
    index_entries: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum CompactKickAdmission {
    Unsupported,
    IndexOrCoordinateOverflow,
    BudgetExceeded,
}

#[derive(Debug, Eq, PartialEq)]
struct PieceKicks {
    ceiling: i16,
    // Missing and present-but-empty are different ordered table identities.
    ordered: OrderedDeltas,
}

#[derive(Debug, Eq, PartialEq)]
struct CompactKickTable {
    width: u8,
    height: u8,
    profile: KickTableProfileId,
    rule: RuleProfileId,
    allow_180: bool,
    pieces: [PieceKicks; 7],
}

/// One request owner. Structural equality checks exact active action content.
/// Self-orientation records have no request in the movement alphabet and
/// therefore project away. A
/// shared-template route must carry this same owner, not reconstruct it from
/// its profile tag or reuse a memo from another request with the same tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompactKickAuthority(Arc<CompactKickTable>);

pub(super) struct CompactPieceKicks<'a> {
    pub(super) width: u8,
    pub(super) height: u8,
    pub(super) ceiling: i16,
    pub(super) allow_180: bool,
    pub(super) ordered: [&'a [(i16, i16)]; SLOTS_PER_PIECE],
}

/// The admission barrier contains only fixed-size metadata and borrowed raw
/// entries. Its constructor has checked every aggregate compiler count before
/// materialization can allocate any candidate-sized normalized buffer.
struct AdmittedKickLayout<'a> {
    width: u8,
    height: u8,
    profile: KickTableProfileId,
    rule: RuleProfileId,
    allow_180: bool,
    ceilings: [i16; 7],
    ordered: [BorrowedOrderedEntries<'a>; 7],
}

impl AdmittedKickLayout<'_> {
    fn materialize(self) -> Result<CompactKickAuthority, CompactKickAdmission> {
        let mut pieces: [PieceKicks; 7] = std::array::from_fn(|index| PieceKicks {
            ceiling: self.ceilings[index],
            ordered: std::array::from_fn(|_| None),
        });
        for (piece, entries) in pieces.iter_mut().zip(self.ordered) {
            for (slot, entry) in entries.into_iter().enumerate() {
                let Some(entry) = entry else {
                    continue;
                };
                let mut normalized = Vec::new();
                normalized
                    .try_reserve_exact(entry.sequence().len())
                    .map_err(|_| CompactKickAdmission::BudgetExceeded)?;
                for delta in normalized_offsets(Some(entry)) {
                    normalized.push(delta);
                }
                piece.ordered[slot] = Some(normalized.into_boxed_slice());
            }
        }
        Ok(CompactKickAuthority(Arc::new(CompactKickTable {
            width: self.width,
            height: self.height,
            profile: self.profile,
            rule: self.rule,
            allow_180: self.allow_180,
            pieces,
        })))
    }
}

fn normalized_offsets(entry: Option<&KickTableEntry>) -> impl Iterator<Item = (i16, i16)> + '_ {
    entry.into_iter().flat_map(|entry| {
        let transition = entry.transition();
        entry.sequence().offsets().iter().map(move |offset| {
            normalized_kick_delta_wide(
                transition.piece(),
                transition.from(),
                transition.to(),
                offset.dx(),
                offset.dy(),
            )
        })
    })
}

impl CompactKickAuthority {
    /// Bind the full verified ordered table, independently of its display ID.
    /// This is the standard-shape all-sky contract, not fixed-spawn gameplay.
    /// Encoded index/coordinate and allocation failures remain checked refusals,
    /// never negative movement answers. A development generation budget is not
    /// silently imposed on the product's verified ordered table.
    pub(super) fn admit_profile(
        width: u8,
        height: u8,
        selected_rule: RuleProfileId,
        verified: &VerifiedKickTableProfile,
    ) -> Result<Self, CompactKickAdmission> {
        Self::admit(width, height, selected_rule, verified, None)
    }

    #[cfg(any(test, not(target_family = "wasm")))]
    pub(super) fn admit_local_order_variant(
        width: u8,
        height: u8,
        verified: &VerifiedKickTableProfile,
    ) -> Result<Self, CompactKickAdmission> {
        let supplied = verified.profile();
        let selected_rule = match supplied.id() {
            KickTableProfileId::Srs90 => RuleProfileId::Srs,
            KickTableProfileId::SrsPlus => RuleProfileId::SrsPlus,
            _ => return Err(CompactKickAdmission::Unsupported),
        };
        let canonical =
            builtin_kick_profile(supplied.id()).ok_or(CompactKickAdmission::Unsupported)?;
        // Keep the existing compact free-sky scope: this tranche only
        // permits ordering changes to connected quarter-/half-turn candidates.
        // A new candidate membership or rotation topology is unqualified.
        if supplied.id() != canonical.id()
            || supplied.source_rule() != canonical.source_rule()
            || supplied.transition_count() != canonical.transition_count()
            || !supplied.entries().iter().all(|entry| {
                let Some(expected) = canonical.sequence_for(entry.transition()) else {
                    return false;
                };
                let actual = entry.sequence().offsets();
                let expected = expected.offsets();
                actual.len() == expected.len()
                    && expected.iter().all(|kick| {
                        actual.iter().filter(|item| *item == kick).count()
                            == expected.iter().filter(|item| *item == kick).count()
                    })
            })
        {
            return Err(CompactKickAdmission::Unsupported);
        }
        Self::admit(width, height, selected_rule, verified, None)
    }

    pub(super) fn dimensions(&self) -> (u8, u8) {
        (self.0.width, self.0.height)
    }

    pub(super) fn profile_id(&self) -> KickTableProfileId {
        self.0.profile
    }

    pub(super) fn retained_bytes(&self) -> usize {
        core::mem::size_of::<CompactKickTable>()
            + 2 * core::mem::size_of::<usize>() // Arc control block.
            + self
                .0
                .pieces
                .iter()
                .flat_map(|piece| piece.ordered.iter().flatten())
                .map(|entries| core::mem::size_of_val(entries.as_ref()))
                .sum::<usize>()
    }

    /// Exact-table storage projection for all seven templates, compilation
    /// scratch, and the worker's movement caches. This includes the authority
    /// payload once; it never substitutes the nominal profile's kick lengths.
    pub(super) fn checked_reachability_peak_upper_bound_bytes(&self) -> Option<u128> {
        super::reachability::checked_authority_reachability_storage_upper_bound(self)
    }

    fn admit(
        width: u8,
        height: u8,
        selected_rule: RuleProfileId,
        verified: &VerifiedKickTableProfile,
        budget: Option<CompileBudget>,
    ) -> Result<Self, CompactKickAdmission> {
        Self::preflight(width, height, selected_rule, verified, budget)?.materialize()
    }

    fn preflight<'a>(
        width: u8,
        height: u8,
        selected_rule: RuleProfileId,
        verified: &'a VerifiedKickTableProfile,
        budget: Option<CompileBudget>,
    ) -> Result<AdmittedKickLayout<'a>, CompactKickAdmission> {
        use CompactKickAdmission::{BudgetExceeded, IndexOrCoordinateOverflow, Unsupported};

        let profile = verified.profile();
        if !(1..=10).contains(&width)
            || !(1..=6).contains(&height)
            || profile.source_rule() != selected_rule
            || !matches!(
                selected_rule,
                RuleProfileId::Srs
                    | RuleProfileId::SrsPlus
                    | RuleProfileId::SrsX
                    | RuleProfileId::Jstris180
                    | RuleProfileId::NoKick
            )
        {
            return Err(Unsupported);
        }
        // The all-sky proof uses normalized lower-left anchors. This is a
        // semantic admission premise, not a special case for a profile ID.
        let registry =
            clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry();
        for kind in PieceKind::STANDARD_TETROMINOES {
            let definition = registry.get(kind).ok_or(Unsupported)?;
            for rotation in RotationState::ALL {
                let cells = definition.shape(rotation).cells();
                if cells.iter().any(|cell| cell.x() < 0 || cell.y() < 0)
                    || !cells.iter().any(|cell| cell.x() == 0)
                    || !cells.iter().any(|cell| cell.y() == 0)
                {
                    return Err(Unsupported);
                }
            }
        }
        let allow_180 = profile.supports_180();
        let mut ordered: [BorrowedOrderedEntries<'_>; 7] = [[None; SLOTS_PER_PIECE]; 7];
        for entry in profile.entries() {
            let transition = entry.transition();
            let from = transition.from();
            let to = transition.to();
            // The interpreter requests CW, CCW and optionally 180 only.
            // An extra verified from==to record is inactive metadata, not
            // an implicit fourth movement action or a reason to reject the
            // other complete ordered lists. Equality identifies the exact
            // projection onto the active action alphabet.
            if from == to {
                continue;
            }
            let slot = usize::from(from.quarter_turns()) * 3
                + rotation_slot(from, to).ok_or(Unsupported)?;
            let piece = &mut ordered[piece_index(transition.piece())];
            let offsets = entry.sequence().offsets();
            if piece[slot].is_some() || entry.unsupported_reason().is_some() {
                return Err(Unsupported);
            }
            u32::try_from(offsets.len()).map_err(|_| IndexOrCoordinateOverflow)?;
            if budget.is_some_and(|budget| offsets.len() > budget.rotation_targets) {
                return Err(BudgetExceeded);
            }
            piece[slot] = Some(entry);
        }

        let mut maximum_candidates = [0_usize; 7];
        for (index, entries) in ordered.iter().enumerate() {
            for orientation in entries.chunks_exact(3) {
                let candidates = orientation
                    .iter()
                    .flatten()
                    .try_fold(0_usize, |count, entry| {
                        count.checked_add(entry.sequence().len())
                    })
                    .ok_or(IndexOrCoordinateOverflow)?;
                maximum_candidates[index] = maximum_candidates[index].max(candidates);
            }
        }
        // C>=H for every valid sky bound. Refuse a clearly impossible budget
        // using only lengths before scanning even one large raw kick list.
        let minimum_piece_poses = 4_usize
            .checked_mul(usize::from(width))
            .and_then(|count| count.checked_mul(usize::from(height) + 1))
            .ok_or(IndexOrCoordinateOverflow)?;
        let minimum_targets = maximum_candidates
            .iter()
            .try_fold(0_usize, |count, &maximum| {
                count.checked_add(minimum_piece_poses.checked_mul(maximum)?)
            })
            .ok_or(IndexOrCoordinateOverflow)?;
        let minimum_poses = minimum_piece_poses
            .checked_mul(7)
            .ok_or(IndexOrCoordinateOverflow)?;
        let minimum_indices = minimum_piece_poses
            .checked_mul(5)
            .and_then(|count| count.checked_add(3))
            .and_then(|count| count.checked_mul(7))
            .ok_or(IndexOrCoordinateOverflow)?;
        if budget.is_some_and(|budget| {
            minimum_poses > budget.poses
                || minimum_targets > budget.rotation_targets
                || minimum_indices > budget.index_entries
        }) {
            return Err(BudgetExceeded);
        }

        let mut ceilings = [i16::from(height); 7];
        let mut poses = 0_usize;
        let mut rotation_targets = 0_usize;
        let mut index_entries = 0_usize;
        for (index, entries) in ordered.iter().enumerate() {
            let ceiling =
                KickSkyBounds::for_ordered_sequences(|slot| normalized_offsets(entries[slot]))
                    .source_ceiling(height);
            ceilings[index] = ceiling;
            let height_with_sky =
                usize::try_from(ceiling + 1).map_err(|_| IndexOrCoordinateOverflow)?;
            let piece_poses = 4_usize
                .checked_mul(usize::from(width))
                .and_then(|count| count.checked_mul(height_with_sky))
                .ok_or(IndexOrCoordinateOverflow)?;
            // u16::MAX is the invalid state sentinel, not a real index.
            if piece_poses > usize::from(u16::MAX) {
                return Err(IndexOrCoordinateOverflow);
            }
            let piece_targets = piece_poses
                .checked_mul(maximum_candidates[index])
                .ok_or(IndexOrCoordinateOverflow)?;
            // Forward offsets and reverse bucket cursors store u32.
            u32::try_from(piece_targets).map_err(|_| IndexOrCoordinateOverflow)?;
            // Forward offsets have 3*n+1 entries; reverse offsets and the
            // concurrently retained reverse cursor each have n+1.
            let piece_indices = piece_poses
                .checked_mul(5)
                .and_then(|count| count.checked_add(3))
                .ok_or(IndexOrCoordinateOverflow)?;
            poses = poses
                .checked_add(piece_poses)
                .ok_or(IndexOrCoordinateOverflow)?;
            rotation_targets = rotation_targets
                .checked_add(piece_targets)
                .ok_or(IndexOrCoordinateOverflow)?;
            index_entries = index_entries
                .checked_add(piece_indices)
                .ok_or(IndexOrCoordinateOverflow)?;
        }
        if budget.is_some_and(|budget| {
            poses > budget.poses
                || rotation_targets > budget.rotation_targets
                || index_entries > budget.index_entries
        }) {
            return Err(BudgetExceeded);
        }
        Ok(AdmittedKickLayout {
            width,
            height,
            profile: profile.id(),
            rule: selected_rule,
            allow_180,
            ceilings,
            ordered,
        })
    }

    pub(super) fn piece(&self, piece: PieceKind) -> CompactPieceKicks<'_> {
        let piece = &self.0.pieces[piece_index(piece)];
        CompactPieceKicks {
            width: self.0.width,
            height: self.0.height,
            ceiling: piece.ceiling,
            allow_180: self.0.allow_180,
            ordered: std::array::from_fn(|slot| piece.ordered[slot].as_deref().unwrap_or(&[])),
        }
    }
}

fn rotation_slot(from: RotationState, to: RotationState) -> Option<usize> {
    if to == from.clockwise() {
        Some(0)
    } else if to == from.counter_clockwise() {
        Some(1)
    } else if to == from.rotated_180() {
        Some(2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use clearra_rules::kicks::{
        KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, KickTransition, SrsKicks,
    };

    use crate::reachability_reference::reference_spawn_locks_for_ordered_profile;

    use super::super::reachability::{
        search_reachable_locks, ReachabilityScratch, ReachabilityTemplate, ReachableLocks,
    };
    use super::*;

    const BUDGET: CompileBudget = CompileBudget {
        poses: 10_000,
        rotation_targets: 100_000,
        index_entries: 50_000,
    };

    fn verify(profile: KickTableProfile) -> VerifiedKickTableProfile {
        VerifiedKickTableProfile::try_new(profile).expect("verified ordered table")
    }

    fn replace_t_kicks(offsets: Vec<KickOffset>) -> VerifiedKickTableProfile {
        let base = SrsKicks::profile();
        let transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut entries = base.entries().to_vec();
        let selected = entries
            .iter_mut()
            .find(|entry| entry.transition() == transition)
            .unwrap();
        *selected = KickTableEntry::new(transition, KickOffsetSequence::new(offsets));
        verify(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            entries,
        ))
    }

    fn exact_locks(
        profile: &VerifiedKickTableProfile,
        authority: &CompactKickAuthority,
        board: u64,
        piece: PieceKind,
    ) -> [u64; 4] {
        let normalized = authority.piece(piece);
        let template = ReachabilityTemplate::compile_authority(authority, piece);
        let actual =
            search_reachable_locks(&template, board, &mut ReachabilityScratch::default(), None);
        assert!(actual.exhaustive);
        let expected = reference_spawn_locks_for_ordered_profile(
            normalized.width,
            normalized.height,
            board,
            piece,
            profile.profile(),
        )
        .unwrap();
        assert_locks(actual.locks, expected, normalized.width, normalized.height);
        expected
    }

    fn assert_locks(actual: ReachableLocks, expected: [u64; 4], width: u8, height: u8) {
        for rotation in RotationState::ALL {
            let expected_anchors = expected[usize::from(rotation.quarter_turns())];
            for y in 0..height {
                for x in 0..width {
                    assert_eq!(
                        actual.contains(width, rotation, x as i8, y as i8),
                        expected_anchors & (1 << (u16::from(y) * u16::from(width) + u16::from(x)))
                            != 0,
                        "anchor {rotation:?} ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn all_builtin_ordered_tables_compile_under_the_same_bound() {
        for id in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let profile = verify(builtin_kick_profile(id).unwrap().clone());
            let bound = CompactKickAuthority::admit(
                10,
                6,
                profile.profile().source_rule(),
                &profile,
                Some(BUDGET),
            )
            .unwrap();
            for piece in PieceKind::STANDARD_TETROMINOES {
                assert!(
                    KickSkyBounds::for_ordered_deltas(bound.piece(piece).ordered)
                        .uses_reduced_ceiling(),
                    "{id:?}/{piece:?}: builtin H+D premise"
                );
                for board in [0, 0x10_4800] {
                    let expected = exact_locks(&profile, &bound, board, piece);
                    let builtin = ReachabilityTemplate::compile(10, 6, piece, id);
                    let actual = search_reachable_locks(
                        &builtin,
                        board,
                        &mut ReachabilityScratch::default(),
                        None,
                    );
                    assert!(actual.exhaustive);
                    assert_locks(actual.locks, expected, 10, 6);
                }
            }
        }
    }

    #[test]
    fn same_id_different_first_kick_changes_two_line_t_i_witness() {
        const INITIAL: u64 = 0x000f_83c7;
        const T_LOCK: u64 = 0x4038;
        const I_ORIGINAL: u64 = 0x3c00;

        let canonical = verify(SrsKicks::profile());
        let base = canonical.profile();
        let mut records = base.entries().to_vec();
        records.reverse();
        let reversed_records = verify(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            records,
        ));
        let transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut offsets = base.sequence_for(transition).unwrap().offsets().to_vec();
        assert_eq!(offsets[2], KickOffset::new(1, -1));
        offsets[..=2].rotate_right(1);
        let changed = replace_t_kicks(offsets);
        let admit = |profile| {
            CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, profile, Some(BUDGET)).unwrap()
        };
        let a = admit(&canonical);
        let b = admit(&changed);
        assert_eq!(a, admit(&reversed_records));
        assert_ne!(a, b);
        assert_eq!(canonical.profile().id(), changed.profile().id());
        assert_eq!(
            canonical.profile().source_rule(),
            changed.profile().source_rule()
        );
        let reaches_t =
            |profile, bound| exact_locks(profile, bound, INITIAL, PieceKind::T)[0] & (1 << 3) != 0;
        assert!(!reaches_t(&canonical, &a));
        assert!(reaches_t(&changed, &b));
        assert!(!reaches_t(&canonical, &a));
        assert_eq!(INITIAL & T_LOCK, 0);
        let after_t = INITIAL | T_LOCK;
        assert_eq!(after_t & 0x3ff, 0x3ff);
        let second_field = after_t >> 10;
        assert_eq!(second_field, 0x3f0);
        assert_eq!(I_ORIGINAL >> 10, 0xf);
        assert_ne!(
            exact_locks(&changed, &b, second_field, PieceKind::I)[0] & 1,
            0
        );
        assert_eq!(second_field | (I_ORIGINAL >> 10), 0x3ff);
    }

    #[test]
    fn local_srs_plus_half_turn_reordering_agrees_with_independent_locks() {
        let base = SrsKicks::srs_plus_profile();
        let canonical = verify(base.clone());
        let mut entries = base.entries().to_vec();
        let entry = entries
            .iter_mut()
            .find(|entry| {
                let transition = entry.transition();
                transition.piece() == PieceKind::T
                    && transition.to() == transition.from().rotated_180()
                    && entry.sequence().len() > 1
            })
            .unwrap();
        let mut offsets = entry.sequence().offsets().to_vec();
        offsets.reverse();
        assert_ne!(offsets.as_slice(), entry.sequence().offsets());
        *entry = KickTableEntry::new(entry.transition(), KickOffsetSequence::new(offsets));
        let reordered = verify(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            entries,
        ));
        let admit =
            |profile| CompactKickAuthority::admit_local_order_variant(10, 4, profile).unwrap();
        let first = admit(&canonical);
        let second = admit(&reordered);
        assert_ne!(first, second);
        for board in [0x0f_83c7, 0x10_4800, 0x00f0_0cf0] {
            exact_locks(&canonical, &first, board, PieceKind::T);
            exact_locks(&reordered, &second, board, PieceKind::T);
        }
    }

    #[test]
    fn profiles_are_not_compiled_when_dimensions_rule_or_budget_are_unqualified() {
        use CompactKickAdmission::{BudgetExceeded, Unsupported};

        let canonical = verify(SrsKicks::profile());
        assert_eq!(
            CompactKickAuthority::admit(10, 7, RuleProfileId::Srs, &canonical, Some(BUDGET)),
            Err(Unsupported)
        );
        assert_eq!(
            CompactKickAuthority::admit(10, 2, RuleProfileId::SrsPlus, &canonical, Some(BUDGET)),
            Err(Unsupported)
        );
        for rule in [
            RuleProfileId::Asc,
            RuleProfileId::Ars,
            RuleProfileId::Custom,
        ] {
            let renamed = verify(KickTableProfile::new(
                KickTableProfileId::Imported,
                rule,
                canonical.profile().entries().to_vec(),
            ));
            assert_eq!(
                CompactKickAuthority::admit_profile(10, 2, rule, &renamed),
                Err(Unsupported),
                "transition verification does not prove {rule:?} origin/spawn semantics",
            );
        }
        for budget in [
            CompileBudget { poses: 1, ..BUDGET },
            CompileBudget {
                rotation_targets: 1,
                ..BUDGET
            },
            CompileBudget {
                index_entries: 1,
                ..BUDGET
            },
        ] {
            assert_eq!(
                CompactKickAuthority::admit(10, 2, RuleProfileId::Srs, &canonical, Some(budget)),
                Err(BudgetExceeded)
            );
        }
    }

    #[test]
    fn raw_lengths_and_expanded_sky_are_admitted_before_normalized_storage_exists() {
        // Each source list is individually below the old coarse count cap,
        // but the full pose/target product is not. Preflight owns only the
        // borrowed entries, so neither refusal can allocate a normalized copy.
        let too_many = replace_t_kicks(vec![KickOffset::new(0, 0); 2_000]);
        let too_tall = replace_t_kicks(vec![KickOffset::new(0, -100); 50]);
        for profile in [&too_many, &too_tall] {
            assert!(profile
                .profile()
                .entries()
                .iter()
                .all(|entry| { entry.sequence().len() < BUDGET.rotation_targets }));
            assert!(matches!(
                CompactKickAuthority::preflight(10, 4, RuleProfileId::Srs, profile, Some(BUDGET)),
                Err(CompactKickAdmission::BudgetExceeded),
            ));
        }
        assert!(
            CompactKickAuthority::preflight(10, 4, RuleProfileId::Srs, &too_many, None,).is_ok(),
            "an explicit fixture budget must not become a product admission limit"
        );
        // The same borrowed stage preserves a valid long ordered list; it
        // hands its proved ceiling to the only stage that may allocate it.
        let accepted = replace_t_kicks(vec![KickOffset::new(0, 0); 300]);
        let layout =
            CompactKickAuthority::preflight(10, 2, RuleProfileId::Srs, &accepted, Some(BUDGET))
                .expect("finite complete prefix fits this compiler budget");
        let ceiling = layout.ceilings[piece_index(PieceKind::T)];
        let authority = layout.materialize().unwrap();
        assert_eq!(authority.piece(PieceKind::T).ceiling, ceiling);
        exact_locks(&accepted, &authority, 0x0f_83c7, PieceKind::T);
    }

    #[test]
    fn arbitrary_raw_offset_endpoints_and_empty_sequences_match_independent_geometry() {
        for offsets in [
            vec![
                KickOffset::new(i8::MAX, i8::MAX),
                KickOffset::new(i8::MIN, i8::MIN),
                KickOffset::new(i8::MIN, i8::MAX),
                KickOffset::new(i8::MAX, i8::MIN),
                KickOffset::new(0, 0),
            ],
            Vec::new(),
        ] {
            let mut entries = Vec::new();
            for piece in PieceKind::STANDARD_TETROMINOES {
                for from in RotationState::ALL {
                    for to in [
                        from.clockwise(),
                        from.counter_clockwise(),
                        from.rotated_180(),
                    ] {
                        entries.push(KickTableEntry::new(
                            KickTransition::new(piece, from, to),
                            KickOffsetSequence::new(offsets.clone()),
                        ));
                    }
                }
            }
            let profile = verify(KickTableProfile::new(
                KickTableProfileId::Custom,
                RuleProfileId::SrsPlus,
                entries,
            ));
            let authority =
                CompactKickAuthority::admit_profile(4, 6, RuleProfileId::SrsPlus, &profile)
                    .unwrap();
            for piece in PieceKind::STANDARD_TETROMINOES {
                if !offsets.is_empty() {
                    assert!(authority.piece(piece).ceiling > i16::from(i8::MAX));
                }
                for board in [0, 0x52_5052] {
                    exact_locks(&profile, &authority, board, piece);
                }
            }
        }
    }

    #[test]
    fn inactive_self_transition_metadata_preserves_the_exact_action_owner() {
        let original = verify(SrsKicks::srs_plus_profile());
        let mut entries = original.profile().entries().to_vec();
        for piece in PieceKind::STANDARD_TETROMINOES {
            for orientation in RotationState::ALL {
                entries.push(KickTableEntry::new(
                    KickTransition::new(piece, orientation, orientation),
                    KickOffsetSequence::new(vec![
                        KickOffset::new(i8::MAX, i8::MAX),
                        KickOffset::new(i8::MIN, i8::MIN),
                    ]),
                ));
            }
        }
        assert!(entries.len() > 84);
        let annotated = verify(KickTableProfile::new(
            original.profile().id(),
            original.profile().source_rule(),
            entries,
        ));
        let first =
            CompactKickAuthority::admit_profile(10, 6, RuleProfileId::SrsPlus, &original).unwrap();
        let second =
            CompactKickAuthority::admit_profile(10, 6, RuleProfileId::SrsPlus, &annotated).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.checked_reachability_peak_upper_bound_bytes(),
            second.checked_reachability_peak_upper_bound_bytes()
        );
        for piece in [PieceKind::I, PieceKind::T, PieceKind::O] {
            let expected = exact_locks(&original, &first, 0x10_4800, piece);
            assert_eq!(exact_locks(&annotated, &second, 0x10_4800, piece), expected);
        }
    }
}
