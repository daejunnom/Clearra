//! Query-bound value packets; graph references are local postorder indices.
use super::*;
use crate::recovery_build::parallel::wire::{read_path, write_path, Reader, Writer};
const TASK: &[u8] = b"RCATK\x01";
const RESULT: &[u8] = b"RCATR\x01";
fn invalid() -> ParallelError {
    ParallelError::InvalidWire("invalid recovery catalog packet")
}
fn write_task(w: &mut Writer, init: &[u8], task: &Task) {
    w.bytes(init);
    w.number(task.ordinal);
    w.byte(task.plan.orientation);
    for tiles in [&task.plan.middle, &task.plan.result] {
        w.number(tiles.len() as u128);
        for tile in tiles {
            w.byte(tile.piece);
            w.words(tile.cells);
        }
    }
}
fn task_read(r: &mut Reader<'_>, init: &[u8]) -> Result<Task, ParallelError> {
    if r.bytes()? != init {
        return Err(invalid());
    }
    let ordinal = r.number()?;
    let orientation = r.byte()?;
    if orientation > 1 {
        return Err(invalid());
    }
    let mut groups = [Vec::new(), Vec::new()];
    for tiles in &mut groups {
        let count = r.count(60)?;
        if count == 0 {
            return Err(invalid());
        }
        tiles
            .try_reserve(count)
            .map_err(|_| Error::MemoryUnavailable)?;
        for _ in 0..count {
            let piece = r.byte()?;
            if piece >= 7 {
                return Err(invalid());
            }
            tiles.push(plan::Tile {
                piece,
                cells: r.words()?,
            });
        }
    }
    let [middle, result] = groups;
    Ok(Task {
        ordinal,
        plan: Plan {
            orientation,
            middle,
            result,
        },
    })
}
pub(super) fn task(init: &[u8], task: &Task) -> Vec<u8> {
    let mut w = Writer(TASK.to_vec());
    write_task(&mut w, init, task);
    w.0
}
pub(super) fn read_task(bytes: &[u8], init: &[u8]) -> Result<Task, ParallelError> {
    let mut r = Reader(bytes);
    r.header(TASK)?;
    let task = task_read(&mut r, init)?;
    r.end()?;
    Ok(task)
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
    for example in [&packet.normal, &packet.recovery] {
        w.flag(example.is_some());
        if let Some(e) = example {
            w.number(e.first_pattern as u128);
            w.number(e.second_pattern as u128);
            write_path(&mut w, &e.path);
        }
    }
    w.0
}
pub(super) fn read_result(bytes: &[u8], init: &[u8]) -> Result<ResultPacket, ParallelError> {
    let mut r = Reader(bytes);
    r.header(RESULT)?;
    let task = task_read(&mut r, init)?;
    let states = r.number()?;
    let count = r.count(bytes.len() / 30)?;
    let mut nodes = Vec::new();
    nodes
        .try_reserve(count)
        .map_err(|_| Error::MemoryUnavailable)?;
    for _ in 0..count {
        let level = u16::from_le_bytes(r.take(2)?.try_into().map_err(|_| invalid())?);
        let mut children = [0_u32; 7];
        for child in &mut children {
            *child = u32::from_le_bytes(r.take(4)?.try_into().map_err(|_| invalid())?);
        }
        nodes.push((level, children));
    }
    let mut roots = [0_u32; 2];
    for root in &mut roots {
        *root = u32::from_le_bytes(r.take(4)?.try_into().map_err(|_| invalid())?);
    }
    let mut examples = [None, None];
    for example in &mut examples {
        if r.flag()? {
            *example = Some(RecoveryBuildExample {
                first_pattern: r.count(usize::MAX)?,
                second_pattern: r.count(usize::MAX)?,
                first_queue: Vec::new(),
                second_queue: Vec::new(),
                path: read_path(&mut r)?,
            });
        }
    }
    r.end()?;
    let [normal, recovery] = examples;
    Ok(ResultPacket {
        task,
        languages: DiagramPacket { nodes, roots },
        states,
        normal,
        recovery,
    })
}
