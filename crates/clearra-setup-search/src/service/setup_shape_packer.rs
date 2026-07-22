use clearra_core_domain::piece::piece_kind::PieceKind;

pub(crate) fn pack_piece_sequence(pieces: &[PieceKind]) -> Option<u64> {
    let mut mask = 0_u64;
    let mut x = 0_u16;
    let mut y = 0_u16;
    let mut row_height = 0_u16;

    for piece in pieces {
        let shape = setup_piece_shape(*piece);
        if x + shape.width > 10 {
            x = 0;
            y += row_height.max(1);
            row_height = 0;
        }
        if y + shape.height > 6 {
            return None;
        }
        let shifted = shift_shape(shape.mask, x, y);
        if mask & shifted != 0 {
            x = 0;
            y += row_height.max(1);
            row_height = 0;
            if y + shape.height > 6 {
                return None;
            }
        }
        let shifted = shift_shape(shape.mask, x, y);
        if mask & shifted != 0 {
            return None;
        }
        mask |= shifted;
        x += shape.width;
        row_height = row_height.max(shape.height);
    }

    (mask != 0).then_some(mask)
}

pub(crate) fn visible_height_for_mask(mask: u64) -> u16 {
    if mask == 0 {
        return 1;
    }
    let highest = 63 - mask.leading_zeros() as u16;
    (highest / 10 + 1).clamp(1, 6)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SetupPieceShape {
    mask: u64,
    width: u16,
    height: u16,
}

fn setup_piece_shape(piece: PieceKind) -> SetupPieceShape {
    match piece {
        PieceKind::I => SetupPieceShape {
            mask: 0b1111,
            width: 4,
            height: 1,
        },
        PieceKind::O => SetupPieceShape {
            mask: 0b0011 | (0b0011 << 10),
            width: 2,
            height: 2,
        },
        PieceKind::T => SetupPieceShape {
            mask: 0b0111 | (0b0010 << 10),
            width: 3,
            height: 2,
        },
        PieceKind::S => SetupPieceShape {
            mask: 0b0110 | (0b0011 << 10),
            width: 3,
            height: 2,
        },
        PieceKind::Z => SetupPieceShape {
            mask: 0b0011 | (0b0110 << 10),
            width: 3,
            height: 2,
        },
        PieceKind::J => SetupPieceShape {
            mask: 0b0111 | (0b0001 << 10),
            width: 3,
            height: 2,
        },
        PieceKind::L => SetupPieceShape {
            mask: 0b0111 | (0b0100 << 10),
            width: 3,
            height: 2,
        },
    }
}

fn shift_shape(mask: u64, x: u16, y: u16) -> u64 {
    mask << (u64::from(y) * 10 + u64::from(x))
}
