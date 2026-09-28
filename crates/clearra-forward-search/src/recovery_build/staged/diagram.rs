//! Reduced ordered seven-way decision diagrams of INPUT queues. Alternatives
//! of legal executions are ORed; drawing a piece is a disjoint input decision.
//! A terminal accepts an entire unread suffix, not one representative queue.
use super::super::RecoveryBuildError as Error;
use std::collections::HashMap;

pub(in crate::recovery_build) type Id = u32;
pub(in crate::recovery_build) const NONE: Id = 0;
pub(in crate::recovery_build) const ALL: Id = 1;
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Node {
    level: u16,
    children: [Id; 7],
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Op {
    Union,
    Intersect,
    Difference,
}
#[derive(Clone, Default)]
pub(in crate::recovery_build) struct Diagram {
    nodes: Vec<Node>,
    unique: HashMap<Node, Id>,
    apply_memo: HashMap<(Op, Id, Id), Id>,
    count_memo: HashMap<(Id, u16, u16), u128>,
}
impl Diagram {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    fn level(&self, id: Id) -> u16 {
        if id < 2 {
            u16::MAX
        } else {
            self.nodes[(id - 2) as usize].level
        }
    }
    pub fn follow(&self, id: Id, level: u16, piece: usize) -> Id {
        if self.level(id) == level {
            self.nodes[(id - 2) as usize].children[piece]
        } else {
            debug_assert!(self.level(id) > level);
            id
        }
    }
    pub fn branch(&mut self, level: u16, children: [Id; 7]) -> Result<Id, Error> {
        if children.iter().all(|&id| id == children[0]) {
            return Ok(children[0]);
        }
        if children.iter().any(|&id| self.level(id) <= level) {
            return Err(Error::PatternDomainUnavailable);
        }
        let node = Node { level, children };
        if let Some(&id) = self.unique.get(&node) {
            return Ok(id);
        }
        let id = u32::try_from(self.nodes.len())
            .ok()
            .and_then(|n| n.checked_add(2))
            .ok_or(Error::CounterOverflow)?;
        self.nodes
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.unique
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.nodes.push(node);
        self.unique.insert(node, id);
        Ok(id)
    }
    pub fn prepend(&mut self, level: u16, piece: usize, child: Id) -> Result<Id, Error> {
        let mut children = [NONE; 7];
        children[piece] = child;
        self.branch(level, children)
    }
    pub fn union(&mut self, a: Id, b: Id) -> Result<Id, Error> {
        self.apply(Op::Union, a, b)
    }
    pub fn intersect(&mut self, a: Id, b: Id) -> Result<Id, Error> {
        self.apply(Op::Intersect, a, b)
    }
    pub fn difference(&mut self, a: Id, b: Id) -> Result<Id, Error> {
        self.apply(Op::Difference, a, b)
    }
    fn apply(&mut self, op: Op, mut a: Id, mut b: Id) -> Result<Id, Error> {
        match op {
            Op::Union => {
                if a == ALL || b == ALL {
                    return Ok(ALL);
                }
                if a == NONE || a == b {
                    return Ok(b);
                }
                if b == NONE {
                    return Ok(a);
                }
            }
            Op::Intersect => {
                if a == NONE || b == NONE {
                    return Ok(NONE);
                }
                if a == ALL || a == b {
                    return Ok(b);
                }
                if b == ALL {
                    return Ok(a);
                }
            }
            Op::Difference => {
                if a == NONE || b == ALL || a == b {
                    return Ok(NONE);
                }
                if b == NONE {
                    return Ok(a);
                }
            }
        }
        if op != Op::Difference && a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if let Some(&id) = self.apply_memo.get(&(op, a, b)) {
            return Ok(id);
        }
        let level = self.level(a).min(self.level(b));
        if level == u16::MAX {
            return Err(Error::PatternDomainUnavailable);
        }
        let mut children = [NONE; 7];
        for (piece, child) in children.iter_mut().enumerate() {
            *child = self.apply(
                op,
                self.follow(a, level, piece),
                self.follow(b, level, piece),
            )?;
        }
        let id = self.branch(level, children)?;
        self.apply_memo
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.apply_memo.insert((op, a, b), id);
        Ok(id)
    }
    pub fn count(&mut self, id: Id, level: u16, end: u16) -> Result<u128, Error> {
        if level > end {
            return Err(Error::PatternDomainUnavailable);
        }
        if id == NONE {
            return Ok(0);
        }
        if level == end {
            return if id == ALL {
                Ok(1)
            } else {
                Err(Error::PatternDomainUnavailable)
            };
        }
        if let Some(&count) = self.count_memo.get(&(id, level, end)) {
            return Ok(count);
        }
        let count = if id == ALL {
            7_u128
                .checked_pow(u32::from(end - level))
                .ok_or(Error::CounterOverflow)?
        } else {
            let next_level = self.level(id);
            if next_level < level || next_level >= end {
                return Err(Error::PatternDomainUnavailable);
            }
            let mut count = 0_u128;
            for piece in 0..7 {
                count = count
                    .checked_add(self.count(
                        self.follow(id, next_level, piece),
                        next_level + 1,
                        end,
                    )?)
                    .ok_or(Error::CounterOverflow)?;
            }
            count
                .checked_mul(
                    7_u128
                        .checked_pow(u32::from(next_level - level))
                        .ok_or(Error::CounterOverflow)?,
                )
                .ok_or(Error::CounterOverflow)?
        };
        self.count_memo
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.count_memo.insert((id, level, end), count);
        Ok(count)
    }
}

/// Value-only DAG, postorder indexed. A task never transports native node IDs.
#[derive(Clone, Debug)]
pub(in crate::recovery_build) struct DiagramPacket {
    pub nodes: Vec<(u16, [u32; 7])>,
    pub roots: [u32; 2],
}
impl Diagram {
    pub fn export(&self, roots: [Id; 2]) -> Result<DiagramPacket, Error> {
        fn visit(
            d: &Diagram,
            id: Id,
            nodes: &mut Vec<(u16, [u32; 7])>,
            seen: &mut HashMap<Id, Id>,
        ) -> Result<Id, Error> {
            if id < 2 {
                return Ok(id);
            }
            if let Some(&r) = seen.get(&id) {
                return Ok(r);
            }
            let node = d
                .nodes
                .get((id - 2) as usize)
                .ok_or(Error::PatternDomainUnavailable)?;
            let mut children = [0; 7];
            for (out, &child) in children.iter_mut().zip(node.children.iter()) {
                *out = visit(d, child, nodes, seen)?;
            }
            let next = u32::try_from(nodes.len())
                .ok()
                .and_then(|v| v.checked_add(2))
                .ok_or(Error::CounterOverflow)?;
            nodes.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            seen.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            nodes.push((node.level, children));
            seen.insert(id, next);
            Ok(next)
        }
        let mut nodes = Vec::new();
        let mut seen = HashMap::new();
        let mut mapped = [0; 2];
        for (out, root) in mapped.iter_mut().zip(roots) {
            *out = visit(self, root, &mut nodes, &mut seen)?;
        }
        Ok(DiagramPacket {
            nodes,
            roots: mapped,
        })
    }
    pub fn import(&mut self, packet: &DiagramPacket, end: u16) -> Result<[Id; 2], Error> {
        let mut ids = vec![NONE, ALL];
        ids.try_reserve(packet.nodes.len())
            .map_err(|_| Error::MemoryUnavailable)?;
        for (level, children) in &packet.nodes {
            if *level >= end {
                return Err(Error::PatternDomainUnavailable);
            }
            let mut mapped = [0; 7];
            for (out, &child) in mapped.iter_mut().zip(children) {
                *out = *ids
                    .get(child as usize)
                    .ok_or(Error::PatternDomainUnavailable)?;
                if self.level(*out) <= *level {
                    return Err(Error::PatternDomainUnavailable);
                }
            }
            ids.push(self.branch(*level, mapped)?);
        }
        Ok([
            *ids.get(packet.roots[0] as usize)
                .ok_or(Error::PatternDomainUnavailable)?,
            *ids.get(packet.roots[1] as usize)
                .ok_or(Error::PatternDomainUnavailable)?,
        ])
    }
}

impl Diagram {
    /// Quotient the input language by identical candidate support. All branches
    /// are visited, with memoized state vectors; no sampled input or 25M rows.
    pub fn support_classes(
        &self,
        roots: &[Id],
        control: &clearra_core_domain::execution_cancellation::ExecutionControl,
    ) -> Result<Vec<Vec<usize>>, Error> {
        use std::collections::{BTreeSet, HashSet};
        let mut pending = vec![roots.to_vec()];
        let mut visited = HashSet::new();
        let mut classes = BTreeSet::new();
        while let Some(state) = pending.pop() {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if visited.contains(&state) {
                continue;
            }
            let level = state
                .iter()
                .map(|&id| self.level(id))
                .min()
                .unwrap_or(u16::MAX);
            if level == u16::MAX {
                let support = state
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &id)| (id == ALL).then_some(i))
                    .collect::<Vec<_>>();
                if !support.is_empty() {
                    classes.insert(support);
                }
            } else {
                pending
                    .try_reserve(7)
                    .map_err(|_| Error::MemoryUnavailable)?;
                for piece in 0..7 {
                    pending.push(
                        state
                            .iter()
                            .map(|&id| self.follow(id, level, piece))
                            .collect(),
                    );
                }
            }
            visited
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            visited.insert(state);
        }
        Ok(classes.into_iter().collect())
    }
}
