use crate::model::SpinStructureQuery;

/// Checks the explicit logical fill window.  This is a query constraint, not
/// an estimate: every cleared row must be inside the configured half-open
/// interval and the requested line count must match exactly.
pub(crate) fn accepts(query: &SpinStructureQuery, cleared_rows: u32, lines: u8) -> bool {
    if !query.line_requirement.accepts(lines) {
        return false;
    }
    let below_top = if query.fill_top == 32 {
        u32::MAX
    } else {
        (1_u32 << query.fill_top) - 1
    };
    let below_bottom = (1_u32 << query.fill_bottom) - 1;
    let allowed = below_top & !below_bottom;
    cleared_rows & !allowed == 0
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    use super::*;
    use crate::{
        logical::{apply_physical_lock, LogicalBoard},
        model::{PieceInventory, SpinLineRequirement, SpinStructureMode},
        StructureBoard,
    };

    #[test]
    fn fill_window_is_half_open_and_exact() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T]).expect("inventory"),
            SpinStructureMode::TSpins,
        );
        query.fill_bottom = 1;
        query.fill_top = 3;
        query.line_requirement = SpinLineRequirement::AtLeast(1);
        assert!(accepts(&query, 1 << 1, 1));
        assert!(accepts(&query, (1 << 1) | (1 << 2), 2));
        assert!(!accepts(&query, 1, 1));
        assert!(!accepts(&query, 1 << 3, 1));
    }

    #[test]
    fn prior_clear_cannot_shift_a_line_into_the_fill_window() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::L]).expect("inventory"),
            SpinStructureMode::AllSpin,
        );
        query.height = 4;
        query.fill_bottom = 0;
        query.fill_top = 3;
        query.line_requirement = SpinLineRequirement::AtLeast(1);

        let initial = StructureBoard::from_rows(&[0x03ff, 0, 0, 0x03fe]).expect("field");
        let logical = LogicalBoard::from_initial(initial);
        let deleted = logical.initial_deleted_rows(query.height);
        let physical_mask = StructureBoard::from_rows(&[0b111, 0, 1]).expect("four-cell lock");
        let lifted = apply_physical_lock(
            logical,
            deleted,
            query.height,
            PieceKind::L,
            RotationState::Zero,
            0,
            physical_mask,
        )
        .expect("bounded lift");

        assert_eq!(lifted.newly_deleted_rows, 1 << 3);
        assert!(accepts(&query, 1 << 2, 1));
        assert!(!accepts(&query, lifted.newly_deleted_rows, 1));
    }
}
