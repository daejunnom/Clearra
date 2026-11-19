//! Conditional traversal certificates lifted to canonical placements.
//!
//! Each record pins its catalog, rule, original-row frame and result scope.
//! A complete forward traversal proves the whole lock family; a reverse
//! trace proves only its Boolean target. Both footprints include failed
//! earlier kicks. Observing the existing primitive avoids a second search.
//! An outside-mask placement is independent only while this row frame holds.

use super::super::{placement_conditions::PlacementConditionCatalog, placement_dependency};
use super::*;

const MAX_RELATIONS: usize = 128;
const MAX_STATIC_MASKS: usize = 2048;
const MAX_INFLUENCE_IDS: usize = 128 * 1024;
const OWNER_ALLOCATION_BYTES: usize =
    core::mem::size_of::<PlacementConditionCatalog>() + 2 * core::mem::size_of::<usize>();

#[cfg(test)]
#[path = "reachability_dependency_differential_tests.rs"]
mod differential_tests;

struct Relation {
    piece: PieceKind,
    frame: LocalRelationRowFrame,
    scope: RelationScope,
    source_board: u64,
    mask: u64,
    occupancy: u64,
    locks: ReachableLocks,
    // A static footprint has narrower result scope than the whole family.
    // Keep its target explicitly; it must never answer another lock query.
    target_condition: Option<StaticMask>,
    // These are concrete catalog row IDs, never kind bits or supply labels.
    influencing_placements: Vec<u32>,
}

#[derive(Clone, Copy)]
enum RelationScope {
    WholeFamily,
    Target(RotationState, i8, i8),
}

impl RelationScope {
    fn contains(self, rotation: RotationState, x: i8, y: i8) -> bool {
        match self {
            Self::WholeFamily => true,
            Self::Target(r, tx, ty) => (r, tx, ty) == (rotation, x, y),
        }
    }
}

#[derive(Clone, Copy)]
struct StaticMask {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    mask: u64,
}

#[derive(Default)]
pub(super) struct PlacementDependencyCache {
    // Pin allocation identity without retaining an obsolete catalog's buffers
    // when a reused workspace sees only hard-drop/exact-board cache hits.
    catalog: Option<std::sync::Weak<PlacementConditionCatalog>>,
    profile: Option<KickTableProfileId>,
    relations: Vec<Relation>,
    static_masks: Vec<StaticMask>,
    influence_ids: usize,
}

impl PlacementDependencyCache {
    pub(super) fn retained_bytes(&self) -> usize {
        self.relations.capacity() * core::mem::size_of::<Relation>()
            + usize::from(self.catalog.is_some()) * OWNER_ALLOCATION_BYTES
            + self.static_masks.capacity() * core::mem::size_of::<StaticMask>()
            + self
                .relations
                .iter()
                .map(|record| {
                    record.influencing_placements.capacity() * core::mem::size_of::<u32>()
                })
                .sum::<usize>()
    }

    pub(super) const fn retained_upper_bound() -> usize {
        MAX_RELATIONS * core::mem::size_of::<Relation>()
            + OWNER_ALLOCATION_BYTES
            + MAX_STATIC_MASKS * core::mem::size_of::<StaticMask>()
            + MAX_INFLUENCE_IDS * core::mem::size_of::<u32>()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lookup(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
    ) -> Option<bool> {
        let frame = self.prepare_scope(catalog, board, profile, x, y, frame)?;
        placement_dependency::record(|m| m.queries += 1);
        let mut different_frame = false;
        for relation in &self.relations {
            if relation.piece != piece || !relation.scope.contains(rotation, x, y) {
                continue;
            }
            if relation.frame != frame {
                different_frame = true;
                continue;
            }
            let family_matches = board & relation.mask == relation.occupancy;
            let target_matches = relation.target_condition.is_some_and(|target| {
                target.rotation == rotation
                    && target.x == x
                    && target.y == y
                    && (board ^ relation.source_board) & target.mask == 0
            });
            if family_matches || target_matches {
                placement_dependency::record(|m| {
                    m.reused_relations += 1;
                    m.cross_board_reuses += u64::from(board != relation.source_board);
                    m.static_reuses += u64::from(!family_matches && target_matches);
                });
                return Some(relation.locks.contains(catalog.width(), rotation, x, y));
            }
        }
        if different_frame {
            placement_dependency::record(|m| m.frame_misses += 1);
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_scope(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        profile: KickTableProfileId,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
    ) -> Option<LocalRelationRowFrame> {
        if !placement_dependency::enabled() {
            return None;
        }
        let owner = catalog.placement_conditions()?;
        let frame = frame?;
        if frame.target_height() != catalog.height()
            || !frame.accepts_physical_board(catalog.width(), board)
            || y < 0
            || y >= catalog.height() as i8
            || x < 0
            || x >= catalog.width() as i8
        {
            return None;
        }
        if self.profile != Some(profile)
            || !self
                .catalog
                .as_ref()
                .is_some_and(|previous| std::sync::Weak::ptr_eq(previous, &Arc::downgrade(owner)))
        {
            self.relations.clear();
            self.static_masks.clear();
            self.influence_ids = 0;
            self.catalog = Some(Arc::downgrade(owner));
            self.profile = Some(profile);
        }
        Some(frame)
    }

    // Observation never starts a traversal: preserve the original short
    // reverse search and its cache-admission policy. This certificate proves
    // exactly one Boolean target, including a closed negative reverse trace.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe_reverse(
        &mut self,
        catalog: &GeometryCatalog,
        template: &ReachabilityTemplate,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
        scratch: &ReachabilityScratch,
        result: &ReverseReachabilityResult,
    ) {
        let Some(frame) = self.prepare_scope(catalog, board, profile, x, y, frame) else {
            return;
        };
        let Some(mask) = reverse_dependency::reverse_lock_dependency_mask(
            template, rotation, x, y, scratch, result,
        ) else {
            return;
        };
        let mut locks = ReachableLocks::default();
        if result.reachable {
            locks.insert(catalog.width(), rotation, x, y);
        }
        placement_dependency::record(|m| {
            m.reverse_relations += 1;
            m.primitive_states += result.visited_state_count as u64;
        });
        self.remember(
            catalog,
            Relation {
                piece,
                frame,
                scope: RelationScope::Target(rotation, x, y),
                source_board: board,
                mask,
                occupancy: board & mask,
                locks,
                target_condition: None,
                influencing_placements: Vec::new(),
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe_full(
        &mut self,
        catalog: &GeometryCatalog,
        template: &ReachabilityTemplate,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
        scratch: &ReachabilityScratch,
        result: &ReachabilitySearchResult,
    ) {
        let Some(frame) = self.prepare_scope(catalog, board, profile, x, y, frame) else {
            return;
        };
        if !result.exhaustive {
            return;
        }

        // A uniform target-specific mask can be compiled without a board or
        // completed tiling. Keep it distinct from the conditional mask below:
        // its scope is ONE target, not the complete lock family.
        let static_mask = self
            .static_masks
            .iter()
            .find(|entry| {
                entry.piece == piece && entry.rotation == rotation && entry.x == x && entry.y == y
            })
            .map(|entry| entry.mask)
            .or_else(|| {
                let mask = independence::static_lock_dependency_mask(template, rotation, x, y)?;
                if self.static_masks.len() < MAX_STATIC_MASKS
                    && self.static_masks.try_reserve_exact(1).is_ok()
                {
                    self.static_masks.push(StaticMask {
                        piece,
                        rotation,
                        x,
                        y,
                        mask,
                    });
                }
                placement_dependency::record(|m| {
                    m.static_masks_compiled += 1;
                    m.static_independent_cells += u64::from(catalog.width())
                        * u64::from(catalog.height())
                        - u64::from(mask.count_ones());
                });
                Some(mask)
            });

        let mut mask = 0_u64;
        for &seed in &template.sky_seeds {
            mask |= template.state_masks[usize::from(seed)];
        }
        for &source in &scratch.queue {
            mask |= local_relation::reached_source_dependency_mask(template, usize::from(source));
        }
        placement_dependency::record(|m| {
            m.full_relations += 1;
            m.primitive_states += result.visited_state_count as u64;
        });
        self.remember(
            catalog,
            Relation {
                piece,
                frame,
                scope: RelationScope::WholeFamily,
                source_board: board,
                mask,
                occupancy: board & mask,
                locks: result.locks,
                influencing_placements: Vec::new(),
                target_condition: static_mask.map(|mask| StaticMask {
                    piece,
                    rotation,
                    x,
                    y,
                    mask,
                }),
            },
        );
    }

    fn remember(&mut self, catalog: &GeometryCatalog, mut relation: Relation) {
        placement_dependency::record(|m| m.generated_relations += 1);
        let Some(owner) = catalog.placement_conditions() else {
            return;
        };
        if let Some(influencing_placements) = owner.influencing_placements(
            relation.mask,
            u16::from(relation.frame.deleted_original_rows()),
        ) {
            placement_dependency::record(|m| {
                m.independent_placement_edges += owner
                    .row_count()
                    .saturating_sub(influencing_placements.len())
                    as u64;
            });
            if self.relations.len() < MAX_RELATIONS
                && self
                    .influence_ids
                    .saturating_add(influencing_placements.capacity())
                    <= MAX_INFLUENCE_IDS
                && self.relations.try_reserve_exact(1).is_ok()
            {
                self.influence_ids += influencing_placements.capacity();
                relation.influencing_placements = influencing_placements;
                self.relations.push(relation);
            }
        }
    }

    // Proof tests deliberately build a complete source family. Production
    // queries call lookup/observe and never pay an extra BFS for a certificate.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn query(
        &mut self,
        catalog: &GeometryCatalog,
        template: &ReachabilityTemplate,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
        scratch: &mut ReachabilityScratch,
    ) -> Option<(bool, usize)> {
        if let Some(reachable) = self.lookup(catalog, board, piece, profile, rotation, x, y, frame)
        {
            return Some((reachable, 0));
        }
        self.prepare_scope(catalog, board, profile, x, y, frame)?;
        let result = search_reachable_locks(template, board, scratch, None);
        self.observe_full(
            catalog, template, board, piece, profile, rotation, x, y, frame, scratch, &result,
        );
        Some((
            result.locks.contains(catalog.width(), rotation, x, y),
            result.visited_state_count,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::placement_dependency::with_mode;
    use super::*;
    use crate::reachability_reference::reference_spawn_lock_anchors;

    fn catalog(height: u8, initial: u64) -> GeometryCatalog {
        let full = (1_u64 << (10 * height)) - 1;
        GeometryCatalog::compile_for_required_cells_on_dimensions(
            10,
            height,
            initial,
            full & !initial,
        )
        .expect("bounded prototype catalog")
    }

    #[test]
    fn cache_identity_does_not_keep_an_obsolete_catalog_payload_alive() {
        with_mode(true, || {
            let catalog = catalog(4, 0);
            let owner = Arc::downgrade(catalog.placement_conditions().unwrap());
            let mut cache = PlacementDependencyCache::default();
            assert!(cache
                .prepare_scope(
                    &catalog,
                    0,
                    KickTableProfileId::SrsPlus,
                    0,
                    0,
                    Some(LocalRelationRowFrame::new(4, 0).unwrap()),
                )
                .is_some());
            drop(catalog);
            assert!(
                owner.upgrade().is_none(),
                "cache must not retain catalog buffers"
            );
            assert_eq!(cache.retained_bytes(), OWNER_ALLOCATION_BYTES);
        });
    }

    #[test]
    fn observed_reverse_results_reuse_only_their_exact_target_and_row_frame() {
        let (_, metrics) = with_mode(true, || {
            let board = (0x1ff_u64 << 30) | (7_u64 << 7);
            let catalog = catalog(6, board);
            let frame = LocalRelationRowFrame::new(6, 0).unwrap();
            let profile = KickTableProfileId::SrsPlus;
            let template = ReachabilityTemplate::compile(10, 6, PieceKind::O, profile);
            for (x, y, expected) in [(2, 4, true), (0, 0, false)] {
                let mut cache = PlacementDependencyCache::default();
                let mut scratch = ReachabilityScratch::default();
                let result = reverse_lock_reachable(
                    &template,
                    board,
                    RotationState::Zero,
                    x,
                    y,
                    &mut scratch,
                );
                assert_eq!(result.reachable, expected);
                cache.observe_reverse(
                    &catalog,
                    &template,
                    board,
                    PieceKind::O,
                    profile,
                    RotationState::Zero,
                    x,
                    y,
                    Some(frame),
                    &scratch,
                    &result,
                );
                assert_eq!(cache.relations.len(), 1);
                let mask = cache.relations[0].mask;
                let outside = ((1_u64 << 60) - 1) & !mask;
                assert_ne!(outside, 0, "nonvacuous target certificate");
                let changed = board ^ (1_u64 << outside.trailing_zeros());
                let oracle =
                    reference_spawn_lock_anchors(10, 6, changed, PieceKind::O, profile).unwrap();
                assert_eq!(oracle[0] & (1_u64 << (y * 10 + x)) != 0, expected);
                assert_eq!(
                    cache.lookup(
                        &catalog,
                        changed,
                        PieceKind::O,
                        profile,
                        RotationState::Zero,
                        x,
                        y,
                        Some(frame),
                    ),
                    Some(expected)
                );
                assert_eq!(
                    cache.lookup(
                        &catalog,
                        changed,
                        PieceKind::O,
                        profile,
                        RotationState::Right,
                        x,
                        y,
                        Some(frame),
                    ),
                    None,
                    "one Boolean proof cannot answer another pose"
                );
                assert_eq!(
                    cache.lookup(
                        &catalog,
                        0,
                        PieceKind::O,
                        profile,
                        RotationState::Zero,
                        0,
                        0,
                        Some(LocalRelationRowFrame::new(6, 1).unwrap()),
                    ),
                    None,
                    "row deletion invalidates the physical frame"
                );
            }
        });
        assert_eq!(metrics.reverse_relations, 2);
        assert_eq!(metrics.full_relations, 0);
        assert_eq!(metrics.cross_board_reuses, 2);
    }

    #[test]
    fn placement_dependency_closed_family_reuses_across_independent_same_kind_placements() {
        let (_, metrics) = with_mode(true, || {
            // A nine-cell shelf leaves no two-wide O entry below it. Changes
            // in that closed lower region are irrelevant to every O lock.
            let shelf = (0x1ff_u64 << 30) | (7_u64 << 7);
            let added_o = (3_u64 << 2) | (3_u64 << 12);
            let catalog = catalog(6, shelf);
            let frame = LocalRelationRowFrame::new(6, 0).unwrap();
            for profile in [
                KickTableProfileId::Srs90,
                KickTableProfileId::SrsPlus,
                KickTableProfileId::SrsX,
                KickTableProfileId::Jstris180,
                KickTableProfileId::NoKick,
            ] {
                let template = ReachabilityTemplate::compile(10, 6, PieceKind::O, profile);
                let mut cache = PlacementDependencyCache::default();
                let mut scratch = ReachabilityScratch::default();
                assert!(
                    cache
                        .query(
                            &catalog,
                            &template,
                            shelf,
                            PieceKind::O,
                            profile,
                            RotationState::Zero,
                            2,
                            4,
                            Some(frame),
                            &mut scratch
                        )
                        .unwrap()
                        .0
                );
                assert_eq!(cache.relations.len(), 1);
                let record = &cache.relations[0];
                assert_eq!(
                    record.mask & added_o,
                    0,
                    "proved disjoint, not inferred from distance"
                );
                let added_id = catalog.skeleton_id(PieceKind::O, added_o).unwrap();
                assert!(!record.influencing_placements.contains(&added_id));
                for board in [shelf, shelf | added_o] {
                    let reference =
                        reference_spawn_lock_anchors(10, 6, board, PieceKind::O, profile).unwrap();
                    for rotation in RotationState::ALL {
                        for y in 0..6 {
                            for x in 0..10 {
                                let expected = reference[rotation.quarter_turns() as usize]
                                    & (1_u64 << (y as usize * 10 + x as usize))
                                    != 0;
                                let (observed, visited) = cache
                                    .query(
                                        &catalog,
                                        &template,
                                        board,
                                        PieceKind::O,
                                        profile,
                                        rotation,
                                        x,
                                        y,
                                        Some(frame),
                                        &mut scratch,
                                    )
                                    .unwrap();
                                assert_eq!(
                                    observed, expected,
                                    "{profile:?}/{rotation:?}@({x},{y})"
                                );
                                assert_eq!(
                                    visited, 0,
                                    "the certificate must really bypass traversal"
                                );
                            }
                        }
                    }
                }
            }
        });
        assert_eq!(metrics.generated_relations, 5);
        assert_eq!(metrics.reused_relations, 5 * 2 * 4 * 6 * 10);
        assert_eq!(metrics.cross_board_reuses, 5 * 4 * 6 * 10);
        assert!(metrics.independent_placement_edges > 0);
        eprintln!("closed-family independence: {metrics:?}");
    }

    #[test]
    fn placement_dependency_changed_original_rows_and_catalog_do_not_reuse_old_proofs() {
        let (_, metrics) = with_mode(true, || {
            let source = catalog(4, 0);
            let profile = KickTableProfileId::SrsPlus;
            let template = ReachabilityTemplate::compile(10, 4, PieceKind::O, profile);
            let mut cache = PlacementDependencyCache::default();
            let mut scratch = ReachabilityScratch::default();
            for deleted in [0, 1, 2] {
                let (_, visited) = cache
                    .query(
                        &source,
                        &template,
                        0,
                        PieceKind::O,
                        profile,
                        RotationState::Zero,
                        0,
                        0,
                        LocalRelationRowFrame::new(4, deleted),
                        &mut scratch,
                    )
                    .unwrap();
                assert!(
                    visited > 0,
                    "a different original-row frame is a different proof scope"
                );
            }
            // Even exactly equal metadata has a new owner. No hash-only hit.
            let other = catalog(4, 0);
            assert!(
                cache
                    .query(
                        &other,
                        &template,
                        0,
                        PieceKind::O,
                        profile,
                        RotationState::Zero,
                        0,
                        0,
                        LocalRelationRowFrame::new(4, 0),
                        &mut scratch
                    )
                    .unwrap()
                    .1
                    > 0
            );
            assert!(cache
                .query(
                    &other,
                    &template,
                    1_u64 << 35,
                    PieceKind::O,
                    profile,
                    RotationState::Zero,
                    0,
                    0,
                    LocalRelationRowFrame::new(4, 1),
                    &mut scratch
                )
                .is_none());
            assert!(cache
                .query(
                    &other,
                    &template,
                    0,
                    PieceKind::O,
                    profile,
                    RotationState::Zero,
                    0,
                    0,
                    None,
                    &mut scratch
                )
                .is_none());
        });
        assert_eq!(metrics.generated_relations, 4);
        assert_eq!(metrics.reused_relations, 0);
        assert_eq!(metrics.frame_misses, 2);
    }

    #[test]
    fn placement_dependency_failed_earlier_kicks_are_not_independent() {
        let (_, metrics) = with_mode(true, || {
            let cells = |values: &[(u8, u8)]| {
                values
                    .iter()
                    .fold(0_u64, |mask, &(x, y)| mask | (1_u64 << (y * 10 + x)))
            };
            // The existing five-wide triple-kick counterexample, bounded on
            // its right by an actual solid wall inside a ten-wide field.
            let wall = (0..6).fold(0_u64, |mask, y| mask | (0x3e0_u64 << (10 * y)));
            let without_j = wall
                | cells(&[
                    (0, 0),
                    (1, 0),
                    (0, 1),
                    (0, 2),
                    (3, 0),
                    (4, 0),
                    (2, 1),
                    (3, 1),
                    (3, 2),
                    (4, 2),
                    (2, 3),
                    (3, 3),
                    (4, 1),
                    (4, 3),
                ]);
            let j = cells(&[(0, 3), (0, 4), (0, 5), (1, 5)]);
            let source = catalog(6, without_j);
            let profile = KickTableProfileId::SrsPlus;
            let template = ReachabilityTemplate::compile(10, 6, PieceKind::T, profile);
            let mut cache = PlacementDependencyCache::default();
            let mut scratch = ReachabilityScratch::default();
            let frame = LocalRelationRowFrame::new(6, 0);
            for (board, expected) in [(without_j, false), (without_j | j, true)] {
                let reference =
                    reference_spawn_lock_anchors(10, 6, board, PieceKind::T, profile).unwrap();
                assert_eq!(
                    reference[RotationState::Right.quarter_turns() as usize] & (1 << 11) != 0,
                    expected
                );
                let (observed, visited) = cache
                    .query(
                        &source,
                        &template,
                        board,
                        PieceKind::T,
                        profile,
                        RotationState::Right,
                        1,
                        1,
                        frame,
                        &mut scratch,
                    )
                    .unwrap();
                assert_eq!(observed, expected);
                assert!(
                    visited > 0,
                    "dependent J must invalidate the prior condition"
                );
            }
            assert_ne!(cache.relations[0].mask & j, 0);
            assert_ne!(cache.relations[0].target_condition.unwrap().mask & j, 0);
        });
        assert_eq!(metrics.generated_relations, 2);
        assert_eq!(metrics.reused_relations, 0);
    }

    #[test]
    fn placement_dependency_disabled_mode_uses_existing_path() {
        let (_, metrics) = with_mode(false, || {
            let source = catalog(2, 0);
            let template =
                ReachabilityTemplate::compile(10, 2, PieceKind::O, KickTableProfileId::Srs90);
            let mut cache = PlacementDependencyCache::default();
            let mut scratch = ReachabilityScratch::default();
            assert!(cache
                .query(
                    &source,
                    &template,
                    0,
                    PieceKind::O,
                    KickTableProfileId::Srs90,
                    RotationState::Zero,
                    0,
                    0,
                    LocalRelationRowFrame::new(2, 0),
                    &mut scratch
                )
                .is_none());
        });
        assert_eq!(
            metrics,
            placement_dependency::PlacementDependencyMetrics::default()
        );
    }
}
