//! Incremental product of stage Build states and unread supply languages.
//! Middle completion delegates to a separate, unrestricted suffix memo. Repair
//! explores only input cylinders not already covered by the normal stages.
use super::super::{
    search::piece_index, RecoveryBuildError as Error, RecoveryBuildFixedReport, RecoveryBuildQuery,
    RecoveryBuildStatus,
};
use super::{
    diagram::{Diagram, Id, ALL, NONE},
    geometry::{Edge, Geometry},
    source::{cancelled, Source},
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Token {
    index: u16,
    piece: u8,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Mode {
    Middle,
    Repair,
    Tail,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Key {
    geometry: u32,
    chain: u32,
    source: Id,
    allowed: Id,
    depth: u16,
    active: Option<Token>,
    hold: Option<Token>,
    can_hold: bool,
    first_used: u8,
    early: u8,
    exchange: [i16; 7],
    middle_counts: [u8; 7],
    result_counts: [u8; 7],
    mode: Mode,
}
#[derive(Clone, Copy)]
enum Map {
    Identity,
    Draw {
        level: u16,
        piece: u8,
    },
    Tail {
        allowed: Id,
    },
    Place {
        from: u32,
        edge: Edge,
        token: Token,
        decision: &'static str,
    },
}
#[derive(Clone, Copy)]
struct Action {
    child: Key,
    map: Map,
}
struct Frame {
    key: Key,
    actions: std::vec::IntoIter<Action>,
    waiting: Option<Map>,
    accepted: Id,
    limit: Id,
}
struct Machine {
    frames: Vec<Frame>,
    pending: Option<Key>,
    returned: Option<Id>,
    result: Option<Id>,
}
impl Machine {
    fn new(root: Key) -> Self {
        Self {
            frames: Vec::new(),
            pending: Some(root),
            returned: None,
            result: None,
        }
    }
}
struct Root {
    key: Key,
    language: Id,
    status: RecoveryBuildStatus,
}

pub(in crate::recovery_build) struct Solver {
    plan: Option<super::super::catalog::Plan>,
    pub diagram: Diagram,
    pub source: Source,
    pub geometry: Geometry,
    query: RecoveryBuildQuery,
    maximum: usize,
    chain_lengths: Vec<u16>,
    chain_states: Vec<super::super::chain::Progress>,
    chain_unique: HashMap<super::super::chain::Progress, u32>,
    memo: HashMap<Key, Id>,
    machine: Option<Machine>,
    root: Option<Key>,
    roots: Vec<Root>,
    orientation: usize,
    repairing: bool,
    pub normal: Id,
    pub recovery: Id,
    pub states: u128,
    pub middle_states: u128,
    pub tail_states: u128,
    pub repair_states: u128,
    pub suffix_hits: u128,
    pub drawn_prefixes: u128,
    pub done: bool,
}
impl Solver {
    pub fn new(
        query: RecoveryBuildQuery,
        source: Source,
        diagram: Diagram,
        geometry: Geometry,
    ) -> Self {
        let chain_lengths = super::super::chain::lengths(&query).expect("validated chain lengths");
        let future = if query.chain_stages.is_empty() {
            geometry.stages[0].prepared.result_pieces
        } else {
            query.chain_stages[1..]
                .iter()
                .map(|s| s.target.count_ones() as usize / 4)
                .sum()
        };
        let maximum = query.early_limit.effective_max(future, future);
        let initial_progress = super::super::chain::Progress::new(query.chain_stages.len());
        Self {
            chain_lengths,
            chain_states: vec![initial_progress.clone()],
            chain_unique: HashMap::from([(initial_progress, 0)]),
            plan: None,
            diagram,
            source,
            geometry,
            query,
            maximum,
            memo: HashMap::new(),
            machine: None,
            root: None,
            roots: Vec::new(),
            orientation: 0,
            repairing: false,
            normal: NONE,
            recovery: NONE,
            states: 0,
            middle_states: 0,
            tail_states: 0,
            repair_states: 0,
            suffix_hits: 0,
            drawn_prefixes: 0,
            done: false,
        }
    }
    pub fn with_plan(mut self, plan: super::super::catalog::Plan) -> Self {
        self.plan = Some(plan);
        self
    }
    fn orientation_count(&self) -> usize {
        if self.plan.is_some() {
            1
        } else {
            self.geometry.roots.len()
        }
    }
    fn geometry_root(&self) -> u32 {
        self.geometry.roots[self
            .plan
            .as_ref()
            .map_or(self.orientation, |p| p.orientation as usize)]
    }
    /// A finite amount of solver work, not one monolithic fixed queue pair.
    pub fn advance(&mut self, fuel: usize, control: &ExecutionControl) -> Result<bool, Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if self.done {
                return Ok(true);
            }
            if self.machine.is_none() {
                if self.orientation == self.orientation_count() {
                    if self.repairing || self.maximum == 0 {
                        self.done = true;
                        continue;
                    }
                    self.orientation = 0;
                    self.repairing = true;
                }
                let covered = if self.repairing {
                    self.diagram.union(self.normal, self.recovery)?
                } else {
                    self.normal
                };
                let allowed = self.diagram.difference(self.source.universe, covered)?;
                if allowed == NONE {
                    self.orientation = self.orientation_count();
                    continue;
                }
                let key = Key {
                    geometry: self.geometry_root(),
                    chain: 0,
                    source: self.source.universe,
                    allowed,
                    depth: 0,
                    active: None,
                    hold: None,
                    can_hold: true,
                    first_used: 0,
                    early: 0,
                    exchange: [0; 7],
                    middle_counts: [0; 7],
                    result_counts: [0; 7],
                    mode: if self.repairing {
                        Mode::Repair
                    } else {
                        Mode::Middle
                    },
                };
                self.root = Some(key);
                self.machine = Some(Machine::new(key));
            }
            let mut machine = self.machine.take().ok_or(Error::PatternDomainUnavailable)?;
            self.tick(&mut machine, control)?;
            if let Some(language) = machine.result {
                let status = if self.repairing {
                    RecoveryBuildStatus::Recovery
                } else {
                    RecoveryBuildStatus::Normal
                };
                self.roots.push(Root {
                    key: self.root.take().ok_or(Error::PatternDomainUnavailable)?,
                    language,
                    status,
                });
                if self.repairing {
                    self.recovery = self.diagram.union(self.recovery, language)?;
                } else {
                    self.normal = self.diagram.union(self.normal, language)?;
                }
                self.orientation += 1;
            } else {
                self.machine = Some(machine);
            }
        }
        Ok(self.done)
    }
    fn tick(&mut self, machine: &mut Machine, control: &ExecutionControl) -> Result<(), Error> {
        if let Some(key) = machine.pending.take() {
            if let Some(&value) = self.memo.get(&key) {
                if key.mode == Mode::Tail {
                    self.suffix_hits += 1;
                }
                machine.returned = Some(value);
                return Ok(());
            }
            self.states += 1;
            match key.mode {
                Mode::Middle => self.middle_states += 1,
                Mode::Repair => self.repair_states += 1,
                Mode::Tail => self.tail_states += 1,
            }
            let limit = self.diagram.intersect(key.source, key.allowed)?;
            let actions = self.actions(key, control)?;
            match actions {
                Prepared::Terminal(value) => {
                    self.remember(key, value)?;
                    machine.returned = Some(value);
                }
                Prepared::Actions(actions) => {
                    machine
                        .frames
                        .try_reserve(1)
                        .map_err(|_| Error::MemoryUnavailable)?;
                    machine.frames.push(Frame {
                        key,
                        actions: actions.into_iter(),
                        waiting: None,
                        accepted: NONE,
                        limit,
                    });
                }
            }
            return Ok(());
        }
        if let Some(value) = machine.returned.take() {
            if let Some(frame) = machine.frames.last_mut() {
                let value = self.map(
                    value,
                    frame
                        .waiting
                        .take()
                        .ok_or(Error::PatternDomainUnavailable)?,
                )?;
                frame.accepted = self.diagram.union(frame.accepted, value)?;
            } else {
                machine.result = Some(value);
            }
            return Ok(());
        }
        let frame = machine
            .frames
            .last_mut()
            .ok_or(Error::PatternDomainUnavailable)?;
        // Once every still-admissible unread queue is certified, other player
        // choices cannot add coverage. This is not a sampled-prefix shortcut.
        let next = if frame.accepted == frame.limit {
            None
        } else {
            frame.actions.next()
        };
        if let Some(action) = next {
            frame.waiting = Some(action.map);
            machine.pending = Some(action.child);
        } else {
            let frame = machine
                .frames
                .pop()
                .ok_or(Error::PatternDomainUnavailable)?;
            self.remember(frame.key, frame.accepted)?;
            machine.returned = Some(frame.accepted);
        }
        Ok(())
    }
    fn remember(&mut self, key: Key, value: Id) -> Result<(), Error> {
        self.memo
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.memo.insert(key, value);
        Ok(())
    }
    fn map(&mut self, value: Id, map: Map) -> Result<Id, Error> {
        match map {
            Map::Draw { level, piece } => self.diagram.prepend(level, usize::from(piece), value),
            Map::Tail { allowed } => self.diagram.intersect(value, allowed),
            Map::Identity | Map::Place { .. } => Ok(value),
        }
    }
    fn actions(&mut self, key: Key, control: &ExecutionControl) -> Result<Prepared, Error> {
        let limit = self.diagram.intersect(key.source, key.allowed)?;
        if limit == NONE {
            return Ok(Prepared::Terminal(NONE));
        }
        let pos = self.geometry.position(key.geometry);
        let stage = &self.geometry.stages[pos.stage as usize];
        let middle_pieces = stage.prepared.middle_pieces;
        let result_pieces = stage.prepared.result_pieces;
        if pos.middle_count() == middle_pieces && pos.result_count() == result_pieces {
            let valid = pos.board == stage.prepared.terminal
                && usize::from(key.first_used) == middle_pieces
                && (self.query.allow_piece_exchange || key.exchange == [0; 7])
                && (key.mode != Mode::Repair
                    || key.early > 0
                    || self.chain_states[key.chain as usize].has_early())
                && self.chain_terminal(key);
            return Ok(Prepared::Terminal(if valid { limit } else { NONE }));
        }
        if pos.middle_count() == middle_pieces && key.mode != Mode::Tail {
            if key.mode == Mode::Repair
                && key.early == 0
                && !self.chain_states[key.chain as usize].has_early()
            {
                return Ok(Prepared::Terminal(NONE));
            }
            // Reuse the exact second-stage continuation independently of which
            // first-stage queue/path reached it. Keep source, held token,
            // repayment, deleted rows, physical board and B2B in the key.
            let child = Key {
                allowed: ALL,
                early: 0,
                mode: Mode::Tail,
                ..key
            };
            return Ok(Prepared::Actions(vec![Action {
                child,
                map: Map::Tail {
                    allowed: key.allowed,
                },
            }]));
        }
        let Some((middle_caps, result_caps)) = self.caps(key, middle_pieces, result_pieces) else {
            return Ok(Prepared::Terminal(NONE));
        };
        let inventory = if self.query.allow_piece_exchange
            && usize::from(self.source.end) == middle_pieces + result_pieces
        {
            match (self.source.first_counts, self.source.second_counts) {
                (Some(first), Some(second)) => {
                    let mut remaining = [0_u8; 7];
                    for p in 0..7 {
                        let Some(value) = first[p]
                            .checked_add(second[p])
                            .and_then(|v| v.checked_sub(key.middle_counts[p]))
                            .and_then(|v| v.checked_sub(key.result_counts[p]))
                        else {
                            return Ok(Prepared::Terminal(NONE));
                        };
                        remaining[p] = value;
                    }
                    Some(remaining)
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(plan) = &self.plan {
            // The catalog already fixes both complete inventories. Do not run
            // another unconstrained exact-cover search at each physical node.
            if !plan.inventory_allows(
                key.middle_counts,
                key.result_counts,
                middle_caps,
                result_caps,
            ) {
                return Ok(Prepared::Terminal(NONE));
            }
        } else if !self.geometry.feasible(
            key.geometry,
            middle_caps,
            result_caps,
            inventory,
            control,
        )? {
            return Ok(Prepared::Terminal(NONE));
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
                } else if key.depth < self.source.end {
                    actions.push(Action {
                        child: Key {
                            active: None,
                            hold: Some(active),
                            can_hold: false,
                            ..key
                        },
                        map: Map::Identity,
                    });
                }
            }
        } else if key.depth < self.source.end {
            for piece in 0..7 {
                let source = self.diagram.follow(key.source, key.depth, piece);
                let allowed = self.diagram.follow(key.allowed, key.depth, piece);
                if source == NONE || allowed == NONE {
                    continue;
                }
                self.drawn_prefixes += 1;
                let child = Key {
                    active: Some(Token {
                        index: key.depth,
                        piece: piece as u8,
                    }),
                    depth: key.depth + 1,
                    source,
                    allowed,
                    ..key
                };
                actions
                    .try_reserve(1)
                    .map_err(|_| Error::MemoryUnavailable)?;
                actions.push(Action {
                    child,
                    map: Map::Draw {
                        level: key.depth,
                        piece: piece as u8,
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
    fn caps(
        &self,
        key: Key,
        middle_pieces: usize,
        result_pieces: usize,
    ) -> Option<([u8; 7], [u8; 7])> {
        let pos = self.geometry.position(key.geometry);
        let mut middle = [(middle_pieces - pos.middle_count()) as u8; 7];
        let mut result = [(result_pieces - pos.result_count()) as u8; 7];
        if !self.query.allow_piece_exchange && usize::from(self.source.first_len) == middle_pieces {
            if let Some(counts) = self.source.first_counts {
                for p in 0..7 {
                    middle[p] = counts[p].checked_sub(key.middle_counts[p])?;
                }
            }
            if usize::from(self.source.end - self.source.first_len) == result_pieces {
                if let Some(counts) = self.source.second_counts {
                    for p in 0..7 {
                        result[p] = counts[p].checked_sub(key.result_counts[p])?;
                    }
                }
            }
        } else if usize::from(self.source.end) == middle_pieces + result_pieces {
            if let (Some(first), Some(second)) =
                (self.source.first_counts, self.source.second_counts)
            {
                for p in 0..7 {
                    let remaining = first[p]
                        .checked_add(second[p])?
                        .checked_sub(key.middle_counts[p])?
                        .checked_sub(key.result_counts[p])?;
                    middle[p] = middle[p].min(remaining);
                    result[p] = result[p].min(remaining);
                }
            }
        }
        Some((middle, result))
    }
    fn placements(
        &mut self,
        key: Key,
        token: Token,
        held: Option<Token>,
        decision: &'static str,
        actions: &mut Vec<Action>,
        control: &ExecutionControl,
    ) -> Result<(), Error> {
        let pos = self.geometry.position(key.geometry);
        let middle_pieces = self.geometry.stages[pos.stage as usize]
            .prepared
            .middle_pieces;
        let first = token.index < self.source.first_len;
        if first && usize::from(key.first_used) == middle_pieces {
            return Ok(());
        }
        let before = pos.middle_count() < middle_pieces;
        for &edge in self
            .geometry
            .edges(key.geometry, usize::from(token.piece), control)?
            .iter()
        {
            if self
                .plan
                .as_ref()
                .is_some_and(|plan| !plan.allows(&self.geometry, key.geometry, edge, token.piece))
            {
                continue;
            }
            if before && edge.result && key.mode == Mode::Middle {
                continue;
            }
            // Early is a placement before middle completion, not a token origin.
            // Drawing ahead through hold must not bypass the user quota.
            let early = before && edge.result;
            if early && usize::from(key.early) >= self.maximum {
                continue;
            }
            let Some(chain) = self.chain_placement(key, edge, token)? else {
                continue;
            };
            let mut child = Key {
                chain,
                geometry: edge.next,
                active: None,
                hold: held,
                can_hold: true,
                first_used: key.first_used + u8::from(first),
                early: key.early + u8::from(early),
                ..key
            };
            let piece = usize::from(token.piece);
            child.exchange[piece] += i16::from(first) - i16::from(!edge.result);
            if edge.result {
                child.result_counts[piece] += 1;
            } else {
                child.middle_counts[piece] += 1;
            }
            actions
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            actions.push(Action {
                child,
                map: Map::Place {
                    from: key.geometry,
                    edge,
                    token,
                    decision,
                },
            });
        }
        Ok(())
    }
    fn chain_terminal(&self, key: Key) -> bool {
        if self.query.chain_stages.is_empty() {
            return true;
        }
        let p = &self.chain_states[key.chain as usize];
        self.query
            .chain_stages
            .iter()
            .enumerate()
            .all(|(i, stage)| {
                (i + 1 == p.used.len()
                    || usize::from(p.used[i]) == stage.target.count_ones() as usize / 4)
                    && (self.query.allow_piece_exchange || p.balance[i] == [0; 7])
            })
    }
    fn chain_placement(
        &mut self,
        key: Key,
        edge: Edge,
        token: Token,
    ) -> Result<Option<u32>, Error> {
        if self.query.chain_stages.is_empty() {
            return Ok(Some(0));
        }
        let pos = self.geometry.position(key.geometry);
        let next = self.geometry.position(edge.next);
        let targets = &self.geometry.stages[pos.stage as usize].chain_targets;
        let owner = if edge.result {
            targets.len() - 1
        } else {
            let cells = next.middle.without(pos.middle);
            let Some(owner) = targets[..targets.len() - 1]
                .iter()
                .position(|t| cells.without(*t).is_empty())
            else {
                return Ok(None);
            };
            owner
        };
        let mut offset = 0u16;
        let mut source = None;
        for (i, len) in self.chain_lengths.iter().enumerate() {
            offset = offset.checked_add(*len).ok_or(Error::CounterOverflow)?;
            if token.index < offset {
                source = Some(i);
                break;
            }
        }
        let source = source.ok_or(Error::PatternDomainUnavailable)?;
        let mut p = self.chain_states[key.chain as usize].clone();
        p.used[source] = p.used[source]
            .checked_add(1)
            .ok_or(Error::CounterOverflow)?;
        if source + 1 < targets.len()
            && usize::from(p.used[source]) > targets[source].count_ones() as usize / 4
        {
            return Ok(None);
        }
        p.balance[source][usize::from(token.piece)] += 1;
        p.balance[owner][usize::from(token.piece)] -= 1;
        let mut prefix = clearra_core_domain::board::standard_pc_board::Board256Mask::EMPTY;
        for (boundary, target) in targets.iter().enumerate().take(owner) {
            prefix = prefix.union(*target);
            if !prefix.without(pos.middle).is_empty() {
                if key.mode == Mode::Middle {
                    return Ok(None);
                }
                p.early[boundary] = p.early[boundary]
                    .checked_add(1)
                    .ok_or(Error::CounterOverflow)?;
                if usize::from(p.early[boundary]) > self.maximum {
                    return Ok(None);
                }
            }
        }
        if let Some(id) = self.chain_unique.get(&p) {
            return Ok(Some(*id));
        }
        let id = u32::try_from(self.chain_states.len()).map_err(|_| Error::CounterOverflow)?;
        self.chain_unique
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.chain_states
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.chain_unique.insert(p.clone(), id);
        self.chain_states.push(p);
        Ok(Some(id))
    }
    fn accepts(&self, mut id: Id, depth: u16, queue: &[PieceKind]) -> bool {
        for (level, &piece) in queue.iter().enumerate().skip(usize::from(depth)) {
            id = self.diagram.follow(id, level as u16, piece_index(piece));
        }
        id == ALL
    }
    /// Extract a real execution from the already proved language. No new
    /// fixed-pair coverage search is performed to manufacture a witness.
    pub fn witness(
        &mut self,
        first: &[PieceKind],
        second: &[PieceKind],
        status: RecoveryBuildStatus,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildFixedReport, Error> {
        let queue = first.iter().chain(second).copied().collect::<Vec<_>>();
        let mut key = self
            .roots
            .iter()
            .find(|r| r.status == status && self.accepts(r.language, 0, &queue))
            .map(|r| r.key)
            .ok_or(Error::PatternDomainUnavailable)?;
        let mut steps = Vec::new();
        loop {
            cancelled(control)?;
            match self.actions(key, control)? {
                Prepared::Terminal(value) => {
                    if !self.accepts(value, key.depth, &queue) {
                        return Err(Error::PatternDomainUnavailable);
                    }
                    let pos = self.geometry.position(key.geometry);
                    let mut before = true;
                    let mut actual_early = 0;
                    for step in &steps {
                        let step: &super::super::RecoveryBuildStep = step;
                        if before && step.result_target {
                            actual_early += 1;
                        }
                        before &= !step.middle_complete;
                    }
                    let stage = &self.geometry.stages[pos.stage as usize];
                    let chain_progress = &self.chain_states[key.chain as usize];
                    if !self.chain_lengths.is_empty() {
                        actual_early =
                            chain_progress.early.iter().copied().max().unwrap_or(0) as usize;
                    }
                    return Ok(RecoveryBuildFixedReport {
                        middle_target: stage.fields.middle.words(),
                        stage_targets: stage.chain_targets.iter().map(|m| m.words()).collect(),
                        stage_source_lengths: self.chain_lengths.clone(),
                        stage_early_counts: chain_progress.early.clone(),
                        status,
                        states: usize::try_from(self.states).map_err(|_| Error::CounterOverflow)?,
                        effective_max_early: self.maximum,
                        actual_early,
                        exchange_balance: key.exchange,
                        steps,
                        terminal_board: pos.board.words(),
                        result_target: self.geometry.stages[pos.stage as usize]
                            .fields
                            .result
                            .words(),
                    });
                }
                Prepared::Actions(actions) => {
                    let mut selected = None;
                    for action in actions {
                        if let Some(&value) = self.memo.get(&action.child) {
                            let value = self.map(value, action.map)?;
                            if self.accepts(value, key.depth, &queue) {
                                selected = Some(action);
                                break;
                            }
                        }
                    }
                    let action = selected.ok_or(Error::PatternDomainUnavailable)?;
                    if let Map::Place {
                        from,
                        edge,
                        token,
                        decision,
                    } = action.map
                    {
                        steps.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
                        steps.push(self.geometry.step(
                            from,
                            edge,
                            usize::from(token.index),
                            decision,
                            usize::from(token.piece),
                        ));
                    }
                    key = action.child;
                }
            }
        }
    }
}
enum Prepared {
    Terminal(Id),
    Actions(Vec<Action>),
}
