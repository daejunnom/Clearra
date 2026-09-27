//! Versioned value-only packets. Every packet binds the exact query bytes, not
//! a probabilistic hash. The outer runtime separately owns job/generation IDs.
use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
use super::super::{RecoveryBuildFields, RecoveryBuildStep};
pub(super) const INIT: &[u8] = b"RBIN\x01";
const TASK: &[u8] = b"RBTK\x01";
const RESULT: &[u8] = b"RBRS\x01";
type Error = RecoveryBuildParallelError;
fn bad() -> Error { Error::InvalidWire("invalid recovery-build packet") }
#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn byte(&mut self, value: u8) { self.0.push(value); }
    fn flag(&mut self, value: bool) { self.byte(u8::from(value)); }
    fn number(&mut self, value: u128) { self.0.extend(value.to_le_bytes()); }
    fn bytes(&mut self, bytes: &[u8]) { self.number(bytes.len() as u128); self.0.extend(bytes); }
    fn text(&mut self, text: &str) { self.bytes(text.as_bytes()); }
    fn words(&mut self, words: [u64; 4]) { for word in words { self.0.extend(word.to_le_bytes()); } }
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let bytes = self.0.get(..len).ok_or_else(bad)?;
        self.0 = &self.0[len..]; Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, Error> { Ok(self.take(1)?[0]) }
    fn flag(&mut self) -> Result<bool, Error> { match self.byte()? { 0 => Ok(false), 1 => Ok(true), _=>Err(bad()) } }
    fn number(&mut self) -> Result<u128, Error> { Ok(u128::from_le_bytes(self.take(16)?.try_into().map_err(|_| bad())?)) }
    fn count(&mut self, max: usize) -> Result<usize, Error> {
        let n = usize::try_from(self.number()?).map_err(|_| bad())?;
        if n > max { Err(bad()) } else { Ok(n) }
    }
    fn bytes(&mut self) -> Result<&'a [u8], Error> { let n = self.count(self.0.len())?; self.take(n) }
    fn text(&mut self) -> Result<&'a str, Error> { std::str::from_utf8(self.bytes()?).map_err(|_| bad()) }
    fn words(&mut self) -> Result<[u64; 4], Error> {
        let mut result=[0;4]; for word in &mut result { *word=u64::from_le_bytes(self.take(8)?.try_into().map_err(|_|bad())?); } Ok(result)
    }
    fn header(&mut self, magic: &[u8]) -> Result<(), Error> { if self.take(magic.len())? != magic { Err(bad()) } else { Ok(()) } }
    fn end(self) -> Result<(), Error> { if self.0.is_empty() { Ok(()) } else { Err(bad()) } }
}
pub(super) fn encode_initialization(q: &RecoveryBuildQuery) -> Vec<u8> {
    let mut w=Writer(INIT.to_vec()); w.byte(q.fields.height);
    w.words(q.fields.initial.words()); w.words(q.fields.middle.words()); w.words(q.fields.result.words());
    w.text(&q.first_supply); w.text(&q.second_supply);
    w.flag(matches!(q.early_limit, CrossStageEarlyLimit::Auto));
    w.number(match q.early_limit { CrossStageEarlyLimit::Auto=>0, CrossStageEarlyLimit::AtMost(n)=>n as u128 });
    w.flag(q.allow_piece_exchange); w.flag(q.hold_enabled); w.flag(q.preserve_b2b); w.flag(q.initial_b2b);
    w.text(q.rule_profile.as_str()); w.text(q.spin_profile.as_str()); w.0
}
pub(super) fn decode_initialization(bytes: &[u8]) -> Result<RecoveryBuildQuery, Error> {
    let mut r=Reader(bytes); r.header(INIT)?;
    let fields=RecoveryBuildFields { height:r.byte()?, initial:Board256Mask::from_words(r.words()?),
        middle:Board256Mask::from_words(r.words()?), result:Board256Mask::from_words(r.words()?) };
    let first_supply=r.text()?.to_owned(); let second_supply=r.text()?.to_owned();
    let auto=r.flag()?; let early=r.count(usize::MAX)?;
    if auto && early!=0 { return Err(bad()); }
    let q=RecoveryBuildQuery { fields, first_supply, second_supply,
        early_limit: if auto {CrossStageEarlyLimit::Auto} else {CrossStageEarlyLimit::AtMost(early)},
        allow_piece_exchange:r.flag()?, hold_enabled:r.flag()?, preserve_b2b:r.flag()?, initial_b2b:r.flag()?,
        rule_profile:RuleProfileId::parse(r.text()?).ok_or_else(bad)?,
        spin_profile:SpinProfileId::parse(r.text()?).ok_or_else(bad)? };
    r.end()?; q.validate()?; Ok(q)
}
fn task_write(w: &mut Writer, init: &[u8], task: Task) {
    w.bytes(init); w.number(task.start); w.byte(task.count as u8); w.byte(task.examples);
}
fn task_read(r: &mut Reader<'_>, init: &[u8]) -> Result<Task, Error> {
    if r.bytes()? != init { return Err(Error::InvalidWire("recovery query binding mismatch")); }
    let task=Task {start:r.number()?,count:usize::from(r.byte()?),examples:r.byte()?};
    if task.count==0 || task.count>MAX_BATCH || task.examples>3 {return Err(bad());} Ok(task)
}
pub(super) fn encode_task(init: &[u8], task: Task)->Vec<u8> {
    let mut w=Writer(TASK.to_vec());task_write(&mut w,init,task);w.0
}
pub(super) fn decode_task(bytes:&[u8],init:&[u8])->Result<Task,Error> {
    let mut r=Reader(bytes);r.header(TASK)?;let task=task_read(&mut r,init)?;r.end()?;Ok(task)
}
fn status_code(status:RecoveryBuildStatus)->u8 {
    match status { RecoveryBuildStatus::Normal=>0,RecoveryBuildStatus::Recovery=>1,RecoveryBuildStatus::NoPath=>2 }
}
fn status(r:&mut Reader<'_>)->Result<RecoveryBuildStatus,Error> {
    match r.byte()? {0=>Ok(RecoveryBuildStatus::Normal),1=>Ok(RecoveryBuildStatus::Recovery),2=>Ok(RecoveryBuildStatus::NoPath),_=>Err(bad())}
}
fn write_path(w:&mut Writer,p:&RecoveryBuildFixedReport) {
    w.byte(status_code(p.status));w.number(p.states as u128);w.number(p.effective_max_early as u128);w.number(p.actual_early as u128);
    for value in p.exchange_balance {w.0.extend(value.to_le_bytes());}
    w.words(p.terminal_board);w.number(p.steps.len() as u128);
    for s in &p.steps {
        w.number(s.source_index as u128);w.flag(s.result_target);w.byte(s.piece.as_ascii() as u8);
        w.byte(s.rotation);w.byte(s.x as u8);w.byte(s.y as u8);w.text(s.hold_decision);
        w.words(s.board_before);w.words(s.placement);w.words(s.board_after);
        w.0.extend(s.cleared_rows.to_le_bytes());w.byte(s.cleared_lines);
        w.flag(s.recognized_spin);w.flag(s.b2b_active);w.flag(s.middle_complete);
        w.number(s.logical_cells.len() as u128);for cell in &s.logical_cells {w.0.extend(cell.to_le_bytes());}
    }
}
fn read_path(r:&mut Reader<'_>)->Result<RecoveryBuildFixedReport,Error> {
    let status=status(r)?;let states=r.count(usize::MAX)?;
    let effective_max_early=r.count(usize::MAX)?;let actual_early=r.count(effective_max_early)?;
    let mut exchange_balance=[0_i16;7];for value in &mut exchange_balance {*value=i16::from_le_bytes(r.take(2)?.try_into().map_err(|_|bad())?);}
    let terminal_board=r.words()?;let count=r.count(120)?;let mut steps=Vec::with_capacity(count);
    for _ in 0..count {
        let source_index=r.count(usize::MAX)?;let result_target=r.flag()?;
        let piece=match r.byte()? {b'I'=>PieceKind::I,b'J'=>PieceKind::J,b'L'=>PieceKind::L,b'O'=>PieceKind::O,b'S'=>PieceKind::S,b'T'=>PieceKind::T,b'Z'=>PieceKind::Z,_=>return Err(bad())};
        let rotation=r.byte()?;let x=r.byte()? as i8;let y=r.byte()? as i8;
        if rotation>3 {return Err(bad());}
        let hold_decision=match r.text()? {"none"=>"none","swap"=>"swap","store"=>"store",_=>return Err(bad())};
        let board_before=r.words()?;let placement=r.words()?;let board_after=r.words()?;
        let cleared_rows=u32::from_le_bytes(r.take(4)?.try_into().map_err(|_|bad())?);let cleared_lines=r.byte()?;
        let recognized_spin=r.flag()?;let b2b_active=r.flag()?;let middle_complete=r.flag()?;
        let n=r.count(48)?;let mut logical_cells=Vec::with_capacity(n);
        for _ in 0..n {logical_cells.push(u16::from_le_bytes(r.take(2)?.try_into().map_err(|_|bad())?));}
        steps.push(RecoveryBuildStep{source_index,result_target,piece,rotation,x,y,hold_decision,
            board_before,placement,board_after,cleared_rows,cleared_lines,recognized_spin,b2b_active,middle_complete,logical_cells});
    }
    Ok(RecoveryBuildFixedReport{status,states,effective_max_early,actual_early,exchange_balance,steps,terminal_board})
}
pub(super) fn encode_result(init:&[u8],batch:&ResultBatch)->Vec<u8> {
    let mut w=Writer(RESULT.to_vec());task_write(&mut w,init,batch.task);
    for record in &batch.records {w.byte(status_code(record.status));w.number(record.states as u128);w.flag(record.path.is_some());if let Some(path)=&record.path{write_path(&mut w,path);}}
    w.0
}
pub(super) fn decode_result(bytes:&[u8],init:&[u8])->Result<ResultBatch,Error> {
    let mut r=Reader(bytes);r.header(RESULT)?;let task=task_read(&mut r,init)?;
    let mut records=Vec::with_capacity(task.count);
    for _ in 0..task.count {let status=status(&mut r)?;let states=r.count(usize::MAX)?;let path=if r.flag()?{Some(read_path(&mut r)?)}else{None};records.push(Record{status,states,path});}
    r.end()?;Ok(ResultBatch{task,records})
}
