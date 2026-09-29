//! Exact support quotient for downstream minimum cover. Every class contains
//! at least one original input and has exactly the same supporting solutions.
//! Class counts are NOT probability weights or a new sampling distribution.
use super::{catalog::RecoveryChainCatalog,RecoveryChainError as Error};
use crate::recovery_build::{RecoveryBuildError as Core,staged::{diagram::{Diagram,Id,ALL,NONE},source::cancelled}};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use std::collections::{BTreeSet,HashSet};

#[derive(Clone,Debug,Eq,PartialEq)]
pub struct RecoveryChainSupportPartition {
    pub solution_keys:Vec<String>,
    /// Each nonempty row lists all solution indices that can cover this class.
    pub constraints:Vec<Vec<usize>>,
    /// The original Cartesian universe, not the number of support classes.
    pub original_pattern_count:u128,
}
impl RecoveryChainCatalog {
    pub fn support_partition(&self,control:&ExecutionControl)->Result<RecoveryChainSupportPartition,Error> {
        if self.solutions.len()!=self.languages.len() || self.solutions.len()!=self.row_keys.len() ||
            self.solutions.iter().zip(&self.row_keys).any(|(row,key)|&row.key!=key) {
            return Err(Core::PatternDomainUnavailable.into());
        }
        let constraints=partition(&self.diagram,&self.languages,self.sources.end(),control)?;
        Ok(RecoveryChainSupportPartition {solution_keys:self.row_keys.clone(),constraints,
            original_pattern_count:self.sources.possible})
    }
}
fn partition(diagram:&Diagram,roots:&[Id],end:u16,control:&ExecutionControl)->Result<Vec<Vec<usize>>,Error> {
    let mut pending=vec![(0_u16,roots.to_vec())];
    let mut seen=HashSet::new();
    let mut classes=BTreeSet::new();
    while let Some((level,roots))=pending.pop() {
        cancelled(control)?;
        if roots.iter().all(|&id|id==NONE) {continue;}
        seen.try_reserve(1).map_err(|_|Core::MemoryUnavailable)?;
        if !seen.insert((level,roots.clone())) {continue;}
        if roots.iter().all(|&id|id==NONE || id==ALL) {
            classes.insert(roots.iter().enumerate().filter_map(|(i,&id)|(id==ALL).then_some(i)).collect::<Vec<_>>());
            continue;
        }
        if level>=end {return Err(Core::PatternDomainUnavailable.into());}
        // Identical children (including skipped levels) only need one visit.
        let mut children=Vec::new();
        for piece in 0..7 {
            let next=roots.iter().map(|&id|diagram.follow(id,level,piece)).collect::<Vec<_>>();
            if !children.contains(&next) {children.push(next);}
        }
        pending.try_reserve(children.len()).map_err(|_|Core::MemoryUnavailable)?;
        pending.extend(children.into_iter().map(|next|(level+1,next)));
    }
    Ok(classes.into_iter().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn support_quotient_matches_independent_exhaustive_assignment_supports() {
        // Three Boolean input positions embedded in the seven-way diagram.
        // Build row languages from independently chosen truth-table masks.
        for masks in [[0x55_u8,0xaa,0xf0],[0xff,0x0f,0x33],[0x00,0x01,0x01],[0x69,0x96,0x7e]] {
            let mut d=Diagram::default();let mut roots=Vec::new();
            for mask in masks {
                let mut row=NONE;
                for assignment in 0..8 {
                    if mask&(1<<assignment)==0 {continue;}
                    let mut branch=ALL;
                    for level in (0..3).rev() {branch=d.prepend(level,(assignment>>level)&1,branch).unwrap();}
                    row=d.union(row,branch).unwrap();
                }
                roots.push(row);
            }
            let mut expected=BTreeSet::new();
            for assignment in 0..8 {
                let support=masks.iter().enumerate().filter_map(|(i,&m)|(m&(1<<assignment)!=0).then_some(i)).collect::<Vec<_>>();
                if !support.is_empty() {expected.insert(support);}
            }
            assert_eq!(partition(&d,&roots,3,&ExecutionControl::default()).unwrap(),expected.into_iter().collect::<Vec<_>>());
        }
    }
    #[test]
    fn a_large_uniform_language_is_one_constraint_not_a_flat_input_bitset() {
        let d=Diagram::default();
        assert_eq!(partition(&d,&[ALL,NONE,ALL],60,&ExecutionControl::default()).unwrap(),vec![vec![0,2]]);
        assert!(partition(&d,&[],60,&ExecutionControl::default()).unwrap().is_empty());
    }
}
