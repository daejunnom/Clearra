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
    piece_column_mod_four_options: [u8; 7],
    // Serialized into the GPU constraint catalog only when that backend is built.
    #[cfg_attr(not(feature = "webgpu-search"), allow(dead_code))]
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
    column_mod_four_residue: u8,
    columns: [u8; 10],
}

#[derive(Clone, Copy)]
struct ResidualProjection {
    signature: u64,
    checker_delta: i8,
    column_mod_four_residue: u8,
}

impl ProjectionCatalog {
    pub fn compile(width: u8, height: u8, rows: &[SkeletonRow]) -> Option<Self> {
        let mut projected_rows = Vec::new();
        projected_rows.try_reserve_exact(rows.len()).ok()?;
        for row in rows {
            let mut columns = [0_u8; 10];
            let mut checker_delta = 0_i8;
            let mut column_mod_four_residue = 0_u8;
            let mut cells = row.cells;
            while cells != 0 {
                let cell = cells.trailing_zeros() as u8;
                cells &= cells - 1;
                let x = cell % width;
                let y = cell / width;
                columns[x as usize] += 1;
                checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
                column_mod_four_residue = (column_mod_four_residue + x) & 3;
            }
            projected_rows.push(ProjectedRow {
                piece: row.piece,
                signature: pack_cells(width, projection_bits(height), row.cells),
                checker_delta,
                column_mod_four_residue,
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
            let mut column_mod_four_residue = 0_u8;
            for cell in row.cells.cells() {
                let x = (cell % u16::from(width)) as u8;
                let y = (cell / u16::from(width)) as u8;
                columns[x as usize] += 1;
                checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
                column_mod_four_residue = (column_mod_four_residue + x) & 3;
            }
            projected_rows.push(ProjectedRow {
                piece: row.piece,
                signature: pack_extended_cells(width, projection_bits(height), row.cells),
                checker_delta,
                column_mod_four_residue,
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
        let mut piece_option_counts = [0_usize; 7];
        for row in rows {
            piece_option_counts[piece_index(row.piece)] =
                piece_option_counts[piece_index(row.piece)].checked_add(1)?;
        }
        for (options, count) in piece_options.iter_mut().zip(piece_option_counts) {
            options.try_reserve_exact(count).ok()?;
        }
        let mut piece_minimum: [Vec<u8>; 7] =
            core::array::from_fn(|_| vec![u8::MAX; width as usize]);
        let mut piece_maximum: [Vec<u8>; 7] = core::array::from_fn(|_| vec![0; width as usize]);
        let mut piece_checker_options = [0_u8; 7];
        let mut piece_column_mod_four_options = [0_u8; 7];

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
            piece_column_mod_four_options[piece] |= 1_u8 << row.column_mod_four_residue;
        }
        for options in &mut piece_options {
            options.sort_unstable();
            options.dedup();
        }
        for minimums in &mut piece_minimum {
            for minimum in minimums {
                if *minimum == u8::MAX {
                    *minimum = 0;
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
            piece_column_mod_four_options,
            standard_checker_rule_certified,
            identity_digest,
        })
    }

    fn project_residual(&self, mut cells: u64) -> ResidualProjection {
        let mut signature = 0_u64;
        let mut checker_delta = 0_i8;
        let mut column_mod_four_residue = 0_u8;
        while cells != 0 {
            let cell = cells.trailing_zeros() as u8;
            cells &= cells - 1;
            let x = cell % self.width;
            let y = cell / self.width;
            signature += 1_u64 << (usize::from(x) * usize::from(self.bits_per_column));
            checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
            column_mod_four_residue = (column_mod_four_residue + x) & 3;
        }
        ResidualProjection {
            signature,
            checker_delta,
            column_mod_four_residue,
        }
    }

    fn project_extended_residual(&self, cells: ExtendedBoard) -> ResidualProjection {
        let mut signature = 0_u64;
        let mut checker_delta = 0_i8;
        let mut column_mod_four_residue = 0_u8;
        for cell in cells.cells() {
            let x = (cell % u16::from(self.width)) as u8;
            let y = (cell / u16::from(self.width)) as u8;
            signature += 1_u64 << (usize::from(x) * usize::from(self.bits_per_column));
            checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
            column_mod_four_residue = (column_mod_four_residue + x) & 3;
        }
        ResidualProjection {
            signature,
            checker_delta,
            column_mod_four_residue,
        }
    }

    pub const fn identity_digest(&self) -> u64 {
        self.identity_digest
    }

    // The GPU catalog owns the packed per-piece column bounds consumer.
    #[cfg_attr(not(feature = "webgpu-search"), allow(dead_code))]
    pub fn piece_column_bounds(&self, piece: usize, column: usize) -> (u8, u8) {
        (
            self.piece_minimum[piece][column],
            self.piece_maximum[piece][column],
        )
    }

    // The GPU catalog owns the checker-rule certification consumer.
    #[cfg_attr(not(feature = "webgpu-search"), allow(dead_code))]
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

    /// Conservative peak while the projected-row scratch, signature scratch,
    /// and retained per-piece option arrays coexist. All counts are checked;
    /// the bound intentionally includes pre-dedup option slots.
    pub fn checked_compile_peak_upper_bound(row_count: usize, width: u8) -> Option<u128> {
        let row_count = row_count as u128;
        row_count
            .checked_mul(core::mem::size_of::<ProjectedRow>() as u128)?
            .checked_add(row_count.checked_mul(core::mem::size_of::<u64>() as u128)?)?
            .checked_add(row_count.checked_mul(core::mem::size_of::<u64>() as u128)?)?
            .checked_add(
                u128::from(width)
                    .checked_mul(14)?
                    .checked_mul(core::mem::size_of::<u8>() as u128)?,
            )
    }

    fn cheap_bounds_allow(&self, counts: [u8; 7], demand: u64) -> bool {
        for x in 0..self.width as usize {
            let requested =
                ((demand >> (x * self.bits_per_column as usize)) & self.column_value_mask) as u16;
            let mut minimum = 0_u16;
            let mut maximum = 0_u16;
            for (piece, count) in counts.iter().copied().enumerate() {
                minimum += u16::from(count) * u16::from(self.piece_minimum[piece][x]);
                maximum += u16::from(count) * u16::from(self.piece_maximum[piece][x]);
            }
            if requested < minimum || requested > maximum {
                return false;
            }
        }
        true
    }

    /// Returns every reachable value of `sum(x) mod 4` for the remaining
    /// piece multiset. On a tetromino-aligned residual, its low bit is exactly
    /// the vertical-parity projection, so that rule is intentionally not
    /// computed a second time.
    fn column_mod_four_domain(&self, counts: [u8; 7]) -> u8 {
        let mut domain = 1_u8;
        for (piece, count) in counts.into_iter().enumerate() {
            for _ in 0..count {
                domain = convolve_mod_four(domain, self.piece_column_mod_four_options[piece]);
                if domain == 0 {
                    return 0;
                }
                if domain == 0b1111 {
                    return domain;
                }
            }
        }
        domain
    }

    fn cheap_counts_may_match(&self, counts: [u8; 7], demand: ResidualProjection) -> bool {
        if self.column_mod_four_domain(counts) & (1_u8 << demand.column_mod_four_residue) == 0 {
            return false;
        }
        self.cheap_bounds_allow(counts, demand.signature)
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
        for (piece, count) in counts.into_iter().enumerate() {
            for _ in 0..count {
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
    /// Cheap, allocation-free negative filter shared by ordinary and extended
    /// ILC geometry. It uses the residual target-frame cells and only residue
    /// options compiled from rows that the active catalog can actually emit.
    /// Any valid completion partitions `remaining` into these catalog rows,
    /// and `sum(x) mod 4` is additive across that partition. Consequently line
    /// count, an existing field, kick rules, and temporal skeleton
    /// normalization are already represented by the caller/catalog; the
    /// filter never assumes a blank even-line opening.
    pub fn cheap_residual_impossible(
        catalog: &ProjectionCatalog,
        targets: &[TargetGroup],
        used_counts: [u8; 7],
        remaining: u64,
    ) -> bool {
        let demand = catalog.project_residual(remaining);
        if demand.checker_delta % 2 != 0 {
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
            if catalog.cheap_counts_may_match(residual_counts, demand) {
                return false;
            }
        }
        saw_target
    }

    pub fn extended_cheap_residual_impossible(
        catalog: &ProjectionCatalog,
        targets: &[[u8; 7]],
        used_counts: [u8; 7],
        remaining: ExtendedBoard,
    ) -> bool {
        let demand = catalog.project_extended_residual(remaining);
        if demand.checker_delta % 2 != 0 {
            return true;
        }
        let mut saw_target = false;
        for counts in targets.iter().copied() {
            if !counts_dominate(counts, used_counts) {
                continue;
            }
            saw_target = true;
            let residual_counts = core::array::from_fn(|piece| counts[piece] - used_counts[piece]);
            if catalog.cheap_counts_may_match(residual_counts, demand) {
                return false;
            }
        }
        saw_target
    }

    /// Exact column/checker refinement. Callers run the cheap residue filter
    /// first; this method is reserved for nodes admitted to the bounded exact
    /// projection cache.
    pub fn exact_residual_impossible(
        &mut self,
        catalog: &ProjectionCatalog,
        targets: &[TargetGroup],
        used_counts: [u8; 7],
        remaining: u64,
    ) -> bool {
        let demand = catalog.project_residual(remaining);
        let checker_delta = demand.checker_delta;
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
                demand.signature,
                Some(checker_bit as u32),
            ) {
                return false;
            }
        }
        saw_target
    }

    pub fn extended_exact_residual_impossible(
        &mut self,
        catalog: &ProjectionCatalog,
        targets: &[[u8; 7]],
        used_counts: [u8; 7],
        remaining: ExtendedBoard,
    ) -> bool {
        let demand = catalog.project_extended_residual(remaining);
        let checker_delta = demand.checker_delta;
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
                demand.signature,
                checker_bit.filter(|_| exact_checker_domain),
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
    ) -> bool {
        if !catalog.exact_projection_is_budgeted(counts) {
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

fn convolve_mod_four(domain: u8, options: u8) -> u8 {
    let domain = domain & 0b1111;
    (if options & 0b0001 != 0 { domain } else { 0 })
        | (if options & 0b0010 != 0 {
            ((domain << 1) | (domain >> 3)) & 0b1111
        } else {
            0
        })
        | (if options & 0b0100 != 0 {
            ((domain << 2) | (domain >> 2)) & 0b1111
        } else {
            0
        })
        | (if options & 0b1000 != 0 {
            ((domain << 3) | (domain >> 1)) & 0b1111
        } else {
            0
        })
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
    for (piece, count) in counts.into_iter().enumerate() {
        if count != 0 && catalog.piece_options[piece].is_empty() {
            return ProjectionCacheEntry::Complete {
                signatures: Box::new([]),
                checker_domain: 0,
            };
        }
        for _ in 0..count {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn projected_row(piece: PieceKind, cells: &[(u8, u8)]) -> ProjectedRow {
        let mut columns = [0_u8; 10];
        let mut checker_delta = 0_i8;
        let mut column_mod_four_residue = 0_u8;
        let mut mask = 0_u64;
        for &(x, y) in cells {
            columns[usize::from(x)] += 1;
            checker_delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
            column_mod_four_residue = (column_mod_four_residue + x) & 3;
            mask |= 1_u64 << (u64::from(y) * 10 + u64::from(x));
        }
        ProjectedRow {
            piece,
            signature: pack_cells(10, projection_bits(6), mask),
            checker_delta,
            column_mod_four_residue,
            columns,
        }
    }

    fn counts(piece: PieceKind, count: u8) -> [u8; 7] {
        let mut counts = [0_u8; 7];
        counts[piece_index(piece)] = count;
        counts
    }

    fn projection_from_signature(
        catalog: &ProjectionCatalog,
        signature: u64,
    ) -> ResidualProjection {
        let mut column_mod_four_residue = 0_u8;
        for x in 0..usize::from(catalog.width) {
            let count = ((signature >> (x * usize::from(catalog.bits_per_column)))
                & catalog.column_value_mask) as u8;
            column_mod_four_residue = (column_mod_four_residue + ((x as u8 & 3) * (count & 3))) & 3;
        }
        ResidualProjection {
            signature,
            checker_delta: 0,
            column_mod_four_residue,
        }
    }

    fn legacy_identity_digest(width: u8, height: u8, rows: &[ProjectedRow]) -> u64 {
        let bits_per_column = projection_bits(height);
        let mut piece_options: [Vec<u64>; 7] = core::array::from_fn(|_| Vec::new());
        let mut piece_checker_options = [0_u8; 7];
        for row in rows {
            let piece = piece_index(row.piece);
            piece_options[piece].push(row.signature);
            let checker_index = row.checker_delta.div_euclid(2) + 2;
            if (0..5).contains(&checker_index) {
                piece_checker_options[piece] |= 1_u8 << checker_index;
            }
        }
        for options in &mut piece_options {
            options.sort_unstable();
            options.dedup();
        }

        let mut digest = mix_digest(0, u64::from(width));
        digest = mix_digest(digest, u64::from(height));
        digest = mix_digest(digest, u64::from(bits_per_column));
        for (row_id, row) in rows.iter().enumerate() {
            digest = mix_digest(digest, row_id as u64);
            digest = mix_digest(digest, row.signature);
        }
        for (piece, options) in piece_options.iter().enumerate() {
            digest = mix_digest(digest, piece as u64);
            digest = mix_digest(digest, u64::from(piece_checker_options[piece]));
            for option in options {
                digest = mix_digest(digest, *option);
            }
        }
        digest
    }

    #[test]
    fn column_mod_four_rejects_a_residual_that_column_bounds_cannot_distinguish() {
        let rows = [
            projected_row(PieceKind::I, &[(0, 0), (0, 1), (2, 0), (2, 1)]),
            projected_row(PieceKind::I, &[(1, 0), (1, 1), (3, 0), (3, 1)]),
        ];
        let catalog = ProjectionCatalog::compile_projected(10, 6, &rows).expect("catalog");
        let demand_cells = 0b1111_u64;
        let demand = catalog.project_residual(demand_cells);

        assert!(catalog.cheap_bounds_allow(counts(PieceKind::I, 1), demand.signature));
        assert_eq!(
            catalog.column_mod_four_domain(counts(PieceKind::I, 1)),
            0b0001
        );
        assert!(!catalog.cheap_counts_may_match(counts(PieceKind::I, 1), demand));
    }

    #[test]
    fn derived_column_mod_four_options_preserve_the_legacy_catalog_identity() {
        let rows = [
            projected_row(PieceKind::I, &[(0, 0), (1, 0), (2, 0), (3, 0)]),
            projected_row(PieceKind::T, &[(4, 0), (5, 0), (6, 0), (5, 1)]),
            projected_row(PieceKind::O, &[(7, 0), (8, 0), (7, 1), (8, 1)]),
        ];
        let catalog = ProjectionCatalog::compile_projected(10, 6, &rows).expect("catalog");

        assert_eq!(
            catalog.identity_digest(),
            legacy_identity_digest(10, 6, &rows)
        );
    }

    #[test]
    fn optimized_mod_four_convolution_matches_the_exhaustive_definition() {
        for domain in 0..16_u8 {
            for options in 0..16_u8 {
                let mut expected = 0_u8;
                for left in 0..4_u8 {
                    for right in 0..4_u8 {
                        if domain & (1_u8 << left) != 0 && options & (1_u8 << right) != 0 {
                            expected |= 1_u8 << ((left + right) & 3);
                        }
                    }
                }
                assert_eq!(convolve_mod_four(domain, options), expected);
            }
        }
    }

    #[test]
    fn mod_four_domain_also_enforces_vertical_parity_without_a_second_rule() {
        let rows = [
            projected_row(PieceKind::I, &[(0, 0), (0, 1), (0, 2), (0, 3)]),
            projected_row(PieceKind::I, &[(0, 0), (1, 0), (2, 0), (3, 0)]),
        ];
        let catalog = ProjectionCatalog::compile_projected(10, 6, &rows).expect("catalog");
        let domain = catalog.column_mod_four_domain(counts(PieceKind::I, 1));

        assert_eq!(domain, 0b0101);
        assert_eq!(
            domain & 0b1010,
            0,
            "odd vertical-parity residues stay impossible"
        );
    }

    #[test]
    fn every_catalog_row_combination_remains_admissible() {
        let rows = [
            projected_row(PieceKind::I, &[(0, 0), (1, 0), (2, 0), (3, 0)]),
            projected_row(PieceKind::I, &[(4, 0), (4, 1), (4, 2), (4, 3)]),
            projected_row(PieceKind::T, &[(5, 0), (6, 0), (7, 0), (6, 1)]),
            projected_row(PieceKind::T, &[(8, 0), (8, 1), (8, 2), (9, 1)]),
        ];
        let catalog = ProjectionCatalog::compile_projected(10, 6, &rows).expect("catalog");
        let mut residual_counts = counts(PieceKind::I, 1);
        residual_counts[piece_index(PieceKind::T)] = 1;

        for i_row in &rows[..2] {
            for t_row in &rows[2..] {
                let demand = catalog
                    .add_projection(i_row.signature, t_row.signature)
                    .expect("rows fit height");
                let demand = projection_from_signature(&catalog, demand);
                assert!(
                    catalog.cheap_counts_may_match(residual_counts, demand),
                    "an exact catalog-row sum must never be rejected"
                );
            }
        }
    }

    #[test]
    fn normalized_residuals_cover_one_through_six_line_targets() {
        let mut rows = Vec::new();
        for y in [0_u8, 2, 4] {
            for x in [0_u8, 2, 4, 6, 8] {
                rows.push(projected_row(
                    PieceKind::O,
                    &[(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)],
                ));
            }
        }
        let catalog = ProjectionCatalog::compile_projected(10, 6, &rows).expect("catalog");

        for lines in 1..=6_u8 {
            // A 10-wide target can have two pre-filled cells on odd line counts.
            // The projection sees only the normalized residual required cells.
            let piece_count = (10 * lines) / 4;
            let mut demand = 0_u64;
            for index in 0..piece_count {
                demand = catalog
                    .add_projection(demand, rows[usize::from(index)].signature)
                    .expect("selected rows fit the six-line projection");
            }
            let demand = projection_from_signature(&catalog, demand);
            assert!(
                catalog.cheap_counts_may_match(counts(PieceKind::O, piece_count), demand),
                "{lines}L normalized residual was rejected"
            );
        }
    }
}
