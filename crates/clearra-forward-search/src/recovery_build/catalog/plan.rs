//! Complete logical tilings, not representative order histories. Enumeration is
//! queue-independent; the exact verifier owns hold, kicks, timing and B2B.
use super::*;
use crate::{
    board::ForwardBoard,
    recovery_build::staged::geometry::{Position, Stage},
};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(in crate::recovery_build) struct Tile {
    pub piece: u8,
    pub cells: [u64; 4],
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::recovery_build) struct Plan {
    pub orientation: u32,
    pub middle: Vec<Tile>,
    pub result: Vec<Tile>,
}
impl Plan {
    pub fn counts(tiles: &[Tile]) -> [u8; 7] {
        let mut counts = [0; 7];
        for tile in tiles {
            counts[usize::from(tile.piece)] += 1;
        }
        counts
    }
    pub fn allows(
        &self,
        g: &Geometry,
        from: u32,
        edge: crate::recovery_build::staged::geometry::Edge,
        piece: u8,
    ) -> bool {
        let a = g.position(from);
        let b = g.position(edge.next);
        let cells = if edge.result {
            b.result.without(a.result)
        } else {
            b.middle.without(a.middle)
        };
        let tiles = if edge.result {
            &self.result
        } else {
            &self.middle
        };
        tiles
            .binary_search(&Tile {
                piece,
                cells: cells.words(),
            })
            .is_ok()
    }
    pub fn key(&self) -> String {
        let mut result = format!("recovery-tiling.v1:{}", self.orientation);
        for (role, tiles) in [('m', &self.middle), ('r', &self.result)] {
            for t in tiles {
                use std::fmt::Write;
                let _ = write!(
                    result,
                    "|{}{}:{:016x}{:016x}{:016x}{:016x}",
                    role, t.piece, t.cells[3], t.cells[2], t.cells[1], t.cells[0]
                );
            }
        }
        result
    }
    pub fn validate(&self, g: &Geometry) -> Result<(), Error> {
        let stage = g
            .stages
            .get(self.orientation as usize)
            .ok_or(Error::PatternDomainUnavailable)?;
        for tile in &self.middle {
            if !stage.chain_targets.is_empty()
                && !stage.chain_targets[..stage.chain_targets.len() - 1]
                    .iter()
                    .any(|target| Mask::from_words(tile.cells).without(*target).is_empty())
            {
                return Err(Error::PatternDomainUnavailable);
            }
        }
        for (tiles, target) in [
            (&self.middle, stage.fields.middle),
            (&self.result, stage.fields.result),
        ] {
            if tiles.len() != target.count_ones() as usize / 4
                || !tiles.windows(2).all(|w| w[0] < w[1])
            {
                return Err(Error::PatternDomainUnavailable);
            }
            let mut used = Mask::EMPTY;
            for t in tiles {
                let mask = Mask::from_words(t.cells);
                if t.piece >= 7
                    || mask.count_ones() != 4
                    || used.intersects(mask)
                    || mask.without(target) != Mask::EMPTY
                {
                    return Err(Error::PatternDomainUnavailable);
                }
                used = used.union(mask);
            }
            if used != target {
                return Err(Error::PatternDomainUnavailable);
            }
        }
        Ok(())
    }
}

/// Queue-independent facts derived from this exact tiling and orientation.
/// Absence of a structural obstruction never admits a move: reachability,
/// hold, bag ownership, stage exchange and spin still use the existing solver.
pub(in crate::recovery_build) struct AnalyzedPlan {
    pub tiling: Plan,
    middle_counts: [u8; 7],
    result_counts: [u8; 7],
    row_dependencies: Option<RowClearDependencies>,
}

impl AnalyzedPlan {
    pub fn new(plan: Plan, geometry: &Geometry) -> Self {
        let row_dependencies = geometry
            .stages
            .get(plan.orientation as usize)
            .and_then(|stage| RowClearDependencies::compile(stage, &plan));
        Self {
            middle_counts: Plan::counts(&plan.middle),
            result_counts: Plan::counts(&plan.result),
            tiling: plan,
            row_dependencies,
        }
    }

    pub fn inventory_allows(
        &self,
        used_middle: [u8; 7],
        used_result: [u8; 7],
        middle_caps: [u8; 7],
        result_caps: [u8; 7],
    ) -> bool {
        (0..7).all(|p| {
            self.middle_counts[p]
                .checked_sub(used_middle[p])
                .is_some_and(|n| n <= middle_caps[p])
                && self.result_counts[p]
                    .checked_sub(used_result[p])
                    .is_some_and(|n| n <= result_caps[p])
        })
    }

    pub fn cannot_complete(&self) -> bool {
        self.row_dependencies
            .as_ref()
            .is_some_and(|dependencies| dependencies.blocked)
    }

    pub fn permits_piece(&self, position: &Position, piece: u8, permit_result: bool) -> bool {
        self.row_dependencies
            .as_ref()
            .is_none_or(|dependencies| dependencies.permits_piece(position, piece, permit_result))
    }
}

struct TileRows {
    cells: Mask,
    occupied: u64,
    gap: u64,
    result: bool,
}

struct RowClearDependencies {
    tiles: Vec<TileRows>,
    by_piece: [u128; 7],
    blocked: bool,
}

impl RowClearDependencies {
    fn compile(stage: &Stage, plan: &Plan) -> Option<Self> {
        let count = plan.middle.len().checked_add(plan.result.len())?;
        if count > u128::BITS as usize || stage.prepared.middle.len() > u64::BITS as usize {
            return None;
        }
        let mut tiles = Vec::new();
        tiles.try_reserve_exact(count).ok()?;
        let mut required = 0u64;
        let mut by_piece = [0u128; 7];
        for (result, source) in [(false, &plan.middle), (true, &plan.result)] {
            for tile in source {
                let cells = Mask::from_words(tile.cells);
                let rows =
                    logical_rows(cells, result.then_some(stage.result_to_logical.as_slice()))?;
                if rows == 0 {
                    return None;
                }
                // A connected physical tetromino touches every row between
                // its low and high cells. Lifting it through the ordered
                // undeleted-row map can leave a gap only at a deleted row.
                let span = (u64::MAX << rows.trailing_zeros()) & (u64::MAX >> rows.leading_zeros());
                let gap = span & !rows;
                required |= gap;
                *by_piece.get_mut(usize::from(tile.piece))? |= 1u128 << tiles.len();
                tiles.push(TileRows {
                    cells,
                    occupied: rows,
                    gap,
                    result,
                });
            }
        }
        if required == 0 {
            return None;
        }
        let initial = ForwardBoard::from_mask(stage.fields.initial);
        let initially_deleted = stage
            .prepared
            .initially_deleted
            .iter()
            .enumerate()
            .fold(0u64, |rows, (y, &deleted)| rows | (u64::from(deleted) << y));
        // Work in the original logical frame, not the compacted start board.
        // No legal path can add cells outside this union.
        let mut uncovered = required & !initially_deleted;
        let mut blocked = false;
        while uncovered != 0 {
            let y = uncovered.trailing_zeros() as usize;
            uncovered &= uncovered - 1;
            let start = if y < usize::from(stage.fields.height) {
                initial.row_bits(10, y as u8)
            } else {
                0
            };
            if start | *stage.prepared.middle.get(y)? | *stage.prepared.result.get(y)? != 1023 {
                blocked = true;
                break;
            }
        }
        if !blocked {
            blocked = has_required_clear_cycle(&tiles, initially_deleted);
        }
        Some(Self {
            tiles,
            by_piece,
            blocked,
        })
    }

    fn permits_piece(&self, position: &Position, piece: u8, permit_result: bool) -> bool {
        let mut candidates = self.by_piece[usize::from(piece)];
        while candidates != 0 {
            let tile = &self.tiles[candidates.trailing_zeros() as usize];
            candidates &= candidates - 1;
            let used = if tile.result {
                position.result
            } else {
                position.middle
            };
            if (!tile.result || permit_result)
                && !tile.cells.intersects(used)
                && tile.occupied & position.deleted == 0
                && tile.gap & !position.deleted == 0
            {
                return true;
            }
        }
        false
    }
}

fn logical_rows(cells: Mask, result_to_logical: Option<&[u8]>) -> Option<u64> {
    let mut rows = 0u64;
    for (word, mut bits) in cells.words().into_iter().enumerate() {
        while bits != 0 {
            let row = (word * u64::BITS as usize + bits.trailing_zeros() as usize) / 10;
            bits &= bits - 1;
            let logical = match result_to_logical {
                Some(map) => usize::from(*map.get(row)?),
                None => row,
            };
            rows |= 1u64.checked_shl(logical as u32)?;
        }
    }
    Some(rows)
}

fn has_required_clear_cycle(tiles: &[TileRows], initially_deleted: u64) -> bool {
    if tiles.is_empty() {
        return false;
    }
    let mut owners = [0u128; u64::BITS as usize];
    for (id, tile) in tiles.iter().enumerate() {
        let mut rows = tile.occupied;
        while rows != 0 {
            owners[rows.trailing_zeros() as usize] |= 1u128 << id;
            rows &= rows - 1;
        }
    }
    // Every owner of a required clear row must place before the dependent
    // lock. This is an AND edge, independent of how pieces arrive or are held.
    let mut predecessors = [0u128; u128::BITS as usize];
    for (id, tile) in tiles.iter().enumerate() {
        let mut rows = tile.gap & !initially_deleted;
        while rows != 0 {
            predecessors[id] |= owners[rows.trailing_zeros() as usize];
            rows &= rows - 1;
        }
    }
    let mut remaining = u128::MAX >> (u128::BITS as usize - tiles.len());
    while remaining != 0 {
        let before = remaining;
        let mut scan = remaining;
        while scan != 0 {
            let bit = scan & scan.wrapping_neg();
            scan ^= bit;
            if predecessors[bit.trailing_zeros() as usize] & remaining == 0 {
                remaining ^= bit;
            }
        }
        if remaining == before {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[test]
fn opposing_row_clear_requirements_form_a_cycle_but_one_way_does_not() {
    let mut tiles = [
        TileRows {
            cells: Mask::EMPTY,
            occupied: 0b0_1101,
            gap: 0b0_0010,
            result: false,
        },
        TileRows {
            cells: Mask::EMPTY,
            occupied: 0b1_0110,
            gap: 0b0_1000,
            result: true,
        },
    ];
    assert!(has_required_clear_cycle(&tiles, 0));
    tiles[1].occupied = 0b0_1110;
    tiles[1].gap = 0;
    assert!(!has_required_clear_cycle(&tiles, 0));
}

fn domain_error(error: BuildStageDomainError) -> Error {
    match error {
        BuildStageDomainError::Cancelled => Error::Cancelled,
        BuildStageDomainError::Allocation => Error::MemoryUnavailable,
        BuildStageDomainError::InvalidField => Error::PatternDomainUnavailable,
    }
}
struct TileFrame {
    caps: [u8; 7],
    remaining: Mask,
    choices: std::vec::IntoIter<(u8, Mask)>,
}
struct Tiles {
    pending: Option<(Mask, [u8; 7])>,
    frames: Vec<TileFrame>,
    path: Vec<Tile>,
}
enum TileAdvance {
    Pending,
    Done,
    Found(Vec<Tile>),
}
impl Tiles {
    fn new(target: Mask, caps: [u8; 7]) -> Self {
        Self {
            pending: Some((target, caps)),
            frames: Vec::new(),
            path: Vec::new(),
        }
    }
    fn advance(
        &mut self,
        domain: &mut BuildStageDomain,
        owners: &[Mask],
        control: &ExecutionControl,
    ) -> Result<TileAdvance, Error> {
        cancelled(control)?;
        if let Some((remaining, caps)) = self.pending.take() {
            if remaining.is_empty() {
                let mut tiles = self.path.clone();
                tiles.sort_unstable();
                return Ok(TileAdvance::Found(tiles));
            }
            if domain
                .can_complete(remaining, caps, control)
                .map_err(domain_error)?
            {
                let choices = domain
                    .completion_choices(remaining, caps, control)
                    .map_err(domain_error)?
                    .into_iter();
                self.frames
                    .try_reserve(1)
                    .map_err(|_| Error::MemoryUnavailable)?;
                self.frames.push(TileFrame {
                    caps,
                    remaining,
                    choices,
                });
            }
            return Ok(TileAdvance::Pending);
        }
        let depth = self.frames.len();
        let Some(frame) = self.frames.last_mut() else {
            return Ok(TileAdvance::Done);
        };
        self.path.truncate(depth - 1);
        if let Some((piece, cells)) = frame.choices.next() {
            if !owners.is_empty() && !owners.iter().any(|owner| cells.without(*owner).is_empty()) {
                return Ok(TileAdvance::Pending);
            }
            let mut caps = frame.caps;
            let Some(next) = caps[usize::from(piece)].checked_sub(1) else {
                return Ok(TileAdvance::Pending);
            };
            caps[usize::from(piece)] = next;
            self.path
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            self.path.push(Tile {
                piece,
                cells: cells.words(),
            });
            self.pending = Some((frame.remaining.without(cells), caps));
        } else {
            self.frames.pop();
        }
        Ok(TileAdvance::Pending)
    }
}

/// Lazy product of the two ILC/APDP domains. A complete first inventory selects
/// its complementary second domain before any hold/reachability expansion.
pub(super) struct Producer {
    geometry: Geometry,
    first_caps: [u8; 7],
    second_caps: [u8; 7],
    total: Option<[u8; 7]>,
    orientation: usize,
    middle: Option<Tiles>,
    result: Option<Tiles>,
    current: Vec<Tile>,
    pub done: bool,
}
impl Producer {
    pub fn new(
        query: &RecoveryBuildQuery,
        source: &Source,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        let geometry = Geometry::new(query, control)?;
        let stage = &geometry.stages[0];
        let n = stage.prepared.middle_pieces;
        let m = stage.prepared.result_pieces;
        let total = if usize::from(source.end) == n + m {
            source
                .first_counts
                .zip(source.second_counts)
                .map(|(a, b)| core::array::from_fn(|p| a[p] + b[p]))
        } else {
            None
        };
        let mut first_caps = [n as u8; 7];
        let mut second_caps = [m as u8; 7];
        if let Some(total) = total {
            for p in 0..7 {
                first_caps[p] = first_caps[p].min(total[p]);
                second_caps[p] = second_caps[p].min(total[p]);
            }
        }
        if !query.allow_piece_exchange && usize::from(source.first_len) == n {
            if let Some(counts) = source.first_counts {
                first_caps = counts;
            }
            if usize::from(source.end - source.first_len) == m {
                if let Some(counts) = source.second_counts {
                    second_caps = counts;
                }
            }
        }
        Ok(Self {
            geometry,
            first_caps,
            second_caps,
            total,
            orientation: 0,
            middle: None,
            result: None,
            current: Vec::new(),
            done: false,
        })
    }
    pub(super) fn stage(&self, orientation: u32) -> Option<&super::super::staged::geometry::Stage> {
        self.geometry.stages.get(orientation as usize)
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<Option<Plan>, Error> {
        for _ in 0..256 {
            cancelled(control)?;
            if self.orientation >= self.geometry.stages.len() {
                self.done = true;
                return Ok(None);
            }
            let stage = &mut self.geometry.stages[self.orientation];
            if let Some(result) = &mut self.result {
                match result.advance(&mut stage.result_domain, &[], control)? {
                    TileAdvance::Found(tiles) => {
                        return Ok(Some(Plan {
                            orientation: self.orientation as u32,
                            middle: self.current.clone(),
                            result: tiles,
                        }))
                    }
                    TileAdvance::Done => {
                        self.result = None;
                    }
                    TileAdvance::Pending => {}
                }
                continue;
            }
            let middle = self
                .middle
                .get_or_insert_with(|| Tiles::new(stage.fields.middle, self.first_caps));
            match middle.advance(
                &mut stage.middle_domain,
                &stage.chain_targets[..stage.chain_targets.len().saturating_sub(1)],
                control,
            )? {
                TileAdvance::Done => {
                    self.middle = None;
                    self.orientation += 1;
                }
                TileAdvance::Pending => {}
                TileAdvance::Found(tiles) => {
                    let mut caps = self.second_caps;
                    if let Some(total) = self.total {
                        let counts = Plan::counts(&tiles);
                        let mut valid = true;
                        for p in 0..7 {
                            if let Some(n) = total[p].checked_sub(counts[p]) {
                                caps[p] = caps[p].min(n);
                            } else {
                                valid = false;
                                break;
                            }
                        }
                        if !valid
                            || caps.iter().map(|&n| usize::from(n)).sum::<usize>()
                                < stage.prepared.result_pieces
                        {
                            continue;
                        }
                    }
                    self.current = tiles;
                    self.result = Some(Tiles::new(stage.fields.result, caps));
                }
            }
        }
        Ok(None)
    }
}
