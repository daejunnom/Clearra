//! Exact-query-bound value packets; old paired packets cannot enter this route.
use super::super::{
    catalog::plan::Tile,
    parallel::wire::{read_path, write_path, Reader, Writer},
    staged::diagram::DiagramPacket,
    RecoveryBuildFixedReport, RecoveryBuildParallelError as E,
};
use super::plan::Plan;
use super::*;
pub(super) const TASK: &[u8] = b"RBCQ\x01";
const RESULT: &[u8] = b"RBCR\x01";
fn bad() -> E {
    E::InvalidWire("invalid recovery chain packet")
}
pub(super) fn task(init: &[u8], ordinal: u128, plan: &Plan) -> Vec<u8> {
    let mut w = Writer(TASK.to_vec());
    w.bytes(init);
    w.number(ordinal);
    w.number(plan.groups.len() as u128);
    for tiles in &plan.groups {
        w.number(tiles.len() as u128);
        for t in tiles {
            w.byte(t.piece);
            w.words(t.cells);
        }
    }
    w.0
}
pub(super) fn read_task(bytes: &[u8], init: &[u8]) -> Result<(u128, Plan), E> {
    let mut r = Reader(bytes);
    r.header(TASK)?;
    if r.bytes()? != init {
        return Err(bad());
    }
    let ordinal = r.number()?;
    let n = r.count(60)?;
    if n < 2 {
        return Err(bad());
    }
    let mut groups = Vec::new();
    let mut targets = Vec::new();
    let mut total = 0;
    for _ in 0..n {
        let m = r.count(60 - total)?;
        if m == 0 {
            return Err(bad());
        }
        total += m;
        let mut tiles = Vec::new();
        let mut target = Mask::EMPTY;
        for _ in 0..m {
            let p = r.byte()?;
            let cells = r.words()?;
            if p >= 7 {
                return Err(bad());
            }
            target = target.union(Mask::from_words(cells));
            tiles.push(Tile { piece: p, cells });
        }
        groups.push(tiles);
        targets.push(target);
    }
    r.end()?;
    Ok((ordinal, Plan { targets, groups }))
}
pub(super) struct ResultPacket {
    pub task: Vec<u8>,
    pub states: u128,
    pub diagram: DiagramPacket,
    pub paths: [Option<RecoveryBuildFixedReport>; 2],
}
pub(super) fn result(p: &ResultPacket) -> Vec<u8> {
    let mut w = Writer(RESULT.to_vec());
    w.bytes(&p.task);
    w.number(p.states);
    w.number(p.diagram.nodes.len() as u128);
    for (level, children) in &p.diagram.nodes {
        w.number(u128::from(*level));
        for c in children {
            w.number(u128::from(*c));
        }
    }
    for root in p.diagram.roots {
        w.number(u128::from(root));
    }
    for path in &p.paths {
        w.flag(path.is_some());
        if let Some(path) = path {
            write_path(&mut w, path);
        }
    }
    w.0
}
pub(super) fn read_result(bytes: &[u8]) -> Result<ResultPacket, E> {
    let mut r = Reader(bytes);
    r.header(RESULT)?;
    let task = r.bytes()?.to_vec();
    let states = r.number()?;
    let count = r.count(bytes.len() / 128)?;
    let mut nodes = Vec::new();
    nodes
        .try_reserve(count)
        .map_err(|_| Error::MemoryUnavailable)?;
    for _ in 0..count {
        let level = r.count(usize::from(u16::MAX))? as u16;
        let mut children = [0_u32; 7];
        for c in &mut children {
            *c = u32::try_from(r.number()?).map_err(|_| bad())?;
        }
        nodes.push((level, children));
    }
    let mut roots = [0; 2];
    for c in &mut roots {
        *c = u32::try_from(r.number()?).map_err(|_| bad())?;
    }
    let mut paths = [None, None];
    for p in &mut paths {
        if r.flag()? {
            *p = Some(read_path(&mut r)?);
        }
    }
    r.end()?;
    Ok(ResultPacket {
        task,
        states,
        diagram: DiagramPacket { nodes, roots },
        paths,
    })
}
