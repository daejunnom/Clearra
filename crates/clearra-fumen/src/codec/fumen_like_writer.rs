mod comment_encoder {
    use std::fmt::Write as _;

    use super::{error::FumenLikeWriteError, value_buffer::FumenValueBuffer};

    const MAX_COMMENT_LENGTH: usize = 4095;
    const COMMENT_TABLE: &[u8] =
        b" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
    const COMMENT_BASE: usize = COMMENT_TABLE.len() + 1;

    pub(super) fn write_comment(
        buffer: &mut FumenValueBuffer,
        index: usize,
        page: &str,
    ) -> Result<(), FumenLikeWriteError> {
        let escaped_length = escaped_comment_length(page);
        if escaped_length > MAX_COMMENT_LENGTH {
            return Err(FumenLikeWriteError::CommentTooLong {
                index,
                length: escaped_length,
            });
        }
        let escaped = escape_comment(page);

        buffer.push(escaped.len(), 2);
        for chunk in escaped.as_bytes().chunks(4) {
            let mut value = 0;
            for (count, byte) in chunk.iter().enumerate() {
                let Some(comment_index) = comment_table_index(*byte) else {
                    return Err(FumenLikeWriteError::UnsupportedCommentCharacter {
                        index,
                        byte: *byte,
                    });
                };
                value += comment_index * COMMENT_BASE.pow(count as u32);
            }
            buffer.push(value, 5);
        }
        Ok(())
    }

    fn escaped_comment_length(page: &str) -> usize {
        page.encode_utf16()
            .map(|code_unit| {
                if is_unescaped_ascii(code_unit) {
                    1
                } else if code_unit <= 0xff {
                    3
                } else {
                    6
                }
            })
            .sum()
    }

    fn escape_comment(page: &str) -> String {
        let mut escaped = String::new();
        for code_unit in page.encode_utf16() {
            if is_unescaped_ascii(code_unit) {
                escaped.push(char::from_u32(u32::from(code_unit)).expect("ASCII code unit"));
            } else if code_unit <= 0xff {
                write!(escaped, "%{code_unit:02X}").expect("writing to a String cannot fail");
            } else {
                write!(escaped, "%u{code_unit:04X}").expect("writing to a String cannot fail");
            }
        }
        escaped
    }

    fn is_unescaped_ascii(code_unit: u16) -> bool {
        code_unit <= 0x7f
            && matches!(
                code_unit as u8,
                b'A'..=b'Z'
                    | b'a'..=b'z'
                    | b'0'..=b'9'
                    | b'@'
                    | b'*'
                    | b'_'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'/'
            )
    }

    fn comment_table_index(byte: u8) -> Option<usize> {
        COMMENT_TABLE
            .iter()
            .position(|candidate| *candidate == byte)
    }
}
mod empty_action_encoder {
    use super::empty_field_encoder::FIELD_BLOCKS;

    pub(super) fn encode_empty_action(colorize: bool, comment: bool) -> usize {
        let lock = true;
        let mirror = false;
        let rise = false;
        let position = 0;
        let rotation = 0;
        let piece = 0;

        let mut value = usize::from(!lock);
        value = value * 2 + usize::from(comment);
        value = value * 2 + usize::from(colorize);
        value = value * 2 + usize::from(mirror);
        value = value * 2 + usize::from(rise);
        value = value * FIELD_BLOCKS + position;
        value = value * 4 + rotation;
        value = value * 8 + piece;
        value
    }
}
mod empty_field_encoder {
    use super::value_buffer::FumenValueBuffer;

    const FIELD_WIDTH: usize = 10;
    const FIELD_TOP: usize = 23;
    const GARBAGE_LINES: usize = 1;
    pub(super) const FIELD_BLOCKS: usize = FIELD_WIDTH * (FIELD_TOP + GARBAGE_LINES);

    pub(super) fn write_empty_field_diff(buffer: &mut FumenValueBuffer) {
        let no_change_field = 8 * FIELD_BLOCKS + (FIELD_BLOCKS - 1);
        buffer.push(no_change_field, 2);
        buffer.push(0, 1);
    }
}
mod error {
    use std::fmt;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FumenLikeWriteError {
        CommentTooLong { index: usize, length: usize },
        UnsupportedCommentCharacter { index: usize, byte: u8 },
        TooManyPages { length: usize, maximum: usize },
    }

    impl fmt::Display for FumenLikeWriteError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::CommentTooLong { index, length } => write!(
                    formatter,
                    "fumen page {index} comment length {length} exceeds the 4095-byte escaped limit"
                ),
                Self::UnsupportedCommentCharacter { index, byte } => write!(
                    formatter,
                    "fumen page {index} contains unsupported escaped byte 0x{byte:02x}"
                ),
                Self::TooManyPages { length, maximum } => write!(
                    formatter,
                    "fumen page count {length} exceeds the {maximum}-page limit"
                ),
            }
        }
    }

    impl std::error::Error for FumenLikeWriteError {}
}
mod event_names {
    use clearra_replay::{ReplayBoardSnapshotPhase, RotationRequest, TraceCompleteness};

    pub(super) fn board_snapshot_phase_name(phase: ReplayBoardSnapshotPhase) -> &'static str {
        match phase {
            ReplayBoardSnapshotPhase::Initial => "initial",
            ReplayBoardSnapshotPhase::BeforePlacement => "before-placement",
            ReplayBoardSnapshotPhase::AfterPlacement => "after-placement",
            ReplayBoardSnapshotPhase::AfterLineClear => "after-line-clear",
        }
    }

    pub(super) fn rotation_request_name(request: RotationRequest) -> &'static str {
        match request {
            RotationRequest::None => "none",
            RotationRequest::Clockwise => "clockwise",
            RotationRequest::CounterClockwise => "counter-clockwise",
            RotationRequest::HalfTurn => "half-turn",
        }
    }

    pub(super) fn trace_completeness_name(completeness: TraceCompleteness) -> &'static str {
        match completeness {
            TraceCompleteness::Complete => "complete",
            TraceCompleteness::MissingKickEvidence => "missing-kick-evidence",
            TraceCompleteness::SampleOnly => "sample-only",
            TraceCompleteness::Incomplete => "incomplete",
        }
    }
}
mod replay_event_page {
    use clearra_replay::ReplayEvent;

    use super::event_names::{
        board_snapshot_phase_name, rotation_request_name, trace_completeness_name,
    };

    pub(super) fn replay_event_page(event_index: usize, event: &ReplayEvent) -> String {
        match event {
            ReplayEvent::TraceMarker(marker) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=trace-marker\nrepresentative={}\nsample={}",
                marker.representative(), marker.sample()
            ),
            ReplayEvent::Placement(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=placement\nstep_index={}\npiece={}\nrotation={}\nx={}\ny={}\nplaced_mask=0x{:016x}",
                event.step_index(), event.piece().as_ascii(), event.rotation().quarter_turns(),
                event.x(), event.y(), event.placed_mask()
            ),
            ReplayEvent::Lock(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=lock\nevent_id={}\noperation_id={}\npiece={}\nrotation={}\nlock_x={}\nlock_y={}\nboard_before=0x{:016x}\nboard_after_place=0x{:016x}\ncleared_lines=0x{:016x}\ncleared_cell_owner_count={}\nboard_after_clear=0x{:016x}",
                event.event_id().0, event.operation_id().0, event.piece().as_ascii(),
                event.rotation().quarter_turns(), event.lock_x(), event.lock_y(),
                event.board_before().mask, event.board_after_place().mask, event.cleared_lines().0,
                event.cleared_cell_owners().len(), event.board_after_clear().mask,
            ),
            ReplayEvent::HoldStore(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=hold-store\nstep_index={}\nstored_piece={}",
                event.step_index(), event.stored_piece().as_ascii()
            ),
            ReplayEvent::HoldSwap(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=hold-swap\nstep_index={}\nheld_piece={}\nactive_piece={}",
                event.step_index(), event.held_piece().as_ascii(), event.active_piece().as_ascii()
            ),
            ReplayEvent::HoldRelease(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=terminal-hold-release\nstep_index={}\nactive_piece={}",
                event.step_index(), event.active_piece().as_ascii()
            ),
            ReplayEvent::Drop(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=drop\nstep_index={}\nfrom_y={}\nto_y={}\ndistance={}",
                event.step_index(), event.from_y(), event.to_y(), event.distance()
            ),
            ReplayEvent::SpinBasis(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=spin-basis\nstep_index={}\npiece={}\nrotation={}\nx={}\ny={}\nboard_before=0x{:016x}\nboard_after_placement=0x{:016x}\ncleared_lines={}",
                event.step_index(), event.piece().as_ascii(), event.rotation().quarter_turns(),
                event.x(), event.y(), event.board_before(), event.board_after_placement(), event.cleared_lines()
            ),
            ReplayEvent::ScoreBasis(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=score-basis\nstep_index={}\npiece={}\ncleared_lines={}\nboard_before=0x{:016x}\nboard_after_line_clear=0x{:016x}",
                event.step_index(), event.piece().as_ascii(), event.cleared_lines(),
                event.board_before(), event.board_after_line_clear()
            ),
            ReplayEvent::BoardSnapshot(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=board-snapshot\nstep_index={}\nphase={}\noccupied=0x{:016x}",
                event.step_index(), board_snapshot_phase_name(event.phase()), event.occupied()
            ),
            ReplayEvent::LineClear(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=line-clear\nstep_index={}\ncleared_lines={}",
                event.step_index(), event.cleared_lines()
            ),
            ReplayEvent::KickEvidence(event) => {
                let predecessor = event.predecessor();
                let result = event.result();
                format!(
                    "kind=replay-event\nevent_index={event_index}\ntype=kick-evidence\nstep_index={}\nfrom_rotation={}\nto_rotation={}\nrotation_request={}\nkick_index={}\nkick_dx={}\nkick_dy={}\nkick_table_id={}\nkick_profile_id={}\nfirst_success_confirmed={}\npredecessor_x={}\npredecessor_y={}\nresult_x={}\nresult_y={}",
                    event.step_index(), event.from_rotation(), event.to_rotation(),
                    rotation_request_name(event.rotation_request()), event.kick_index(),
                    event.kick_dx(), event.kick_dy(), event.kick_table_id(), event.kick_profile_id(),
                    event.first_success_confirmed(), predecessor.0, predecessor.1, result.0, result.1
                )
            }
            ReplayEvent::MovementEvidence(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=movement-evidence\nstep_index={}\npath_complete={}\nlast_action_was_rotation={}\nused_kick={}\nused_180={}\nrotation_evidence_complete={}",
                event.step_index(),
                event.path_complete(),
                event.last_action_was_rotation(),
                event.used_kick(),
                event.used_180(),
                event.rotation_evidence_complete(),
            ),
            ReplayEvent::TraceCompleteness(event) => format!(
                "kind=replay-event\nevent_index={event_index}\ntype=trace-completeness\ncompleteness={}",
                trace_completeness_name(event.completeness())
            ),
        }
    }
}
mod replay_trace_pages {
    use super::replay_event_page::replay_event_page;

    pub(super) fn replay_trace_pages(trace: &clearra_replay::ReplayTrace) -> Vec<String> {
        let mut pages = vec![format!(
            "kind=replay-trace\nvariant_id={}\nrepresentative={}\nsample={}\ntrace_steps={}\ncanonical_key={}\ncolored_cells={}",
            trace.variant_id(), trace.representative(), trace.sample(), trace.trace_steps(),
            trace.canonical_key(), trace.colored_cell_ownership().owned_cell_count()
        )];

        for step in trace.solution_trace().steps() {
            let placement = step.placement();
            pages.push(format!(
                "kind=replay-step\nvariant_id={}\nstep_index={}\npiece={}\nrotation={}\nx={}\ny={}\ncleared_lines={}\nboard_after_line_clear=0x{:016x}",
                trace.variant_id(), step.step_index(), placement.piece_kind().as_ascii(),
                placement.rotation().quarter_turns(), placement.x(), placement.y(),
                step.line_clear().cleared_lines(), step.board_after().after_line_clear().occupied()
            ));
        }
        for (event_index, event) in trace.events().iter().enumerate() {
            pages.push(replay_event_page(event_index, event));
        }
        pages
    }
}
mod value_buffer {
    const ENCODE_TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub(super) struct FumenValueBuffer {
        values: Vec<usize>,
    }

    impl FumenValueBuffer {
        pub(super) fn push(&mut self, value: usize, split_count: usize) {
            let mut current = value;
            for _ in 0..split_count {
                self.values.push(current % ENCODE_TABLE.len());
                current /= ENCODE_TABLE.len();
            }
        }
    }

    impl std::fmt::Display for FumenValueBuffer {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for value in &self.values {
                formatter.write_str(&(ENCODE_TABLE[*value] as char).to_string())?;
            }
            Ok(())
        }
    }
}
mod wrap_data {
    pub(super) fn wrap_data(data: &str) -> String {
        if data.len() < 41 {
            return data.to_owned();
        }

        let first_len = data.len().min(42);
        let mut chunks = vec![data[..first_len].to_owned()];
        let mut tail = &data[first_len..];
        while !tail.is_empty() {
            let split_at = tail.len().min(47);
            chunks.push(tail[..split_at].to_owned());
            tail = &tail[split_at..];
        }
        chunks.join("?")
    }
}
mod writer {
    use crate::codec::fumen_like_trace::FumenLikeTrace;
    use crate::codec::FUMEN_MAX_PAGES;

    use super::{
        comment_encoder::write_comment, empty_action_encoder::encode_empty_action,
        empty_field_encoder::write_empty_field_diff, error::FumenLikeWriteError,
        replay_trace_pages::replay_trace_pages, value_buffer::FumenValueBuffer,
        wrap_data::wrap_data,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct FumenLikeWriter;

    impl FumenLikeWriter {
        pub fn write(trace: &FumenLikeTrace) -> Result<String, FumenLikeWriteError> {
            Self::try_write(trace)
        }
    }
    impl FumenLikeWriter {
        pub fn write_replay_trace(
            trace: &clearra_replay::ReplayTrace,
        ) -> Result<String, FumenLikeWriteError> {
            Self::write(&FumenLikeTrace::new(replay_trace_pages(trace)))
        }
    }
    impl FumenLikeWriter {
        pub fn try_write(trace: &FumenLikeTrace) -> Result<String, FumenLikeWriteError> {
            if trace.pages().len() > FUMEN_MAX_PAGES {
                return Err(FumenLikeWriteError::TooManyPages {
                    length: trace.pages().len(),
                    maximum: FUMEN_MAX_PAGES,
                });
            }
            let mut buffer = FumenValueBuffer::default();
            for (index, page) in trace.pages().iter().enumerate() {
                write_empty_field_diff(&mut buffer);
                buffer.push(encode_empty_action(index == 0, true), 3);
                write_comment(&mut buffer, index, page)?;
            }
            Ok(format!("v115@{}", wrap_data(&buffer.to_string())))
        }
    }
}

pub use error::FumenLikeWriteError;
pub use writer::FumenLikeWriter;

#[cfg(test)]
use super::fumen_like_trace::FumenLikeTrace;

#[cfg(test)]
#[path = "fumen_like_writer_tests.rs"]
mod tests;
