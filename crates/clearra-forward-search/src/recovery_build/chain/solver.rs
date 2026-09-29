//! Iterative product of physical Build states and unread input languages. Every
//! draw consumes its actual token; a stage boundary never refills or clears hold.
use super::super::{
    staged::diagram::{Diagram, Id, ALL, NONE},
    RecoveryBuildFixedReport, RecoveryBuildStatus,
};
use super::*;
use super::{
    geometry::{Edge, Geometry},
    plan::Plan,
    source::Source,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Token {
    index: u16,
    piece: u8,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    placed: u64,
    source: Id,
    depth: u16,
    active: Option<Token>,
    hold: Option<Token>,
    can_hold: bool,
    used: Vec<u8>,
    counts: Vec<[u8; 7]>,
    early: Vec<u8>,
    b2b: bool,
}
#[derive(Clone, Copy)]
enum Map {
    Identity,
    Draw {
        level: u16,
        piece: u8,
    },
    Place {
        from: u64,
        edge: Edge,
        token: Token,
        decision: &'static str,
        b2b: bool,
    },
}
struct Action {
    key: Key,
    map: Map,
}
struct Frame {
    key: Key,
    actions: std::vec::IntoIter<Action>,
    waiting: Option<Map>,
    accepted: [Id; 2],
}
enum Prepared {
    Terminal([Id; 2]),
    Actions(Vec<Action>),
}
pub(super) struct Solver {
    pub diagram: Diagram,
    pub source: Source,
    pub geometry: Geometry,
    query: RecoveryBuildQuery,
    target_counts: Vec<[u8; 7]>,
    maximum: Vec<usize>,
    root: Key,
    memo: HashMap<Key, [Id; 2]>,
    frames: Vec<Frame>,
    pending: Option<Key>,
    returned: Option<[Id; 2]>,
    pub result: Option<[Id; 2]>,
    pub states: u128,
}
impl Solver {
    pub fn new(
        q: RecoveryBuildQuery,
        plan: Plan,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        let mut diagram = Diagram::default();
        let source = Source::new(&q, &mut diagram, control)?;
        let target_counts = plan.counts();
        let maximum = (0..q.stages.len() - 1)
            .map(|b| {
                let n = plan.groups[b + 1..].iter().map(Vec::len).sum();
                q.early_limit.effective_max(n, n)
            })
            .collect();
        let root = Key {
            placed: 0,
            source: source.universe,
            depth: 0,
            active: None,
            hold: None,
            can_hold: true,
            used: vec![0; q.stages.len()],
            counts: if q.allow_piece_exchange {
                Vec::new()
            } else {
                vec![[0; 7]; q.stages.len()]
            },
            early: vec![0; q.stages.len() - 1],
            b2b: q.initial_b2b,
        };
        let geometry = Geometry::new(q.clone(), plan)?;
        Ok(Self {
            diagram,
            source,
            geometry,
            query: q,
            target_counts,
            maximum,
            root: root.clone(),
            memo: HashMap::new(),
            frames: Vec::new(),
            pending: Some(root),
            returned: None,
            result: None,
            states: 0,
        })
    }
    pub fn restrict(&mut self, queues: &[Vec<PieceKind>]) -> Result<(), Error> {
        if self.states != 0 {
            return Err(Error::PatternDomainUnavailable);
        }
        let mut word = ALL;
        for (i, p) in queues
            .iter()
            .flatten()
            .enumerate()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            word = self
                .diagram
                .prepend(i as u16, super::super::search::piece_index(*p), word)?;
        }
        self.root.source = self.diagram.intersect(word, self.source.universe)?;
        self.pending = Some(self.root.clone());
        Ok(())
    }
    fn map(&mut self, ids: [Id; 2], map: Map) -> Result<[Id; 2], Error> {
        if let Map::Draw { level, piece } = map {
            Ok([
                self.diagram.prepend(level, usize::from(piece), ids[0])?,
                self.diagram.prepend(level, usize::from(piece), ids[1])?,
            ])
        } else {
            Ok(ids)
        }
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<bool, Error> {
        for _ in 0..1024 {
            cancelled(control)?;
            if self.result.is_some() {
                return Ok(true);
            }
            if let Some(key) = self.pending.take() {
                if let Some(&v) = self.memo.get(&key) {
                    self.returned = Some(v);
                    continue;
                }
                self.states = self.states.checked_add(1).ok_or(Error::CounterOverflow)?;
                match self.actions(&key, control)? {
                    Prepared::Terminal(v) => {
                        self.memo
                            .try_reserve(1)
                            .map_err(|_| Error::MemoryUnavailable)?;
                        self.memo.insert(key, v);
                        self.returned = Some(v);
                    }
                    Prepared::Actions(actions) => {
                        self.frames
                            .try_reserve(1)
                            .map_err(|_| Error::MemoryUnavailable)?;
                        self.frames.push(Frame {
                            key,
                            actions: actions.into_iter(),
                            waiting: None,
                            accepted: [NONE; 2],
                        });
                    }
                }
                continue;
            }
            if let Some(v) = self.returned.take() {
                if self.frames.is_empty() {
                    self.result = Some(v);
                    continue;
                }
                let map = self
                    .frames
                    .last_mut()
                    .and_then(|f| f.waiting.take())
                    .ok_or(Error::PatternDomainUnavailable)?;
                let v = self.map(v, map)?;
                let frame = self
                    .frames
                    .last_mut()
                    .ok_or(Error::PatternDomainUnavailable)?;
                for (accepted, id) in frame.accepted.iter_mut().zip(v) {
                    *accepted = self.diagram.union(*accepted, id)?;
                }
                continue;
            }
            let frame = self
                .frames
                .last_mut()
                .ok_or(Error::PatternDomainUnavailable)?;
            // Only full NORMAL coverage can terminate alternative player paths;
            // a repair witness must never hide a normal realization.
            let action = if frame.accepted[0] == frame.key.source {
                None
            } else {
                frame.actions.next()
            };
            if let Some(action) = action {
                frame.waiting = Some(action.map);
                self.pending = Some(action.key);
            } else {
                let frame = self.frames.pop().ok_or(Error::PatternDomainUnavailable)?;
                self.memo
                    .try_reserve(1)
                    .map_err(|_| Error::MemoryUnavailable)?;
                self.memo.insert(frame.key, frame.accepted);
                self.returned = Some(frame.accepted);
            }
        }
        Ok(self.result.is_some())
    }
    fn actions(&mut self, key: &Key, control: &ExecutionControl) -> Result<Prepared, Error> {
        if key.source == NONE {
            return Ok(Prepared::Terminal([NONE; 2]));
        }
        if key.placed == self.geometry.all() {
            let valid = key
                .used
                .iter()
                .zip(&self.geometry.plan.groups)
                .all(|(&n, g)| usize::from(n) == g.len())
                && (self.query.allow_piece_exchange || key.counts == self.target_counts);
            let mut out = [NONE; 2];
            if valid {
                out[usize::from(key.early.iter().any(|&n| n != 0))] = key.source;
            }
            return Ok(Prepared::Terminal(out));
        }
        let mut actions = Vec::new();
        if let Some(active) = key.active {
            self.placements(
                key,
                active,
                key.hold,
                if key.can_hold { "none" } else { "store" },
                &mut actions,
                control,
            )?;
            if key.can_hold && self.query.hold_enabled {
                if let Some(held) = key.hold {
                    self.placements(key, held, Some(active), "swap", &mut actions, control)?;
                } else if key.depth < self.source.end() {
                    let mut child = key.clone();
                    child.active = None;
                    child.hold = Some(active);
                    child.can_hold = false;
                    actions.push(Action {
                        key: child,
                        map: Map::Identity,
                    });
                }
            }
        } else if key.depth < self.source.end() {
            for p in 0..7 {
                let source = self.diagram.follow(key.source, key.depth, p);
                if source == NONE {
                    continue;
                }
                let mut child = key.clone();
                child.source = source;
                child.active = Some(Token {
                    index: key.depth,
                    piece: p as u8,
                });
                child.depth += 1;
                actions.push(Action {
                    key: child,
                    map: Map::Draw {
                        level: key.depth,
                        piece: p as u8,
                    },
                });
            }
        } else if let Some(held) = key.hold {
            self.placements(
                key,
                held,
                None,
                "release-held-at-terminal",
                &mut actions,
                control,
            )?;
        }
        Ok(Prepared::Actions(actions))
    }
    fn placements(
        &mut self,
        key: &Key,
        token: Token,
        held: Option<Token>,
        decision: &'static str,
        out: &mut Vec<Action>,
        control: &ExecutionControl,
    ) -> Result<(), Error> {
        let origin = self.source.origin(token.index);
        let piece = usize::from(token.piece);
        if usize::from(key.used[origin]) >= self.geometry.plan.groups[origin].len() {
            return Ok(());
        }
        if !self.query.allow_piece_exchange
            && key.counts[origin][piece] >= self.target_counts[origin][piece]
        {
            return Ok(());
        }
        let prefix = self.geometry.position(key.placed)?.prefix;
        for &edge in self
            .geometry
            .edges(key.placed, token.piece, control)?
            .iter()
        {
            let mut child = key.clone();
            let mut valid = true;
            for b in prefix..edge.stage {
                if usize::from(child.early[b]) >= self.maximum[b] {
                    valid = false;
                    break;
                }
                child.early[b] += 1;
            }
            if !valid {
                continue;
            }
            child.placed |= 1 << edge.tile;
            child.active = None;
            child.hold = held;
            child.can_hold = true;
            child.used[origin] += 1;
            if !self.query.allow_piece_exchange {
                child.counts[origin][piece] += 1;
            }
            if edge.lines > 0 {
                child.b2b = edge.lines == 4 || edge.board.is_empty() || edge.spin;
            }
            let b2b = child.b2b;
            out.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            out.push(Action {
                key: child,
                map: Map::Place {
                    from: key.placed,
                    edge,
                    token,
                    decision,
                    b2b,
                },
            });
        }
        Ok(())
    }
    fn accepts(&self, mut id: Id, depth: u16, queue: &[PieceKind]) -> bool {
        for (level, p) in queue.iter().enumerate().skip(usize::from(depth)) {
            id = self
                .diagram
                .follow(id, level as u16, super::super::search::piece_index(*p));
        }
        id == ALL
    }
    pub fn witness(
        &mut self,
        queues: Vec<Vec<PieceKind>>,
        kind: usize,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildFixedReport, Error> {
        let pattern_indices = self.source.indices(&queues, control)?;
        let word = queues.iter().flatten().copied().collect::<Vec<_>>();
        let mut key = self.root.clone();
        let mut steps = Vec::new();
        let mut stages = Vec::new();
        loop {
            cancelled(control)?;
            match self.actions(&key, control)? {
                Prepared::Terminal(ids) => {
                    if !self.accepts(ids[kind], key.depth, &word) {
                        return Err(Error::PatternDomainUnavailable);
                    }
                    let board = self.geometry.position(key.placed)?.board;
                    let maximum = *self.maximum.iter().max().unwrap_or(&0);
                    let actual = *key.early.iter().max().unwrap_or(&0);
                    let mut exchange = [0_i16; 7];
                    for s in &steps {
                        let s: &super::super::RecoveryBuildStep = s;
                        let p = super::super::search::piece_index(s.piece);
                        exchange[p] +=
                            i16::from(s.source_index < usize::from(self.source.offsets[1]))
                                - i16::from(!s.result_target);
                    }
                    return Ok(RecoveryBuildFixedReport {
                        status: if kind == 0 {
                            RecoveryBuildStatus::Normal
                        } else {
                            RecoveryBuildStatus::Recovery
                        },
                        states: usize::try_from(self.states).map_err(|_| Error::CounterOverflow)?,
                        effective_max_early: maximum,
                        actual_early: usize::from(actual),
                        exchange_balance: exchange,
                        steps,
                        terminal_board: board.words(),
                        middle_target: self.geometry.plan.targets[0].words(),
                        result_target: self
                            .geometry
                            .plan
                            .targets
                            .last()
                            .ok_or(Error::PatternDomainUnavailable)?
                            .words(),
                        chain: Some(RecoveryChainWitness {
                            targets: self
                                .geometry
                                .plan
                                .targets
                                .iter()
                                .map(|m| m.words())
                                .collect(),
                            queues,
                            pattern_indices,
                            placement_stages: stages,
                            early_by_boundary: key.early,
                        }),
                    });
                }
                Prepared::Actions(actions) => {
                    let mut chosen = None;
                    for action in actions {
                        if let Some(&ids) = self.memo.get(&action.key) {
                            let ids = self.map(ids, action.map)?;
                            if self.accepts(ids[kind], key.depth, &word) {
                                chosen = Some(action);
                                break;
                            }
                        }
                    }
                    let action = chosen.ok_or(Error::PatternDomainUnavailable)?;
                    if let Map::Place {
                        from,
                        edge,
                        token,
                        decision,
                        b2b,
                    } = action.map
                    {
                        steps.push(self.geometry.step(
                            from,
                            edge,
                            token.index,
                            token.piece,
                            decision,
                            b2b,
                        )?);
                        stages.push(edge.stage as u8);
                    }
                    key = action.key;
                }
            }
        }
    }
}
