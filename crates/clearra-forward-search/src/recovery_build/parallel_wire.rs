//! Length-checked little-endian wire, versioned separately from other solvers.
//! Exact initialization bytes bind every task and receipt to its input/rules.
use super::parallel::{Outcome, RECOVERY_PAIRS_PER_TASK};
use super::{
    RecoveryBuildError as Error, RecoveryBuildFields, RecoveryBuildQuery, RecoveryBuildStatus,
};
use crate::CrossStageEarlyLimit;
use clearra_core_domain::board::standard_pc_board::Board256Mask;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
const INIT: &[u8; 8] = b"RCBI\x01\0\0\0";
const TASK: &[u8; 8] = b"RCBT\x01\0\0\0";
const RESULT: &[u8; 8] = b"RCBR\x01\0\0\0";
pub(super) fn accepts(b: &[u8]) -> bool {
    b.starts_with(INIT)
}
fn number(out: &mut Vec<u8>, value: u128) {
    out.extend(value.to_le_bytes());
}
fn text(out: &mut Vec<u8>, s: &str) {
    number(out, s.len() as u128);
    out.extend(s.as_bytes());
}
struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let result = self.bytes.get(..len).ok_or(Error::InvalidParallelWire)?;
        self.bytes = &self.bytes[len..];
        Ok(result)
    }
    fn num(&mut self) -> Result<u128, Error> {
        Ok(u128::from_le_bytes(
            self.take(16)?
                .try_into()
                .map_err(|_| Error::InvalidParallelWire)?,
        ))
    }
    fn usize(&mut self) -> Result<usize, Error> {
        usize::try_from(self.num()?).map_err(|_| Error::InvalidParallelWire)
    }
    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn flag(&mut self) -> Result<bool, Error> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidParallelWire),
        }
    }
    fn text(&mut self) -> Result<&'a str, Error> {
        let len = self.usize()?;
        std::str::from_utf8(self.take(len)?).map_err(|_| Error::InvalidParallelWire)
    }
    fn done(self) -> Result<(), Error> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidParallelWire)
        }
    }
}
pub(super) fn initialization(q: &RecoveryBuildQuery) -> Vec<u8> {
    let mut b = INIT.to_vec();
    b.push(q.fields.height);
    for board in [&q.fields.initial, &q.fields.middle, &q.fields.result] {
        for word in board.words() {
            b.extend(word.to_le_bytes());
        }
    }
    text(&mut b, &q.first_supply);
    text(&mut b, &q.second_supply);
    match q.early_limit {
        CrossStageEarlyLimit::Auto => b.push(0),
        CrossStageEarlyLimit::AtMost(n) => {
            b.push(1);
            number(&mut b, n as u128);
        }
    }
    b.extend([
        u8::from(q.allow_piece_exchange),
        u8::from(q.hold_enabled),
        u8::from(q.preserve_b2b),
        u8::from(q.initial_b2b),
    ]);
    text(&mut b, q.rule_profile.as_str());
    text(&mut b, q.spin_profile.as_str());
    b
}
pub(super) fn read_initialization(bytes: &[u8]) -> Result<RecoveryBuildQuery, Error> {
    let mut r = Reader { bytes };
    if r.take(8)? != INIT {
        return Err(Error::InvalidParallelWire);
    }
    let height = r.byte()?;
    let mut boards = [Board256Mask::EMPTY; 3];
    for board in &mut boards {
        let mut words = [0; 4];
        for word in &mut words {
            *word = u64::from_le_bytes(
                r.take(8)?
                    .try_into()
                    .map_err(|_| Error::InvalidParallelWire)?,
            );
        }
        *board = Board256Mask::from_words(words);
    }
    let first_supply = r.text()?.to_owned();
    let second_supply = r.text()?.to_owned();
    let early_limit = match r.byte()? {
        0 => CrossStageEarlyLimit::Auto,
        1 => CrossStageEarlyLimit::AtMost(r.usize()?),
        _ => return Err(Error::InvalidParallelWire),
    };
    let allow_piece_exchange = r.flag()?;
    let hold_enabled = r.flag()?;
    let preserve_b2b = r.flag()?;
    let initial_b2b = r.flag()?;
    let rule_profile = RuleProfileId::parse(r.text()?).ok_or(Error::InvalidParallelWire)?;
    let spin_profile = SpinProfileId::parse(r.text()?).ok_or(Error::InvalidParallelWire)?;
    r.done()?;
    Ok(RecoveryBuildQuery {
        fields: RecoveryBuildFields {
            height,
            initial: boards[0],
            middle: boards[1],
            result: boards[2],
        },
        first_supply,
        second_supply,
        early_limit,
        allow_piece_exchange,
        hold_enabled,
        preserve_b2b,
        initial_b2b,
        rule_profile,
        spin_profile,
    })
}
fn header(kind: &[u8], init: &[u8], start: u128, count: usize) -> Vec<u8> {
    let mut b = kind.to_vec();
    number(&mut b, init.len() as u128);
    b.extend(init);
    number(&mut b, start);
    number(&mut b, count as u128);
    b
}
fn read_header<'a>(
    bytes: &'a [u8],
    kind: &[u8],
    init: &[u8],
) -> Result<(Reader<'a>, u128, usize), Error> {
    let mut r = Reader { bytes };
    if r.take(8)? != kind {
        return Err(Error::InvalidParallelWire);
    }
    let len = r.usize()?;
    if r.take(len)? != init {
        return Err(Error::InvalidParallelWire);
    }
    let start = r.num()?;
    let count = r.usize()?;
    if count == 0 || count > RECOVERY_PAIRS_PER_TASK {
        return Err(Error::InvalidParallelWire);
    }
    Ok((r, start, count))
}
pub(super) fn task(init: &[u8], start: u128, count: usize) -> Vec<u8> {
    header(TASK, init, start, count)
}
pub(super) fn read_task(bytes: &[u8], init: &[u8]) -> Result<(u128, usize), Error> {
    let (r, start, count) = read_header(bytes, TASK, init)?;
    r.done()?;
    Ok((start, count))
}
pub(super) fn result(init: &[u8], start: u128, outcomes: &[Outcome]) -> Vec<u8> {
    let mut b = header(RESULT, init, start, outcomes.len());
    for row in outcomes {
        b.push(match row.status {
            RecoveryBuildStatus::Normal => 0,
            RecoveryBuildStatus::Recovery => 1,
            RecoveryBuildStatus::NoPath => 2,
        });
        b.extend(row.states.to_le_bytes());
    }
    b
}
pub(super) fn read_result(bytes: &[u8], init: &[u8]) -> Result<(u128, Vec<Outcome>), Error> {
    let (mut r, start, count) = read_header(bytes, RESULT, init)?;
    if r.bytes.len() != count * 9 {
        return Err(Error::InvalidParallelWire);
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(|_| Error::MemoryUnavailable)?;
    for _ in 0..count {
        let status = match r.byte()? {
            0 => RecoveryBuildStatus::Normal,
            1 => RecoveryBuildStatus::Recovery,
            2 => RecoveryBuildStatus::NoPath,
            _ => return Err(Error::InvalidParallelWire),
        };
        let states = u64::from_le_bytes(
            r.take(8)?
                .try_into()
                .map_err(|_| Error::InvalidParallelWire)?,
        );
        rows.push(Outcome { status, states });
    }
    r.done()?;
    Ok((start, rows))
}
