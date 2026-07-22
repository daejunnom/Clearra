use std::collections::{HashMap, HashSet};

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    catalog::SkeletonRow, extended_board::ExtendedBoard,
    extended_inverse_catalog::ExtendedSkeletonRow, geometry::TargetGroup, mix_digest, piece_index,
};

const MAX_REACHABLE_PROJECTIONS: usize = 262_144;
const MAX_ADAPTIVE_PROJECTION_COMBINATIONS: usize = 16_384;
const CHECKER_OFFSET: i8 = 32;
const MAX_EXACT_CHECKER_PIECES: u16 = 16;

#[derive(Clone, Debug)]
pub(super) struct ProjectionCatalog {
    width: u8,
    height: u8,
    bits_per_column: u8,
    column_value_mask: u64,
    piece_options: [Vec<u64>; 7],
    piece_minimum: [Vec<u8>; 7],
    piece_maximum: [Vec<u8>; 7],
    piece_checker_options: [u8; 7],
    standard_checker_rule_certified: bool,
    identity_digest: u64,
}

#[derive(Debug)]
enum ProjectionCacheEntry {
    Complete {
        signatures: Box<[u64]>,
        checker_domain: u128,
    },
    Unavailable,
}

#[derive(Debug, Default)]
pub(super) struct ProjectionReachabilityCache {
    entries: HashMap<u64, ProjectionCacheEntry>,
}

#[derive(Clone, Copy)]
struct ProjectedRow {
    piece: PieceKind,
    signature: u64,
    checker_delta: i8,
    columns: [u8; 10],
}

impl ProjectionCatalog {
    pub fn compile(width: u8, height: u8, rows: &[SkeletonRow]) -> Option<Self> {
        let mut projected_rows = Vec::new();
        projected_rows.try_reserve_exact(rows.len()).ok()?;
        for row in rows {
            let mut columns = [0_u8; 10];
            let mut checker_delta = 0_i8;
            let mut cells = row.cells;
            while cells != 0 {
                let cell = cells.trailing_zeros() as u8;
                cells &= cells - 1;
                let x = cell % width;
                let y = cell / width;
                columns[x as usize] += 1;
                checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
            }
            projected_rows.push(ProjectedRow {
                piece: row.piece,
                signature: pack_cells(width, projection_bits(height), row.cells),
                checker_delta,
                columns,
            });
        }
        Self::compile_projected(width, height, &projected_rows)
    }

    pub fn compile_extended(width: u8, height: u8, rows: &[ExtendedSkeletonRow]) -> Option<Self> {
        let mut projected_rows = Vec::new();
        projected_rows.try_reserve_exact(rows.len()).ok()?;
        for row in rows {
            let mut columns = [0_u8; 10];
            let mut checker_delta = 0_i8;
            for cell in row.cells.cells() {
                let x = (cell % u16::from(width)) as u8;
                let y = (cell / u16::from(width)) as u8;
                columns[x as usize] += 1;
                checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
            }
            projected_rows.push(ProjectedRow {
                piece: row.piece,
                signature: pack_extended_cells(width, projection_bits(height), row.cells),
                checker_delta,
                columns,
            });
        }
        Self::compile_projected(width, height, &projected_rows)
    }

    fn compile_projected(width: u8, height: u8, rows: &[ProjectedRow]) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let bits_per_column = projection_bits(height);
        if usize::from(width) * usize::from(bits_per_column) > u64::BITS as usize {
            return None;
        }
        let column_value_mask = (1_u64 << bits_per_column) - 1;
        let mut row_signatures = Vec::new();
        row_signatures.try_reserve_exact(rows.len()).ok()?;
        let mut piece_options: [Vec<u64>; 7] = core::array::from_fn(|_| Vec::new());
        let mut piece_minimum: [Vec<u8>; 7] =
            core::array::from_fn(|_| vec![u8::MAX; width as usize]);
        let mut piece_maximum: [Vec<u8>; 7] = core::array::from_fn(|_| vec![0; width as usize]);
        let mut piece_checker_options = [0_u8; 7];

        for row in rows {
            let signature = row.signature;
            row_signatures.push(row.signature);
            let piece = piece_index(row.piece);
            piece_options[piece].push(signature);
            for x in 0..width as usize {
                piece_minimum[piece][x] = piece_minimum[piece][x].min(row.columns[x]);
                piece_maximum[piece][x] = piece_maximum[piece][x].max(row.columns[x]);
            }
            let checker_index = row.checker_delta.div_euclid(2) + 2;
            if (0..5).contains(&checker_index) {
                piece_checker_options[piece] |= 1_u8 << checker_index;
            }
        }
        for options in &mut piece_options {
            options.sort_unstable();
            options.dedup();
        }
        for piece in 0..7 {
            for x in 0..width as usize {
                if piece_minimum[piece][x] == u8::MAX {
                    piece_minimum[piece][x] = 0;
                }
            }
        }

        let mut identity_digest = mix_digest(0, u64::from(width));
        identity_digest = mix_digest(identity_digest, u64::from(height));
        identity_digest = mix_digest(identity_digest, u64::from(bits_per_column));
        for (row_id, signature) in row_signatures.iter().copied().enumerate() {
            identity_digest = mix_digest(identity_digest, row_id as u64);
            identity_digest = mix_digest(identity_digest, signature);
        }
        for (piece, options) in piece_options.iter().enumerate() {
            identity_digest = mix_digest(identity_digest, piece as u64);
            identity_digest = mix_digest(identity_digest, u64::from(piece_checker_options[piece]));
            for option in options {
                identity_digest = mix_digest(identity_digest, *option);
            }
        }

        let standard_checker_rule_certified =
            piece_checker_options
                .iter()
                .enumerate()
                .all(|(piece, options)| {
                    if piece == 2 {
                        *options & !((1_u8 << 1) | (1_u8 << 3)) == 0
                    } else {
                        *options & !(1_u8 << 2) == 0
                    }
                });

        Some(Self {
            width,
            height,
            bits_per_column,
            column_value_mask,
            piece_options,
            piece_minimum,
            piece_maximum,
            piece_checker_options,
            standard_checker_rule_certified,
            identity_digest,
        })
    }

    pub fn demand_signature(&self, cells: u64) -> u64 {
        pack_cells(self.width, self.bits_per_column, cells)
    }

    pub fn checker_delta(&self, mut cells: u64) -> i8 {
        let mut delta = 0_i8;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let x = cell % self.width;
            let y = cell / self.width;
            delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
        }
        delta
    }

    pub fn extended_demand_signature(&self, cells: ExtendedBoard) -> u64 {
        pack_extended_cells(self.width, self.bits_per_column, cells)
    }

    pub fn extended_checker_delta(&self, cells: ExtendedBoard) -> i8 {
        let mut delta = 0_i8;
        for cell in cells.cells() {
            let x = (cell % u16::from(self.width)) as u8;
            let y = (cell / u16::from(self.width)) as u8;
            delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
        }
        delta
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    pub fn piece_column_bounds(&self, piece: usize, column: usize) -> (u8, u8) {
        (
            self.piece_minimum[piece][column],
            self.piece_maximum[piece][column],
        )
    }

    pub const fn standard_checker_rule_certified(&self) -> bool {
        self.standard_checker_rule_certified
    }

    pub fn retained_bytes(&self) -> usize {
        self.piece_options
            .iter()
            .map(|options| options.capacity() * core::mem::size_of::<u64>())
            .sum::<usize>()
            + self
                .piece_minimum
                .iter()
                .chain(&self.piece_maximum)
                .map(|values| values.capacity() * core::mem::size_of::<u8>())
                .sum::<usize>()
    }

    fn cheap_bounds_allow(&self, counts: [u8; 7], demand: u64) -> bool {
        for x in 0..self.width as usize {
            let requested =
                ((demand >> (x * self.bits_per_column as usize)) & self.column_value_mask) as u16;
            let mut minimum = 0_u16;
            let mut maximum = 0_u16;
            for piece in 0..7 {
                minimum += u16::from(counts[piece]) * u16::from(self.piece_minimum[piece][x]);
                maximum += u16::from(counts[piece]) * u16::from(self.piece_maximum[piece][x]);
            }
            if requested < minimum || requested > maximum {
                return false;
            }
        }
        true
    }

    fn add_projection(&self, left: u64, right: u64) -> Option<u64> {
        let mut result = 0_u64;
        for x in 0..self.width as usize {
            let shift = x * self.bits_per_column as usize;
            let value = ((left >> shift) & self.column_value_mask)
                + ((right >> shift) & self.column_value_mask);
            if value > u64::from(self.height) {
                return None;
            }
            result |= value << shift;
        }
        Some(result)
    }

    fn checker_domain(&self, counts: [u8; 7]) -> u128 {
        let mut domain = 1_u128 << CHECKER_OFFSET;
        for piece in 0..7 {
            for _ in 0..counts[piece] {
                let mut next = 0_u128;
                let options = self.piece_checker_options[piece];
                for option_index in 0..5_i8 {
                    if options & (1_u8 << option_index) == 0 {
                        continue;
                    }
                    let delta = option_index - 2;
                    next |= if delta >= 0 {
                        domain.checked_shl(delta as u32).unwrap_or(0)
                    } else {
                        domain.checked_shr((-delta) as u32).unwrap_or(0)
                    };
                }
                domain = next;
            }
        }
        domain
    }

    fn exact_projection_is_budgeted(&self, counts: [u8; 7]) -> bool {
        let mut combinations = 1_usize;
        for (piece, count) in counts.into_iter().enumerate() {
            for _ in 0..count {
                combinations = combinations.saturating_mul(self.piece_options[piece].len());
                if combinations > MAX_ADAPTIVE_PROJECTION_COMBINATIONS {
                    return false;
                }
            }
        }
        true
    }
}

impl ProjectionReachabilityCache {
    pub fn residual_impossible(
        &mut self,
        catalog: &ProjectionCatalog,
        targets: &[TargetGroup],
        used_counts: [u8; 7],
        remaining: u64,
        exact_projection_enabled: bool,
    ) -> bool {
        let demand = catalog.demand_signature(remaining);
        let checker_delta = catalog.checker_delta(remaining);
        if checker_delta % 2 != 0 {
            return true;
        }
        let checker_bit = i16::from(checker_delta.div_euclid(2) + CHECKER_OFFSET);
        if !(0..128).contains(&checker_bit) {
            return true;
        }

        let mut saw_target = false;
        for target in targets {
            let counts = target.key.counts();
            if !counts_dominate(counts, used_counts) {
                continue;
            }
            saw_target = true;
            let residual_counts = core::array::from_fn(|piece| counts[piece] - used_counts[piece]);
            if self.counts_may_match(
                catalog,
                residual_counts,
                demand,
                Some(checker_bit as u32),
                exact_projection_enabled,
            ) {
                return false;
            }
        }
        saw_target
    }

    pub fn extended_residual_impossible(
        &mut self,
        catalog: &ProjectionCatalog,
        targets: &[[u8; 7]],
        used_counts: [u8; 7],
        remaining: ExtendedBoard,
        exact_projection_enabled: bool,
    ) -> bool {
        let demand = catalog.extended_demand_signature(remaining);
        let checker_delta = catalog.extended_checker_delta(remaining);
        if checker_delta % 2 != 0 {
            return true;
        }
        let checker_bit = i16::from(checker_delta.div_euclid(2) + CHECKER_OFFSET);
        let checker_bit = (0..128)
            .contains(&checker_bit)
            .then_some(checker_bit as u32);
        let mut saw_target = false;
        for counts in targets.iter().copied() {
            if !counts_dominate(counts, used_counts) {
                continue;
            }
            saw_target = true;
            let residual_counts = core::array::from_fn(|piece| counts[piece] - used_counts[piece]);
            // The u128 checker domain is centered at 32. Up to 16 tetrominoes,
            // every possible intermediate +/-2 transfer remains representable.
            // Larger residuals still use exact column projection, but checker
            // parity cannot safely authorize a prune because a valid sum may
            // leave this window and return later.
            let exact_checker_domain = residual_counts
                .iter()
                .map(|count| u16::from(*count))
                .sum::<u16>()
                <= MAX_EXACT_CHECKER_PIECES;
            if self.counts_may_match(
                catalog,
                residual_counts,
                demand,
                checker_bit.filter(|_| exact_checker_domain),
                exact_projection_enabled,
            ) {
                return false;
            }
        }
        saw_target
    }

    pub fn retained_bytes(&self) -> usize {
        self.entries.capacity()
            * (core::mem::size_of::<u64>() + core::mem::size_of::<ProjectionCacheEntry>())
            + self
                .entries
                .values()
                .map(|entry| match entry {
                    ProjectionCacheEntry::Complete { signatures, .. } => {
                        signatures.len() * core::mem::size_of::<u64>()
                    }
                    ProjectionCacheEntry::Unavailable => 0,
                })
                .sum::<usize>()
    }

    fn entry<'a>(
        &'a mut self,
        catalog: &ProjectionCatalog,
        counts: [u8; 7],
    ) -> &'a ProjectionCacheEntry {
        let key = pack_projection_counts(counts);
        self.entries
            .entry(key)
            .or_insert_with(|| compile_reachable_projections(catalog, counts))
    }

    fn counts_may_match(
        &mut self,
        catalog: &ProjectionCatalog,
        counts: [u8; 7],
        demand: u64,
        checker_bit: Option<u32>,
        exact_projection_enabled: bool,
    ) -> bool {
        if !catalog.cheap_bounds_allow(counts, demand) {
            return false;
        }
        if !exact_projection_enabled || !catalog.exact_projection_is_budgeted(counts) {
            return true;
        }
        match self.entry(catalog, counts) {
            ProjectionCacheEntry::Complete {
                signatures,
                checker_domain,
            } => {
                checker_bit.is_none_or(|bit| checker_domain & (1_u128 << bit) != 0)
                    && signatures.binary_search(&demand).is_ok()
            }
            ProjectionCacheEntry::Unavailable => true,
        }
    }
}

fn compile_reachable_projections(
    catalog: &ProjectionCatalog,
    counts: [u8; 7],
) -> ProjectionCacheEntry {
    let checker_domain = catalog.checker_domain(counts);
    let mut current = HashSet::new();
    if current.try_reserve(1).is_err() {
        return ProjectionCacheEntry::Unavailable;
    }
    current.insert(0_u64);
    for piece in 0..7 {
        if counts[piece] != 0 && catalog.piece_options[piece].is_empty() {
            return ProjectionCacheEntry::Complete {
                signatures: Box::new([]),
                checker_domain: 0,
            };
        }
        for _ in 0..counts[piece] {
            let mut next = HashSet::new();
            let reserve = current
                .len()
                .saturating_mul(catalog.piece_options[piece].len())
                .min(MAX_REACHABLE_PROJECTIONS);
            if next.try_reserve(reserve).is_err() {
                return ProjectionCacheEntry::Unavailable;
            }
            for left in current.iter().copied() {
                for right in catalog.piece_options[piece].iter().copied() {
                    let Some(signature) = catalog.add_projection(left, right) else {
                        continue;
                    };
                    next.insert(signature);
                    if next.len() > MAX_REACHABLE_PROJECTIONS {
                        return ProjectionCacheEntry::Unavailable;
                    }
                }
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }
    }
    let mut signatures = current.into_iter().collect::<Vec<_>>();
    signatures.sort_unstable();
    ProjectionCacheEntry::Complete {
        signatures: signatures.into_boxed_slice(),
        checker_domain,
    }
}

fn pack_cells(width: u8, bits_per_column: u8, mut cells: u64) -> u64 {
    let mut signature = 0_u64;
    while cells != 0 {
        let cell = cells.trailing_zeros() as u8;
        cells &= cells - 1;
        let shift = usize::from(cell % width) * usize::from(bits_per_column);
        signature += 1_u64 << shift;
    }
    signature
}

fn pack_extended_cells(width: u8, bits_per_column: u8, cells: ExtendedBoard) -> u64 {
    let mut signature = 0_u64;
    for cell in cells.cells() {
        let shift = usize::from(cell % u16::from(width)) * usize::from(bits_per_column);
        signature += 1_u64 << shift;
    }
    signature
}

const fn projection_bits(height: u8) -> u8 {
    (u8::BITS - height.leading_zeros()) as u8
}

fn pack_projection_counts(counts: [u8; 7]) -> u64 {
    counts
        .into_iter()
        .enumerate()
        .fold(0_u64, |packed, (piece, count)| {
            packed | (u64::from(count) << (piece * 8))
        })
}

fn counts_dominate(counts: [u8; 7], used_counts: [u8; 7]) -> bool {
    (0..7).all(|piece| counts[piece] >= used_counts[piece])
}
