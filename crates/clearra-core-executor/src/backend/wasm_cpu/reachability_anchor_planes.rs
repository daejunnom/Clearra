//! Exact full-family movement on sets of normalized shape anchors.
//!
//! Each bit denotes an anchor of one rotation on one immutable board. The
//! ordered kick loop removes a source as soon as its first free destination
//! is found, even when that destination was reached earlier. This is the
//! existing movement graph's least fixed point, not a relaxed dependency DAG.
//! No operation-subset universe or Boolean decision diagram is constructed.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::piece::rotation::RotationState;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{KickOffset, KickTableProfile};

use super::{
    ReachabilitySearchResult, ReachabilityTemplate, ReachableLocks, ReverseReachabilityResult,
};
use crate::backend::wasm_cpu::kick_profiles::normalized_kick_delta_wide;

const ROTATIONS: usize = 4;
const TRANSITIONS: usize = 12;
const WORD_BITS: usize = u64::BITS as usize;
const MAX_WORDS: usize = 64;
const MAX_WIDTH: usize = 10;
const HORIZONTAL_DELTAS: usize = 2 * MAX_WIDTH - 1;
// This bounds raw ordered-list work, independently of its compact CSR size.
// Refusing a larger list preserves the existing scalar query and its trace;
// no prefix of a caller's actual kick table is substituted for the table.
const MAX_KICKS_PER_TRANSITION: usize = 32;
const MAX_TOTAL_KICKS: usize = 256;

// Keep the source dispatch separate from unrelated local-search-ab features.
// The isolated default-feature comparison must qualify this before promotion.
const FULL_FORWARD_DEFAULT: bool = false;
const HYBRID_REVERSE_DEFAULT: bool = false;

#[cfg(test)]
thread_local! {
    static TEST_MODE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

pub(super) fn full_forward_enabled() -> bool {
    enabled(FULL_FORWARD_DEFAULT)
}

pub(super) fn hybrid_reverse_enabled() -> bool {
    enabled(HYBRID_REVERSE_DEFAULT)
}

fn enabled(product_default: bool) -> bool {
    #[cfg(test)]
    if let Some(enabled) = TEST_MODE.with(std::cell::Cell::get) {
        return enabled;
    }
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    {
        static MODE: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
        return MODE
            .get_or_init(|| match std::env::var("CLEARRA_ANCHOR_PLANES").as_deref() {
                Ok("1") => Some(true),
                Ok("0") => Some(false),
                _ => None,
            })
            .unwrap_or(product_default);
    }
    #[cfg(not(all(feature = "local-search-ab", not(target_family = "wasm"))))]
    product_default
}

#[cfg(test)]
pub(super) fn with_mode<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_MODE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(TEST_MODE.with(|mode| mode.replace(Some(enabled))));
    run()
}

/// Borrow the exact movement owner without making per-query normalized Vecs.
/// A builtin table applies its constant shape-origin correction lazily; an
/// imported authority already owns normalized slices and supplies them intact.
pub(super) enum OrderedKicks<'a> {
    Normalized([&'a [(i16, i16)]; TRANSITIONS]),
    Raw {
        sequences: [&'a [KickOffset]; TRANSITIONS],
        origins: [(i16, i16); TRANSITIONS],
    },
}

impl<'a> OrderedKicks<'a> {
    pub(super) fn from_profile(
        piece: PieceKind,
        profile: &'a KickTableProfile,
        allow_180: bool,
    ) -> Self {
        let mut sequences = [&[][..]; TRANSITIONS];
        let mut origins = [(0, 0); TRANSITIONS];
        let mut seen = 0_u16;
        // sequence_for is a linear first-entry lookup. One pass preserves
        // that identity, including an empty first entry, without twelve scans.
        for entry in profile.entries() {
            let transition = entry.transition();
            if transition.piece() != piece {
                continue;
            }
            let from = transition.from();
            let to = transition.to();
            let turn = (to.quarter_turns() + 4 - from.quarter_turns()) % 4;
            let slot = match turn {
                1 => 0,
                3 => 1,
                2 if allow_180 => 2,
                _ => continue,
            };
            let index = usize::from(from.quarter_turns()) * 3 + slot;
            let bit = 1_u16 << index;
            if seen & bit == 0 {
                seen |= bit;
                origins[index] = normalized_kick_delta_wide(piece, from, to, 0, 0);
                sequences[index] = entry.sequence().offsets();
            }
        }
        Self::Raw { sequences, origins }
    }

    fn within_work_bound(&self) -> bool {
        let mut total = 0_usize;
        for transition in 0..TRANSITIONS {
            let count = match self {
                Self::Normalized(sequences) => sequences[transition].len(),
                Self::Raw { sequences, .. } => sequences[transition].len(),
            };
            if count > MAX_KICKS_PER_TRANSITION {
                return false;
            }
            total += count;
            if total > MAX_TOTAL_KICKS {
                return false;
            }
        }
        true
    }

    fn iter(&self, transition: usize) -> KickIter<'_> {
        match self {
            Self::Normalized(sequences) => KickIter::Normalized(sequences[transition].iter()),
            Self::Raw { sequences, origins } => KickIter::Raw {
                offsets: sequences[transition].iter(),
                origin: origins[transition],
            },
        }
    }
}

enum KickIter<'a> {
    Normalized(core::slice::Iter<'a, (i16, i16)>),
    Raw {
        offsets: core::slice::Iter<'a, KickOffset>,
        origin: (i16, i16),
    },
}

impl Iterator for KickIter<'_> {
    type Item = (i16, i16);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Normalized(values) => values.next().copied(),
            Self::Raw { offsets, origin } => offsets.next().map(|offset| {
                (
                    i16::from(offset.dx()) + origin.0,
                    i16::from(offset.dy()) + origin.1,
                )
            }),
        }
    }
}

/// These masks describe source columns, before applying a two-dimensional
/// translation. A raw bit shift alone would incorrectly wrap across a wall.
const COLUMN_PATTERNS: [[u128; HORIZONTAL_DELTAS]; MAX_WIDTH] = column_patterns();

const fn column_patterns() -> [[u128; HORIZONTAL_DELTAS]; MAX_WIDTH] {
    let mut patterns = [[0; HORIZONTAL_DELTAS]; MAX_WIDTH];
    let mut width = 1;
    while width <= MAX_WIDTH {
        let row = (1_u128 << width) - 1;
        let mut index = 0;
        while index < HORIZONTAL_DELTAS {
            let dx = index as i32 - (MAX_WIDTH as i32 - 1);
            let columns = if dx >= width as i32 || dx <= -(width as i32) {
                0
            } else if dx >= 0 {
                row >> dx
            } else {
                row & (row << -dx)
            };
            let mut repeated = columns;
            let mut period = width;
            while period < u128::BITS as usize {
                repeated |= repeated << period;
                period *= 2;
            }
            patterns[width - 1][index] = repeated;
            index += 1;
        }
        width += 1;
    }
    patterns
}

/// Counts set operations, separately from the number of reached scalar poses.
/// It does not multiply a word operation by the number of represented anchors.
#[derive(Clone, Copy, Debug, Default)]
#[cfg(test)]
pub(super) struct AnchorPlaneWork {
    pub(super) frontier_pops: usize,
    pub(super) translation_word_tests: usize,
    pub(super) rotation_word_tests: usize,
    pub(super) insertion_word_tests: usize,
}

pub(super) struct AnchorPlaneResult {
    pub(super) search: ReachabilitySearchResult,
    #[cfg(test)]
    pub(super) work: AnchorPlaneWork,
}

pub(super) struct AnchorReverseResult {
    pub(super) search: ReverseReachabilityResult,
    #[cfg(test)]
    pub(super) work: AnchorPlaneWork,
}

/// Query the complete lock family without retaining any additional heap.
///
/// `piece` and `ordered` must be the same normalized geometry and complete
/// ordered sequences used to compile `template`. In particular a display
/// profile ID cannot substitute for an imported table's actual authority.
/// The adapter retains the scalar traversal for certificate trace consumers.
///
/// Refusal is distinct from an empty family. The caller must use the original
/// exact traversal if this finite stack representation does not fit. Ordinary
/// small planes use only the two-word specialization, including their sky.
pub(super) fn search_reachable_locks(
    template: &ReachabilityTemplate,
    piece: PieceKind,
    board: u64,
    ordered: &OrderedKicks<'_>,
) -> Option<AnchorPlaneResult> {
    if !ordered.within_work_bound() {
        return None;
    }
    let geometry = anchor_geometry(template, piece, board)?;
    match geometry.words {
        1..=2 => Some(search::<2>(template, board, ordered, geometry)),
        3..=8 => Some(search::<8>(template, board, ordered, geometry)),
        9..=MAX_WORDS => Some(search::<MAX_WORDS>(template, board, ordered, geometry)),
        _ => None,
    }
}

/// Continue one already validated exact reverse query. `discovered` is the
/// scalar queue of unique poses; only its unprocessed suffix becomes pending.
/// Earlier processed poses remain visited and are never expanded a second time.
/// No scalar trace is written: callers requiring one must retain the scalar
/// primitive, including queries composed from qualified non-sky exits.
pub(super) fn finish_reverse_search(
    template: &ReachabilityTemplate,
    piece: PieceKind,
    board: u64,
    ordered: &OrderedKicks<'_>,
    discovered: &[u16],
    processed: usize,
) -> Option<AnchorReverseResult> {
    if !ordered.within_work_bound() || discovered.is_empty() || processed > discovered.len() {
        return None;
    }
    let geometry = anchor_geometry(template, piece, board)?;
    match geometry.words {
        1..=2 => reverse::<2>(template, board, ordered, geometry, discovered, processed),
        3..=8 => reverse::<8>(template, board, ordered, geometry, discovered, processed),
        9..=MAX_WORDS => {
            reverse::<MAX_WORDS>(template, board, ordered, geometry, discovered, processed)
        }
        _ => None,
    }
}

fn anchor_geometry(
    template: &ReachabilityTemplate,
    piece: PieceKind,
    board: u64,
) -> Option<AnchorGeometry> {
    let width = usize::from(template.width);
    let height = usize::from(template.height);
    let ceiling = usize::try_from(template.ceiling).ok()?;
    let area = width.checked_mul(height)?;
    let anchor_count = width.checked_mul(ceiling.checked_add(1)?)?;
    let state_count = anchor_count.checked_mul(ROTATIONS)?;
    if !(1..=MAX_WIDTH).contains(&width)
        || !(1..=6).contains(&height)
        || !(1..=WORD_BITS).contains(&area)
        || ceiling < height
        || template.state_masks.len() != state_count
        || template.sky_seed_words.len() != state_count.div_ceil(WORD_BITS)
        || (area < WORD_BITS && board >> area != 0)
    {
        return None;
    }
    let words = anchor_count.div_ceil(WORD_BITS);
    let definition = standard_tetromino_registry().get(piece)?;
    let cells = RotationState::ALL.map(|rotation| {
        definition
            .shape(rotation)
            .cells()
            .map(|cell| (cell.x(), cell.y()))
    });
    if cells.iter().flatten().any(|&(x, y)| x < 0 || y < 0) {
        return None;
    }
    Some(AnchorGeometry {
        width,
        area,
        anchor_count,
        words,
        cells,
    })
}

struct AnchorGeometry {
    width: usize,
    area: usize,
    anchor_count: usize,
    words: usize,
    cells: [[(i8, i8); 4]; ROTATIONS],
}

fn search<const CAPACITY: usize>(
    template: &ReachabilityTemplate,
    board: u64,
    ordered: &OrderedKicks<'_>,
    geometry: AnchorGeometry,
) -> AnchorPlaneResult {
    let free = free_anchor_planes::<CAPACITY>(board, &geometry);
    let AnchorGeometry {
        width,
        area,
        anchor_count,
        words,
        ..
    } = geometry;
    let mut reached = [[0_u64; CAPACITY]; ROTATIONS];
    let mut pending = [[0_u64; CAPACITY]; ROTATIONS];
    let mut active = 0_u8;
    let mut reached_count = 0;
    for rotation in 0..ROTATIONS {
        for word in 0..words {
            // Use the immutable template's actual seed set. Its rotation
            // planes need not begin at a machine-word boundary.
            let seeds = read_word(
                &template.sky_seed_words,
                rotation * anchor_count + word * 64,
            ) & free[rotation][word];
            reached[rotation][word] = seeds;
            pending[rotation][word] = seeds;
            reached_count += seeds.count_ones() as usize;
            if seeds != 0 {
                active |= 1 << rotation;
            }
        }
    }

    #[cfg(test)]
    let mut work = AnchorPlaneWork::default();
    while active != 0 {
        let from = active.trailing_zeros() as usize;
        active &= !(1 << from);
        let delta = core::mem::replace(&mut pending[from], [0; CAPACITY]);
        #[cfg(test)]
        {
            work.frontier_pops += 1;
        }
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0)] {
            let mut admitted = [0; CAPACITY];
            let shift = translation_sources(
                width,
                &delta[..words],
                &free[from][..words],
                dx,
                dy,
                &mut admitted[..words],
                #[cfg(test)]
                &mut work.translation_word_tests,
            );
            let Some(shift) = shift else {
                continue;
            };
            #[cfg(test)]
            {
                work.insertion_word_tests += words;
            }
            let added = insert_shifted(
                &admitted[..words],
                shift,
                &mut reached[from][..words],
                &mut pending[from][..words],
            );
            reached_count += added;
            if added != 0 {
                active |= 1 << from;
            }
        }
        let slots = if template.allow_180 { 3 } else { 2 };
        for (slot, turn) in [1, 3, 2].into_iter().take(slots).enumerate() {
            let to = (from + turn) % ROTATIONS;
            let mut remaining = delta;
            for (dx, dy) in ordered.iter(from * 3 + slot) {
                let mut admitted = [0; CAPACITY];
                let shift = translation_sources(
                    width,
                    &remaining[..words],
                    &free[to][..words],
                    dx,
                    dy,
                    &mut admitted[..words],
                    #[cfg(test)]
                    &mut work.rotation_word_tests,
                );
                let Some(shift) = shift else {
                    continue;
                };
                let mut has_remaining = false;
                for word in 0..words {
                    // First success belongs to the source. An already visited
                    // destination still suppresses every later kick here.
                    remaining[word] &= !admitted[word];
                    has_remaining |= remaining[word] != 0;
                }
                #[cfg(test)]
                {
                    work.insertion_word_tests += words;
                }
                let added = insert_shifted(
                    &admitted[..words],
                    shift,
                    &mut reached[to][..words],
                    &mut pending[to][..words],
                );
                reached_count += added;
                if added != 0 {
                    active |= 1 << to;
                }
                if !has_remaining {
                    break;
                }
            }
        }
    }

    let lock_domain = if area == WORD_BITS {
        u64::MAX
    } else {
        (1_u64 << area) - 1
    };
    let mut locks = ReachableLocks::default();
    for rotation in 0..ROTATIONS {
        // A lower free anchor means this pose is not grounded. Row zero has
        // no lower anchor and is therefore grounded, exactly as in the scalar
        // traversal. Preserve its clipped-sky lock identities as well.
        locks.anchors[rotation] =
            reached[rotation][0] & !(free[rotation][0] << width) & lock_domain;
    }
    AnchorPlaneResult {
        search: ReachabilitySearchResult {
            locks,
            visited_state_count: reached_count,
            exhaustive: true,
        },
        #[cfg(test)]
        work,
    }
}

fn reverse<const CAPACITY: usize>(
    template: &ReachabilityTemplate,
    board: u64,
    ordered: &OrderedKicks<'_>,
    geometry: AnchorGeometry,
    discovered: &[u16],
    processed: usize,
) -> Option<AnchorReverseResult> {
    let free = free_anchor_planes::<CAPACITY>(board, &geometry);
    let AnchorGeometry {
        width,
        anchor_count,
        words,
        ..
    } = geometry;
    let mut reached = [[0_u64; CAPACITY]; ROTATIONS];
    let mut pending = [[0_u64; CAPACITY]; ROTATIONS];
    let mut active = 0_u8;
    for (position, &index) in discovered.iter().enumerate() {
        let rotation = usize::from(index) / anchor_count;
        let anchor = usize::from(index) % anchor_count;
        let word = anchor / WORD_BITS;
        let bit = 1_u64 << (anchor % WORD_BITS);
        if rotation >= ROTATIONS
            || free[rotation][word] & bit == 0
            || reached[rotation][word] & bit != 0
        {
            return None;
        }
        reached[rotation][word] |= bit;
        if position >= processed {
            pending[rotation][word] |= bit;
            active |= 1 << rotation;
        }
    }
    let mut processed_count = processed;
    #[cfg(test)]
    let mut work = AnchorPlaneWork::default();
    while active != 0 {
        let to = active.trailing_zeros() as usize;
        active &= !(1 << to);
        let delta = core::mem::replace(&mut pending[to], [0; CAPACITY]);
        #[cfg(test)]
        {
            work.frontier_pops += 1;
        }
        processed_count += delta[..words]
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        if delta[..words].iter().enumerate().any(|(word, &bits)| {
            bits & read_word(
                &template.sky_seed_words,
                to * anchor_count + word * WORD_BITS,
            ) != 0
        }) {
            return Some(AnchorReverseResult {
                search: ReverseReachabilityResult {
                    reachable: true,
                    visited_state_count: processed_count,
                },
                #[cfg(test)]
                work,
            });
        }

        // Invert the existing down/left/right edges. A reverse step upward
        // is a predecessor of a real descent, not a new player action.
        for (dx, dy) in [(0, 1), (1, 0), (-1, 0)] {
            let mut admitted = [0; CAPACITY];
            let shift = translation_sources(
                width,
                &delta[..words],
                &free[to][..words],
                dx,
                dy,
                &mut admitted[..words],
                #[cfg(test)]
                &mut work.translation_word_tests,
            );
            let Some(shift) = shift else {
                continue;
            };
            #[cfg(test)]
            {
                work.insertion_word_tests += words;
            }
            if insert_shifted(
                &admitted[..words],
                shift,
                &mut reached[to][..words],
                &mut pending[to][..words],
            ) != 0
            {
                active |= 1 << to;
            }
        }

        let slots = if template.allow_180 { 3 } else { 2 };
        for (slot, turn) in [1, 3, 2].into_iter().take(slots).enumerate() {
            let from = (to + ROTATIONS - turn) % ROTATIONS;
            let mut remaining: [u64; CAPACITY] =
                core::array::from_fn(|word| free[from][word] & !reached[from][word]);
            for (dx, dy) in ordered.iter(from * 3 + slot) {
                let mut admitted = [0; CAPACITY];
                let shift = translation_sources(
                    width,
                    &remaining[..words],
                    &free[to][..words],
                    dx,
                    dy,
                    &mut admitted[..words],
                    #[cfg(test)]
                    &mut work.rotation_word_tests,
                );
                let Some(shift) = shift else {
                    continue;
                };
                let mut has_remaining = false;
                for word in 0..words {
                    // Earlier FREE destinations suppress later kicks whether
                    // or not those destinations belong to this reverse delta.
                    // Only after removing every first-success source may the
                    // selected destinations be intersected with the frontier.
                    remaining[word] &= !admitted[word];
                    has_remaining |= remaining[word] != 0;
                    admitted[word] &= shifted_word(&delta[..words], word, -shift);
                }
                #[cfg(test)]
                {
                    work.insertion_word_tests += words;
                }
                if insert_shifted(
                    &admitted[..words],
                    0,
                    &mut reached[from][..words],
                    &mut pending[from][..words],
                ) != 0
                {
                    active |= 1 << from;
                }
                if !has_remaining {
                    break;
                }
            }
        }
    }
    Some(AnchorReverseResult {
        search: ReverseReachabilityResult {
            reachable: false,
            visited_state_count: processed_count,
        },
        #[cfg(test)]
        work,
    })
}

fn free_anchor_planes<const CAPACITY: usize>(
    board: u64,
    geometry: &AnchorGeometry,
) -> [[u64; CAPACITY]; ROTATIONS] {
    let width = geometry.width;
    let words = geometry.words;
    let final_bits = geometry.anchor_count % WORD_BITS;
    let final_mask = if final_bits == 0 {
        u64::MAX
    } else {
        (1_u64 << final_bits) - 1
    };
    let mut free = [[0_u64; CAPACITY]; ROTATIONS];
    for (rotation, cells) in geometry.cells.iter().enumerate() {
        let max_x = cells
            .iter()
            .map(|&(x, _)| i16::from(x))
            .max()
            .expect("a tetromino has four cells");
        let pattern = source_column_pattern(width, max_x);
        // Nonnegative normalized offsets can only project an occupied cell
        // to an earlier anchor. Since B has at most 64 bits, every blocker is
        // in the first anchor word, even when the sky requires more words.
        let blocked = cells.iter().fold(0, |occupied, &(x, y)| {
            let shift = x as u32 + width as u32 * y as u32;
            occupied | board.checked_shr(shift).unwrap_or(0)
        });
        for word in 0..words {
            free[rotation][word] = column_word(pattern, width, word);
        }
        free[rotation][words - 1] &= final_mask;
        free[rotation][0] &= !blocked;
    }
    free
}

fn source_column_pattern(width: usize, dx: i16) -> u128 {
    if dx <= -(width as i16) || dx >= width as i16 {
        return 0;
    }
    let index = (dx + MAX_WIDTH as i16 - 1) as usize;
    COLUMN_PATTERNS[width - 1][index]
}

fn column_word(pattern: u128, width: usize, word: usize) -> u64 {
    (pattern >> ((word * WORD_BITS) % width)) as u64
}

/// Read a word starting at any bit, including a nonaligned rotation plane.
fn read_word(words: &[u64], first_bit: usize) -> u64 {
    let index = first_bit / WORD_BITS;
    let offset = first_bit % WORD_BITS;
    let low = words.get(index).copied().unwrap_or(0) >> offset;
    if offset == 0 {
        low
    } else {
        low | (words.get(index + 1).copied().unwrap_or(0) << (WORD_BITS - offset))
    }
}

/// Return a translated word with zero beyond both ends of the finite plane.
fn shifted_word(words: &[u64], destination: usize, shift: i32) -> u64 {
    if shift < 0 {
        return read_word(words, destination * WORD_BITS + (-shift) as usize);
    }
    let shift = shift as usize;
    let word_shift = shift / WORD_BITS;
    let bit_shift = shift % WORD_BITS;
    let Some(source) = destination.checked_sub(word_shift) else {
        return 0;
    };
    let high = words.get(source).copied().unwrap_or(0) << bit_shift;
    if bit_shift == 0 || source == 0 {
        high
    } else {
        high | (words.get(source - 1).copied().unwrap_or(0) >> (WORD_BITS - bit_shift))
    }
}

/// Intersect source anchors with the inverse-translated free destinations.
/// The source-column condition prevents row wrapping independently of the
/// destination shape's narrower valid-column range.
fn translation_sources(
    width: usize,
    sources: &[u64],
    destination_free: &[u64],
    dx: i16,
    dy: i16,
    admitted: &mut [u64],
    #[cfg(test)] word_tests: &mut usize,
) -> Option<i32> {
    let columns = source_column_pattern(width, dx);
    if columns == 0 {
        return None;
    }
    let shift = i32::from(dx) + width as i32 * i32::from(dy);
    #[cfg(test)]
    {
        *word_tests += admitted.len();
    }
    let mut any = false;
    for (word, value) in admitted.iter_mut().enumerate() {
        *value = sources[word]
            & column_word(columns, width, word)
            & shifted_word(destination_free, word, -shift);
        any |= *value != 0;
    }
    any.then_some(shift)
}

fn insert_shifted(admitted: &[u64], shift: i32, reached: &mut [u64], pending: &mut [u64]) -> usize {
    let mut count = 0;
    for word in 0..reached.len() {
        let added = shifted_word(admitted, word, shift) & !reached[word];
        reached[word] |= added;
        pending[word] |= added;
        count += added.count_ones() as usize;
    }
    count
}

#[cfg(test)]
mod tests {
    use clearra_rules::kicks::{
        KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, KickTableProfileId,
        KickTransition, SrsKicks, VerifiedKickTableProfile,
    };

    use super::super::{
        grounded_index, reverse_lock_reachable, reverse_lock_reachable_in_request, state_index,
        AnchorMovementRequest, State, INVALID_STATE_MASK,
    };
    use super::*;
    use crate::backend::wasm_cpu::compact_kick_authority::CompactKickAuthority;
    use crate::backend::wasm_cpu::kick_profiles::builtin_kick_profile;
    use crate::backend::wasm_cpu::reachability::{
        search_reachable_locks as scalar_search, ReachabilityScratch,
    };
    use crate::reachability_reference::reference_spawn_locks_for_ordered_profile;

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    fn assert_scalar(
        template: &ReachabilityTemplate,
        piece: PieceKind,
        board: u64,
        ordered: &OrderedKicks<'_>,
        scratch: &mut ReachabilityScratch,
    ) -> AnchorPlaneResult {
        let actual = search_reachable_locks(template, piece, board, ordered)
            .expect("qualified finite anchor domain");
        let expected = scalar_search(template, board, scratch, None);
        assert_eq!(
            actual.search.locks.anchors, expected.locks.anchors,
            "{piece:?} board {board:#018x} at {}x{}",
            template.width, template.height,
        );
        assert_eq!(
            actual.search.visited_state_count,
            expected.visited_state_count
        );
        assert!(actual.work.frontier_pops <= actual.search.visited_state_count);
        assert!(actual.search.exhaustive);
        actual
    }

    #[test]
    fn anchor_planes_borrowed_sequences_keep_the_first_entry_including_empty() {
        let base = SrsKicks::srs_plus_profile();
        let first = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);
        let mut entries = vec![KickTableEntry::new(
            first,
            KickOffsetSequence::new(Vec::new()),
        )];
        entries.extend_from_slice(base.entries());
        let profile = KickTableProfile::new(base.id(), base.source_rule(), entries);
        let ordered = OrderedKicks::from_profile(PieceKind::T, &profile, true);
        for from in RotationState::ALL {
            for (slot, to) in [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ]
            .into_iter()
            .enumerate()
            {
                let expected: Vec<_> = profile
                    .sequence_for(KickTransition::new(PieceKind::T, from, to))
                    .into_iter()
                    .flat_map(|sequence| sequence.offsets())
                    .map(|offset| {
                        normalized_kick_delta_wide(PieceKind::T, from, to, offset.dx(), offset.dy())
                    })
                    .collect();
                assert_eq!(
                    ordered
                        .iter(usize::from(from.quarter_turns()) * 3 + slot)
                        .collect::<Vec<_>>(),
                    expected,
                );
            }
        }
        assert_eq!(
            ordered.iter(0).count(),
            0,
            "an empty first entry is authoritative"
        );
        let quarter_turns = OrderedKicks::from_profile(PieceKind::T, &profile, false);
        for from in 0..ROTATIONS {
            assert_eq!(quarter_turns.iter(from * 3 + 2).count(), 0);
        }
    }

    #[test]
    fn anchor_planes_long_imported_tables_keep_the_complete_scalar_trace() {
        let base = SrsKicks::srs_plus_profile();
        let long_transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let entries = base
            .entries()
            .iter()
            .map(|entry| {
                if entry.transition() == long_transition {
                    KickTableEntry::new(
                        long_transition,
                        KickOffsetSequence::new(vec![
                            KickOffset::new(0, 0);
                            MAX_KICKS_PER_TRANSITION + 1
                        ]),
                    )
                } else {
                    entry.clone()
                }
            })
            .collect();
        let profile = VerifiedKickTableProfile::try_new(KickTableProfile::new(
            KickTableProfileId::Custom,
            base.source_rule(),
            entries,
        ))
        .unwrap();
        let authority =
            CompactKickAuthority::admit_profile(10, 6, base.source_rule(), &profile).unwrap();
        let piece = PieceKind::T;
        let template = ReachabilityTemplate::compile_authority(&authority, piece);
        let ordered = OrderedKicks::Normalized(authority.piece(piece).ordered);
        assert!(!ordered.within_work_bound());
        // The aggregate cap is independent of the per-transition cap.
        let repeated = [(0_i16, 0_i16); MAX_TOTAL_KICKS / TRANSITIONS + 1];
        assert!(!OrderedKicks::Normalized([repeated.as_slice(); TRANSITIONS]).within_work_bound());
        assert!(search_reachable_locks(&template, piece, 0, &ordered).is_none());
        let mut scalar_scratch = ReachabilityScratch::default();
        let mut fallback_scratch = ReachabilityScratch::default();
        let mut long_queries = 0;
        with_mode(true, || {
            crate::backend::wasm_cpu::placement_dependency::with_mode(false, || {
                for board in [0, (0x1ff_u64 << 30) | (7 << 7)] {
                    let expected = reference_spawn_locks_for_ordered_profile(
                        10,
                        6,
                        board,
                        piece,
                        profile.profile(),
                    )
                    .unwrap();
                    for rotation in RotationState::ALL {
                        for y in 0..6_i8 {
                            for x in 0..10_i8 {
                                let scalar = reverse_lock_reachable(
                                    &template,
                                    board,
                                    rotation,
                                    x,
                                    y,
                                    &mut scalar_scratch,
                                );
                                let fallback = reverse_lock_reachable_in_request(
                                    &template,
                                    board,
                                    rotation,
                                    x,
                                    y,
                                    &mut fallback_scratch,
                                    AnchorMovementRequest {
                                        piece,
                                        profile: KickTableProfileId::NoKick,
                                        authority: Some(&authority),
                                    },
                                );
                                let wanted = expected[usize::from(rotation.quarter_turns())]
                                    & (1_u64 << (10 * y as usize + x as usize))
                                    != 0;
                                assert_eq!(scalar.reachable, wanted);
                                assert_eq!(fallback.reachable, wanted);
                                assert_eq!(
                                    fallback.visited_state_count,
                                    scalar.visited_state_count
                                );
                                assert_eq!(fallback_scratch.queue, scalar_scratch.queue);
                                long_queries += usize::from(scalar.visited_state_count > 32);
                            }
                        }
                    }
                }
            });
        });
        assert!(
            long_queries > 0,
            "the refused handoff is reached after 32 scalar poses"
        );
    }

    fn assert_reverse_family(
        template: &ReachabilityTemplate,
        piece: PieceKind,
        board: u64,
        ordered: &OrderedKicks<'_>,
        request: AnchorMovementRequest<'_>,
        expected: [u64; ROTATIONS],
    ) -> (usize, usize) {
        let mut scalar_scratch = ReachabilityScratch::default();
        let mut hybrid_scratch = ReachabilityScratch::default();
        let mut negatives = 0;
        let mut handed_off = 0;
        for rotation in RotationState::ALL {
            for y in 0..template.height as i8 {
                for x in 0..template.width as i8 {
                    let target = state_index(
                        template.width,
                        template.ceiling,
                        State {
                            rotation,
                            x,
                            y: i16::from(y),
                        },
                    )
                    .unwrap();
                    if template.state_masks[target] == INVALID_STATE_MASK
                        || board & template.state_masks[target] != 0
                        || !grounded_index(template, board, target)
                    {
                        continue;
                    }
                    let scalar = reverse_lock_reachable(
                        template,
                        board,
                        rotation,
                        x,
                        y,
                        &mut scalar_scratch,
                    );
                    let from_target =
                        finish_reverse_search(template, piece, board, ordered, &[target as u16], 0)
                            .expect("valid complete target query");
                    assert!(
                        from_target.work.frontier_pops <= from_target.search.visited_state_count
                    );
                    let hybrid = reverse_lock_reachable_in_request(
                        template,
                        board,
                        rotation,
                        x,
                        y,
                        &mut hybrid_scratch,
                        request,
                    );
                    let anchor = usize::from(template.width) * y as usize + x as usize;
                    let expected =
                        expected[usize::from(rotation.quarter_turns())] & (1_u64 << anchor) != 0;
                    assert_eq!(scalar.reachable, expected);
                    assert_eq!(
                        from_target.search.reachable, expected,
                        "complete reverse {piece:?}/{rotation:?} ({x},{y}), board {board:#018x}"
                    );
                    assert_eq!(
                        hybrid.reachable, expected,
                        "hybrid reverse {piece:?}/{rotation:?} ({x},{y}), board {board:#018x}"
                    );
                    if scalar.visited_state_count <= 32 {
                        assert_eq!(hybrid.visited_state_count, scalar.visited_state_count);
                        assert_eq!(hybrid_scratch.queue, scalar_scratch.queue);
                    }
                    if !expected {
                        negatives += 1;
                        assert_eq!(
                            from_target.search.visited_state_count, scalar.visited_state_count,
                            "a complete negative closes the identical predecessor set"
                        );
                        assert_eq!(hybrid.visited_state_count, scalar.visited_state_count);
                    }
                    handed_off +=
                        usize::from(hybrid_scratch.queue.len() < scalar_scratch.queue.len());
                }
            }
        }
        (negatives, handed_off)
    }

    #[test]
    fn anchor_planes_reverse_and_handoff_preserve_every_grounded_builtin_target() {
        let boards = [
            0,
            (0x1ff_u64 << 30) | (7 << 7),
            0x0155_02aa_0155_02aa,
            0x0003_0003_0003_0003,
            0x0222_0111_0222_0111,
        ];
        let mut negatives = 0;
        let mut handed_off = 0;
        with_mode(true, || {
            crate::backend::wasm_cpu::placement_dependency::with_mode(false, || {
                for profile in PROFILES {
                    let authority = builtin_kick_profile(profile).unwrap();
                    for piece in PieceKind::STANDARD_TETROMINOES {
                        let template = ReachabilityTemplate::compile(10, 6, piece, profile);
                        let ordered =
                            OrderedKicks::from_profile(piece, authority, template.allow_180);
                        for board in boards {
                            let expected = reference_spawn_locks_for_ordered_profile(
                                10, 6, board, piece, authority,
                            )
                            .unwrap();
                            let (closed, continued) = assert_reverse_family(
                                &template,
                                piece,
                                board,
                                &ordered,
                                AnchorMovementRequest {
                                    piece,
                                    profile,
                                    authority: None,
                                },
                                expected,
                            );
                            negatives += closed;
                            handed_off += continued;
                        }
                    }
                }
            });
        });
        assert!(negatives > 0, "the fixtures include exact closed negatives");
        assert!(
            handed_off > 0,
            "the actual 32-pose product handoff was exercised"
        );
    }

    #[test]
    fn anchor_planes_match_every_two_row_board_and_builtin_piece_profile() {
        let mut scratch = ReachabilityScratch::default();
        for profile in PROFILES {
            let authority = builtin_kick_profile(profile).unwrap();
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(4, 2, piece, profile);
                let ordered = OrderedKicks::from_profile(piece, authority, template.allow_180);
                for board in 0..1_u64 << 8 {
                    assert_scalar(&template, piece, board, &ordered, &mut scratch);
                }
            }
        }
    }

    #[test]
    fn anchor_planes_keep_multiword_sky_and_non_aligned_rotation_domains() {
        let boards = [
            0,
            0x03ff_ffff_ffff_ffff,
            0x0155_02aa_0155_02aa,
            0x01ff_0000_0000_0080,
            0x0003_0003_0003_0003,
            0x0222_0111_0222_0111,
        ];
        let mut scratch = ReachabilityScratch::default();
        for profile in PROFILES {
            let authority = builtin_kick_profile(profile).unwrap();
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(10, 6, piece, profile);
                let ordered = OrderedKicks::from_profile(piece, authority, template.allow_180);
                assert!(10 * (template.ceiling as usize + 1) > WORD_BITS);
                for board in boards {
                    let actual = assert_scalar(&template, piece, board, &ordered, &mut scratch);
                    let expected =
                        reference_spawn_locks_for_ordered_profile(10, 6, board, piece, authority)
                            .unwrap();
                    assert_eq!(actual.search.locks.anchors, expected);
                }
            }
        }
    }

    #[test]
    fn anchor_planes_follow_imported_kick_order_and_wide_offsets() {
        let base = SrsKicks::srs_plus_profile();
        let entries = base
            .entries()
            .iter()
            .map(|entry| {
                let transition = entry.transition();
                if transition.piece() != PieceKind::O {
                    return entry.clone();
                }
                KickTableEntry::new(
                    transition,
                    KickOffsetSequence::new(vec![
                        KickOffset::new(i8::MIN, i8::MAX),
                        KickOffset::new(0, i8::MAX),
                        KickOffset::new(4, 0),
                        KickOffset::new(0, 0),
                        KickOffset::new(0, i8::MIN),
                    ]),
                )
            })
            .collect();
        let profile = VerifiedKickTableProfile::try_new(KickTableProfile::new(
            KickTableProfileId::Custom,
            base.source_rule(),
            entries,
        ))
        .unwrap();
        let authority =
            CompactKickAuthority::admit_profile(6, 5, base.source_rule(), &profile).unwrap();
        let template = ReachabilityTemplate::compile_authority(&authority, PieceKind::O);
        let ordered = OrderedKicks::Normalized(authority.piece(PieceKind::O).ordered);
        assert!(6 * (template.ceiling as usize + 1) > 8 * WORD_BITS);
        let mut scratch = ReachabilityScratch::default();
        let wall = (0..5).fold(0_u64, |board, y| board | (0b001100 << (6 * y)));
        for board in [0, wall, wall | (0b110000 << 12)] {
            let actual = assert_scalar(&template, PieceKind::O, board, &ordered, &mut scratch);
            let expected = reference_spawn_locks_for_ordered_profile(
                6,
                5,
                board,
                PieceKind::O,
                profile.profile(),
            )
            .unwrap();
            assert_eq!(actual.search.locks.anchors, expected);
            let (dispatched, _) =
                crate::backend::wasm_cpu::placement_dependency::with_mode(false, || {
                    with_mode(true, || {
                        let result = super::super::try_anchor_plane_lock_family(
                            &template,
                            PieceKind::O,
                            board,
                            KickTableProfileId::NoKick,
                            Some(&authority),
                        );
                        assert_reverse_family(
                            &template,
                            PieceKind::O,
                            board,
                            &ordered,
                            AnchorMovementRequest {
                                piece: PieceKind::O,
                                profile: KickTableProfileId::NoKick,
                                authority: Some(&authority),
                            },
                            expected,
                        );
                        result
                    })
                });
            let dispatched =
                dispatched.expect("the imported ordered owner takes precedence over a nominal ID");
            assert_eq!(dispatched.locks.anchors, expected);
        }
    }

    #[test]
    fn anchor_planes_dispatch_preserves_scalar_certificate_trace_ownership() {
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::T;
        let template = ReachabilityTemplate::compile(10, 6, piece, profile);
        with_mode(true, || {
            let (without_trace, _) =
                crate::backend::wasm_cpu::placement_dependency::with_mode(false, || {
                    super::super::try_anchor_plane_lock_family(&template, piece, 0, profile, None)
                });
            assert!(without_trace.is_some());
            let (with_trace, _) =
                crate::backend::wasm_cpu::placement_dependency::with_mode(true, || {
                    super::super::try_anchor_plane_lock_family(&template, piece, 0, profile, None)
                });
            assert!(
                with_trace.is_none(),
                "the caller must retain its scalar trace"
            );
        });
    }

    #[test]
    fn anchor_planes_translation_never_wraps_at_walls_or_word_boundaries() {
        for width in [1, 4, 7, 10] {
            let anchors: usize = width * 13;
            let words = anchors.div_ceil(WORD_BITS);
            let mut sources = [0_u64; 3];
            let mut free = [0_u64; 3];
            for anchor in 0..anchors {
                sources[anchor / WORD_BITS] |= 1 << (anchor % WORD_BITS);
                free[anchor / WORD_BITS] |= 1 << (anchor % WORD_BITS);
            }
            for (dx, dy) in [
                (-127, 0),
                (-9, 2),
                (-1, 0),
                (1, 0),
                (9, -2),
                (0, -8),
                (0, 8),
                (127, 0),
            ] {
                let mut actual = [0_u64; 3];
                translation_sources(
                    width,
                    &sources[..words],
                    &free[..words],
                    dx,
                    dy,
                    &mut actual[..words],
                    &mut 0,
                );
                let mut expected = [0_u64; 3];
                for anchor in 0..anchors {
                    let x = (anchor % width) as i16 + dx;
                    let y = (anchor / width) as i16 + dy;
                    if (0..width as i16).contains(&x) && (0..13).contains(&y) {
                        expected[anchor / WORD_BITS] |= 1 << (anchor % WORD_BITS);
                    }
                }
                assert_eq!(actual, expected, "width {width}, offset ({dx},{dy})");
            }
        }
    }

    #[test]
    fn anchor_planes_refusal_is_not_an_empty_movement_answer() {
        let profile = builtin_kick_profile(KickTableProfileId::SrsPlus).unwrap();
        let piece = PieceKind::T;
        let template = ReachabilityTemplate::compile(10, 6, piece, profile.id());
        let ordered = OrderedKicks::from_profile(piece, profile, template.allow_180);
        assert!(search_reachable_locks(&template, piece, 1 << 63, &ordered).is_none());
        // A larger internally valid normalized domain remains the scalar
        // owner's responsibility; refusal does not synthesize a negative.
        let distant = [(0_i16, -512_i16)];
        let ordered = [distant.as_slice(); TRANSITIONS];
        let template = ReachabilityTemplate::compile_normalized(10, 6, piece, true, 518, ordered);
        assert!(
            search_reachable_locks(&template, piece, 0, &OrderedKicks::Normalized(ordered))
                .is_none()
        );
    }
}
