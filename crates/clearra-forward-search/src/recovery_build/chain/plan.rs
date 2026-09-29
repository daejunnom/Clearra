//! Queue-independent complete tilings. Every selected tile is a four-cell
//! logical placement, not an Arm-Pair or a single temporal realization.
use super::{fields::{self, OrientationAdvance, Orientations}, source::Sources,
    RecoveryChainError as Error, RecoveryChainQuery};
use crate::recovery_build::{RecoveryBuildError as Core, staged::source::cancelled};
use clearra_core_domain::{board::standard_pc_board::Board256Mask as Mask,
    execution_cancellation::ExecutionControl};
use clearra_core_executor::backend::{BuildStageDomain, BuildStageDomainError};
use clearra_problem::BuildProbabilityField;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Tile { pub piece: u8, pub cells: [u64; 4] }
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Plan {
    pub targets: Vec<Mask>,
    pub stages: Vec<Vec<Tile>>,
}
impl Plan {
    pub fn counts(tiles: &[Tile]) -> [u8; 7] {
        let mut counts=[0;7];
        for tile in tiles { counts[usize::from(tile.piece)]+=1; }
        counts
    }
    pub fn allows(&self, target:usize, piece:u8, cells:Mask)->bool {
        self.stages[target].binary_search(&Tile {piece,cells:cells.words()}).is_ok()
    }
    pub fn key(&self)->String {
        use std::fmt::Write;
        let mut text=String::from("recovery-chain-tiling.v1");
        for (stage,tiles) in self.stages.iter().enumerate() {
            let _=write!(text,"|stage{stage}");
            for t in tiles {
                let _=write!(text,"|{}:{:016x}{:016x}{:016x}{:016x}",t.piece,t.cells[3],t.cells[2],t.cells[1],t.cells[0]);
            }
        }
        text
    }
    pub fn validate(&self,q:&RecoveryChainQuery)->Result<(),Error> {
        let mut oriented=q.clone();oriented.targets=self.targets.clone();fields::validate(&oriented)?;
        if self.stages.len()!=self.targets.len() {return Err(Core::PatternDomainUnavailable.into());}
        for (tiles,&target) in self.stages.iter().zip(&self.targets) {
            if tiles.len()!=target.count_ones() as usize/4 || !tiles.windows(2).all(|w|w[0]<w[1]) {
                return Err(Core::PatternDomainUnavailable.into());
            }
            let mut used=Mask::EMPTY;
            for tile in tiles {
                let mask=Mask::from_words(tile.cells);
                if tile.piece>=7 || mask.count_ones()!=4 || used.intersects(mask) || !mask.without(target).is_empty() {
                    return Err(Core::PatternDomainUnavailable.into());
                }
                used=used.union(mask);
            }
            if used!=target {return Err(Core::PatternDomainUnavailable.into());}
        }
        Ok(())
    }
}
pub(super) fn domain_error(error:BuildStageDomainError)->Error {
    match error {
        BuildStageDomainError::InvalidField=>Core::PatternDomainUnavailable,
        BuildStageDomainError::Allocation=>Core::MemoryUnavailable,
        BuildStageDomainError::Cancelled=>Core::Cancelled,
    }.into()
}
pub(super) fn domains(q:&RecoveryChainQuery,targets:&[Mask],control:&ExecutionControl)->Result<Vec<BuildStageDomain>,Error> {
    let union=targets.iter().fold(q.initial,|u,&t|u.union(t));
    let mut domains=Vec::new();
    for &target in targets {
        cancelled(control)?;
        let h=(0..q.height).rev().find(|&y|(0..10).any(|x|target.contains_index(u16::from(y)*10+x))).map_or(1,|y|y+1);
        let clip=Mask::all_cells(u16::from(h)*10).map_err(|_|Core::BoardOutsideField)?;
        let context=union.without(target);
        let base=Mask::from_words(core::array::from_fn(|w|context.words()[w]&clip.words()[w]));
        let field=BuildProbabilityField::from_words_preserving_height(h,base.words(),target.words()).map_err(|_|Core::BoardOutsideField)?;
        domains.push(BuildStageDomain::compile(field,control).map_err(domain_error)?);
    }
    Ok(domains)
}
struct TileFrame { caps:[u8;7], remaining:Mask, choices:std::vec::IntoIter<(u8,Mask)> }
struct Tiles { pending:Option<(Mask,[u8;7])>, frames:Vec<TileFrame>, path:Vec<Tile> }
enum TileAdvance { Pending, Done, Found(Vec<Tile>) }
impl Tiles {
    fn new(target:Mask,caps:[u8;7])->Self {Self {pending:Some((target,caps)),frames:Vec::new(),path:Vec::new()}}
    fn advance(&mut self,domain:&mut BuildStageDomain,control:&ExecutionControl)->Result<TileAdvance,Error> {
        cancelled(control)?;
        if let Some((remaining,caps))=self.pending.take() {
            if remaining.is_empty() {
                let mut tiles=self.path.clone();tiles.sort_unstable();return Ok(TileAdvance::Found(tiles));
            }
            if domain.can_complete(remaining,caps,control).map_err(domain_error)? {
                let choices=domain.completion_choices(remaining,caps,control).map_err(domain_error)?.into_iter();
                self.frames.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
                self.frames.push(TileFrame {caps,remaining,choices});
            }
            return Ok(TileAdvance::Pending);
        }
        let depth=self.frames.len();
        let Some(frame)=self.frames.last_mut() else {return Ok(TileAdvance::Done)};
        self.path.truncate(depth-1);
        if let Some((piece,cells))=frame.choices.next() {
            let mut caps=frame.caps;
            let Some(next)=caps[usize::from(piece)].checked_sub(1) else {return Ok(TileAdvance::Pending)};
            caps[usize::from(piece)]=next;
            self.path.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
            self.path.push(Tile {piece,cells:cells.words()});
            self.pending=Some((frame.remaining.without(cells),caps));
        } else {self.frames.pop();}
        Ok(TileAdvance::Pending)
    }
}
/// A lazy product of stage tilings, not a product of input queue lists.
pub(super) struct Producer {
    orientations:Orientations,
    targets:Option<Vec<Mask>>,
    domains:Vec<BuildStageDomain>,
    caps:Vec<[u8;7]>,
    total:Option<[u8;7]>,
    stack:Vec<Tiles>,
    path:Vec<Vec<Tile>>,
    pub done:bool,
}
impl Producer {
    pub fn new(q:&RecoveryChainQuery,sources:&Sources)->Result<Self,Error> {
        let demands=q.targets.iter().map(|t|(t.count_ones()/4) as u8).collect::<Vec<_>>();
        let mut total=Some([0_u8;7]);
        if usize::from(sources.end())!=demands.iter().map(|&n|usize::from(n)).sum::<usize>() {total=None;}
        if total.is_some() {
            for counts in &sources.fixed_counts {
                let Some(counts)=counts else {total=None;break};
                for (a,b) in total.as_mut().ok_or(Error::Incomplete)?.iter_mut().zip(counts) {
                    *a=a.checked_add(*b).ok_or(Core::CounterOverflow)?;
                }
            }
        }
        let caps=demands.iter().enumerate().map(|(i,&d)| {
            let mut cap=[d;7];
            if let Some(t)=total {for p in 0..7 {cap[p]=cap[p].min(t[p]);}}
            if !q.allow_piece_exchange && sources.len(i)==usize::from(d) {
                if let Some(counts)=sources.fixed_counts[i] {cap=counts;}
            }
            cap
        }).collect();
        Ok(Self {orientations:Orientations::new(q),targets:None,domains:Vec::new(),caps,total,
            stack:Vec::new(),path:Vec::new(),done:demands.iter().enumerate().any(|(i,&d)|sources.len(i)<usize::from(d))})
    }
    fn remaining_caps(&self,stage:usize)->Option<[u8;7]> {
        let mut cap=self.caps[stage];
        if let Some(mut remaining)=self.total {
            for tiles in &self.path {
                let counts=Plan::counts(tiles);
                for p in 0..7 {remaining[p]=remaining[p].checked_sub(counts[p])?;}
            }
            for p in 0..7 {cap[p]=cap[p].min(remaining[p]);}
        }
        Some(cap)
    }
    pub fn advance(&mut self,q:&RecoveryChainQuery,control:&ExecutionControl)->Result<Option<Plan>,Error> {
        for _ in 0..256 {
            cancelled(control)?;
            if self.done {return Ok(None);}
            if self.targets.is_none() {
                match self.orientations.advance(q,control)? {
                    OrientationAdvance::Pending=>continue,
                    OrientationAdvance::Done=>{self.done=true;return Ok(None)},
                    OrientationAdvance::Found(targets)=>{
                        self.domains=domains(q,&targets,control)?;
                        self.stack.push(Tiles::new(targets[0],self.caps[0]));
                        self.targets=Some(targets);self.path.clear();
                    },
                }
            }
            let stage=self.stack.len().checked_sub(1).ok_or(Error::Incomplete)?;
            self.path.truncate(stage);
            match self.stack[stage].advance(&mut self.domains[stage],control)? {
                TileAdvance::Pending=>{},
                TileAdvance::Done=>{
                    self.stack.pop();
                    if self.stack.is_empty() {self.targets=None;self.domains.clear();self.path.clear();}
                },
                TileAdvance::Found(tiles)=>{
                    self.path.push(tiles);
                    let targets=self.targets.as_ref().ok_or(Error::Incomplete)?;
                    if stage+1==targets.len() {
                        let plan=Plan {targets:targets.clone(),stages:self.path.clone()};
                        plan.validate(q)?;
                        return Ok(Some(plan));
                    }
                    if let Some(cap)=self.remaining_caps(stage+1) {
                        if cap.iter().map(|&n|usize::from(n)).sum::<usize>()>=targets[stage+1].count_ones() as usize/4 {
                            self.stack.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
                            self.stack.push(Tiles::new(targets[stage+1],cap));
                        }
                    }
                },
            }
        }
        Ok(None)
    }
}
