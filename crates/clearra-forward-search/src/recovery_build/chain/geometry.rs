//! Complete ILC/APDP domains in the common logical frame. Other-stage final
//! cells are a relaxation of row-clear availability, never a physical board.
use super::super::catalog::plan::{Tile, TileAdvance, Tiles};
use super::super::staged::source::cancelled;
use super::*;
use crate::board::{place_and_clear, ForwardBoard};
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};
use clearra_problem::BuildProbabilityField;
use std::fmt::Write;
fn domain_error(e: BuildStageDomainError) -> Error {
    match e {
        BuildStageDomainError::Cancelled => Error::Cancelled,
        BuildStageDomainError::Allocation => Error::MemoryUnavailable,
        BuildStageDomainError::InvalidField => Error::PatternDomainUnavailable,
    }
}
fn reflected(mask: Mask, height: u8) -> Result<Mask, Error> {
    mask.mirrored_horizontally(10, u16::from(height))
        .map_err(|_| Error::BoardOutsideField)
}
/// At boundary k, reflect every destination k..N together. Subsequent
/// boundaries remain independent choices only when their nominal base permits
/// it. Every resulting orientation is physically verified, not inferred.
fn orientations(query: &RecoveryChainQuery, c: &ExecutionControl) -> Result<Vec<Vec<Mask>>, Error> {
    let mut variants = vec![query.targets.clone()];
    for boundary in 0..query.targets.len() {
        let count = variants.len();
        for i in 0..count {
            cancelled(c)?;
            let targets = &variants[i];
            let prefix = targets[..boundary]
                .iter()
                .fold(query.initial, |a, b| a.union(*b));
            let (board, _, _) = place_and_clear(10, query.height, ForwardBoard::from_mask(prefix));
            let base = Mask::from_words(board.words());
            if reflected(base, query.height)? != base {
                continue;
            }
            let mut other = targets.clone();
            for target in &mut other[boundary..] {
                *target = reflected(*target, query.height)?;
            }
            if !variants.contains(&other) {
                variants
                    .try_reserve(1)
                    .map_err(|_| Error::MemoryUnavailable)?;
                variants.push(other);
            }
        }
    }
    Ok(variants)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Placement {
    pub stage: usize,
    pub piece: u8,
    pub cells: Mask,
}
#[derive(Clone)]
pub(super) struct Plan {
    pub targets: Vec<Mask>,
    pub tiles: Vec<Placement>,
    pub prefix_bits: Vec<u64>,
    pub demand: Vec<u8>,
    pub terminal: ForwardBoard,
}
impl Plan {
    fn new(
        q: &RecoveryChainQuery,
        targets: Vec<Mask>,
        groups: &[Vec<Tile>],
    ) -> Result<Self, Error> {
        let mut tiles = Vec::new();
        let mut prefix_bits = Vec::new();
        let mut demand = Vec::new();
        for (stage, group) in groups.iter().enumerate() {
            demand.push(u8::try_from(group.len()).map_err(|_| Error::CounterOverflow)?);
            for tile in group {
                tiles.push(Placement {
                    stage,
                    piece: tile.piece,
                    cells: Mask::from_words(tile.cells),
                });
            }
            if tiles.len() > 60 {
                return Err(Error::BoardOutsideField);
            }
            prefix_bits.push((1_u64 << tiles.len()) - 1);
        }
        let full = targets.iter().fold(q.initial, |a, b| a.union(*b));
        let (terminal, _, _) = place_and_clear(10, q.height, ForwardBoard::from_mask(full));
        Ok(Self {
            targets,
            tiles,
            prefix_bits,
            demand,
            terminal,
        })
    }
    pub fn key(&self) -> String {
        let mut out = String::from("recovery-chain-tiling.v1");
        for t in &self.tiles {
            let w = t.cells.words();
            let _ = write!(
                out,
                "|{}:{}:{:016x}{:016x}{:016x}{:016x}",
                t.stage, t.piece, w[3], w[2], w[1], w[0]
            );
        }
        out
    }
    pub fn complete(&self, used: u64, boundary: usize) -> bool {
        used & self.prefix_bits[boundary] == self.prefix_bits[boundary]
    }
}
struct Variant {
    targets: Vec<Mask>,
    domains: Vec<BuildStageDomain>,
}
pub(super) enum Produced {
    Pending,
    Done,
    Plan(Plan),
}
pub(super) struct Producer {
    query: RecoveryChainQuery,
    variants: Vec<Variant>,
    orientation: usize,
    frames: Vec<Tiles>,
    chosen: Vec<Vec<Tile>>,
    caps: Vec<[u8; 7]>,
    total: Option<[u8; 7]>,
    done: bool,
}
impl Producer {
    pub fn new(
        q: &RecoveryChainQuery,
        s: &source::Source,
        c: &ExecutionControl,
    ) -> Result<Self, Error> {
        let mut variants = Vec::new();
        for targets in orientations(q, c)? {
            let all = targets.iter().fold(q.initial, |a, b| a.union(*b));
            let mut domains = Vec::new();
            for target in &targets {
                cancelled(c)?;
                let h = (0..q.height)
                    .rev()
                    .find(|&y| (0..10).any(|x| target.contains_index(u16::from(y) * 10 + x)))
                    .map_or(1, |y| y + 1);
                let clip =
                    Mask::all_cells(u16::from(h) * 10).map_err(|_| Error::BoardOutsideField)?;
                let context = all.without(*target).without(all.without(clip));
                let field = BuildProbabilityField::from_words_preserving_height(
                    h,
                    context.words(),
                    target.words(),
                )
                .map_err(|_| Error::BoardOutsideField)?;
                domains.push(BuildStageDomain::compile(field, c).map_err(domain_error)?);
            }
            variants.push(Variant { targets, domains });
        }
        let demand = q
            .targets
            .iter()
            .map(|m| m.count_ones() as usize / 4)
            .collect::<Vec<_>>();
        let mut total = Some([0_u8; 7]);
        for counts in &s.counts {
            total = match (total, *counts) {
                (Some(a), Some(b)) => {
                    let mut n = [0_u8; 7];
                    for p in 0..7 {
                        n[p] = a[p].checked_add(b[p]).ok_or(Error::CounterOverflow)?;
                    }
                    Some(n)
                }
                _ => None,
            };
        }
        let caps = demand
            .iter()
            .enumerate()
            .map(|(i, &n)| {
                let mut caps = [n as u8; 7];
                if let Some(total) = total {
                    for p in 0..7 {
                        caps[p] = caps[p].min(total[p]);
                    }
                }
                if !q.allow_piece_exchange && s.length(i) == n {
                    if let Some(counts) = s.counts[i] {
                        caps = counts;
                    }
                }
                caps
            })
            .collect::<Vec<_>>();
        // Only exact consumption allows a remaining-inventory equality join.
        if usize::from(s.end) != demand.iter().sum::<usize>() {
            total = None;
        }
        Ok(Self {
            query: q.clone(),
            variants,
            orientation: 0,
            frames: Vec::new(),
            chosen: Vec::new(),
            caps,
            total,
            done: false,
        })
    }
    fn remaining_caps(&self, stage: usize) -> Option<[u8; 7]> {
        let mut caps = self.caps[stage];
        if let Some(mut total) = self.total {
            for group in &self.chosen {
                for tile in group {
                    let p = usize::from(tile.piece);
                    total[p] = total[p].checked_sub(1)?;
                }
            }
            for p in 0..7 {
                caps[p] = caps[p].min(total[p]);
            }
        }
        Some(caps)
    }
    pub fn advance(&mut self, c: &ExecutionControl) -> Result<Produced, Error> {
        cancelled(c)?;
        if self.done {
            return Ok(Produced::Done);
        }
        if self.orientation == self.variants.len() {
            self.done = true;
            return Ok(Produced::Done);
        }
        if self.frames.is_empty() {
            self.chosen.clear();
            self.frames.push(Tiles::new(
                self.variants[self.orientation].targets[0],
                self.caps[0],
            ));
        }
        let depth = self.frames.len() - 1;
        self.chosen.truncate(depth);
        match self.frames[depth].advance(&mut self.variants[self.orientation].domains[depth], c)? {
            TileAdvance::Pending => Ok(Produced::Pending),
            TileAdvance::Done => {
                self.frames.pop();
                if self.frames.is_empty() {
                    self.orientation += 1;
                }
                Ok(Produced::Pending)
            }
            TileAdvance::Found(group) => {
                self.chosen.push(group);
                if depth + 1 == self.query.targets.len() {
                    let plan = Plan::new(
                        &self.query,
                        self.variants[self.orientation].targets.clone(),
                        &self.chosen,
                    )?;
                    Ok(Produced::Plan(plan))
                } else if let Some(caps) = self.remaining_caps(depth + 1) {
                    self.frames.push(Tiles::new(
                        self.variants[self.orientation].targets[depth + 1],
                        caps,
                    ));
                    Ok(Produced::Pending)
                } else {
                    Ok(Produced::Pending)
                }
            }
        }
    }
}
