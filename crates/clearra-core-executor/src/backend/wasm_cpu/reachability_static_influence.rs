//! Exact target-query quotient by a board-independent movement footprint.
//!
//! A masked key never asserts reachability by itself. It reuses an already
//! exact Boolean answer under the same immutable movement template. Full-board
//! masks skip this extra cache. O's direct row algebra needs no cached answer.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StaticInfluenceMetrics {
    pub queries: usize,
    pub reachable: usize,
    pub unreachable: usize,
    pub row_steps: usize,
    pub cache_hits: usize,
    pub masked_queries: usize,
    pub ignored_cells: usize,
    pub mask_compilations: usize,
}

impl StaticInfluenceMetrics {
    pub(crate) fn accumulate(&mut self, next: Self) {
        self.queries = self.queries.saturating_add(next.queries);
        self.reachable = self.reachable.saturating_add(next.reachable);
        self.unreachable = self.unreachable.saturating_add(next.unreachable);
        self.row_steps = self.row_steps.saturating_add(next.row_steps);
        self.cache_hits = self.cache_hits.saturating_add(next.cache_hits);
        self.masked_queries = self.masked_queries.saturating_add(next.masked_queries);
        self.ignored_cells = self.ignored_cells.saturating_add(next.ignored_cells);
        self.mask_compilations = self
            .mask_compilations
            .saturating_add(next.mask_compilations);
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn answer(&mut self, reachable: bool, row_steps: usize, cache_hit: bool) {
        self.queries = self.queries.saturating_add(1);
        self.reachable = self.reachable.saturating_add(usize::from(reachable));
        self.unreachable = self.unreachable.saturating_add(usize::from(!reachable));
        self.row_steps = self.row_steps.saturating_add(row_steps);
        self.cache_hits = self.cache_hits.saturating_add(usize::from(cache_hit));
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
mod experiment {
    use std::{cell::Cell, sync::Weak};

    use super::super::*;
    use super::StaticInfluenceMetrics;

    const CACHE_SETS: usize = 256;
    const CACHE_WAYS: usize = 4;

    thread_local! {
        static TEST_MODE: Cell<Option<bool>> = const { Cell::new(None) };
    }

    pub(crate) fn enabled() -> bool {
        if let Some(mode) = TEST_MODE.with(Cell::get) {
            return mode;
        }
        #[cfg(feature = "local-search-ab")]
        {
            static MODE: OnceLock<bool> = OnceLock::new();
            *MODE.get_or_init(|| {
                std::env::var("CLEARRA_STATIC_INFLUENCE").is_ok_and(|value| value == "1")
            })
        }
        #[cfg(not(feature = "local-search-ab"))]
        {
            false
        }
    }

    #[cfg(test)]
    pub(crate) fn with_mode<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
        struct Restore(Option<bool>);
        impl Drop for Restore {
            fn drop(&mut self) {
                TEST_MODE.with(|mode| mode.set(self.0));
            }
        }
        let _restore = Restore(TEST_MODE.with(|mode| mode.replace(Some(enabled))));
        action()
    }

    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    struct Entry {
        occupancy: u64,
        pose: u16,
        piece: u8,
        // 0 = unused; 1 = exact negative; 2 = exact positive.
        result: u8,
        reserved: [u8; 4],
    }

    #[derive(Clone, Copy, Default)]
    #[repr(C, align(64))]
    struct CacheSet([Entry; CACHE_WAYS]);

    const _: () = assert!(core::mem::size_of::<Entry>() == 16);
    const _: () = assert!(core::mem::size_of::<CacheSet>() == 64);

    #[derive(Clone, Copy)]
    // Internal ticket: consume before changing an owner. The production
    // caller holds one fixed template throughout prepare/lookup/insert.
    pub(crate) struct Query {
        occupancy: u64,
        pose: u16,
        piece: u8,
        set: usize,
    }

    #[derive(Default)]
    pub(crate) struct StaticInfluenceCache {
        // A Weak pins allocation identity without retaining old pose buffers.
        owners: [Option<Weak<ReachabilityTemplate>>; 7],
        sets: Vec<CacheSet>,
    }

    impl StaticInfluenceCache {
        pub(crate) const fn retained_upper_bound() -> usize {
            CACHE_SETS * core::mem::size_of::<CacheSet>()
                + 7 * (core::mem::size_of::<ReachabilityTemplate>()
                    + 2 * core::mem::size_of::<usize>())
        }

        pub(crate) fn retained_bytes(&self) -> usize {
            self.sets.capacity() * core::mem::size_of::<CacheSet>()
                + self.owners.iter().filter(|owner| owner.is_some()).count()
                    * (core::mem::size_of::<ReachabilityTemplate>()
                        + 2 * core::mem::size_of::<usize>())
        }

        #[allow(clippy::too_many_arguments)]
        pub(crate) fn prepare(
            &mut self,
            template: &Arc<ReachabilityTemplate>,
            board: u64,
            piece: PieceKind,
            rotation: RotationState,
            x: i8,
            y: i8,
            metrics: &mut StaticInfluenceMetrics,
        ) -> Option<Query> {
            let cells = usize::from(template.width) * usize::from(template.height);
            if !(1..=64).contains(&cells)
                || x < 0
                || y < 0
                || x >= template.width as i8
                || y >= template.height as i8
            {
                return None;
            }
            let all = u64::MAX >> (64 - cells);
            if board & !all != 0 {
                return None;
            }
            let pose = (usize::from(rotation.quarter_turns()) * usize::from(template.height)
                + y as usize)
                * usize::from(template.width)
                + x as usize;
            let slot = template.static_dependency_masks.get(pose)?;
            let mut compiled = false;
            let mask = *slot.get_or_init(|| {
                compiled = true;
                independence::static_lock_dependency_mask(template, rotation, x, y)
            });
            metrics.mask_compilations = metrics
                .mask_compilations
                .saturating_add(usize::from(compiled));
            let mask = mask?;
            if mask == all || mask & !all != 0 {
                return None;
            }
            let piece = piece_index(piece) as u8;
            let owner = &mut self.owners[usize::from(piece)];
            if !owner
                .as_ref()
                .is_some_and(|previous| previous.as_ptr() == Arc::as_ptr(template))
            {
                self.sets.fill(CacheSet::default());
                *owner = Some(Arc::downgrade(template));
            }
            let occupancy = board & mask;
            let pose = u16::try_from(pose).ok()?;
            let hash = mix_digest(occupancy, u64::from(pose) | (u64::from(piece) << 16));
            metrics.masked_queries = metrics.masked_queries.saturating_add(1);
            metrics.ignored_cells = metrics
                .ignored_cells
                .saturating_add((all & !mask).count_ones() as usize);
            Some(Query {
                occupancy,
                pose,
                piece,
                set: hash as usize & (CACHE_SETS - 1),
            })
        }

        pub(crate) fn lookup(&self, query: Query) -> Option<bool> {
            self.sets.get(query.set)?.0.iter().find_map(|entry| {
                (entry.result != 0
                    && entry.occupancy == query.occupancy
                    && entry.pose == query.pose
                    && entry.piece == query.piece)
                    .then_some(entry.result == 2)
            })
        }

        /// Store only a completed Boolean target result. Never mark a whole
        /// family exhaustive or store an unqualified hard-drop failure.
        pub(crate) fn insert(&mut self, query: Query, reachable: bool) {
            if self.sets.is_empty() {
                if self.sets.try_reserve_exact(CACHE_SETS).is_err() {
                    return;
                }
                self.sets.resize(CACHE_SETS, CacheSet::default());
            }
            let entries = &mut self.sets[query.set].0;
            let index = entries
                .iter()
                .position(|entry| {
                    entry.result == 0
                        || (entry.occupancy == query.occupancy
                            && entry.pose == query.pose
                            && entry.piece == query.piece)
                })
                .unwrap_or_else(|| {
                    entries.rotate_right(1);
                    0
                });
            entries[index] = Entry {
                occupancy: query.occupancy,
                pose: query.pose,
                piece: query.piece,
                result: if reachable { 2 } else { 1 },
                reserved: [0; 4],
            };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::reachability_reference::reference_spawn_lock_anchors;

        #[test]
        fn normalized_target_reuses_exact_positive_and_negative_outside_the_mask() {
            with_mode(true, || {
                let template = Arc::new(ReachabilityTemplate::compile(
                    4,
                    4,
                    PieceKind::O,
                    KickTableProfileId::Srs90,
                ));
                let mut cache = StaticInfluenceCache::default();
                let mut metrics = StaticInfluenceMetrics::default();
                // This target observes 0xff60; bottom row and two side support
                // cells are irrelevant. Check both complete Boolean outcomes.
                for (board, expected) in [(1_u64 << 5, true), (0, false)] {
                    let query = cache
                        .prepare(
                            &template,
                            board,
                            PieceKind::O,
                            RotationState::Zero,
                            1,
                            2,
                            &mut metrics,
                        )
                        .unwrap();
                    cache.insert(query, expected);
                    for outside in [1, 15, 0x90, 0x9f] {
                        let changed = board | outside;
                        let oracle = reference_spawn_lock_anchors(
                            4,
                            4,
                            changed,
                            PieceKind::O,
                            KickTableProfileId::Srs90,
                        )
                        .unwrap();
                        assert_eq!(oracle[0] & (1 << 9) != 0, expected);
                        let next = cache
                            .prepare(
                                &template,
                                changed,
                                PieceKind::O,
                                RotationState::Zero,
                                1,
                                2,
                                &mut metrics,
                            )
                            .unwrap();
                        assert_eq!(cache.lookup(next), Some(expected));
                    }
                }
                assert_eq!(metrics.mask_compilations, 1);
                assert!(cache.retained_bytes() <= StaticInfluenceCache::retained_upper_bound());
                assert_eq!(core::mem::align_of::<CacheSet>(), 64);
            });
        }

        #[test]
        fn target_scope_owner_and_full_board_admission_are_preserved() {
            with_mode(true, || {
                let first = Arc::new(ReachabilityTemplate::compile(
                    4,
                    4,
                    PieceKind::O,
                    KickTableProfileId::Srs90,
                ));
                let mut cache = StaticInfluenceCache::default();
                let mut metrics = StaticInfluenceMetrics::default();
                let query = cache
                    .prepare(
                        &first,
                        1 << 5,
                        PieceKind::O,
                        RotationState::Zero,
                        1,
                        2,
                        &mut metrics,
                    )
                    .unwrap();
                cache.insert(query, true);
                let different = cache
                    .prepare(
                        &first,
                        1 << 5,
                        PieceKind::O,
                        RotationState::Right,
                        1,
                        2,
                        &mut metrics,
                    )
                    .unwrap();
                assert_eq!(cache.lookup(different), None);
                let second = Arc::new(ReachabilityTemplate::compile(
                    4,
                    4,
                    PieceKind::O,
                    KickTableProfileId::Srs90,
                ));
                let replaced = cache
                    .prepare(
                        &second,
                        1 << 5,
                        PieceKind::O,
                        RotationState::Zero,
                        1,
                        2,
                        &mut metrics,
                    )
                    .unwrap();
                assert_eq!(cache.lookup(replaced), None, "owner identity is exact");
                assert!(
                    cache
                        .prepare(
                            &second,
                            0,
                            PieceKind::O,
                            RotationState::Zero,
                            1,
                            0,
                            &mut metrics
                        )
                        .is_none(),
                    "whole-board mask adds no extra cache lookup"
                );
                assert!(cache
                    .prepare(
                        &second,
                        1 << 16,
                        PieceKind::O,
                        RotationState::Zero,
                        1,
                        2,
                        &mut metrics
                    )
                    .is_none());
            });
        }
    }
}

#[cfg(test)]
pub(crate) use experiment::with_mode;
#[cfg(any(test, feature = "local-search-ab"))]
pub(super) use experiment::{enabled, StaticInfluenceCache};
