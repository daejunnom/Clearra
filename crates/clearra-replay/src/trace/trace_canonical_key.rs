use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{
    trace::{hold_decision::HoldDecision, solution_trace::SolutionTrace},
    ScoringExecutionEdge,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraceCanonicalKey {
    steps: Vec<TraceStepCanonicalKey>,
}

impl TraceCanonicalKey {
    pub fn from_trace(trace: &SolutionTrace) -> Self {
        let steps = trace
            .steps()
            .iter()
            .map(TraceStepCanonicalKey::from_step)
            .collect();
        Self { steps }
    }

    pub fn from_scoring_path(
        layout: Board64Layout,
        edges: &[ScoringExecutionEdge],
        hold_decisions: &[HoldDecision],
    ) -> Option<Self> {
        if edges.len() != hold_decisions.len() {
            return None;
        }
        let registry = standard_tetromino_registry();
        let mut steps = Vec::with_capacity(edges.len());
        for (step_index, (edge, hold_decision)) in edges
            .iter()
            .copied()
            .zip(hold_decisions.iter().copied())
            .enumerate()
        {
            let definition = registry.get(edge.piece())?;
            let x = u16::try_from(edge.x()).ok()?;
            let y = u16::try_from(edge.y()).ok()?;
            let placement = PlacementMask::new(layout, definition, edge.rotation(), x, y).ok()?;
            steps.push(TraceStepCanonicalKey {
                active_piece: edge.piece(),
                input_cursor: step_index,
                output_cursor: step_index + 1,
                input_hold_piece: None,
                output_hold_piece: None,
                hold_decision,
                placed_piece: edge.piece(),
                rotation: edge.rotation(),
                x,
                y,
                mask: placement.mask(),
            });
        }
        Some(Self { steps })
    }
}
impl TraceCanonicalKey {
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}
impl TraceCanonicalKey {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}
impl TraceCanonicalKey {
    pub fn stable_key(&self) -> String {
        stable_key(self.steps.iter().copied())
    }

    /// Format directly from the trace, without allocating an intermediate key
    /// vector, per-step strings, hold strings, or a joined second copy.
    pub fn stable_key_from_trace(trace: &SolutionTrace) -> String {
        stable_key(trace.steps().iter().map(TraceStepCanonicalKey::from_step))
    }

    /// Exact requested capacity of the single output String. Callers governing
    /// allocations must also check its actual capacity after reservation.
    pub fn checked_trace_key_bytes(trace: &SolutionTrace) -> Option<u128> {
        key_length(trace.steps().iter().map(TraceStepCanonicalKey::from_step))
            .map(|bytes| bytes as u128)
    }

    /// Compare canonical bytes without allocating any trace/key representation.
    /// Formatting and matching share the same writer so their contracts cannot
    /// silently diverge when a canonical field is added or changed.
    pub fn matches_trace_key(trace: &SolutionTrace, expected: &str) -> bool {
        let mut writer = KeyMatcher {
            remaining: expected,
        };
        write_key(
            &mut writer,
            trace.steps().iter().map(TraceStepCanonicalKey::from_step),
        )
        .is_ok()
            && writer.remaining.is_empty()
    }
}

struct KeyLength(usize);

impl Write for KeyLength {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0 = self.0.checked_add(value.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

struct KeyMatcher<'a> {
    remaining: &'a str,
}

impl Write for KeyMatcher<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.remaining = self.remaining.strip_prefix(value).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn write_key(
    writer: &mut impl Write,
    steps: impl Iterator<Item = TraceStepCanonicalKey>,
) -> fmt::Result {
    writer.write_str("trk1:")?;
    for (index, step) in steps.enumerate() {
        if index != 0 {
            writer.write_char('~')?;
        }
        step.write_key(writer)?;
    }
    Ok(())
}

fn key_length(steps: impl Iterator<Item = TraceStepCanonicalKey>) -> Option<usize> {
    let mut writer = KeyLength(0);
    write_key(&mut writer, steps).ok()?;
    Some(writer.0)
}

fn stable_key(steps: impl Iterator<Item = TraceStepCanonicalKey> + Clone) -> String {
    let capacity = key_length(steps.clone()).expect("canonical trace key length overflow");
    let mut key = String::with_capacity(capacity);
    write_key(&mut key, steps).expect("writing a reserved canonical String cannot fail");
    key
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct TraceStepCanonicalKey {
    active_piece: PieceKind,
    input_cursor: usize,
    output_cursor: usize,
    input_hold_piece: Option<PieceKind>,
    output_hold_piece: Option<PieceKind>,
    hold_decision: HoldDecision,
    placed_piece: PieceKind,
    rotation: RotationState,
    x: u16,
    y: u16,
    mask: u64,
}

impl TraceStepCanonicalKey {
    fn from_step(step: &crate::trace::PlacementStep) -> Self {
        let placement = step.placement();
        let decision = step.piece_decision();
        Self {
            active_piece: decision.active_piece(),
            input_cursor: decision.input_cursor(),
            output_cursor: decision.output_cursor(),
            input_hold_piece: decision.input_hold_piece(),
            output_hold_piece: decision.output_hold_piece(),
            hold_decision: decision.hold_decision(),
            placed_piece: placement.piece_kind(),
            rotation: placement.rotation(),
            x: placement.x(),
            y: placement.y(),
            mask: placement.mask(),
        }
    }

    fn write_key(&self, writer: &mut impl Write) -> fmt::Result {
        write!(
            writer,
            "a{}i{}o{}ih{}oh{}d{}p{}r{}x{}y{}m{:016x}",
            self.active_piece.as_ascii(),
            self.input_cursor,
            self.output_cursor,
            HoldPieceDisplay(self.input_hold_piece),
            HoldPieceDisplay(self.output_hold_piece),
            HoldDecisionDisplay(self.hold_decision),
            self.placed_piece.as_ascii(),
            self.rotation.quarter_turns(),
            self.x,
            self.y,
            self.mask,
        )
    }
}

struct HoldPieceDisplay(Option<PieceKind>);

impl fmt::Display for HoldPieceDisplay {
    fn fmt(&self, writer: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(piece) => writer.write_char(piece.as_ascii()),
            None => writer.write_str("none"),
        }
    }
}

struct HoldDecisionDisplay(HoldDecision);

impl fmt::Display for HoldDecisionDisplay {
    fn fmt(&self, writer: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            HoldDecision::None => writer.write_str("none"),
            HoldDecision::SwapWithHold {
                incoming_piece,
                held_piece,
            } => write!(
                writer,
                "swap{}{}",
                incoming_piece.as_ascii(),
                held_piece.as_ascii()
            ),
            HoldDecision::StoreIncoming {
                stored_piece,
                drawn_piece,
            } => write!(
                writer,
                "store{}{}",
                stored_piece.as_ascii(),
                drawn_piece.as_ascii()
            ),
            HoldDecision::ReleaseHeldAtTerminal { held_piece } => {
                write!(writer, "terminal{}", held_piece.as_ascii())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::{
        layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
    };
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

    use crate::{
        board::board64_state::Board64State,
        trace::{
            BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep,
            SolutionTrace,
        },
    };

    use super::*;

    #[test]
    fn canonical_key_is_equal_for_equivalent_trace_steps() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let registry = standard_tetromino_registry();
        let piece = registry.get(PieceKind::O).expect("O");
        let placement =
            PlacementMask::new(layout, piece, RotationState::Zero, 0, 0).expect("placement");
        let board = Board64State::empty(layout);
        let step = PlacementStep::new(
            0,
            PieceDecision::new(PieceKind::O, 0, 1, None, None, HoldDecision::None),
            placement,
            board,
            BoardAfterStep::new(board, board),
            LineClearEvent::new(0),
        );
        let first = SolutionTrace::new(vec![step]);
        let second = SolutionTrace::new(vec![step]);

        assert_eq!(
            TraceCanonicalKey::from_trace(&first),
            TraceCanonicalKey::from_trace(&second)
        );
        assert_eq!(
            TraceCanonicalKey::from_trace(&first).stable_key(),
            "trk1:aOi0o1ihnoneohnonednonepOr0x0y0m0000000000000c03"
        );
        let streamed = TraceCanonicalKey::stable_key_from_trace(&first);
        assert_eq!(streamed, TraceCanonicalKey::from_trace(&first).stable_key());
        assert_eq!(
            TraceCanonicalKey::checked_trace_key_bytes(&first),
            Some(streamed.len() as u128)
        );
        assert!(TraceCanonicalKey::matches_trace_key(&first, &streamed));
        assert!(!TraceCanonicalKey::matches_trace_key(&first, ""));
        assert!(!TraceCanonicalKey::matches_trace_key(
            &first,
            &streamed[..streamed.len() - 1]
        ));
        assert!(!TraceCanonicalKey::matches_trace_key(
            &first,
            &(streamed.clone() + "x")
        ));
        assert!(!TraceCanonicalKey::matches_trace_key(
            &first,
            &streamed.replace("trk1:", "trk1:한")
        ));
    }

    // Keep the former allocation-heavy implementation as an independent byte
    // reference in tests only. Public trk1 identities must not change.
    fn legacy_step_key(step: &TraceStepCanonicalKey) -> String {
        let hold_piece = |piece: Option<PieceKind>| {
            piece
                .map(|piece| piece.as_ascii().to_string())
                .unwrap_or_else(|| "none".to_owned())
        };
        let decision = match step.hold_decision {
            HoldDecision::None => "none".to_owned(),
            HoldDecision::SwapWithHold {
                incoming_piece,
                held_piece,
            } => format!("swap{}{}", incoming_piece.as_ascii(), held_piece.as_ascii()),
            HoldDecision::StoreIncoming {
                stored_piece,
                drawn_piece,
            } => format!("store{}{}", stored_piece.as_ascii(), drawn_piece.as_ascii()),
            HoldDecision::ReleaseHeldAtTerminal { held_piece } => {
                format!("terminal{}", held_piece.as_ascii())
            }
        };
        format!(
            "a{}i{}o{}ih{}oh{}d{}p{}r{}x{}y{}m{:016x}",
            step.active_piece.as_ascii(),
            step.input_cursor,
            step.output_cursor,
            hold_piece(step.input_hold_piece),
            hold_piece(step.output_hold_piece),
            decision,
            step.placed_piece.as_ascii(),
            step.rotation.quarter_turns(),
            step.x,
            step.y,
            step.mask
        )
    }

    #[test]
    fn streaming_key_preserves_all_hold_variants_extreme_values_and_separators() {
        let decisions = [
            HoldDecision::None,
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::I,
                held_piece: PieceKind::T,
            },
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::S,
                drawn_piece: PieceKind::Z,
            },
            HoldDecision::ReleaseHeldAtTerminal {
                held_piece: PieceKind::J,
            },
        ];
        let steps = decisions
            .into_iter()
            .enumerate()
            .map(|(index, hold_decision)| TraceStepCanonicalKey {
                active_piece: PieceKind::L,
                input_cursor: usize::MAX - index,
                output_cursor: index,
                input_hold_piece: if index % 2 == 0 {
                    None
                } else {
                    Some(PieceKind::O)
                },
                output_hold_piece: if index % 2 == 0 {
                    Some(PieceKind::T)
                } else {
                    None
                },
                hold_decision,
                placed_piece: PieceKind::L,
                rotation: RotationState::Zero,
                x: u16::MAX,
                y: index as u16,
                mask: u64::MAX - index as u64,
            })
            .collect::<Vec<_>>();
        let expected = format!(
            "trk1:{}",
            steps
                .iter()
                .map(legacy_step_key)
                .collect::<Vec<_>>()
                .join("~")
        );
        let key = TraceCanonicalKey { steps };
        assert_eq!(key.stable_key(), expected);
        assert_eq!(key_length(key.steps.iter().copied()), Some(expected.len()));
        let mut matcher = KeyMatcher {
            remaining: &expected,
        };
        write_key(&mut matcher, key.steps.iter().copied()).unwrap();
        assert!(matcher.remaining.is_empty());
    }

    #[test]
    fn empty_key_and_length_overflow_are_explicit() {
        let trace = SolutionTrace::new(Vec::new());
        assert_eq!(TraceCanonicalKey::stable_key_from_trace(&trace), "trk1:");
        assert_eq!(TraceCanonicalKey::checked_trace_key_bytes(&trace), Some(5));
        assert!(TraceCanonicalKey::matches_trace_key(&trace, "trk1:"));
        assert!(!TraceCanonicalKey::matches_trace_key(&trace, "trk1:~"));
        let mut length = KeyLength(usize::MAX);
        assert!(length.write_str("x").is_err());
        assert_eq!(length.0, usize::MAX);
    }
}
use core::fmt::{self, Write};
