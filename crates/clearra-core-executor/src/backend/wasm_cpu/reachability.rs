use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest, ScoringLockEvidence};
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId, KickTransition};

use super::{
    catalog::{GeometryCatalog, InstantiatedRealization},
    kick_profiles::builtin_kick_profile,
    mix_digest, piece_index,
};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ReachableLocks {
    anchors: [u64; 4],
}

impl ReachableLocks {
    pub fn insert(&mut self, width: u8, rotation: RotationState, x: i8, y: i8) {
        if x < 0 || y < 0 || x >= width as i8 {
            return;
        }
        let anchor = y as usize * width as usize + x as usize;
        if anchor < 64 {
            self.anchors[rotation.quarter_turns() as usize] |= 1_u64 << anchor;
        }
    }

    pub fn contains(&self, width: u8, rotation: RotationState, x: i8, y: i8) -> bool {
        if x < 0 || y < 0 || x >= width as i8 {
            return false;
        }
        let anchor = y as usize * width as usize + x as usize;
        anchor < 64 && self.anchors[rotation.quarter_turns() as usize] & (1_u64 << anchor) != 0
    }

    pub fn union_with(&mut self, other: Self) {
        for (destination, source) in self.anchors.iter_mut().zip(other.anchors) {
            *destination |= source;
        }
    }

    fn contains_all(&self, desired: Self) -> bool {
        self.anchors
            .iter()
            .zip(desired.anchors)
            .all(|(found, wanted)| found & wanted == wanted)
    }
}

/// Query-local reusable storage for one exhaustive SRS+ reachability search.
/// The queue uses a cursor rather than `VecDeque`, so a cache miss performs no
/// allocation after the largest board seen by the worker has been prepared.
#[derive(Default)]
pub(super) struct ReachabilityScratch {
    visited_generations: Vec<u16>,
    generation: u16,
    queue: Vec<u16>,
}

impl ReachabilityScratch {
    fn retained_bytes(&self) -> usize {
        self.visited_generations.capacity() * core::mem::size_of::<u16>()
            + self.queue.capacity() * core::mem::size_of::<u16>()
    }

    fn begin_search(&mut self, state_count: usize) -> u16 {
        if self.visited_generations.len() < state_count {
            self.visited_generations.resize(state_count, 0);
            if self.queue.capacity() < state_count {
                self.queue.reserve(state_count - self.queue.capacity());
            }
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited_generations.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        self.generation
    }
}

pub(super) struct ReachabilitySearchResult {
    pub locks: ReachableLocks,
    pub visited_state_count: usize,
    pub exhaustive: bool,
}

const REACHABILITY_CACHE_CAPACITY: usize = 1 << 16;
const REACHABILITY_CACHE_WAYS: usize = 4;
const SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS: u8 = 32;
const LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS: u8 = 16;

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct ReachabilityCacheKey {
    board: u64,
    piece_code: u8,
    observation: u8,
    exhaustive: bool,
    valid: bool,
    reserved: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<ReachabilityCacheKey>() == 16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReachabilityCacheLookup {
    Reachable,
    ExhaustivelyUnreachable,
    Search {
        exhaustive: bool,
        admit: bool,
        key_present: bool,
    },
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct ReachabilityObservation {
    tag: u16,
    count: u8,
    valid: bool,
}

const _: () = assert!(core::mem::size_of::<ReachabilityObservation>() == 4);

/// Query-local, bounded exact reachability cache.
///
/// Hot keys and cold lock bitsets are split so a direct-map miss touches only
/// sixteen bytes. Partial searches may cache proven-reachable locks, but a
/// missing lock is authoritative only when `exhaustive` is set. Replacement
/// and allocation failure therefore degrade to a cache miss, never pruning.
#[derive(Default)]
struct ReachabilityCache {
    keys: Vec<ReachabilityCacheKey>,
    locks: Vec<ReachableLocks>,
    unreachable_locks: Vec<ReachableLocks>,
    observations: Vec<ReachabilityObservation>,
    exhaustive_observations: u8,
    allocation_disabled: bool,
}

impl ReachabilityCache {
    #[allow(clippy::too_many_arguments)]
    fn query(
        &mut self,
        board: u64,
        piece: PieceKind,
        width: u8,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> ReachabilityCacheLookup {
        if self.keys.is_empty() && !self.initialize() {
            return ReachabilityCacheLookup::Search {
                exhaustive: false,
                admit: false,
                key_present: false,
            };
        }
        let piece_code = piece_index(piece) as u8 + 1;
        let hash = reachability_cache_hash(board, piece_code);
        let set_start = reachability_cache_set_start(hash, self.keys.len());
        for index in set_start..set_start + REACHABILITY_CACHE_WAYS {
            let key = &mut self.keys[index];
            if !key.valid || key.board != board || key.piece_code != piece_code {
                continue;
            }
            if self.locks[index].contains(width, rotation, x, y) {
                return ReachabilityCacheLookup::Reachable;
            }
            if self.unreachable_locks[index].contains(width, rotation, x, y) {
                return ReachabilityCacheLookup::ExhaustivelyUnreachable;
            }
            if key.exhaustive {
                return ReachabilityCacheLookup::ExhaustivelyUnreachable;
            }
            let exhaustive = key.observation >= self.exhaustive_observations;
            key.observation = key.observation.saturating_add(1);
            return ReachabilityCacheLookup::Search {
                exhaustive,
                admit: true,
                key_present: true,
            };
        }
        let prior_observations = self.observe(hash);
        ReachabilityCacheLookup::Search {
            exhaustive: false,
            admit: prior_observations > 0,
            key_present: false,
        }
    }

    fn insert(
        &mut self,
        board: u64,
        piece: PieceKind,
        locks: ReachableLocks,
        unreachable_locks: ReachableLocks,
        exhaustive: bool,
        admit: bool,
    ) {
        if !exhaustive && !admit {
            return;
        }
        if self.keys.is_empty() && !self.initialize() {
            return;
        }
        let piece_code = piece_index(piece) as u8 + 1;
        let hash = reachability_cache_hash(board, piece_code);
        let set_start = reachability_cache_set_start(hash, self.keys.len());
        let mut victim = set_start;
        let mut victim_observation = u8::MAX;
        for index in set_start..set_start + REACHABILITY_CACHE_WAYS {
            let key = &self.keys[index];
            if key.valid && key.board == board && key.piece_code == piece_code {
                self.locks[index].union_with(locks);
                self.unreachable_locks[index].union_with(unreachable_locks);
                self.keys[index].exhaustive |= exhaustive;
                self.keys[index].observation = self.keys[index].observation.max(1);
                return;
            }
            if !key.valid {
                victim = index;
                victim_observation = 0;
                break;
            }
            if key.observation < victim_observation {
                victim = index;
                victim_observation = key.observation;
            }
        }
        if !exhaustive && victim_observation > 1 {
            return;
        }
        self.keys[victim] = ReachabilityCacheKey {
            board,
            piece_code,
            observation: 1,
            exhaustive,
            valid: true,
            reserved: [0; 4],
        };
        self.locks[victim] = locks;
        self.unreachable_locks[victim] = unreachable_locks;
    }

    fn configure(&mut self, operation_count: usize) {
        self.exhaustive_observations = if operation_count >= 7 {
            LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS
        } else {
            SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS
        };
    }

    fn observe(&mut self, hash: u64) -> u8 {
        let index = hash as usize & (self.observations.len() - 1);
        let tag = (hash >> 48) as u16;
        let observation = &mut self.observations[index];
        if !observation.valid || observation.tag != tag {
            *observation = ReachabilityObservation {
                tag,
                count: 1,
                valid: true,
            };
            return 0;
        }
        let prior = observation.count;
        observation.count = observation.count.saturating_add(1);
        prior
    }

    fn initialize(&mut self) -> bool {
        if self.allocation_disabled {
            return false;
        }
        if self
            .keys
            .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
            .is_err()
            || self
                .locks
                .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
                .is_err()
            || self
                .unreachable_locks
                .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
                .is_err()
            || self
                .observations
                .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
                .is_err()
        {
            self.keys = Vec::new();
            self.locks = Vec::new();
            self.unreachable_locks = Vec::new();
            self.observations = Vec::new();
            self.allocation_disabled = true;
            return false;
        }
        self.keys
            .resize(REACHABILITY_CACHE_CAPACITY, ReachabilityCacheKey::default());
        self.locks
            .resize(REACHABILITY_CACHE_CAPACITY, ReachableLocks::default());
        self.unreachable_locks
            .resize(REACHABILITY_CACHE_CAPACITY, ReachableLocks::default());
        self.observations.resize(
            REACHABILITY_CACHE_CAPACITY,
            ReachabilityObservation::default(),
        );
        if self.exhaustive_observations == 0 {
            self.exhaustive_observations = SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS;
        }
        true
    }

    fn retained_bytes(&self) -> usize {
        self.keys.capacity() * core::mem::size_of::<ReachabilityCacheKey>()
            + self.locks.capacity() * core::mem::size_of::<ReachableLocks>()
            + self.unreachable_locks.capacity() * core::mem::size_of::<ReachableLocks>()
            + self.observations.capacity() * core::mem::size_of::<ReachabilityObservation>()
    }
}

fn reachability_cache_hash(board: u64, piece_code: u8) -> u64 {
    mix_digest(board, u64::from(piece_code))
}

fn reachability_cache_set_start(hash: u64, capacity: usize) -> usize {
    let set_count = capacity / REACHABILITY_CACHE_WAYS;
    (hash as usize & (set_count - 1)) * REACHABILITY_CACHE_WAYS
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ReachabilityMetrics {
    pub lock_queries: usize,
    pub harddrop_queries: usize,
    pub harddrop_hits: usize,
    pub cache_reachable_hits: usize,
    pub cache_unreachable_hits: usize,
    pub cache_key_misses: usize,
    pub partial_searches: usize,
    pub exhaustive_searches: usize,
}

/// Reusable query-owned reachability engine shared by every buildability
/// stage. Templates are immutable after first use; mutable scratch and cache
/// never escape the query session.
pub(super) struct ReachabilityWorkspace {
    cache: ReachabilityCache,
    scratch: ReachabilityScratch,
    templates: [Option<ReachabilityTemplate>; 7],
    kick_profile_id: KickTableProfileId,
    generated_states: usize,
    metrics: ReachabilityMetrics,
}

impl Default for ReachabilityWorkspace {
    fn default() -> Self {
        Self {
            cache: ReachabilityCache::default(),
            scratch: ReachabilityScratch::default(),
            templates: std::array::from_fn(|_| None),
            kick_profile_id: KickTableProfileId::SrsPlus,
            generated_states: 0,
            metrics: ReachabilityMetrics::default(),
        }
    }
}

impl ReachabilityWorkspace {
    pub fn lock_harddrop_reachable_instantiated(
        &mut self,
        width: u8,
        board: u64,
        realization: InstantiatedRealization,
    ) -> bool {
        self.metrics.harddrop_queries = self.metrics.harddrop_queries.saturating_add(1);
        let grounded =
            realization.lock_y == 0 || board & (realization.lock_mask >> usize::from(width)) != 0;
        let reachable = grounded && board & harddrop_path_mask(realization.lock_mask, width) == 0;
        self.metrics.harddrop_hits = self
            .metrics
            .harddrop_hits
            .saturating_add(usize::from(reachable));
        reachable
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lock_reachable_instantiated(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        realization: InstantiatedRealization,
    ) -> bool {
        self.metrics.lock_queries = self.metrics.lock_queries.saturating_add(1);
        if self.lock_harddrop_reachable_instantiated(catalog.width(), board, realization) {
            return true;
        }
        self.prepare_template(catalog, piece);
        self.lock_reachable_cached(
            catalog,
            board,
            piece,
            realization.rotation,
            realization.x,
            realization.lock_y,
        )
    }

    pub fn prepare_template(&mut self, catalog: &GeometryCatalog, piece: PieceKind) {
        let _ = self.template(catalog, piece);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scoring_lock_evidence(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> ScoringLockEvidence {
        self.prepare_template(catalog, piece);
        let template = self.templates[piece_index(piece)]
            .as_ref()
            .expect("scoring evidence initialized the reachability template");
        let immobile = scoring_lock_is_immobile(template, board, rotation, x, y);
        best_scoring_lock_evidence(template, board, rotation, x, y, &mut self.scratch)
            .unwrap_or_else(|| ScoringLockEvidence::no_rotation(rotation))
            .with_immobile_before_clear(immobile)
    }

    pub fn lock_reachable_after_harddrop_miss(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> bool {
        self.metrics.lock_queries = self.metrics.lock_queries.saturating_add(1);
        self.lock_reachable_cached(catalog, board, piece, rotation, x, y)
    }

    fn lock_reachable_cached(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> bool {
        let (exhaustive, admit, key_present) =
            match self
                .cache
                .query(board, piece, catalog.width(), rotation, x, y)
            {
                ReachabilityCacheLookup::Reachable => {
                    self.metrics.cache_reachable_hits =
                        self.metrics.cache_reachable_hits.saturating_add(1);
                    return true;
                }
                ReachabilityCacheLookup::ExhaustivelyUnreachable => {
                    self.metrics.cache_unreachable_hits =
                        self.metrics.cache_unreachable_hits.saturating_add(1);
                    return false;
                }
                ReachabilityCacheLookup::Search {
                    exhaustive,
                    admit,
                    key_present,
                } => (exhaustive, admit, key_present),
            };
        self.metrics.cache_key_misses = self
            .metrics
            .cache_key_misses
            .saturating_add(usize::from(!key_present));
        if exhaustive {
            self.metrics.exhaustive_searches = self.metrics.exhaustive_searches.saturating_add(1);
        } else {
            self.metrics.partial_searches = self.metrics.partial_searches.saturating_add(1);
        }
        let mut desired = ReachableLocks::default();
        desired.insert(catalog.width(), rotation, x, y);
        let template_index = piece_index(piece);
        let template = self.templates[template_index]
            .as_ref()
            .expect("harddrop query initialized the reachability template");
        let (reachable, locks, unreachable_locks, all_locks_exhaustive, visited_state_count) =
            if exhaustive {
                let result = search_reachable_locks(template, board, &mut self.scratch, None);
                (
                    result.locks.contains(catalog.width(), rotation, x, y),
                    result.locks,
                    ReachableLocks::default(),
                    result.exhaustive,
                    result.visited_state_count,
                )
            } else {
                let result =
                    reverse_lock_reachable(template, board, rotation, x, y, &mut self.scratch);
                (
                    result.reachable,
                    if result.reachable {
                        desired
                    } else {
                        ReachableLocks::default()
                    },
                    if result.reachable {
                        ReachableLocks::default()
                    } else {
                        desired
                    },
                    false,
                    result.visited_state_count,
                )
            };
        self.generated_states = self.generated_states.saturating_add(visited_state_count);
        self.cache.insert(
            board,
            piece,
            locks,
            unreachable_locks,
            all_locks_exhaustive,
            admit,
        );
        reachable
    }

    pub fn configure(&mut self, operation_count: usize) {
        self.cache.configure(operation_count);
    }

    pub fn configure_kick_profile(&mut self, profile_id: KickTableProfileId) {
        if self.kick_profile_id == profile_id {
            return;
        }
        self.kick_profile_id = profile_id;
        self.templates = std::array::from_fn(|_| None);
        self.cache = ReachabilityCache::default();
    }

    pub const fn generated_state_count(&self) -> usize {
        self.generated_states
    }

    pub const fn metrics(&self) -> ReachabilityMetrics {
        self.metrics
    }

    pub fn retained_bytes(&self) -> usize {
        self.cache.retained_bytes()
            + self
                .templates
                .iter()
                .flatten()
                .map(ReachabilityTemplate::retained_bytes)
                .sum::<usize>()
            + self.scratch.retained_bytes()
    }

    fn template(&mut self, catalog: &GeometryCatalog, piece: PieceKind) -> &ReachabilityTemplate {
        let index = piece_index(piece);
        let profile_id = self.kick_profile_id;
        self.templates[index].get_or_insert_with(|| {
            ReachabilityTemplate::compile(catalog.width(), catalog.height(), piece, profile_id)
        })
    }
}

const INVALID_STATE_MASK: u64 = u64::MAX;

pub(super) struct ReachabilityTemplate {
    width: u8,
    height: u8,
    ceiling: i8,
    allow_180: bool,
    state_masks: Vec<u64>,
    translation_targets: Vec<[u16; 3]>,
    reverse_translation_sources: Vec<[u16; 3]>,
    sky_seeds: Vec<u16>,
    sky_seed_words: Vec<u64>,
    rotation_target_offsets: Vec<u32>,
    rotation_targets: Vec<u16>,
    rotation_kick_indices: Vec<u8>,
    reverse_rotation_offsets: Vec<u32>,
    reverse_rotation_sources: Vec<ReverseRotationSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct ReverseRotationSource {
    source: u16,
    slot: u8,
    prior_target_count: u8,
}

const _: () = assert!(core::mem::size_of::<ReverseRotationSource>() == 4);

impl ReachabilityTemplate {
    pub fn compile(
        width: u8,
        height: u8,
        piece: PieceKind,
        profile_id: KickTableProfileId,
    ) -> Self {
        let profile = builtin_kick_profile(profile_id)
            .expect("WASM reachability only compiles connected exact kick profiles");
        let allow_180 = profile.supports_180();
        let ceiling = source_ceiling(height, piece, allow_180, profile);
        let state_count = 4 * (ceiling as usize + 1) * width as usize;
        let definition = standard_tetromino_registry()
            .get(piece)
            .expect("standard tetromino exists");
        let mut state_masks = vec![INVALID_STATE_MASK; state_count];
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for y in 0..=ceiling {
                for x in 0..width as i8 {
                    let state = State { rotation, x, y };
                    let index = state_index(width, ceiling, state)
                        .expect("compiled reachability state is in range");
                    let mut mask = 0_u64;
                    let mut valid = true;
                    for cell in shape.cells() {
                        let cell_x = i16::from(x) + i16::from(cell.x());
                        let cell_y = i16::from(y) + i16::from(cell.y());
                        if cell_x < 0 || cell_x >= i16::from(width) || cell_y < 0 {
                            valid = false;
                            break;
                        }
                        if cell_y < i16::from(height) {
                            mask |= 1_u64 << (cell_y as usize * width as usize + cell_x as usize);
                        }
                    }
                    if valid {
                        state_masks[index] = mask;
                    }
                }
            }
        }
        let mut kick_deltas: [Vec<(i8, i8)>; 12] = std::array::from_fn(|_| Vec::new());
        for from in RotationState::ALL {
            let targets = [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ];
            for (slot, to) in targets.into_iter().enumerate() {
                if slot == 2 && !allow_180 {
                    continue;
                }
                let index = rotation_transition_index(from, slot);
                if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                    kick_deltas[index].extend(sequence.offsets().iter().map(|offset| {
                        normalized_kick_delta(piece, from, to, offset.dx(), offset.dy())
                    }));
                }
            }
        }
        let (rotation_target_offsets, rotation_targets, rotation_kick_indices) =
            compile_rotation_targets(width, ceiling, &state_masks, &kick_deltas);
        let (translation_targets, sky_seeds) =
            compile_translation_targets_and_seeds(width, height, ceiling, &state_masks);
        let reverse_translation_sources = compile_reverse_translation_sources(&translation_targets);
        let sky_seed_words = compile_seed_words(state_count, &sky_seeds);
        let (reverse_rotation_offsets, reverse_rotation_sources) = compile_reverse_rotation_sources(
            state_count,
            &rotation_target_offsets,
            &rotation_targets,
        );
        Self {
            width,
            height,
            ceiling,
            allow_180,
            state_masks,
            translation_targets,
            reverse_translation_sources,
            sky_seeds,
            sky_seed_words,
            rotation_target_offsets,
            rotation_targets,
            rotation_kick_indices,
            reverse_rotation_offsets,
            reverse_rotation_sources,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.state_masks.capacity() * core::mem::size_of::<u64>()
            + self.translation_targets.capacity() * core::mem::size_of::<[u16; 3]>()
            + self.reverse_translation_sources.capacity() * core::mem::size_of::<[u16; 3]>()
            + self.sky_seeds.capacity() * core::mem::size_of::<u16>()
            + self.sky_seed_words.capacity() * core::mem::size_of::<u64>()
            + self.rotation_target_offsets.capacity() * core::mem::size_of::<u32>()
            + self.rotation_targets.capacity() * core::mem::size_of::<u16>()
            + self.rotation_kick_indices.capacity() * core::mem::size_of::<u8>()
            + self.reverse_rotation_offsets.capacity() * core::mem::size_of::<u32>()
            + self.reverse_rotation_sources.capacity()
                * core::mem::size_of::<ReverseRotationSource>()
    }

    fn is_sky_seed(&self, state_index: usize) -> bool {
        self.sky_seed_words[state_index / 64] & (1_u64 << (state_index % 64)) != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct State {
    rotation: RotationState,
    x: i8,
    y: i8,
}

pub(super) fn search_reachable_locks(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    desired: Option<ReachableLocks>,
) -> ReachabilitySearchResult {
    let width = template.width;
    let height = template.height;
    let allow_180 = template.allow_180;
    let ceiling = template.ceiling;
    let state_count = 4 * (ceiling as usize + 1) * width as usize;
    let generation = scratch.begin_search(state_count);

    for &seed in &template.sky_seeds {
        push_index_if_placeable(
            template,
            board,
            seed,
            &mut scratch.visited_generations,
            generation,
            &mut scratch.queue,
        );
    }

    let mut locks = ReachableLocks::default();
    let mut queue_cursor = 0;
    while queue_cursor < scratch.queue.len() {
        let state_index = usize::from(scratch.queue[queue_cursor]);
        let state = state_from_index(width, ceiling, state_index);
        queue_cursor += 1;
        if state.y < height as i8 && grounded_index(template, board, state_index) {
            let anchor = state.y as usize * width as usize + state.x as usize;
            locks.anchors[state.rotation.quarter_turns() as usize] |= 1_u64 << anchor;
            if desired.is_some_and(|wanted| locks.contains_all(wanted)) {
                return ReachabilitySearchResult {
                    locks,
                    visited_state_count: queue_cursor,
                    exhaustive: false,
                };
            }
        }

        for &target in &template.translation_targets[state_index] {
            if target != INVALID_STATE_INDEX {
                push_index_if_placeable(
                    template,
                    board,
                    target,
                    &mut scratch.visited_generations,
                    generation,
                    &mut scratch.queue,
                );
            }
        }

        let rotation_count = if allow_180 { 3 } else { 2 };
        for slot in 0..rotation_count {
            if let Some(rotated) = first_successful_kick(template, board, state_index, slot) {
                push_index_if_placeable(
                    template,
                    board,
                    rotated,
                    &mut scratch.visited_generations,
                    generation,
                    &mut scratch.queue,
                );
            }
        }
    }
    ReachabilitySearchResult {
        locks,
        visited_state_count: queue_cursor,
        exhaustive: true,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ReverseReachabilityResult {
    reachable: bool,
    visited_state_count: usize,
}

fn reverse_lock_reachable(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
) -> ReverseReachabilityResult {
    let target = State { rotation, x, y };
    let Some(target_index) = state_index(template.width, template.ceiling, target) else {
        return ReverseReachabilityResult::default();
    };
    if template.state_masks[target_index] == INVALID_STATE_MASK
        || board & template.state_masks[target_index] != 0
        || !grounded_index(template, board, target_index)
    {
        return ReverseReachabilityResult::default();
    }

    reverse_lock_reachable_large(template, board, target_index, scratch)
}

fn reverse_lock_reachable_large(
    template: &ReachabilityTemplate,
    board: u64,
    target_index: usize,
    scratch: &mut ReachabilityScratch,
) -> ReverseReachabilityResult {
    let state_count = template.state_masks.len();
    let generation = scratch.begin_search(state_count);
    // `reverse_lock_reachable` already proved that the target is a valid,
    // collision-free grounded lock. Seed it directly so every short reverse
    // search does not repeat those board-mask checks.
    scratch.visited_generations[target_index] = generation;
    scratch.queue.push(target_index as u16);

    let mut queue_cursor = 0usize;
    while queue_cursor < scratch.queue.len() {
        let current = usize::from(scratch.queue[queue_cursor]);
        queue_cursor += 1;
        if template.is_sky_seed(current) {
            return ReverseReachabilityResult {
                reachable: true,
                visited_state_count: queue_cursor,
            };
        }

        for &source in &template.reverse_translation_sources[current] {
            if source == INVALID_STATE_INDEX {
                continue;
            }
            push_index_if_placeable(
                template,
                board,
                source,
                &mut scratch.visited_generations,
                generation,
                &mut scratch.queue,
            );
        }

        let reverse_start = template.reverse_rotation_offsets[current] as usize;
        let reverse_end = template.reverse_rotation_offsets[current + 1] as usize;
        for source in &template.reverse_rotation_sources[reverse_start..reverse_end] {
            if !reverse_rotation_is_first_success(template, board, *source) {
                continue;
            }
            push_index_if_placeable(
                template,
                board,
                source.source,
                &mut scratch.visited_generations,
                generation,
                &mut scratch.queue,
            );
        }
    }

    ReverseReachabilityResult {
        reachable: false,
        visited_state_count: queue_cursor,
    }
}

fn best_scoring_lock_evidence(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
) -> Option<ScoringLockEvidence> {
    let target = State { rotation, x, y };
    let target_index = state_index(template.width, template.ceiling, target)?;
    if template.state_masks[target_index] == INVALID_STATE_MASK
        || board & template.state_masks[target_index] != 0
        || !grounded_index(template, board, target_index)
    {
        return None;
    }

    let start = template.reverse_rotation_offsets[target_index] as usize;
    let end = template.reverse_rotation_offsets[target_index + 1] as usize;
    let mut best = None::<((bool, bool, u8), ScoringLockEvidence)>;
    for source in &template.reverse_rotation_sources[start..end] {
        if !reverse_rotation_is_first_success(template, board, *source) {
            continue;
        }
        let source_index = usize::from(source.source);
        if template.state_masks[source_index] == INVALID_STATE_MASK
            || board & template.state_masks[source_index] != 0
            || !reverse_lock_reachable_large(template, board, source_index, scratch).reachable
        {
            continue;
        }
        let transition = source_index * 3 + usize::from(source.slot);
        let target_offset = template.rotation_target_offsets[transition] as usize
            + usize::from(source.prior_target_count);
        let kick_index = template.rotation_kick_indices[target_offset];
        let predecessor = state_from_index(template.width, template.ceiling, source_index);
        let request = match source.slot {
            0 => RotationRequest::Clockwise,
            1 => RotationRequest::CounterClockwise,
            _ => RotationRequest::HalfTurn,
        };
        let evidence = ScoringLockEvidence::rotation(
            predecessor.rotation,
            request,
            kick_index,
            target.x - predecessor.x,
            target.y - predecessor.y,
            predecessor.x,
            predecessor.y,
        );
        let is_quarter_turn = source.slot < 2;
        let rank = (
            is_quarter_turn && kick_index == 4,
            is_quarter_turn,
            kick_index,
        );
        if best.as_ref().is_none_or(|(current, _)| rank > *current) {
            best = Some((rank, evidence));
        }
    }
    best.map(|(_, evidence)| evidence)
}

fn scoring_lock_is_immobile(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> bool {
    let Some(target_index) =
        state_index(template.width, template.ceiling, State { rotation, x, y })
    else {
        return false;
    };
    let translations_blocked = template.translation_targets[target_index]
        .iter()
        .all(|target| {
            *target == INVALID_STATE_INDEX
                || board & template.state_masks[usize::from(*target)] != 0
        });
    let upward_blocked = state_index(
        template.width,
        template.ceiling,
        State {
            rotation,
            x,
            y: y.saturating_add(1),
        },
    )
    .is_none_or(|target| {
        template.state_masks[target] == INVALID_STATE_MASK
            || board & template.state_masks[target] != 0
    });
    translations_blocked && upward_blocked
}

fn reverse_rotation_is_first_success(
    template: &ReachabilityTemplate,
    board: u64,
    source: ReverseRotationSource,
) -> bool {
    let transition = usize::from(source.source) * 3 + usize::from(source.slot);
    let prior_start = template.rotation_target_offsets[transition] as usize;
    let prior_end = prior_start + usize::from(source.prior_target_count);
    template.rotation_targets[prior_start..prior_end]
        .iter()
        .all(|target| board & template.state_masks[usize::from(*target)] != 0)
}

const INVALID_STATE_INDEX: u16 = u16::MAX;

fn push_index_if_placeable(
    template: &ReachabilityTemplate,
    board: u64,
    state_index: u16,
    visited_generations: &mut [u16],
    generation: u16,
    queue: &mut Vec<u16>,
) {
    let index = usize::from(state_index);
    if template.state_masks[index] == INVALID_STATE_MASK || board & template.state_masks[index] != 0
    {
        return;
    }
    if visited_generations[index] != generation {
        visited_generations[index] = generation;
        queue.push(state_index);
    }
}

fn state_index(width: u8, ceiling: i8, state: State) -> Option<usize> {
    if state.x < 0 || state.y < 0 || state.x >= width as i8 || state.y > ceiling {
        return None;
    }
    Some(
        (state.rotation.quarter_turns() as usize * (ceiling as usize + 1) + state.y as usize)
            * width as usize
            + state.x as usize,
    )
}

fn grounded_index(template: &ReachabilityTemplate, board: u64, state_index: usize) -> bool {
    let state = state_from_index(template.width, template.ceiling, state_index);
    if state.y == 0 {
        return true;
    }
    let down = template.translation_targets[state_index][0];
    down == INVALID_STATE_INDEX || board & template.state_masks[usize::from(down)] != 0
}

fn first_successful_kick(
    template: &ReachabilityTemplate,
    board: u64,
    source: usize,
    transition_slot: usize,
) -> Option<u16> {
    let transition = source * 3 + transition_slot;
    let start = template.rotation_target_offsets[transition] as usize;
    let end = template.rotation_target_offsets[transition + 1] as usize;
    for &target in &template.rotation_targets[start..end] {
        if board & template.state_masks[usize::from(target)] == 0 {
            return Some(target);
        }
    }
    None
}

fn compile_translation_targets_and_seeds(
    width: u8,
    height: u8,
    ceiling: i8,
    state_masks: &[u64],
) -> (Vec<[u16; 3]>, Vec<u16>) {
    let mut translations = Vec::with_capacity(state_masks.len());
    let mut seeds = Vec::new();
    for source in 0..state_masks.len() {
        let state = state_from_index(width, ceiling, source);
        let candidates = [
            State {
                y: state.y - 1,
                ..state
            },
            State {
                x: state.x - 1,
                ..state
            },
            State {
                x: state.x + 1,
                ..state
            },
        ];
        let mut targets = [INVALID_STATE_INDEX; 3];
        for (slot, candidate) in candidates.into_iter().enumerate() {
            let Some(target) = state_index(width, ceiling, candidate) else {
                continue;
            };
            if state_masks[target] == INVALID_STATE_MASK {
                continue;
            }
            targets[slot] = u16::try_from(target).unwrap_or(INVALID_STATE_INDEX);
        }
        translations.push(targets);
        if state.y >= height as i8 && state_masks[source] != INVALID_STATE_MASK {
            if let Ok(source) = u16::try_from(source) {
                seeds.push(source);
            }
        }
    }
    (translations, seeds)
}

#[inline]
fn harddrop_path_mask(lock_mask: u64, width: u8) -> u64 {
    let shift = u32::from(width);
    let mut path = lock_mask;
    path |= path << shift;
    path |= path << (shift * 2);
    if shift * 4 < u64::BITS {
        path |= path << (shift * 4);
    }
    path
}

fn compile_reverse_translation_sources(forward: &[[u16; 3]]) -> Vec<[u16; 3]> {
    let mut reverse = vec![[INVALID_STATE_INDEX; 3]; forward.len()];
    for (source, targets) in forward.iter().enumerate() {
        let Ok(source) = u16::try_from(source) else {
            continue;
        };
        for (slot, &target) in targets.iter().enumerate() {
            if target != INVALID_STATE_INDEX {
                reverse[usize::from(target)][slot] = source;
            }
        }
    }
    reverse
}

fn compile_rotation_targets(
    width: u8,
    ceiling: i8,
    state_masks: &[u64],
    kick_deltas: &[Vec<(i8, i8)>; 12],
) -> (Vec<u32>, Vec<u16>, Vec<u8>) {
    let mut offsets = Vec::with_capacity(state_masks.len() * 3 + 1);
    let mut targets = Vec::new();
    let mut kick_indices = Vec::new();
    offsets.push(0);
    for source in 0..state_masks.len() {
        let state = state_from_index(width, ceiling, source);
        let rotations = [
            state.rotation.clockwise(),
            state.rotation.counter_clockwise(),
            state.rotation.rotated_180(),
        ];
        for (slot, to) in rotations.into_iter().enumerate() {
            let transition = rotation_transition_index(state.rotation, slot);
            for (kick_index, &(dx, dy)) in kick_deltas[transition].iter().enumerate() {
                let candidate = State {
                    rotation: to,
                    x: state.x + dx,
                    y: state.y + dy,
                };
                let Some(target) = state_index(width, ceiling, candidate) else {
                    continue;
                };
                if state_masks[target] == INVALID_STATE_MASK {
                    continue;
                }
                let Ok(target) = u16::try_from(target) else {
                    continue;
                };
                targets.push(target);
                kick_indices.push(u8::try_from(kick_index).unwrap_or(u8::MAX));
            }
            offsets.push(targets.len() as u32);
        }
    }
    (offsets, targets, kick_indices)
}

fn compile_seed_words(state_count: usize, seeds: &[u16]) -> Vec<u64> {
    let mut words = vec![0_u64; state_count.div_ceil(64)];
    for &seed in seeds {
        let index = usize::from(seed);
        words[index / 64] |= 1_u64 << (index % 64);
    }
    words
}

fn compile_reverse_rotation_sources(
    state_count: usize,
    forward_offsets: &[u32],
    forward_targets: &[u16],
) -> (Vec<u32>, Vec<ReverseRotationSource>) {
    let mut counts = vec![0_u32; state_count + 1];
    for &target in forward_targets {
        counts[usize::from(target) + 1] = counts[usize::from(target) + 1].saturating_add(1);
    }
    for index in 1..counts.len() {
        counts[index] = counts[index].saturating_add(counts[index - 1]);
    }
    let offsets = counts.clone();
    let mut cursors = counts;
    let mut sources = vec![
        ReverseRotationSource {
            source: 0,
            slot: 0,
            prior_target_count: 0,
        };
        forward_targets.len()
    ];
    for transition in 0..state_count * 3 {
        let start = forward_offsets[transition] as usize;
        let end = forward_offsets[transition + 1] as usize;
        let Ok(source) = u16::try_from(transition / 3) else {
            continue;
        };
        let slot = (transition % 3) as u8;
        for (prior_target_count, &target) in forward_targets[start..end].iter().enumerate() {
            let Ok(prior_target_count) = u8::try_from(prior_target_count) else {
                continue;
            };
            let target = usize::from(target);
            let cursor = cursors[target] as usize;
            sources[cursor] = ReverseRotationSource {
                source,
                slot,
                prior_target_count,
            };
            cursors[target] += 1;
        }
    }
    (offsets, sources)
}

fn state_from_index(width: u8, ceiling: i8, index: usize) -> State {
    let rotation_stride = (ceiling as usize + 1) * width as usize;
    let rotation = RotationState::from_quarter_turns((index / rotation_stride) as u8)
        .expect("compiled reachability rotation is valid");
    let remainder = index % rotation_stride;
    State {
        rotation,
        x: (remainder % width as usize) as i8,
        y: (remainder / width as usize) as i8,
    }
}

fn rotation_transition_index(from: RotationState, transition_slot: usize) -> usize {
    from.quarter_turns() as usize * 3 + transition_slot
}

fn source_ceiling(height: u8, piece: PieceKind, allow_180: bool, profile: &KickTableProfile) -> i8 {
    let maximum_downward = profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .filter(|entry| allow_180 || !entry.transition().is_180())
        .flat_map(|entry| {
            entry.sequence().offsets().iter().map(move |offset| {
                normalized_kick_delta(
                    piece,
                    entry.transition().from(),
                    entry.transition().to(),
                    offset.dx(),
                    offset.dy(),
                )
                .1
            })
        })
        .filter(|dy| *dy < 0)
        .map(|dy| -dy)
        .max()
        .unwrap_or(0);
    height.saturating_add(maximum_downward as u8) as i8
}

fn normalized_kick_delta(
    piece: PieceKind,
    from: RotationState,
    to: RotationState,
    kick_dx: i8,
    kick_dy: i8,
) -> (i8, i8) {
    let (from_x, from_y) = normalized_rotation_center(piece, from);
    let (to_x, to_y) = normalized_rotation_center(piece, to);
    (kick_dx + from_x - to_x, kick_dy + from_y - to_y)
}

fn normalized_rotation_center(piece: PieceKind, rotation: RotationState) -> (i8, i8) {
    const JLSTZ: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, 1)];
    const I: [(i8, i8); 4] = [(0, 0), (-2, 2), (0, 1), (-1, 2)];
    let index = rotation.quarter_turns() as usize;
    match piece_index(piece) {
        0 => I[index],
        1 => (0, 0),
        _ => JLSTZ[index],
    }
}
// SRP rationale: this module has one behavior-level change reason: exhaustive SRS+ lock reachability over compact WASM boards.
