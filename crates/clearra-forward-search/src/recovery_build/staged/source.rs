//! The two original probability spaces stay independent. Only a bounded range
//! of first queues is expanded; a P7 second source is a 128-state remainder DAG.
use super::super::{search::piece_index, RecoveryBuildError as Error};
use super::diagram::{Diagram, Id, ALL, NONE};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_supply::pattern_universe::{
    materialized_pattern_universe::UniformCompactPatternSource, MaterializedPatternUniverse,
};
use std::collections::HashMap;

pub(in crate::recovery_build) const PIECES: [PieceKind; 7] = [
    PieceKind::I,
    PieceKind::J,
    PieceKind::L,
    PieceKind::O,
    PieceKind::S,
    PieceKind::T,
    PieceKind::Z,
];
pub(in crate::recovery_build) fn cancelled(control: &ExecutionControl) -> Result<(), Error> {
    if control.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
#[derive(Clone)]
struct Atom {
    mask: u8,
    draws: u16,
}
#[derive(Clone)]
pub(in crate::recovery_build) struct Source {
    pub first_len: u16,
    pub end: u16,
    pub universe: Id,
    pub second: Id,
    pub first_counts: Option<[u8; 7]>,
    pub second_counts: Option<[u8; 7]>,
    pub compact_second: bool,
}
impl Source {
    pub fn for_population(
        diagram: &mut Diagram,
        prepared: &super::super::population::PreparedPopulation,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        if prepared.stages.is_empty() {
            return Self::compile_all(diagram, &prepared.first, &prepared.second, control);
        }
        use clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet;
        let mut root = ALL;
        let mut offset = 0u16;
        let mut first_counts = Some([0u8; 7]);
        let mut final_root = NONE;
        let mut final_counts = None;
        let mut first_len = 0;
        for (i, universe) in prepared.stages.iter().enumerate() {
            cancelled(control)?;
            // The public queue-expression materializer is a uniform finite
            // universe. Check that contract instead of assuming weight = 1/N.
            if universe.weights()
                != &WeightedPatternSet::uniform(universe.pattern_count())
                    .map_err(|_| Error::PatternDomainUnavailable)?
            {
                return Err(Error::PatternDomainUnavailable);
            }
            let len =
                u16::try_from(universe.sequence_len_at(0)).map_err(|_| Error::CounterOverflow)?;
            let end = offset.checked_add(len).ok_or(Error::CounterOverflow)?;
            let (part, counts) = if let Some(atoms) = compact_atoms(universe) {
                let part = compile_atoms(
                    diagram,
                    &atoms,
                    0,
                    atoms[0].mask,
                    atoms[0].draws,
                    offset,
                    &mut HashMap::new(),
                    control,
                )?;
                let mut counts = [0u8; 7];
                let mut fixed = true;
                for atom in atoms {
                    fixed &= u32::from(atom.draws) == atom.mask.count_ones();
                    for (p, n) in counts.iter_mut().enumerate() {
                        *n = n
                            .checked_add(u8::from(atom.mask & (1 << p) != 0))
                            .ok_or(Error::CounterOverflow)?;
                    }
                }
                (part, fixed.then_some(counts))
            } else {
                let mut part = NONE;
                let mut counts = None;
                let mut same = true;
                for rank in 0..universe.pattern_count() {
                    cancelled(control)?;
                    let queue = universe.sequence_at(rank);
                    if queue.len() != usize::from(len) {
                        return Err(Error::PatternDomainUnavailable);
                    }
                    let branch = encode_queue(diagram, &queue, offset, ALL)?;
                    part = diagram.union(part, branch)?;
                    let inventory = inventory(&queue)?;
                    if let Some(old) = counts {
                        same &= old == inventory;
                    } else {
                        counts = Some(inventory);
                    }
                }
                (part, if same { counts } else { None })
            };
            if diagram.count(part, offset, end)? != universe.pattern_count() as u128 {
                return Err(Error::PatternDomainUnavailable);
            }
            root = diagram.intersect(root, part)?;
            if i + 1 == prepared.stages.len() {
                first_len = offset;
                final_root = part;
                final_counts = counts;
            } else {
                first_counts = match first_counts.zip(counts) {
                    Some((a, b)) => {
                        let mut sum = [0u8; 7];
                        for p in 0..7 {
                            sum[p] = a[p].checked_add(b[p]).ok_or(Error::CounterOverflow)?;
                        }
                        Some(sum)
                    }
                    None => None,
                };
            }
            offset = end;
        }
        if diagram.count(root, 0, offset)? != prepared.possible {
            return Err(Error::PatternDomainUnavailable);
        }
        Ok(Self {
            first_len,
            end: offset,
            universe: root,
            second: final_root,
            first_counts,
            second_counts: final_counts,
            compact_second: true,
        })
    }
    pub fn compile_all(
        diagram: &mut Diagram,
        first: &MaterializedPatternUniverse,
        second: &MaterializedPatternUniverse,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        let Some(atoms) = compact_atoms(first) else {
            return Self::compile(diagram, first, second, 0, first.pattern_count(), control);
        };
        let mut source = Self::compile(diagram, first, second, 0, 1, control)?;
        let root = compile_atoms(
            diagram,
            &atoms,
            0,
            atoms[0].mask,
            atoms[0].draws,
            0,
            &mut HashMap::new(),
            control,
        )?;
        if diagram.count(root, 0, source.first_len)? != first.pattern_count() as u128 {
            return Err(Error::PatternDomainUnavailable);
        }
        source.universe = diagram.intersect(root, source.second)?;
        let mut counts = [0_u8; 7];
        let mut fixed = true;
        for atom in &atoms {
            fixed &= u32::from(atom.draws) == atom.mask.count_ones();
            for (p, n) in counts.iter_mut().enumerate() {
                *n = n
                    .checked_add(u8::from(atom.mask & (1 << p) != 0))
                    .ok_or(Error::CounterOverflow)?;
            }
        }
        source.first_counts = fixed.then_some(counts);
        Ok(source)
    }
    pub fn compile(
        diagram: &mut Diagram,
        first: &MaterializedPatternUniverse,
        second: &MaterializedPatternUniverse,
        start: usize,
        count: usize,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        cancelled(control)?;
        let first_len =
            u16::try_from(first.sequence_len_at(start)).map_err(|_| Error::CounterOverflow)?;
        let second_len =
            u16::try_from(second.sequence_len_at(0)).map_err(|_| Error::CounterOverflow)?;
        let end = first_len
            .checked_add(second_len)
            .ok_or(Error::CounterOverflow)?;
        let atoms = compact_atoms(second);
        let (second_root, compact_second, second_counts) = if let Some(atoms) = atoms {
            let root = compile_atoms(
                diagram,
                &atoms,
                0,
                atoms[0].mask,
                atoms[0].draws,
                first_len,
                &mut HashMap::new(),
                control,
            )?;
            if diagram.count(root, first_len, end)? != second.pattern_count() as u128 {
                return Err(Error::PatternDomainUnavailable);
            }
            let mut counts = [0_u8; 7];
            let mut fixed = true;
            for atom in &atoms {
                fixed &= u32::from(atom.draws) == atom.mask.count_ones();
                for (p, n) in counts.iter_mut().enumerate() {
                    *n = n
                        .checked_add(u8::from(atom.mask & (1 << p) != 0))
                        .ok_or(Error::CounterOverflow)?;
                }
            }
            (root, true, fixed.then_some(counts))
        } else {
            let mut root = NONE;
            let mut counts = None;
            let mut same = true;
            for i in 0..second.pattern_count() {
                cancelled(control)?;
                let queue = second.sequence_at(i);
                if queue.len() != usize::from(second_len) {
                    return Err(Error::PatternDomainUnavailable);
                }
                let branch = encode_queue(diagram, &queue, first_len, ALL)?;
                root = diagram.union(root, branch)?;
                let c = inventory(&queue)?;
                if let Some(old) = counts {
                    same &= old == c;
                } else {
                    counts = Some(c);
                }
            }
            (root, false, if same { counts } else { None })
        };
        let mut universe = NONE;
        let mut first_counts = None;
        let mut same = true;
        for i in start..start.checked_add(count).ok_or(Error::CounterOverflow)? {
            cancelled(control)?;
            let queue = first.sequence_at(i);
            if queue.len() != usize::from(first_len) {
                return Err(Error::PatternDomainUnavailable);
            }
            let root = encode_queue(diagram, &queue, 0, second_root)?;
            universe = diagram.union(universe, root)?;
            let counts = inventory(&queue)?;
            if let Some(old) = first_counts {
                same &= old == counts;
            } else {
                first_counts = Some(counts);
            }
        }
        Ok(Self {
            first_len,
            end,
            universe,
            second: second_root,
            first_counts: if same { first_counts } else { None },
            second_counts,
            compact_second,
        })
    }
    pub fn follow_first(&self, diagram: &Diagram, mut language: Id, queue: &[PieceKind]) -> Id {
        for (level, &piece) in queue.iter().enumerate() {
            language = diagram.follow(language, level as u16, piece_index(piece));
        }
        language
    }
    pub fn accepts_second(&self, diagram: &Diagram, mut language: Id, queue: &[PieceKind]) -> bool {
        for (level, &piece) in queue.iter().enumerate() {
            language = diagram.follow(language, self.first_len + level as u16, piece_index(piece));
        }
        language == ALL
    }
}
fn inventory(queue: &[PieceKind]) -> Result<[u8; 7], Error> {
    let mut counts = [0_u8; 7];
    for &p in queue {
        counts[piece_index(p)] = counts[piece_index(p)]
            .checked_add(1)
            .ok_or(Error::CounterOverflow)?;
    }
    Ok(counts)
}
fn encode_queue(
    diagram: &mut Diagram,
    queue: &[PieceKind],
    offset: u16,
    mut tail: Id,
) -> Result<Id, Error> {
    for (level, &p) in queue.iter().enumerate().rev() {
        tail = diagram.prepend(offset + level as u16, piece_index(p), tail)?;
    }
    Ok(tail)
}
fn compile_atoms(
    diagram: &mut Diagram,
    atoms: &[Atom],
    atom: usize,
    mask: u8,
    left: u16,
    level: u16,
    memo: &mut HashMap<(usize, u8, u16), Id>,
    control: &ExecutionControl,
) -> Result<Id, Error> {
    cancelled(control)?;
    if left == 0 {
        return if atom + 1 == atoms.len() {
            Ok(ALL)
        } else {
            compile_atoms(
                diagram,
                atoms,
                atom + 1,
                atoms[atom + 1].mask,
                atoms[atom + 1].draws,
                level,
                memo,
                control,
            )
        };
    }
    if let Some(&id) = memo.get(&(atom, mask, left)) {
        return Ok(id);
    }
    let mut children = [NONE; 7];
    for (p, child) in children.iter_mut().enumerate() {
        if mask & (1 << p) != 0 {
            *child = compile_atoms(
                diagram,
                atoms,
                atom,
                mask & !(1 << p),
                left - 1,
                level + 1,
                memo,
                control,
            )?;
        }
    }
    let id = diagram.branch(level, children)?;
    memo.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
    memo.insert((atom, mask, left), id);
    Ok(id)
}

fn compact_atoms(universe: &MaterializedPatternUniverse) -> Option<Vec<Atom>> {
    match universe.uniform_compact_source() {
        Some(UniformCompactPatternSource::FactorizedExpression(shape))
            if shape.full_sequence_len() == shape.visible_sequence_len() =>
        {
            Some(
                shape
                    .atoms()
                    .map(|a| Atom {
                        mask: a
                            .choices()
                            .iter()
                            .fold(0, |m, &p| m | (1 << piece_index(p))),
                        draws: a.draw_count() as u16,
                    })
                    .collect::<Vec<_>>(),
            )
        }
        Some(UniformCompactPatternSource::Standard7Bag { sequence_len, .. }) => {
            let mut atoms = Vec::new();
            let mut left = sequence_len;
            while left > 0 {
                atoms.push(Atom {
                    mask: 127,
                    draws: left.min(7) as u16,
                });
                left = left.saturating_sub(7);
            }
            Some(atoms)
        }
        _ => None,
    }
}
