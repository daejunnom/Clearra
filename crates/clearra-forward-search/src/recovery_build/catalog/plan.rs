//! Complete logical tilings, not representative order histories. Enumeration is
//! queue-independent; the exact verifier owns hold, kicks, timing and B2B.
use super::*;
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(in crate::recovery_build) struct Tile {
    pub piece: u8,
    pub cells: [u64; 4],
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::recovery_build) struct Plan {
    pub orientation: u8,
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
    pub fn inventory_allows(
        &self,
        used_middle: [u8; 7],
        used_result: [u8; 7],
        middle_caps: [u8; 7],
        result_caps: [u8; 7],
    ) -> bool {
        let middle = Self::counts(&self.middle);
        let result = Self::counts(&self.result);
        (0..7).all(|p| {
            middle[p]
                .checked_sub(used_middle[p])
                .is_some_and(|n| n <= middle_caps[p])
                && result[p]
                    .checked_sub(used_result[p])
                    .is_some_and(|n| n <= result_caps[p])
        })
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
            .get(usize::from(self.orientation))
            .ok_or(Error::PatternDomainUnavailable)?;
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
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<Option<Plan>, Error> {
        for _ in 0..256 {
            cancelled(control)?;
            if self.orientation >= self.geometry.stages.len() {
                self.done = true;
                return Ok(None);
            }
            let stage = &mut self.geometry.stages[self.orientation];
            if let Some(result) = &mut self.result {
                match result.advance(&mut stage.result_domain, control)? {
                    TileAdvance::Found(tiles) => {
                        return Ok(Some(Plan {
                            orientation: self.orientation as u8,
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
            match middle.advance(&mut stage.middle_domain, control)? {
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
