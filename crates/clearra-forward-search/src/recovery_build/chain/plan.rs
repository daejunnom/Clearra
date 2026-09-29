//! Lazy, inventory-coupled product of the existing ILC tiling enumerators.
//! A plan is a full tiling; its many physical/hold orders remain in the verifier.
use super::{
    fields::{OrientationAdvance, Orientations},
    solver::compile_domains,
    source::Sources,
    RecoveryChainError as Error, RecoveryChainQuery,
};
use crate::recovery_build::{
    catalog::plan::{Tile, TileAdvance, Tiles},
    staged::source::cancelled,
    RecoveryBuildError as Core,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
};
use clearra_core_executor::backend::BuildStageDomain;
use std::fmt::Write;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Plan {
    pub targets: Vec<Mask>,
    pub stages: Vec<Vec<Tile>>,
}
impl Plan {
    pub fn key(&self) -> String {
        // Geometry, NOT the enumeration ordinal, gives the stable identity.
        let mut key = String::from("recovery-chain-tiling.v1");
        for (stage, tiles) in self.stages.iter().enumerate() {
            for t in tiles {
                let _ = write!(
                    key,
                    "|{stage}/{}:{:016x}{:016x}{:016x}{:016x}",
                    t.piece, t.cells[3], t.cells[2], t.cells[1], t.cells[0]
                );
            }
        }
        key
    }
    pub fn validate(&self, q: &RecoveryChainQuery) -> Result<(), Error> {
        let mut oriented = q.clone();
        oriented.targets = self.targets.clone();
        super::fields::validate(&oriented)?;
        if self.stages.len() != self.targets.len() {
            return Err(Core::PatternDomainUnavailable.into());
        }
        for (tiles, &target) in self.stages.iter().zip(&self.targets) {
            if tiles.len() != target.count_ones() as usize / 4
                || !tiles.windows(2).all(|w| w[0] < w[1])
            {
                return Err(Core::PatternDomainUnavailable.into());
            }
            let mut occupied = Mask::EMPTY;
            for t in tiles {
                let cells = Mask::from_words(t.cells);
                if t.piece >= 7
                    || cells.count_ones() != 4
                    || occupied.intersects(cells)
                    || !cells.without(target).is_empty()
                {
                    return Err(Core::PatternDomainUnavailable.into());
                }
                occupied = occupied.union(cells);
            }
            if occupied != target {
                return Err(Core::PatternDomainUnavailable.into());
            }
        }
        Ok(())
    }
    pub fn allows(&self, stage: usize, piece: u8, mask: Mask) -> bool {
        self.stages.get(stage).is_some_and(|tiles| {
            tiles
                .binary_search(&Tile {
                    piece,
                    cells: mask.words(),
                })
                .is_ok()
        })
    }
    pub fn remaining_fits(&self, stage: usize, used: Mask, caps: [u8; 7]) -> bool {
        let mut needed = [0_u8; 7];
        let mut observed = Mask::EMPTY;
        for t in &self.stages[stage] {
            let cells = Mask::from_words(t.cells);
            if cells.intersects(used) {
                if !cells.without(used).is_empty() {
                    return false;
                }
                observed = observed.union(cells);
            } else {
                needed[usize::from(t.piece)] += 1;
            }
        }
        observed == used && (0..7).all(|p| needed[p] <= caps[p])
    }
}

pub(super) struct Producer {
    query: RecoveryChainQuery,
    orientations: Orientations,
    targets: Option<Vec<Mask>>,
    domains: Vec<BuildStageDomain>,
    caps: Vec<[u8; 7]>,
    total: Option<[u8; 7]>,
    cursors: Vec<Tiles>,
    selected: Vec<Vec<Tile>>,
    pub done: bool,
}
impl Producer {
    pub fn new(query: &RecoveryChainQuery, sources: &Sources) -> Result<Self, Error> {
        let demands = query
            .targets
            .iter()
            .map(|m| (m.count_ones() / 4) as u8)
            .collect::<Vec<_>>();
        let mut total = Some([0_u8; 7]);
        if usize::from(sources.end()) != demands.iter().map(|&d| usize::from(d)).sum::<usize>() {
            total = None;
        }
        if let Some(counts) = &mut total {
            for source in &sources.fixed_counts {
                let Some(source) = source else {
                    total = None;
                    break;
                };
                for p in 0..7 {
                    counts[p] = counts[p]
                        .checked_add(source[p])
                        .ok_or(Core::CounterOverflow)?;
                }
            }
        }
        let mut caps = demands.iter().map(|&n| [n; 7]).collect::<Vec<_>>();
        for (i, cap) in caps.iter_mut().enumerate() {
            if let Some(total) = total {
                for p in 0..7 {
                    cap[p] = cap[p].min(total[p]);
                }
            }
            if !query.allow_piece_exchange && sources.len(i) == usize::from(demands[i]) {
                if let Some(counts) = sources.fixed_counts[i] {
                    for p in 0..7 {
                        cap[p] = cap[p].min(counts[p]);
                    }
                }
            }
        }
        Ok(Self {
            query: query.clone(),
            orientations: Orientations::new(query),
            targets: None,
            domains: Vec::new(),
            caps,
            total,
            cursors: Vec::new(),
            selected: Vec::new(),
            done: false,
        })
    }
    fn next_caps(&self, depth: usize) -> Option<[u8; 7]> {
        let mut cap = self.caps[depth];
        if let Some(mut rest) = self.total {
            for stage in self.selected.iter().take(depth) {
                for t in stage {
                    rest[usize::from(t.piece)] = rest[usize::from(t.piece)].checked_sub(1)?;
                }
            }
            for p in 0..7 {
                cap[p] = cap[p].min(rest[p]);
            }
        }
        (cap.iter().map(|&n| usize::from(n)).sum::<usize>()
            >= self.query.targets[depth].count_ones() as usize / 4)
            .then_some(cap)
    }
    pub fn advance(
        &mut self,
        fuel: usize,
        control: &ExecutionControl,
    ) -> Result<Option<Plan>, Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if self.done {
                return Ok(None);
            }
            if self.targets.is_none() {
                match self.orientations.advance(&self.query, control)? {
                    OrientationAdvance::Pending => continue,
                    OrientationAdvance::Done => {
                        self.done = true;
                        return Ok(None);
                    }
                    OrientationAdvance::Found(targets) => {
                        self.domains = compile_domains(&self.query, &targets, control)?;
                        self.cursors = vec![Tiles::new(targets[0], self.caps[0])];
                        self.selected.clear();
                        self.targets = Some(targets);
                    }
                }
                continue;
            }
            if self.cursors.is_empty() {
                self.targets = None;
                self.domains.clear();
                continue;
            }
            let depth = self.cursors.len() - 1;
            self.selected.truncate(depth);
            match self.cursors[depth].advance(&mut self.domains[depth], control)? {
                TileAdvance::Pending => {}
                TileAdvance::Done => {
                    self.cursors.pop();
                }
                TileAdvance::Found(tiles) => {
                    self.selected
                        .try_reserve(1)
                        .map_err(|_| Core::MemoryUnavailable)?;
                    self.selected.push(tiles);
                    let targets = self.targets.as_ref().ok_or(Error::Incomplete)?;
                    if depth + 1 == targets.len() {
                        let plan = Plan {
                            targets: targets.clone(),
                            stages: self.selected.clone(),
                        };
                        plan.validate(&self.query)?;
                        return Ok(Some(plan));
                    }
                    if let Some(caps) = self.next_caps(depth + 1) {
                        self.cursors
                            .try_reserve(1)
                            .map_err(|_| Core::MemoryUnavailable)?;
                        self.cursors.push(Tiles::new(targets[depth + 1], caps));
                    }
                }
            }
        }
        Ok(None)
    }
}
