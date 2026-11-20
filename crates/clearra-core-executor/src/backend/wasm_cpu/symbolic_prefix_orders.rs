//! A strict bounded Boolean solver for one exact Board64 PC tiling.
//!
//! It never calls BuildUp or concrete-board pose search. It prices construction
//! separately from order-language consumption; bounded refusal isn't a miss.

use core::cell::{Cell, RefCell};
use std::sync::Arc;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_rules::kicks::KickTableProfileId;

use crate::performance::{ExecutorSearchStage, SearchStageSpan};

use super::{
    catalog::{GeometryCatalog, InstantiatedRealization},
    geometry::GeometryCandidate,
    kick_profiles::builtin_kick_profile,
    piece_order_language::PieceOrderLanguageCache,
    placement_conditions::AtomicRowProjection,
    placement_poset::CandidateGeometry,
    reachability::{harddrop_path_mask, PrefixPoseWork, ReachabilityWorkspace},
    symbolic_prefix_domain::{PrefixDomain, MAX_OPERATIONS, MAX_PREFIX_WORDS},
    symbolic_prefix_lanes::PrefixLaneBatch,
    WasmExactSearchError,
};

const MAX_FRAMES: usize = 64;
const CELL_PLANES: usize = 64;
const MAX_CATALOG_STATIC_GUARD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SymbolicPrefixWork {
    pub candidates_attempted: usize,
    pub candidates_certified: usize,
    pub represented_prefixes: u64,
    pub assignment_words: usize,
    pub frame_domain_splits: usize,
    pub frame_count: usize,
    pub occupancy_plane_terms: usize,
    pub frame_kind_groups: usize,
    pub target_operations: usize,
    pub concrete_guards: usize,
    pub guard_cell_terms: usize,
    pub projected_locks: usize,
    pub ungrounded_rejections: usize,
    pub harddrop_certificates: usize,
    pub tested_realizations: usize,
    pub composition_words: usize,
    pub upper_composition_words: usize,
    pub upper_pruned_unknown_edges: u64,
    pub upper_remaining_unknown_edges: u64,
    pub subsumed_lanes: u64,
    pub upper_replayed_frames: usize,
    pub upper_reloaded_placements: usize,
    pub catalog_static_guard_hits: u64,
    pub catalog_static_guard_bytes: usize,
    pub packed_frame_projections: usize,
    pub packed_segments: usize,
    pub language_states: u64,
    pub language_transitions: usize,
    pub frontier_compilations: u64,
    pub pose: PrefixPoseWork,
}

thread_local! {
    static STRICT_WORK: RefCell<Option<SymbolicPrefixWork>> = const { RefCell::new(None) };
    static USE_TARGET_REVERSE: Cell<bool> = const { Cell::new(false) };
    static USE_UPPER_BOUND: Cell<bool> = const { Cell::new(false) };
    static PACK_UPPER_LANES: Cell<bool> = const { Cell::new(false) };
    static LEGACY_UPPER: Cell<bool> = const { Cell::new(false) };
    static CATALOG_STATIC_GUARDS: RefCell<Option<Arc<CatalogStaticGuards>>> =
        const { RefCell::new(None) };
}

pub(super) fn with_catalog_static_guards<T>(
    table: Arc<CatalogStaticGuards>,
    run: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Arc<CatalogStaticGuards>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CATALOG_STATIC_GUARDS.with(|slot| slot.replace(self.0.take()));
        }
    }
    let _restore = Restore(CATALOG_STATIC_GUARDS.with(|slot| slot.replace(Some(table))));
    run()
}

#[cfg(all(
    feature = "local-search-ab",
    feature = "parallel",
    not(target_family = "wasm")
))]
pub(super) fn with_legacy_upper<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY_UPPER.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(LEGACY_UPPER.with(|slot| slot.replace(enabled)));
    run()
}

pub(super) fn with_packed_upper_target<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            PACK_UPPER_LANES.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(PACK_UPPER_LANES.with(|mode| mode.replace(true)));
    with_upper_bound(|| with_target_reverse(run))
}
pub(super) fn with_upper_bound<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            USE_UPPER_BOUND.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(USE_UPPER_BOUND.with(|mode| mode.replace(true)));
    run()
}

pub(super) fn with_target_reverse<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            USE_TARGET_REVERSE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(USE_TARGET_REVERSE.with(|mode| mode.replace(true)));
    run()
}

pub(super) fn with_strict_prefix_order<T>(run: impl FnOnce() -> T) -> (T, SymbolicPrefixWork) {
    struct Restore(Option<SymbolicPrefixWork>);
    impl Drop for Restore {
        fn drop(&mut self) {
            STRICT_WORK.with(|slot| slot.replace(self.0));
        }
    }
    let _restore = Restore(STRICT_WORK.with(|slot| slot.replace(Some(Default::default()))));
    (run(), STRICT_WORK.with(|slot| slot.borrow().unwrap()))
}

pub(super) fn in_strict_prefix_order() -> bool {
    STRICT_WORK.with(|slot| slot.borrow().is_some())
}

pub(super) fn record_strict_prefix_work<T>(run: impl FnOnce(&mut SymbolicPrefixWork) -> T) -> T {
    STRICT_WORK.with(|slot| {
        run(slot
            .borrow_mut()
            .as_mut()
            .expect("strict prefix experiment"))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SymbolicPrefixRefusal {
    Unsupported,
    RelationLimit,
    InconsistentProjection,
    Cancelled,
}

pub(super) struct SymbolicPrefixOrders {
    // A bit represents one executed subset, not a replay or duplicate pose.
    permitted_parents: [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
    pieces: [PieceKind; MAX_OPERATIONS],
    operation_count: usize,
    active_prefixes: [u64; MAX_PREFIX_WORDS],
    #[cfg(test)]
    complete_language_only: bool,
}

impl SymbolicPrefixOrders {
    // Restrict true preconditions to the original-row frame that admits the
    // concrete ILC realization. Equal physical boards across frames aren't
    // evidence that the same operations or coordinates are legal.
    #[cfg(test)]
    pub(super) fn compile(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        profile: KickTableProfileId,
        control: &ExecutionControl,
        work: &mut SymbolicPrefixWork,
    ) -> Result<Self, SymbolicPrefixRefusal> {
        Self::compile_with_workspace(
            catalog,
            candidate,
            profile,
            control,
            &mut ReachabilityWorkspace::default(),
            work,
            0,
        )
    }

    pub(super) fn compile_with_workspace(
        catalog: &GeometryCatalog,
        candidate: &GeometryCandidate,
        profile: KickTableProfileId,
        control: &ExecutionControl,
        workspace: &mut ReachabilityWorkspace,
        work: &mut SymbolicPrefixWork,
        profile_scale: u64,
    ) -> Result<Self, SymbolicPrefixRefusal> {
        use SymbolicPrefixRefusal::{Cancelled, InconsistentProjection, Unsupported};

        if control.is_cancelled() {
            return Err(Cancelled);
        }
        let upper_span =
            SearchStageSpan::begin_scaled(ExecutorSearchStage::StrictPrefixUpper, profile_scale);
        let operation_count = candidate.row_ids().len();
        let domain = PrefixDomain::new(operation_count).ok_or(Unsupported)?;
        if builtin_kick_profile(profile).is_none() {
            return Err(Unsupported);
        }
        let geometry =
            CandidateGeometry::for_symbolic_prefix(catalog, candidate).ok_or(Unsupported)?;
        let mut pieces = [PieceKind::I; MAX_OPERATIONS];
        pieces[..operation_count].copy_from_slice(geometry.pieces());
        workspace.configure_kick_profile(profile, false);
        let floor = (1_u64 << catalog.width()) - 1;
        let target_reverse = USE_TARGET_REVERSE.with(Cell::get);
        let bound = USE_UPPER_BOUND.with(Cell::get);
        let mut permitted_parents = [[0_u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        let mut possible_parents = [[0_u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        let mut frames_by_word = [[0_u64; MAX_FRAMES]; MAX_PREFIX_WORDS];
        if bound && !LEGACY_UPPER.with(Cell::get) {
            CATALOG_STATIC_GUARDS.with(|slot| {
                let table = slot.borrow();
                let table = table.as_deref();
                if let Some(table) = table {
                    if !core::ptr::eq(table.catalog.as_ref(), catalog) {
                        return Err(InconsistentProjection);
                    }
                    // This byte count belongs to one shared table, not one worker.
                    work.catalog_static_guard_bytes = table.logical_bytes();
                }
                UpperFrameGuards {
                    catalog,
                    candidate,
                    geometry: &geometry,
                    domain: &domain,
                    table,
                }
                .compile(
                    &mut permitted_parents,
                    &mut possible_parents,
                    &mut frames_by_word,
                    control,
                    work,
                )
            })?;
        } else {
            for word in 0..domain.word_count() {
                let owners = domain.word_owners(word);
                let frame_domains =
                    compile_frame_domains(&geometry, &owners, domain.nonterminal_lanes(word), work);
                if bound {
                    frames_by_word[word] = frame_domains;
                }
                work.assignment_words += 1;
                for (deleted, lanes) in frame_domains
                    .into_iter()
                    .enumerate()
                    .filter(|(_, lanes)| *lanes != 0)
                {
                    if control.is_cancelled() {
                        return Err(Cancelled);
                    }
                    work.frame_count += 1;
                    let (initial, placements) = geometry.symbolic_frame(deleted as u16);
                    let occupied = occupancy_planes(
                        initial,
                        &placements[..operation_count],
                        &owners,
                        lanes,
                        work,
                    );
                    let mut locks: [Vec<InstantiatedRealization>; MAX_OPERATIONS] =
                        core::array::from_fn(|_| Vec::new());
                    let mut unresolved = [0; MAX_OPERATIONS];
                    for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
                        let applicable = lanes & !owners[operation];
                        if applicable == 0 {
                            continue;
                        }
                        let mut realizations = catalog.instantiations(row_id, deleted as u16);
                        let Some(first) = realizations.next() else {
                            continue;
                        };
                        let footprint = placements[operation];
                        work.projected_locks += 1;
                        work.concrete_guards += applicable.count_ones() as usize;
                        if footprint.count_ones() != 4
                            || occupied_by_any(&occupied, footprint, work) & applicable != 0
                        {
                            return Err(InconsistentProjection);
                        }
                        let grounded = if footprint & floor != 0 {
                            applicable
                        } else {
                            applicable
                                & occupied_by_any(&occupied, footprint >> catalog.width(), work)
                        };
                        let corridor = harddrop_path_mask(footprint, catalog.width());
                        let direct = grounded & !occupied_by_any(&occupied, corridor, work);
                        work.ungrounded_rejections +=
                            (applicable & !grounded).count_ones() as usize;
                        work.harddrop_certificates += direct.count_ones() as usize;
                        permitted_parents[operation][word] |= direct;
                        if bound {
                            possible_parents[operation][word] |= grounded;
                        }
                        unresolved[operation] = grounded & !direct;
                        // The optimistic pass needs only a validated footprint, not
                        // owned realizations. Exact non-bound queries retain only
                        // poses for which hard drop did not already prove entry.
                        for lock in core::iter::once(first).chain(realizations) {
                            if lock.lock_mask != footprint {
                                return Err(InconsistentProjection);
                            }
                            if !bound && unresolved[operation] != 0 {
                                locks[operation].push(lock);
                            }
                        }
                    }
                    if !bound {
                        let resolved = FrameQueries {
                            pieces: geometry.pieces(),
                            occupied: &occupied,
                            locks: &locks,
                            unresolved,
                        }
                        .resolve(
                            catalog,
                            workspace,
                            control,
                            work,
                            target_reverse,
                        )?;
                        for (operation, lanes) in resolved.into_iter().enumerate() {
                            permitted_parents[operation][word] |= lanes;
                        }
                    }
                }
            }
        }
        if bound {
            // The missing edges are not physical negatives. They cannot
            // participate in a complete word even with optimistic movement.
            // Use this only when the caller wants the complete language.
            let possible_live =
                domain.potentially_live_edges(&possible_parents, &mut work.upper_composition_words);
            let mut surviving_by_operation = [[0_u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
            for word in 0..domain.word_count() {
                if control.is_cancelled() {
                    return Err(Cancelled);
                }
                for (operation, retained) in surviving_by_operation.iter_mut().enumerate() {
                    let previously_unknown =
                        possible_parents[operation][word] & !permitted_parents[operation][word];
                    retained[word] = previously_unknown & possible_live[operation][word];
                    work.upper_pruned_unknown_edges +=
                        u64::from((previously_unknown & !retained[word]).count_ones());
                    work.upper_remaining_unknown_edges += u64::from(retained[word].count_ones());
                    permitted_parents[operation][word] &= possible_live[operation][word];
                }
            }
            if subsumption_enabled()? {
                let removed = super::symbolic_prefix_subsumption::elide_covered_suffix_edges(
                    geometry.pieces(),
                    &possible_live,
                    &permitted_parents,
                    &mut surviving_by_operation,
                );
                work.upper_remaining_unknown_edges = work
                    .upper_remaining_unknown_edges
                    .checked_sub(removed)
                    .ok_or(InconsistentProjection)?;
                work.subsumed_lanes += removed;
            }
            upper_span.finish(1);
            let reverse_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::StrictPrefixReverse,
                profile_scale,
            );
            if PACK_UPPER_LANES.with(Cell::get) {
                PackedFrameQueries {
                    catalog,
                    candidate,
                    geometry: &geometry,
                    domain: &domain,
                    surviving: &surviving_by_operation,
                    frames_by_word: &frames_by_word,
                }
                .resolve(&mut permitted_parents, workspace, control, work)?;
            } else {
                for word in 0..domain.word_count() {
                    if control.is_cancelled() {
                        return Err(Cancelled);
                    }
                    let surviving: [u64; MAX_OPERATIONS] =
                        core::array::from_fn(|operation| surviving_by_operation[operation][word]);
                    let affected = surviving
                        .iter()
                        .fold(0, |lanes, &operation| lanes | operation);
                    if affected == 0 {
                        continue;
                    }
                    // Frame membership depends only on the executed subset.
                    // Restrict the first partition instead of splitting again.
                    let owners = domain.word_owners(word);
                    for (deleted, lanes) in frames_by_word[word]
                        .iter()
                        .map(|&lanes| lanes & affected)
                        .enumerate()
                        .filter(|(_, bits)| *bits != 0)
                    {
                        if control.is_cancelled() {
                            return Err(Cancelled);
                        }
                        work.upper_replayed_frames += 1;
                        let (initial, placements) = geometry.symbolic_frame(deleted as u16);
                        let occupied = occupancy_planes(
                            initial,
                            &placements[..operation_count],
                            &owners,
                            lanes,
                            work,
                        );
                        let unresolved = surviving.map(|parents| parents & lanes);
                        let mut locks: [Vec<InstantiatedRealization>; MAX_OPERATIONS] =
                            core::array::from_fn(|_| Vec::new());
                        for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
                            if unresolved[operation] == 0 {
                                continue;
                            }
                            work.upper_reloaded_placements += 1;
                            locks[operation].extend(catalog.instantiations(row_id, deleted as u16));
                            if locks[operation].is_empty() {
                                return Err(InconsistentProjection);
                            }
                        }
                        let resolved = FrameQueries {
                            pieces: geometry.pieces(),
                            occupied: &occupied,
                            locks: &locks,
                            unresolved,
                        }
                        .resolve(
                            catalog,
                            workspace,
                            control,
                            work,
                            target_reverse,
                        )?;
                        for (operation, allowed) in resolved.into_iter().enumerate() {
                            permitted_parents[operation][word] |= allowed;
                        }
                    }
                }
            }
            reverse_span.finish(1);
        } else {
            upper_span.finish(1);
        }
        if control.is_cancelled() {
            return Err(Cancelled);
        }
        let closure_span =
            SearchStageSpan::begin_scaled(ExecutorSearchStage::StrictPrefixLanguage, profile_scale);
        let active_prefixes =
            domain.live_from_start(&permitted_parents, &mut work.composition_words);
        closure_span.finish(1);
        Ok(Self {
            permitted_parents,
            pieces,
            operation_count,
            active_prefixes,
            #[cfg(test)]
            complete_language_only: bound,
        })
    }

    #[cfg(test)]
    pub(super) fn local_edge_permission(&self, prefix: usize, operation: usize) -> Option<bool> {
        if self.complete_language_only
            || operation >= self.operation_count
            || prefix >= (1 << self.operation_count)
        {
            return None;
        }
        Some(self.permitted_parents[operation][prefix / 64] & (1_u64 << (prefix % 64)) != 0)
    }

    pub(super) fn compile_language(
        &self,
        cache: &mut PieceOrderLanguageCache,
        work: &mut SymbolicPrefixWork,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        work.language_states += self
            .active_prefixes
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum::<u64>();
        let previous = cache.frontier_compilations();
        let language = cache.compile_closed_prefix_language(
            &self.pieces[..self.operation_count],
            &self.permitted_parents[..self.operation_count],
            &self.active_prefixes,
            &mut work.language_transitions,
        )?;
        work.frontier_compilations += cache.frontier_compilations().saturating_sub(previous) as u64;
        Ok(language)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockStatus {
    Missing,
    Invalid,
    Valid,
}

#[derive(Clone, Copy)]
struct CatalogStaticGuard {
    // Even a missing/unusable lock can own surviving cells after execution.
    footprint: u64,
    support: u64,
    corridor: u64,
    status: LockStatus,
    static_ground: bool,
    static_block: bool,
}

impl CatalogStaticGuard {
    fn compile(
        catalog: &GeometryCatalog,
        row: u32,
        deleted: u16,
        footprint: u64,
        initial: u64,
    ) -> Self {
        Self::from_realizations(
            catalog.width(),
            footprint,
            initial,
            catalog.instantiations(row, deleted),
        )
    }

    fn from_realizations(
        width: u8,
        footprint: u64,
        initial: u64,
        mut realizations: impl Iterator<Item = InstantiatedRealization>,
    ) -> Self {
        let status = match realizations.next() {
            None => LockStatus::Missing,
            Some(first)
                if footprint.count_ones() != 4
                    || first.lock_mask != footprint
                    || realizations.any(|lock| lock.lock_mask != footprint) =>
            {
                LockStatus::Invalid
            }
            Some(_) => LockStatus::Valid,
        };
        if status != LockStatus::Valid {
            return Self {
                footprint,
                support: 0,
                corridor: 0,
                status,
                static_ground: false,
                static_block: false,
            };
        }
        let support = footprint >> width;
        let corridor = harddrop_path_mask(footprint, width);
        let floor = (1_u64 << width) - 1;
        Self {
            footprint,
            support,
            corridor,
            status,
            static_ground: footprint & floor != 0 || initial & support != 0,
            static_block: initial & corridor != 0,
        }
    }
}

/// One bounded frame-major table for one immutable catalog. A failed build
/// is an experiment refusal, never unreachability evidence.
pub(super) struct CatalogStaticGuards {
    catalog: Arc<GeometryCatalog>,
    rows: Vec<CatalogStaticGuard>,
}

impl CatalogStaticGuards {
    pub(super) fn compile(
        catalog: Arc<GeometryCatalog>,
        control: &ExecutionControl,
    ) -> Result<Self, SymbolicPrefixRefusal> {
        use SymbolicPrefixRefusal::{Cancelled, RelationLimit, Unsupported};
        let height = catalog.height();
        let width = catalog.width();
        let row_count = catalog.skeleton_count();
        if !(1..=6).contains(&height) || !(1..=10).contains(&width) || row_count == 0 {
            return Err(Unsupported);
        }
        let frame_count = 1usize << height;
        let row_count_u32 = u32::try_from(row_count).map_err(|_| RelationLimit)?;
        let slots = frame_count.checked_mul(row_count).ok_or(RelationLimit)?;
        let slot_bytes = core::mem::size_of::<CatalogStaticGuard>();
        if slots > MAX_CATALOG_STATIC_GUARD_BYTES / slot_bytes {
            return Err(RelationLimit);
        }
        let mut rows = Vec::new();
        rows.try_reserve_exact(slots).map_err(|_| RelationLimit)?;
        if rows.capacity() > MAX_CATALOG_STATIC_GUARD_BYTES / slot_bytes {
            return Err(RelationLimit);
        }
        for deleted in 0..frame_count as u16 {
            let projection = AtomicRowProjection::new(width, height, deleted).ok_or(Unsupported)?;
            let initial = projection.physical_board(catalog.initial_board());
            for row in 0..row_count_u32 {
                if row & 63 == 0 && control.is_cancelled() {
                    return Err(Cancelled);
                }
                let footprint = projection.physical_board(catalog.skeleton(row).cells);
                rows.push(CatalogStaticGuard::compile(
                    &catalog, row, deleted, footprint, initial,
                ));
            }
        }
        Ok(Self { catalog, rows })
    }

    fn frame(&self, deleted: usize) -> Option<&[CatalogStaticGuard]> {
        let row_count = self.catalog.skeleton_count();
        let start = deleted.checked_mul(row_count)?;
        self.rows.get(start..start.checked_add(row_count)?)
    }

    fn logical_bytes(&self) -> usize {
        self.rows.capacity() * core::mem::size_of::<CatalogStaticGuard>()
    }
}

fn subsumption_enabled() -> Result<bool, SymbolicPrefixRefusal> {
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        // This is a process-scoped, local-only axis, like the language and
        // storage experiments. No environment reads occur per candidate.
        if let Some(enabled) = ENABLED.get() {
            return Ok(*enabled);
        }
        let enabled = match std::env::var("CLEARRA_STRICT_SUBSUMPTION") {
            Ok(value) if value == "1" => true,
            Ok(value) if value == "0" => false,
            Err(std::env::VarError::NotPresent) => false,
            _ => return Err(SymbolicPrefixRefusal::Unsupported),
        };
        Ok(*ENABLED.get_or_init(|| enabled))
    }
    #[cfg(not(all(feature = "local-search-ab", not(target_family = "wasm"))))]
    {
        Ok(false)
    }
}

struct UpperFrameGuards<'a> {
    catalog: &'a GeometryCatalog,
    candidate: &'a GeometryCandidate,
    geometry: &'a CandidateGeometry,
    domain: &'a PrefixDomain,
    table: Option<&'a CatalogStaticGuards>,
}

impl UpperFrameGuards<'_> {
    fn compile(
        self,
        direct_parents: &mut [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
        possible_parents: &mut [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
        frames_by_word: &mut [[u64; MAX_FRAMES]; MAX_PREFIX_WORDS],
        control: &ExecutionControl,
        work: &mut SymbolicPrefixWork,
    ) -> Result<(), SymbolicPrefixRefusal> {
        use SymbolicPrefixRefusal::{Cancelled, InconsistentProjection};
        let mut words_by_frame = [0_u16; MAX_FRAMES];
        for (word, frames) in frames_by_word[..self.domain.word_count()]
            .iter_mut()
            .enumerate()
        {
            if control.is_cancelled() {
                return Err(Cancelled);
            }
            let active = self.domain.nonterminal_lanes(word);
            let active_frames = partition_active_frames(self.geometry, word, active, frames, work);
            for frame in set_bit_indices(active_frames) {
                words_by_frame[frame] |= 1_u16 << word;
            }
        }
        for (frame, frame_words) in words_by_frame.into_iter().enumerate() {
            if frame_words == 0 {
                continue;
            }
            if control.is_cancelled() {
                return Err(Cancelled);
            }
            work.frame_count += frame_words.count_ones() as usize;
            let catalog_frame = match self.table {
                Some(table) => Some(table.frame(frame).ok_or(InconsistentProjection)?),
                None => None,
            };
            let (initial, placements) = if let Some(catalog_frame) = catalog_frame {
                let mut projected = [0; super::MAX_BOARD64_PIECES];
                for (&row, footprint) in self.candidate.row_ids().iter().zip(&mut projected) {
                    *footprint = catalog_frame
                        .get(row as usize)
                        .ok_or(InconsistentProjection)?
                        .footprint;
                }
                (0, projected)
            } else {
                self.geometry.symbolic_frame(frame as u16)
            };
            let placements = &placements[..self.candidate.row_ids().len()];
            for (operation, (&row, &footprint)) in
                self.candidate.row_ids().iter().zip(placements).enumerate()
            {
                let operation_bit = 1_u16 << operation;
                let applicable =
                    |word: usize| frames_by_word[word][frame] & !owner_lanes(operation_bit, word);
                if !set_bit_indices(u64::from(frame_words)).any(|word| applicable(word) != 0) {
                    continue;
                }
                // CandidateGeometry proves original cells disjoint. For a
                // fixed deletion mask, surviving cells map injectively, so
                // an unexecuted operation cannot collide with any owner in
                // that frame. Still require every actual ILC realization to
                // be exactly the projected four-cell target.
                let guard = if let Some(catalog_frame) = catalog_frame {
                    work.catalog_static_guard_hits += 1;
                    // The same row was bounds-checked when reading the footprint.
                    catalog_frame[row as usize]
                } else {
                    CatalogStaticGuard::compile(self.catalog, row, frame as u16, footprint, initial)
                };
                match guard.status {
                    LockStatus::Missing => continue,
                    LockStatus::Invalid => return Err(InconsistentProjection),
                    LockStatus::Valid => work.projected_locks += 1,
                }
                let mut supporting = 0_u16;
                let mut blocking = 0_u16;
                for (owner, &cells) in placements.iter().enumerate() {
                    if owner == operation {
                        continue;
                    }
                    if !guard.static_ground && cells & guard.support != 0 {
                        supporting |= 1 << owner;
                    }
                    if !guard.static_block && cells & guard.corridor != 0 {
                        blocking |= 1 << owner;
                    }
                }
                for word in set_bit_indices(u64::from(frame_words)) {
                    let lanes = applicable(word);
                    let grounded = if guard.static_ground {
                        lanes
                    } else {
                        lanes & owner_lanes(supporting, word)
                    };
                    let direct = if guard.static_block {
                        0
                    } else {
                        grounded & !owner_lanes(blocking, word)
                    };
                    work.concrete_guards += lanes.count_ones() as usize;
                    work.ungrounded_rejections += (lanes & !grounded).count_ones() as usize;
                    work.harddrop_certificates += direct.count_ones() as usize;
                    possible_parents[operation][word] |= grounded;
                    direct_parents[operation][word] |= direct;
                }
            }
        }
        Ok(())
    }
}

fn set_bit_indices(mut mask: u64) -> impl Iterator<Item = usize> {
    core::iter::from_fn(move || {
        if mask == 0 {
            return None;
        }
        let index = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        Some(index)
    })
}

/// Lanes whose executed subset contains at least one owner, before
/// restricting to the word's actual nonterminal frame membership.
fn owner_lanes(owners: u16, word: usize) -> u64 {
    const ANY_LOW_OWNER: [u64; 64] = {
        let mut table = [0; 64];
        let mut low_subset = 0;
        while low_subset < 64 {
            let mut lane = 0;
            while lane < 64 {
                if lane & low_subset != 0 {
                    table[low_subset] |= 1 << lane;
                }
                lane += 1;
            }
            low_subset += 1;
        }
        table
    };
    if word & usize::from(owners >> 6) != 0 {
        u64::MAX
    } else {
        ANY_LOW_OWNER[usize::from(owners & 63)]
    }
}

fn partition_active_frames(
    geometry: &CandidateGeometry,
    word: usize,
    active: u64,
    frames: &mut [u64; MAX_FRAMES],
    work: &mut SymbolicPrefixWork,
) -> u64 {
    work.assignment_words += 1;
    work.represented_prefixes += u64::from(active.count_ones());
    if active == 0 {
        return 0;
    }
    frames[0] = active;
    let mut occupied_frames = 1_u64;
    for (contributors, rows) in geometry.symbolic_clear_groups() {
        // All contributors execute exactly when none is in the complement.
        // XOR the high-bit word and reverse the 64 low-bit lane indices:
        // reverse_bits maps each bit at i to (63 - i), also i XOR 63.
        let completed = !owner_lanes(contributors, word ^ (MAX_PREFIX_WORDS - 1)).reverse_bits();
        if completed & active == 0 {
            continue;
        }
        let rows = usize::from(rows);
        // The groups have disjoint original rows. Iterate a snapshot so the
        // frames newly receiving this group's rows cannot be split twice.
        for frame in set_bit_indices(occupied_frames) {
            work.frame_domain_splits += 1;
            let release = frames[frame] & completed;
            if release == 0 {
                continue;
            }
            frames[frame] ^= release;
            if frames[frame] == 0 {
                occupied_frames &= !(1_u64 << frame);
            }
            let destination = frame | rows;
            frames[destination] |= release;
            occupied_frames |= 1_u64 << destination;
        }
    }
    occupied_frames
}

struct FrameQueries<'a> {
    pieces: &'a [PieceKind],
    occupied: &'a [u64; CELL_PLANES],
    locks: &'a [Vec<InstantiatedRealization>; MAX_OPERATIONS],
    unresolved: [u64; MAX_OPERATIONS],
}

struct PackedFrameQueries<'a> {
    catalog: &'a GeometryCatalog,
    candidate: &'a GeometryCandidate,
    geometry: &'a CandidateGeometry,
    domain: &'a PrefixDomain,
    surviving: &'a [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
    frames_by_word: &'a [[u64; MAX_FRAMES]; MAX_PREFIX_WORDS],
}

impl PackedFrameQueries<'_> {
    fn resolve(
        self,
        permitted: &mut [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
        workspace: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
        work: &mut SymbolicPrefixWork,
    ) -> Result<(), SymbolicPrefixRefusal> {
        use SymbolicPrefixRefusal::{Cancelled, InconsistentProjection, RelationLimit};
        // Intersect each original frame partition with live unknown edges.
        // Different original row masks never share a pose proof.
        let mut live_frames = 0_u64;
        for (word, frames) in self.frames_by_word[..self.domain.word_count()]
            .iter()
            .enumerate()
        {
            if control.is_cancelled() {
                return Err(Cancelled);
            }
            let affected = self
                .surviving
                .iter()
                .fold(0, |lanes, operation| lanes | operation[word]);
            if affected == 0 {
                continue;
            }
            for (frame, &lanes) in frames.iter().enumerate() {
                if lanes & affected != 0 {
                    live_frames |= 1_u64 << frame;
                    work.upper_replayed_frames += 1;
                }
            }
        }
        while live_frames != 0 {
            if control.is_cancelled() {
                return Err(Cancelled);
            }
            let deleted = live_frames.trailing_zeros() as usize;
            live_frames &= live_frames - 1;
            work.packed_frame_projections += 1;
            let (initial, placements) = self.geometry.symbolic_frame(deleted as u16);
            for (operation, &row) in self.candidate.row_ids().iter().enumerate() {
                let mut selected: [u64; MAX_PREFIX_WORDS] = core::array::from_fn(|word| {
                    self.surviving[operation][word] & self.frames_by_word[word][deleted]
                });
                if selected.iter().all(|&lanes| lanes == 0) {
                    continue;
                }
                work.upper_reloaded_placements += 1;
                let locks: Vec<_> = self.catalog.instantiations(row, deleted as u16).collect();
                if locks.is_empty() {
                    return Err(InconsistentProjection);
                }
                while selected.iter().any(|&lanes| lanes != 0) {
                    let batch = PrefixLaneBatch::gather(&mut selected, self.domain)
                        .ok_or(InconsistentProjection)?;
                    work.packed_segments += batch.segment_count();
                    let lanes = batch.valid_lanes();
                    let occupied = occupancy_planes(
                        initial,
                        &placements[..self.geometry.pieces().len()],
                        &batch.packed_owners(self.domain),
                        lanes,
                        work,
                    );
                    work.target_operations += 1;
                    let proved = workspace
                        .compile_prefix_target_lanes_from_planes(
                            self.catalog,
                            self.geometry.pieces()[operation],
                            &occupied,
                            lanes,
                            &locks,
                            control,
                            &mut work.pose,
                        )
                        .map_err(map_relation_error)?
                        .ok_or(RelationLimit)?;
                    if !batch.scatter(proved, &mut permitted[operation]) {
                        return Err(InconsistentProjection);
                    }
                }
            }
        }
        Ok(())
    }
}

impl FrameQueries<'_> {
    fn resolve(
        self,
        catalog: &GeometryCatalog,
        workspace: &mut ReachabilityWorkspace,
        control: &ExecutionControl,
        work: &mut SymbolicPrefixWork,
        target_reverse: bool,
    ) -> Result<[u64; MAX_OPERATIONS], SymbolicPrefixRefusal> {
        use SymbolicPrefixRefusal::{InconsistentProjection, RelationLimit};
        let mut resolved = [0_u64; MAX_OPERATIONS];
        if target_reverse {
            for (operation, &piece) in self.pieces.iter().enumerate() {
                let lanes = self.unresolved[operation];
                if lanes == 0 {
                    continue;
                }
                work.target_operations += 1;
                resolved[operation] = workspace
                    .compile_prefix_target_lanes_from_planes(
                        catalog,
                        piece,
                        &self.occupied.map(|owners| owners & lanes),
                        lanes,
                        &self.locks[operation],
                        control,
                        &mut work.pose,
                    )
                    .map_err(map_relation_error)?
                    .ok_or(RelationLimit)?;
            }
            return Ok(resolved);
        }
        for piece in PieceKind::STANDARD_TETROMINOES {
            let mut need_poses = 0_u64;
            for (operation, &kind) in self.pieces.iter().enumerate() {
                if kind == piece {
                    need_poses |= self.unresolved[operation];
                }
            }
            if need_poses == 0 {
                continue;
            }
            let selected_occupancy = self.occupied.map(|column| column & need_poses);
            work.frame_kind_groups += 1;
            let relation = workspace
                .compile_prefix_lock_relation_from_planes(
                    catalog,
                    piece,
                    &selected_occupancy,
                    need_poses,
                    control,
                    &mut work.pose,
                )
                .map_err(map_relation_error)?
                .ok_or(RelationLimit)?;
            for (operation, &kind) in self.pieces.iter().enumerate() {
                if kind != piece || self.unresolved[operation] == 0 {
                    continue;
                }
                let mut remaining = self.unresolved[operation];
                for &lock in &self.locks[operation] {
                    work.tested_realizations += 1;
                    remaining &= !relation
                        .grounded_lanes(lock)
                        .ok_or(InconsistentProjection)?;
                    if remaining == 0 {
                        break;
                    }
                }
                resolved[operation] = self.unresolved[operation] & !remaining;
            }
        }
        Ok(resolved)
    }
}

fn map_relation_error(error: WasmExactSearchError) -> SymbolicPrefixRefusal {
    match error {
        WasmExactSearchError::Cancelled => SymbolicPrefixRefusal::Cancelled,
        _ => SymbolicPrefixRefusal::RelationLimit,
    }
}

pub(super) fn compile_frame_domains(
    geometry: &CandidateGeometry,
    owners: &[u64; MAX_OPERATIONS],
    active: u64,
    work: &mut SymbolicPrefixWork,
) -> [u64; MAX_FRAMES] {
    work.represented_prefixes += u64::from(active.count_ones());
    split_frame_domains(geometry, owners, active, &mut work.frame_domain_splits)
}

fn split_frame_domains(
    geometry: &CandidateGeometry,
    owners: &[u64; MAX_OPERATIONS],
    active: u64,
    splits: &mut usize,
) -> [u64; MAX_FRAMES] {
    let mut frames = [0_u64; MAX_FRAMES];
    frames[0] = active;
    for (mut contributors, rows) in geometry.symbolic_clear_groups() {
        let mut completed = active;
        while contributors != 0 {
            completed &= owners[contributors.trailing_zeros() as usize];
            contributors &= contributors - 1;
        }
        let rows = usize::from(rows);
        for frame in 0..MAX_FRAMES {
            // Contributed row groups are pairwise disjoint. Newly produced
            // frames carry this group's rows and cannot be split again here.
            if frame & rows != 0 || frames[frame] == 0 {
                continue;
            }
            *splits += 1;
            let release = frames[frame] & completed;
            frames[frame] ^= release;
            frames[frame | rows] |= release;
        }
    }
    frames
}

pub(super) fn occupancy_planes(
    initial: u64,
    placements: &[u64],
    owners: &[u64; MAX_OPERATIONS],
    active: u64,
    work: &mut SymbolicPrefixWork,
) -> [u64; CELL_PLANES] {
    let mut occupied = [0_u64; CELL_PLANES];
    let static_and_owned = core::iter::once((initial, active)).chain(
        placements
            .iter()
            .zip(owners)
            .map(|(&cells, &bits)| (cells, active & bits)),
    );
    for (mut cells, lanes) in static_and_owned.filter(|&(_, lanes)| lanes != 0) {
        while cells != 0 {
            occupied[cells.trailing_zeros() as usize] |= lanes;
            cells &= cells - 1;
            work.occupancy_plane_terms += 1;
        }
    }
    occupied
}

fn occupied_by_any(
    occupied: &[u64; CELL_PLANES],
    mut footprint: u64,
    work: &mut SymbolicPrefixWork,
) -> u64 {
    let mut lanes = 0;
    while footprint != 0 {
        lanes |= occupied[footprint.trailing_zeros() as usize];
        footprint &= footprint - 1;
        work.guard_cell_terms += 1;
    }
    lanes
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::rotation::RotationState;

    use super::*;

    #[test]
    fn catalog_static_guard_keeps_missing_locks_and_invalid_footprints_distinct() {
        let projected = 3 | 3 << 10;
        let good = InstantiatedRealization {
            lock_mask: projected,
            rotation: RotationState::Zero,
            x: 0,
            lock_y: 0,
        };
        let invalid = InstantiatedRealization {
            lock_mask: projected << 1,
            ..good
        };
        let missing = CatalogStaticGuard::from_realizations(10, projected, 0, core::iter::empty());
        assert_eq!(missing.status, LockStatus::Missing);
        assert_eq!(missing.footprint, projected);
        let mismatch =
            CatalogStaticGuard::from_realizations(10, projected, 0, [good, invalid].into_iter());
        assert_eq!(mismatch.status, LockStatus::Invalid);
        assert_eq!(mismatch.footprint, projected);
        let valid = CatalogStaticGuard::from_realizations(10, projected, 0, [good].into_iter());
        assert_eq!(valid.status, LockStatus::Valid);
        assert!(valid.static_ground);
        assert!(!valid.static_block);
    }

    #[test]
    fn catalog_static_guards_preserve_strict_edges_and_reject_another_catalog() {
        let low_o = 3 | 3 << 10;
        let high_o = low_o << 20;
        let required = low_o | high_o;
        let initial = ((1_u64 << 40) - 1) & !required;
        let catalog = Arc::new(
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, initial, required)
                .unwrap(),
        );
        let rows = [low_o, high_o].map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        let control = ExecutionControl::default();
        let table = Arc::new(CatalogStaticGuards::compile(Arc::clone(&catalog), &control).unwrap());

        let run = |catalog: &GeometryCatalog, work: &mut SymbolicPrefixWork| {
            with_packed_upper_target(|| {
                SymbolicPrefixOrders::compile(
                    catalog,
                    &candidate,
                    KickTableProfileId::Srs90,
                    &control,
                    work,
                )
            })
        };
        let mut work_a = SymbolicPrefixWork::default();
        let mut work_b = SymbolicPrefixWork::default();
        let baseline = run(&catalog, &mut work_a).unwrap();
        let cached =
            with_catalog_static_guards(Arc::clone(&table), || run(&catalog, &mut work_b)).unwrap();
        assert_eq!(baseline.permitted_parents, cached.permitted_parents);
        assert_eq!(baseline.active_prefixes, cached.active_prefixes);
        assert_eq!(work_a.harddrop_certificates, work_b.harddrop_certificates);
        assert_eq!(
            work_a.upper_remaining_unknown_edges,
            work_b.upper_remaining_unknown_edges
        );
        assert_eq!(work_a.catalog_static_guard_hits, 0);
        assert_eq!(work_a.catalog_static_guard_bytes, 0);
        assert!(work_b.catalog_static_guard_hits > 0);
        assert_eq!(work_b.catalog_static_guard_bytes, table.logical_bytes());
        assert!(table.logical_bytes() <= MAX_CATALOG_STATIC_GUARD_BYTES);

        let distinct_catalog = catalog.as_ref().clone();
        let wrong = with_catalog_static_guards(table, || {
            run(&distinct_catalog, &mut SymbolicPrefixWork::default())
        });
        assert!(matches!(
            wrong,
            Err(SymbolicPrefixRefusal::InconsistentProjection)
        ));
    }

    #[test]
    fn owner_lanes_match_concrete_any_and_all_subsets() {
        for owners in 0..1_u16 << MAX_OPERATIONS {
            for word in 0..MAX_PREFIX_WORDS {
                let any = owner_lanes(owners, word);
                let all = !owner_lanes(owners, word ^ (MAX_PREFIX_WORDS - 1)).reverse_bits();
                for lane in 0..64 {
                    let subset = (word * 64 + lane) as u16;
                    assert_eq!(any & (1 << lane) != 0, subset & owners != 0);
                    assert_eq!(all & (1 << lane) != 0, subset & owners == owners);
                }
            }
        }
    }
}
