//! The same ILC/APDP domains as Build. Only whole tile inventories are joined;
//! no independent-stage movement, hold or B2B result is assumed composable.
use super::super::catalog::plan::{Tile, TileAdvance, Tiles};
use super::source::Source;
use super::*;
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};
use clearra_problem::BuildProbabilityField;
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Plan {
    pub targets: Vec<Mask>,
    pub groups: Vec<Vec<Tile>>,
}
impl Plan {
    pub fn key(&self) -> String {
        use std::fmt::Write;
        let mut key = String::from("recovery-chain-tiling.v1");
        for (i, tiles) in self.groups.iter().enumerate() {
            for t in tiles {
                let _ = write!(
                    key,
                    "|{i}/{}:{:016x}{:016x}{:016x}{:016x}",
                    t.piece, t.cells[3], t.cells[2], t.cells[1], t.cells[0]
                );
            }
        }
        key
    }
    pub fn counts(&self) -> Vec<[u8; 7]> {
        self.groups
            .iter()
            .map(|tiles| {
                let mut c = [0; 7];
                for t in tiles {
                    c[usize::from(t.piece)] += 1;
                }
                c
            })
            .collect()
    }
    pub fn pieces(&self) -> usize {
        self.groups.iter().map(Vec::len).sum()
    }
    pub fn validate(&self, q: &RecoveryBuildQuery) -> Result<(), Error> {
        if self.groups.len() != q.stages.len()
            || self.targets.len() != self.groups.len()
            || self.pieces() > 60
        {
            return Err(Error::PatternDomainUnavailable);
        }
        let mut prefix = q.fields.initial;
        // At most two parity states are needed to verify a reflected suffix.
        let mut parities = vec![false];
        for (i, (tiles, &target)) in self.groups.iter().zip(&self.targets).enumerate() {
            let nominal = project(prefix, prefix, q.fields.height);
            let possible = can_reflect(
                nominal,
                project(target, prefix, q.fields.height),
                q.fields.height,
            )?;
            let mut next = Vec::new();
            for parity in parities {
                for toggle in [false, true] {
                    if toggle && !possible {
                        continue;
                    }
                    let parity = parity ^ toggle;
                    let expected = if parity {
                        reflected(q.stages[i].target, q.fields.height)?
                    } else {
                        q.stages[i].target
                    };
                    if target == expected && !next.contains(&parity) {
                        next.push(parity);
                    }
                }
            }
            if next.is_empty() {
                return Err(Error::PatternDomainUnavailable);
            }
            parities = next;
            if tiles.len() * 4 != target.count_ones() as usize
                || !tiles.windows(2).all(|w| w[0] < w[1])
            {
                return Err(Error::PatternDomainUnavailable);
            }
            let mut occupied = Mask::EMPTY;
            for t in tiles {
                let m = Mask::from_words(t.cells);
                if t.piece >= 7
                    || m.count_ones() != 4
                    || m.without(target) != Mask::EMPTY
                    || occupied.intersects(m)
                {
                    return Err(Error::PatternDomainUnavailable);
                }
                occupied = occupied.union(m);
            }
            if occupied != target || prefix.intersects(target) {
                return Err(Error::PatternDomainUnavailable);
            }
            prefix = prefix.union(target);
        }
        Ok(())
    }
}
fn domain_error(e: BuildStageDomainError) -> Error {
    match e {
        BuildStageDomainError::Allocation => Error::MemoryUnavailable,
        BuildStageDomainError::Cancelled => Error::Cancelled,
        BuildStageDomainError::InvalidField => Error::PatternDomainUnavailable,
    }
}
struct Orientation {
    depth: usize,
    targets: Vec<Mask>,
    prefix: Mask,
}
pub(super) struct Producer {
    query: RecoveryBuildQuery,
    orientations: Vec<Orientation>,
    seen: HashSet<(usize, Vec<[u64; 4]>)>,
    targets: Vec<Mask>,
    domains: Vec<BuildStageDomain>,
    frames: Vec<Tiles>,
    chosen: Vec<Vec<Tile>>,
    total: Option<[u8; 7]>,
    limits: Vec<[u8; 7]>,
    pub done: bool,
}
impl Producer {
    pub fn new(q: RecoveryBuildQuery, s: &Source) -> Self {
        let total = s.whole_inventory(
            q.stages
                .iter()
                .map(|v| v.target.count_ones() as usize / 4)
                .sum(),
        );
        let limits = q
            .stages
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let n = v.target.count_ones() as usize / 4;
                let mut caps = [n as u8; 7];
                if let Some(total) = total {
                    for p in 0..7 {
                        caps[p] = caps[p].min(total[p]);
                    }
                }
                if !q.allow_piece_exchange && usize::from(s.offsets[i + 1] - s.offsets[i]) == n {
                    if let Some(counts) = s.counts[i] {
                        caps = counts;
                    }
                }
                caps
            })
            .collect();
        let root = Orientation {
            depth: 0,
            targets: q.stages.iter().map(|s| s.target).collect(),
            prefix: q.fields.initial,
        };
        Self {
            query: q,
            orientations: vec![root],
            seen: HashSet::new(),
            targets: Vec::new(),
            domains: Vec::new(),
            frames: Vec::new(),
            chosen: Vec::new(),
            total,
            limits,
            done: false,
        }
    }
    fn caps(&self, stage: usize) -> Option<[u8; 7]> {
        let mut caps = self.limits[stage];
        if let Some(mut remaining) = self.total {
            for group in &self.chosen {
                for tile in group {
                    let p = usize::from(tile.piece);
                    remaining[p] = remaining[p].checked_sub(1)?;
                }
            }
            for p in 0..7 {
                caps[p] = caps[p].min(remaining[p]);
            }
        }
        Some(caps)
    }
    fn orientation_tick(&mut self, control: &ExecutionControl) -> Result<bool, Error> {
        let Some(state) = self.orientations.pop() else {
            self.done = true;
            return Ok(false);
        };
        cancelled(control)?;
        if !self.seen.insert((
            state.depth,
            state.targets.iter().map(|m| m.words()).collect(),
        )) {
            return Ok(false);
        }
        if state.depth == state.targets.len() {
            self.targets = state.targets;
            let whole = self
                .targets
                .iter()
                .fold(self.query.fields.initial, |m, t| m.union(*t));
            self.domains.clear();
            self.chosen.clear();
            self.frames.clear();
            for &target in &self.targets {
                let height = (0..self.query.fields.height)
                    .rev()
                    .find(|&y| (0..10).any(|x| target.contains_index(u16::from(y) * 10 + x)))
                    .map_or(1, |y| y + 1);
                let clip = Mask::all_cells(u16::from(height) * 10)
                    .map_err(|_| Error::BoardOutsideField)?;
                let base = Mask::from_words(core::array::from_fn(|w| {
                    whole.without(target).words()[w] & clip.words()[w]
                }));
                self.domains.push(
                    BuildStageDomain::compile(
                        BuildProbabilityField::from_words_preserving_height(
                            height,
                            base.words(),
                            target.words(),
                        )
                        .map_err(|_| Error::BoardOutsideField)?,
                        control,
                    )
                    .map_err(domain_error)?,
                );
            }
            self.frames
                .push(Tiles::new(self.targets[0], self.limits[0]));
            return Ok(true);
        }
        let i = state.depth;
        let h = self.query.fields.height;
        if can_reflect(
            project(state.prefix, state.prefix, h),
            project(state.targets[i], state.prefix, h),
            h,
        )? {
            let mut targets = state.targets.clone();
            for t in &mut targets[i..] {
                *t = reflected(*t, h)?;
            }
            if targets != state.targets {
                self.orientations.push(Orientation {
                    depth: i + 1,
                    prefix: state.prefix.union(targets[i]),
                    targets,
                });
            }
        }
        self.orientations.push(Orientation {
            depth: i + 1,
            prefix: state.prefix.union(state.targets[i]),
            targets: state.targets,
        });
        Ok(false)
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<Option<Plan>, Error> {
        for _ in 0..256 {
            cancelled(control)?;
            if self.done {
                return Ok(None);
            }
            if self.frames.is_empty() {
                self.orientation_tick(control)?;
                continue;
            }
            let i = self.frames.len() - 1;
            self.chosen.truncate(i);
            match self.frames[i].advance(&mut self.domains[i], control)? {
                TileAdvance::Pending => {}
                TileAdvance::Done => {
                    self.frames.pop();
                }
                TileAdvance::Found(tiles) => {
                    self.chosen.push(tiles);
                    if i + 1 == self.targets.len() {
                        let p = Plan {
                            targets: self.targets.clone(),
                            groups: self.chosen.clone(),
                        };
                        if let Some(total) = self.total {
                            let counts = p.counts();
                            if (0..7).any(|k| {
                                counts.iter().map(|c| u16::from(c[k])).sum::<u16>()
                                    != u16::from(total[k])
                            }) {
                                continue;
                            }
                        }
                        return Ok(Some(p));
                    }
                    if let Some(caps) = self.caps(i + 1) {
                        self.frames.push(Tiles::new(self.targets[i + 1], caps));
                    }
                }
            }
        }
        Ok(None)
    }
}
