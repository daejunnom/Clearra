use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::hold_automaton::HoldAutomatonState;

use super::piece_multiset_group::PieceMultisetKey;

/// Reusable exact projection of one concrete queue's hold language.
///
/// The placed multiset is conserved as `initial hold + drawn prefix - final
/// hold`. An occupied initial hold consumes exactly `n` queue pieces; an empty
/// hold either stays unused and consumes `n`, or is used and consumes `n + 1`.
/// Enumerating the possible final hold piece is therefore equivalent to the
/// full one-slot HoldAutomaton for unordered Packing supply.
pub(super) struct ReachableMultisetWorkspace {
    results: Vec<PieceMultisetKey>,
}

impl ReachableMultisetWorkspace {
    pub(super) fn new(placed_piece_count: usize) -> Self {
        Self {
            results: Vec::with_capacity(
                PieceKind::STANDARD_TETROMINOES
                    .len()
                    .min(placed_piece_count.saturating_add(1))
                    .saturating_add(1),
            ),
        }
    }

    pub(super) fn reachable_multisets(
        &mut self,
        sequence: &[PieceKind],
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> &[PieceMultisetKey] {
        self.results.clear();
        let start_cursor = usize::from(initial_hold.cursor());
        if start_cursor > sequence.len() {
            return &self.results;
        }
        let available = &sequence[start_cursor..];

        if !hold_enabled {
            if let Some(prefix) = available.get(..placed_piece_count) {
                self.results
                    .push(PieceMultisetKey::from_pieces(prefix.iter().copied()));
            }
            return &self.results;
        }

        match initial_hold.hold_piece() {
            Some(initial_piece) => {
                let Some(prefix) = available.get(..placed_piece_count) else {
                    return &self.results;
                };
                let mut available_multiset = PieceMultisetKey::from_pieces(prefix.iter().copied());
                available_multiset.push(initial_piece);
                self.record_final_hold_options(available_multiset, Some(initial_piece), prefix);
            }
            None => {
                if let Some(prefix) = available.get(..placed_piece_count) {
                    self.results
                        .push(PieceMultisetKey::from_pieces(prefix.iter().copied()));
                }
                if let Some(prefix_with_extra_draw) =
                    available.get(..placed_piece_count.saturating_add(1))
                {
                    let available_multiset =
                        PieceMultisetKey::from_pieces(prefix_with_extra_draw.iter().copied());
                    self.record_final_hold_options(
                        available_multiset,
                        None,
                        prefix_with_extra_draw,
                    );
                }
            }
        }

        self.results.sort_unstable();
        self.results.dedup();
        &self.results
    }

    fn record_final_hold_options(
        &mut self,
        available_multiset: PieceMultisetKey,
        initial_hold: Option<PieceKind>,
        drawn_prefix: &[PieceKind],
    ) {
        let mut final_hold_mask = initial_hold.map_or(0, piece_bit);
        for piece in drawn_prefix.iter().copied() {
            final_hold_mask |= piece_bit(piece);
        }
        for piece in PieceKind::STANDARD_TETROMINOES {
            if final_hold_mask & piece_bit(piece) == 0 {
                continue;
            }
            let mut placed = available_multiset;
            if placed.remove(piece) {
                self.results.push(placed);
            }
        }
    }
}

const fn piece_bit(piece: PieceKind) -> u8 {
    1_u8 << match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}
