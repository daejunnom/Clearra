use super::{RecoveryChainError as Error, RecoveryChainQuery};
use crate::{board::{place_and_clear, ForwardBoard}, recovery_build::RecoveryBuildError as Core};
use clearra_core_domain::{board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl};
use clearra_problem::BuildProbabilityField;
use std::collections::BTreeSet;

pub(super) fn validate(q: &RecoveryChainQuery) -> Result<(), Error> {
    if !(1..=24).contains(&q.height) { return Err(Core::InvalidHeight.into()); }
    if !(2..=60).contains(&q.targets.len()) { return Err(Error::StageCount); }
    if q.targets.len() != q.supplies.len() { return Err(Error::SupplyCount); }
    if q.initial.fits_cell_count(u16::from(q.height) * 10) != Ok(true) {
        return Err(Core::BoardOutsideField.into());
    }
    let mut occupied = q.initial;
    for (index, &target) in q.targets.iter().enumerate() {
        if target.fits_cell_count(u16::from(q.height) * 10) != Ok(true) { return Err(Core::BoardOutsideField.into()); }
        if target.is_empty() || target.count_ones() % 4 != 0 { return Err(Error::InvalidStageArea(index)); }
        if occupied.intersects(target) { return Err(Error::OverlappingStage(index)); }
        occupied = occupied.union(target);
    }
    Ok(())
}
pub(super) fn full_rows(mask: Mask, height: u8) -> u32 {
    let b = ForwardBoard::from_mask(mask);
    (0..height).fold(0, |full,y| full | if b.row_bits(10,y)==1023 { 1_u32<<y } else {0})
}
pub(super) fn compact(mask: Mask, deleted: u32, height: u8) -> Mask {
    let b=ForwardBoard::from_mask(mask);
    let mut out=Mask::EMPTY;
    let mut row=0;
    for y in 0..height {
        if deleted & (1_u32<<y) != 0 { continue; }
        let bits=b.row_bits(10,y);
        for x in 0..10_u16 {
            if bits & (1<<x) != 0 {
                out=out.union(Mask::singleton(row*10+x).expect("validated chain row"));
            }
        }
        row+=1;
    }
    out
}
pub(super) fn lift(mask: ForwardBoard, deleted: u32, height: u8) -> Option<Mask> {
    let map=(0..height).filter(|&y| deleted & (1_u32<<y)==0).collect::<Vec<_>>();
    let mut out=Mask::EMPTY;
    for physical in 0..height {
        let bits=mask.row_bits(10,physical);
        if bits==0 {continue;}
        let logical=*map.get(usize::from(physical))?;
        for x in 0..10_u16 {
            if bits & (1<<x)!=0 {out=out.union(Mask::singleton(u16::from(logical)*10+x).ok()?);}
        }
    }
    Some(out)
}
pub(super) fn delete_rows(rows:u32, deleted:u32, height:u8)->u32 {
    let mut out=deleted;
    for (physical,logical) in (0..height).filter(|&y|deleted&(1_u32<<y)==0).enumerate() {
        if rows & (1_u32<<physical)!=0 {out|=1_u32<<logical;}
    }
    out
}

pub(super) enum OrientationAdvance { Pending, Found(Vec<Mask>), Done }
pub(super) struct Orientations {
    pending: Vec<(usize,Vec<Mask>)>,
    seen: BTreeSet<Vec<Mask>>,
}
impl Orientations {
    pub fn new(q:&RecoveryChainQuery)->Self {Self {pending:vec![(0,q.targets.clone())],seen:BTreeSet::new()}}
    pub fn advance(&mut self,q:&RecoveryChainQuery,control:&ExecutionControl)->Result<OrientationAdvance,Error> {
        super::super::staged::source::cancelled(control)?;
        let Some((boundary,targets))=self.pending.pop() else {return Ok(OrientationAdvance::Done)};
        if boundary==targets.len() {
            return Ok(if self.seen.insert(targets.clone()) {OrientationAdvance::Found(targets)} else {OrientationAdvance::Pending});
        }
        let prefix=targets[..boundary].iter().fold(q.initial,|base,&t|base.union(t));
        let (base,_,_)=place_and_clear(10,q.height,ForwardBoard::from_mask(prefix));
        let deleted=full_rows(prefix,q.height);
        let next_target=compact(targets[boundary],deleted,q.height);
        let applicable=BuildProbabilityField::from_words_preserving_height(q.height,base.words(),next_target.words())
            .map_err(|_|Core::BoardOutsideField)?
            .with_horizontal_mirror_included(true).includes_applicable_horizontal_mirror();
        if applicable {
            let mut other=targets.clone();
            for t in &mut other[boundary..] {
                *t=t.mirrored_horizontally(10,u16::from(q.height)).map_err(|_|Core::BoardOutsideField)?;
            }
            if other!=targets {
                self.pending.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
                self.pending.push((boundary+1,other));
            }
        }
        // Reflection is a target alternative, not an assumption that the
        // incoming pieces, held token, or custom kick table are symmetric.
        self.pending.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
        self.pending.push((boundary+1,targets));
        Ok(OrientationAdvance::Pending)
    }
}
