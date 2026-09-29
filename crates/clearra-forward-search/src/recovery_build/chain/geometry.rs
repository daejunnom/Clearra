use super::super::{catalog::plan::Tile, staged::source::PIECES, RecoveryBuildStep};
use super::plan::Plan;
use super::*;
use crate::{
    board::{place_and_clear, ForwardBoard},
    reachability::{ReachabilityWorkspace, ReachableLock},
    search::t_corner_counts,
};
use clearra_replay::ScoringExecutionEdge;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy, event::SpinDetector, profile::SpinProfile,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy)]
pub(super) struct Position {
    pub board: ForwardBoard,
    pub deleted: u32,
    pub prefix: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub tile: usize,
    pub stage: usize,
    pub lock: ReachableLock,
    pub board: ForwardBoard,
    pub rows: u32,
    pub lines: u8,
    pub spin: bool,
}
pub(super) struct Geometry {
    pub plan: Plan,
    pub tiles: Vec<(usize, Tile)>,
    pub stage_bits: Vec<u64>,
    query: RecoveryBuildQuery,
    reach: ReachabilityWorkspace,
    profile: SpinProfile,
    positions: HashMap<u64, Position>,
    edges: HashMap<(u64, u8), Arc<[Edge]>>,
}
impl Geometry {
    pub fn new(q: RecoveryBuildQuery, plan: Plan) -> Result<Self, Error> {
        plan.validate(&q)?;
        let tiles = plan
            .groups
            .iter()
            .enumerate()
            .flat_map(|(s, g)| g.iter().cloned().map(move |t| (s, t)))
            .collect::<Vec<_>>();
        let mut stage_bits = vec![0_u64; plan.groups.len()];
        for (i, (stage, _)) in tiles.iter().enumerate() {
            stage_bits[*stage] |= 1_u64 << i;
        }
        let reach = ReachabilityWorkspace::new(q.fields.height, q.rule_profile)
            .map_err(|_| Error::UnsupportedRuleProfile)?;
        let profile = SpinProfile::builtin(q.spin_profile);
        Ok(Self {
            plan,
            tiles,
            stage_bits,
            query: q,
            reach,
            profile,
            positions: HashMap::new(),
            edges: HashMap::new(),
        })
    }
    pub fn all(&self) -> u64 {
        (1_u64 << self.tiles.len()) - 1
    }
    pub fn position(&mut self, placed: u64) -> Result<Position, Error> {
        if let Some(&p) = self.positions.get(&placed) {
            return Ok(p);
        }
        let mut occupied = self.query.fields.initial;
        for (i, (_, tile)) in self.tiles.iter().enumerate() {
            if placed & (1 << i) != 0 {
                occupied = occupied.union(Mask::from_words(tile.cells));
            }
        }
        let deleted = (0..self.query.fields.height).fold(0_u32, |m, y| {
            m | if (0..10).all(|x| occupied.contains_index(u16::from(y) * 10 + x)) {
                1_u32 << y
            } else {
                0
            }
        });
        let board = ForwardBoard::from_mask(project(occupied, occupied, self.query.fields.height));
        let prefix = self
            .stage_bits
            .iter()
            .take_while(|&&b| placed & b == b)
            .count();
        let p = Position {
            board,
            deleted,
            prefix,
        };
        self.positions
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.positions.insert(placed, p);
        Ok(p)
    }
    pub fn edges(
        &mut self,
        placed: u64,
        piece: u8,
        control: &ExecutionControl,
    ) -> Result<Arc<[Edge]>, Error> {
        if let Some(v) = self.edges.get(&(placed, piece)) {
            return Ok(Arc::clone(v));
        }
        cancelled(control)?;
        let pos = self.position(placed)?;
        let h = self.query.fields.height;
        let map = (0..h)
            .filter(|&y| pos.deleted & (1 << y) == 0)
            .collect::<Vec<_>>();
        let locks = self
            .reach
            .reachable_locks(pos.board, PIECES[usize::from(piece)], true, true)
            .to_vec();
        let mut out = Vec::new();
        for lock in locks {
            cancelled(control)?;
            let mut cells = Mask::EMPTY;
            let mut valid = true;
            for y in 0..h {
                let bits = lock.mask.row_bits(10, y);
                if bits == 0 {
                    continue;
                }
                let Some(&logical) = map.get(usize::from(y)) else {
                    valid = false;
                    break;
                };
                for x in 0..10_u16 {
                    if bits & (1 << x) != 0 {
                        cells = cells.union(
                            Mask::singleton(u16::from(logical) * 10 + x)
                                .map_err(|_| Error::BoardOutsideField)?,
                        );
                    }
                }
            }
            if !valid {
                continue;
            }
            let Some((tile, (stage, _))) = self.tiles.iter().enumerate().find(|(i, (_, t))| {
                placed & (1_u64 << i) == 0 && t.piece == piece && t.cells == cells.words()
            }) else {
                continue;
            };
            let stage = *stage;
            let (board, rows, lines) =
                place_and_clear(10, h, pos.board.union_for_height(lock.mask, h));
            let (corners, front) = t_corner_counts(
                pos.board,
                h,
                PIECES[usize::from(piece)],
                lock.rotation,
                lock.x,
                lock.y,
            );
            let pc = lines > 0 && board.is_empty();
            let scoring = ScoringExecutionEdge::new(
                0,
                0,
                PIECES[usize::from(piece)],
                lock.rotation,
                lock.x,
                lock.y,
                lines,
                corners,
                front,
                lock.evidence.scoring(lock.rotation, lock.immobile),
            )
            .with_perfect_clear(pc);
            let spin =
                SpinDetector::detect_scoring_edge_with_profile(scoring, self.profile).is_some();
            if self.query.preserve_b2b
                && !BackToBackPreservationPolicy::new(self.profile).allows(scoring)
            {
                continue;
            }
            if self.position(placed | (1 << tile))?.board != board {
                return Err(Error::PatternDomainUnavailable);
            }
            out.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            out.push(Edge {
                tile,
                stage,
                lock,
                board,
                rows,
                lines,
                spin,
            });
        }
        let out: Arc<[Edge]> = out.into();
        self.edges
            .try_reserve(1)
            .map_err(|_| Error::MemoryUnavailable)?;
        self.edges.insert((placed, piece), Arc::clone(&out));
        Ok(out)
    }
    pub fn step(
        &mut self,
        placed: u64,
        edge: Edge,
        index: u16,
        piece: u8,
        decision: &'static str,
        b2b: bool,
    ) -> Result<RecoveryBuildStep, Error> {
        let before = self.position(placed)?;
        let next = self.position(placed | (1 << edge.tile))?;
        let mask = ForwardBoard::from_mask(Mask::from_words(self.tiles[edge.tile].1.cells));
        Ok(RecoveryBuildStep {
            source_index: usize::from(index),
            result_target: edge.stage > 0,
            piece: PIECES[usize::from(piece)],
            rotation: edge.lock.rotation.quarter_turns(),
            x: edge.lock.x,
            y: edge.lock.y,
            hold_decision: decision,
            board_before: before.board.words(),
            placement: edge.lock.mask.words(),
            board_after: edge.board.words(),
            cleared_rows: edge.rows,
            cleared_lines: edge.lines,
            recognized_spin: edge.spin,
            b2b_active: b2b,
            middle_complete: next.prefix > 0,
            logical_cells: (0..self.query.fields.height)
                .map(|y| mask.row_bits(10, y))
                .collect(),
        })
    }
}
