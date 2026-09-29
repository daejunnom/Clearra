//! Versioned exact-query packets with bounded counts and postorder DAG indices.
use super::*;
use crate::recovery_build::{
    catalog::plan::Tile,
    parallel::wire::{Reader, Writer},
    RecoveryBuildParallelError,
};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
const INIT: &[u8] = b"RCHIN\x01";
const TASK: &[u8] = b"RCHTK\x01";
const RESULT: &[u8] = b"RCHRS\x01";
impl From<RecoveryBuildParallelError> for Error {
    fn from(error: RecoveryBuildParallelError) -> Self {
        match error {
            RecoveryBuildParallelError::Search(e) => Self::Core(e),
            RecoveryBuildParallelError::InvalidWire(reason) => Self::InvalidPacket(reason),
            RecoveryBuildParallelError::InvalidState(reason) => Self::InvalidState(reason),
        }
    }
}
fn bad() -> Error {
    Error::InvalidPacket("invalid recovery chain packet")
}
pub(super) fn initialization(q: &RecoveryChainQuery) -> Vec<u8> {
    let mut w = Writer(INIT.to_vec());
    w.byte(q.height);
    w.words(q.initial.words());
    w.number(q.targets.len() as u128);
    for (target, supply) in q.targets.iter().zip(&q.supplies) {
        w.words(target.words());
        w.text(supply);
    }
    w.flag(matches!(q.early_limit, CrossStageEarlyLimit::Auto));
    w.number(match q.early_limit {
        CrossStageEarlyLimit::Auto => 0,
        CrossStageEarlyLimit::AtMost(n) => n as u128,
    });
    for value in [
        q.allow_piece_exchange,
        q.hold_enabled,
        q.preserve_b2b,
        q.initial_b2b,
    ] {
        w.flag(value);
    }
    w.text(q.rule_profile.as_str());
    w.text(q.spin_profile.as_str());
    w.0
}
pub(super) fn read_initialization(bytes: &[u8]) -> Result<RecoveryChainQuery, Error> {
    let mut r = Reader(bytes);
    r.header(INIT)?;
    let height = r.byte()?;
    let initial = Mask::from_words(r.words()?);
    let n = r.count(60)?;
    if n < 2 {
        return Err(bad());
    }
    let mut targets = Vec::with_capacity(n);
    let mut supplies = Vec::with_capacity(n);
    for _ in 0..n {
        targets.push(Mask::from_words(r.words()?));
        supplies.push(r.text()?.to_owned());
    }
    let auto = r.flag()?;
    let early = r.count(usize::MAX)?;
    if auto && early != 0 {
        return Err(bad());
    }
    let q = RecoveryChainQuery {
        height,
        initial,
        targets,
        supplies,
        early_limit: if auto {
            CrossStageEarlyLimit::Auto
        } else {
            CrossStageEarlyLimit::AtMost(early)
        },
        allow_piece_exchange: r.flag()?,
        hold_enabled: r.flag()?,
        preserve_b2b: r.flag()?,
        initial_b2b: r.flag()?,
        rule_profile: RuleProfileId::parse(r.text()?).ok_or_else(bad)?,
        spin_profile: SpinProfileId::parse(r.text()?).ok_or_else(bad)?,
    };
    r.end()?;
    q.validate()?;
    if initialization(&q) != bytes {
        return Err(bad());
    }
    Ok(q)
}
fn write_task(w: &mut Writer, init: &[u8], task: &Task) {
    w.bytes(init);
    w.number(task.ordinal);
    w.number(task.plan.stages.len() as u128);
    for (target, tiles) in task.plan.targets.iter().zip(&task.plan.stages) {
        w.words(target.words());
        w.number(tiles.len() as u128);
        for tile in tiles {
            w.byte(tile.piece);
            w.words(tile.cells);
        }
    }
}
fn task_read(r: &mut Reader<'_>, init: &[u8], q: &RecoveryChainQuery) -> Result<Task, Error> {
    if r.bytes()? != init {
        return Err(Error::InvalidPacket("chain query binding mismatch"));
    }
    let ordinal = r.number()?;
    let n = r.count(60)?;
    if n != q.targets.len() {
        return Err(bad());
    }
    let mut targets = Vec::with_capacity(n);
    let mut stages = Vec::with_capacity(n);
    let mut total = 0;
    for _ in 0..n {
        let target = Mask::from_words(r.words()?);
        let count = r.count(60)?;
        total += count;
        if total > 60 || count == 0 || count != target.count_ones() as usize / 4 {
            return Err(bad());
        }
        let mut tiles = Vec::with_capacity(count);
        for _ in 0..count {
            let piece = r.byte()?;
            if piece >= 7 {
                return Err(bad());
            }
            tiles.push(Tile {
                piece,
                cells: r.words()?,
            });
        }
        targets.push(target);
        stages.push(tiles);
    }
    let plan = Plan { targets, stages };
    plan.validate(q)?;
    Ok(Task { ordinal, plan })
}
pub(super) fn task(init: &[u8], task: &Task) -> Vec<u8> {
    let mut w = Writer(TASK.to_vec());
    write_task(&mut w, init, task);
    w.0
}
pub(super) fn read_task(bytes: &[u8], init: &[u8], q: &RecoveryChainQuery) -> Result<Task, Error> {
    let mut r = Reader(bytes);
    r.header(TASK)?;
    let t = task_read(&mut r, init, q)?;
    r.end()?;
    Ok(t)
}
pub(super) fn result(init: &[u8], packet: &ResultPacket) -> Vec<u8> {
    let mut w = Writer(RESULT.to_vec());
    write_task(&mut w, init, &packet.task);
    w.number(packet.states);
    w.number(packet.languages.nodes.len() as u128);
    for (level, children) in &packet.languages.nodes {
        w.0.extend(level.to_le_bytes());
        for child in children {
            w.0.extend(child.to_le_bytes());
        }
    }
    for root in packet.languages.roots {
        w.0.extend(root.to_le_bytes());
    }
    w.0
}
pub(super) fn read_result(
    bytes: &[u8],
    init: &[u8],
    q: &RecoveryChainQuery,
) -> Result<ResultPacket, Error> {
    let mut r = Reader(bytes);
    r.header(RESULT)?;
    let task = task_read(&mut r, init, q)?;
    let states = r.number()?;
    let count = r.count(r.0.len() / 30)?;
    let mut nodes = Vec::new();
    nodes
        .try_reserve(count)
        .map_err(|_| Core::MemoryUnavailable)?;
    for _ in 0..count {
        let level = u16::from_le_bytes(r.take(2)?.try_into().map_err(|_| bad())?);
        let mut children = [0_u32; 7];
        for child in &mut children {
            *child = u32::from_le_bytes(r.take(4)?.try_into().map_err(|_| bad())?);
        }
        nodes.push((level, children));
    }
    let mut roots = [0_u32; 2];
    for root in &mut roots {
        *root = u32::from_le_bytes(r.take(4)?.try_into().map_err(|_| bad())?);
    }
    r.end()?;
    Ok(ResultPacket {
        task,
        states,
        languages: DiagramPacket { nodes, roots },
    })
}
