"""Integrate the explicit generic chain source on the verified mirror base.
The product adapter/UI will remain on the existing path until separately wired.
"""
from pathlib import Path
import shutil
import subprocess

BASE = '2a849b8e95c812f2b81d954ab91f3ee377e3a67a'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == BASE
helper = Path('_local/chain-helper')
core = Path('crates/clearra-forward-search/src/recovery_build')
destination = core / 'chain'
assert not destination.exists()
shutil.copytree(helper / destination, destination)
assert {p.name for p in destination.iterdir()} == {'mod.rs', 'source.rs', 'geometry.rs', 'solver.rs', 'tests.rs'}

def edit(path, old, new, count=1):
    p=Path(path)
    s=p.read_text()
    assert s.count(old)==count, (str(path), old[:80], s.count(old))
    p.write_text(s.replace(old,new))

edit(core/'mod.rs', 'mod catalog;\n', '''mod catalog;
mod chain;
pub use chain::{RecoveryChainQuery, RecoveryChainSearch, RecoveryChainReport, RecoveryChainSolution, RecoveryChainWitness};
''')
edit('crates/clearra-forward-search/src/lib.rs', 'pub use recovery_build::{\n', '''pub use recovery_build::{
    RecoveryChainQuery, RecoveryChainSearch, RecoveryChainReport, RecoveryChainSolution, RecoveryChainWitness,
''')
edit(core/'catalog.rs','mod plan;','pub(in crate::recovery_build) mod plan;')
for old,new in [
    ('enum TileAdvance','pub(in crate::recovery_build) enum TileAdvance'),
    ('struct Tiles','pub(in crate::recovery_build) struct Tiles'),
    ('    fn new(target: Mask, caps: [u8; 7])','    pub(in crate::recovery_build) fn new(target: Mask, caps: [u8; 7])'),
    ('    fn advance(\n','    pub(in crate::recovery_build) fn advance(\n'),
]:
    edit(core/'catalog/plan.rs',old,new)
p=core/'staged/source.rs'
p.write_text(p.read_text()+'''
/// Compile a single stage at its absolute source offset. The terminal accepts
/// all subsequent stages. This shares the ordinary two-stage source semantics.
pub(in crate::recovery_build) fn segment(
    diagram: &mut Diagram, universe: &MaterializedPatternUniverse, offset: u16,
    control: &ExecutionControl,
) -> Result<(Id, Option<[u8; 7]>, bool), Error> {
    if let Some(atoms) = compact_atoms(universe).filter(|a| !a.is_empty()) {
        let root = compile_atoms(diagram, &atoms, 0, atoms[0].mask, atoms[0].draws,
            offset, &mut HashMap::new(), control)?;
        let len = u16::try_from(universe.sequence_len_at(0)).map_err(|_|Error::CounterOverflow)?;
        let end = offset.checked_add(len).ok_or(Error::CounterOverflow)?;
        if diagram.count(root, offset, end)? != universe.pattern_count() as u128 {
            return Err(Error::PatternDomainUnavailable);
        }
        let mut counts = [0_u8; 7]; let mut fixed = true;
        for atom in &atoms {
            fixed &= u32::from(atom.draws) == atom.mask.count_ones();
            for (p,n) in counts.iter_mut().enumerate() {
                *n = n.checked_add(u8::from(atom.mask & (1<<p) != 0)).ok_or(Error::CounterOverflow)?;
            }
        }
        return Ok((root, fixed.then_some(counts), true));
    }
    let mut root = NONE; let mut counts = None; let mut same = true;
    let len = universe.sequence_len_at(0);
    for i in 0..universe.pattern_count() {
        cancelled(control)?;
        let queue = universe.sequence_at(i);
        if queue.len() != len {return Err(Error::PatternDomainUnavailable);}
        let branch = encode_queue(diagram,&queue,offset,ALL)?;
        root = diagram.union(root,branch)?;
        let value = inventory(&queue)?;
        if let Some(old) = counts {same &= old == value;} else {counts = Some(value);}
    }
    Ok((root, if same {counts} else {None}, false))
}
''')
print(subprocess.check_output(['git','diff','--stat'],text=True))
