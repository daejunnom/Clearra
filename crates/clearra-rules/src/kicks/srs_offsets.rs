use clearra_core_domain::piece::rotation::RotationState;

use super::kick_table::KickOffset;

pub fn jlstz_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Right) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(-1, 1),
            KickOffset::new(0, -2),
            KickOffset::new(-1, -2),
        ],
        (Right, Zero) | (Right, Two) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(1, 0),
            KickOffset::new(1, -1),
            KickOffset::new(0, 2),
            KickOffset::new(1, 2),
        ],
        (Two, Right) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(-1, 1),
            KickOffset::new(0, -2),
            KickOffset::new(-1, -2),
        ],
        (Two, Left) | (Zero, Left) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(1, 0),
            KickOffset::new(1, 1),
            KickOffset::new(0, -2),
            KickOffset::new(1, -2),
        ],
        (Left, Two) | (Left, Zero) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(-1, -1),
            KickOffset::new(0, 2),
            KickOffset::new(-1, 2),
        ],
        _ => vec![],
    }
}

pub fn srs_i_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Right) | (Left, Two) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(-2, 0),
            KickOffset::new(1, 0),
            KickOffset::new(-2, -1),
            KickOffset::new(1, 2),
        ],
        (Right, Zero) | (Two, Left) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(2, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(2, 1),
            KickOffset::new(-1, -2),
        ],
        (Right, Two) | (Zero, Left) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(-1, 0),
            KickOffset::new(2, 0),
            KickOffset::new(-1, 2),
            KickOffset::new(2, -1),
        ],
        (Two, Right) | (Left, Zero) => vec![
            KickOffset::new(0, 0),
            KickOffset::new(1, 0),
            KickOffset::new(-2, 0),
            KickOffset::new(1, -2),
            KickOffset::new(-2, 1),
        ],
        _ => vec![],
    }
}

pub fn srs_plus_i_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Right) => offsets([(0, 0), (1, 0), (-2, 0), (-2, -1), (1, 2)]),
        (Right, Zero) => offsets([(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)]),
        (Right, Two) => offsets([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        (Two, Right) => offsets([(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)]),
        (Zero, Left) => offsets([(0, 0), (-1, 0), (2, 0), (2, -1), (-1, 2)]),
        (Left, Zero) => offsets([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        (Left, Two) => offsets([(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)]),
        (Two, Left) => offsets([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        _ => vec![],
    }
}

pub fn eight_direction_transitions() -> [(RotationState, RotationState); 8] {
    use RotationState::{Left, Right, Two, Zero};

    [
        (Zero, Right),
        (Right, Two),
        (Two, Left),
        (Left, Zero),
        (Zero, Left),
        (Left, Two),
        (Two, Right),
        (Right, Zero),
    ]
}

pub fn one_eighty_transitions() -> [(RotationState, RotationState); 4] {
    use RotationState::{Left, Right, Two, Zero};

    [(Zero, Two), (Right, Left), (Two, Zero), (Left, Right)]
}

pub fn twelve_direction_transitions() -> [(RotationState, RotationState); 12] {
    let quarter_turns = eight_direction_transitions();
    let half_turns = one_eighty_transitions();
    [
        quarter_turns[0],
        quarter_turns[1],
        quarter_turns[2],
        quarter_turns[3],
        quarter_turns[4],
        quarter_turns[5],
        quarter_turns[6],
        quarter_turns[7],
        half_turns[0],
        half_turns[1],
        half_turns[2],
        half_turns[3],
    ]
}

pub fn srs_plus_jlstz_180_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Two) => offsets([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)]),
        (Two, Zero) => offsets([(0, 0), (0, -1), (-1, -1), (1, -1), (-1, 0), (1, 0)]),
        (Right, Left) => offsets([(0, 0), (1, 0), (1, 2), (1, 1), (0, 2), (0, 1)]),
        (Left, Right) => offsets([(0, 0), (-1, 0), (-1, 2), (-1, 1), (0, 2), (0, 1)]),
        _ => vec![],
    }
}

pub fn jstris_180_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Two) => offsets([(0, 0), (0, 1)]),
        (Right, Left) => offsets([(0, 0), (1, 0)]),
        (Two, Zero) => offsets([(0, 0), (0, -1)]),
        (Left, Right) => offsets([(0, 0), (-1, 0)]),
        _ => vec![],
    }
}

pub fn srs_plus_i_180_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    jstris_180_offsets(from, to)
}

/// TETR.IO SRS-X offsets for J/L/S/T/Z in Clearra's y-up coordinate system.
///
/// TETR.IO tests the unshifted rotation before consulting its table, so every
/// Clearra sequence explicitly begins with `(0, 0)`. The public source table
/// uses screen coordinates (positive y points down), hence the sign inversion
/// in the four half-turn tables below. Quarter turns are ordinary SRS.
pub fn srs_x_jlstz_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Two) => offsets([
            (0, 0),
            (1, 0),
            (2, 0),
            (1, -1),
            (2, -1),
            (-1, 0),
            (-2, 0),
            (-1, -1),
            (-2, -1),
            (0, 1),
            (3, 0),
            (-3, 0),
        ]),
        (Right, Left) => offsets([
            (0, 0),
            (0, -1),
            (0, -2),
            (-1, -1),
            (-1, -2),
            (0, 1),
            (0, 2),
            (-1, 1),
            (-1, 2),
            (1, 0),
            (0, -3),
            (0, 3),
        ]),
        (Two, Zero) => offsets([
            (0, 0),
            (-1, 0),
            (-2, 0),
            (-1, 1),
            (-2, 1),
            (1, 0),
            (2, 0),
            (1, 1),
            (2, 1),
            (0, -1),
            (-3, 0),
            (3, 0),
        ]),
        (Left, Right) => offsets([
            (0, 0),
            (0, -1),
            (0, -2),
            (1, -1),
            (1, -2),
            (0, 1),
            (0, 2),
            (1, 1),
            (1, 2),
            (-1, 0),
            (0, -3),
            (0, 3),
        ]),
        _ => jlstz_offsets(from, to),
    }
}

/// TETR.IO SRS-X offsets for I in Clearra's y-up coordinate system.
pub fn srs_x_i_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Two) => offsets([(0, 0), (-1, 0), (-2, 0), (1, 0), (2, 0), (0, -1)]),
        (Right, Left) => offsets([(0, 0), (0, -1), (0, -2), (0, 1), (0, 2), (-1, 0)]),
        (Two, Zero) => offsets([(0, 0), (1, 0), (2, 0), (-1, 0), (-2, 0), (0, 1)]),
        (Left, Right) => offsets([(0, 0), (0, -1), (0, -2), (0, 1), (0, 2), (1, 0)]),
        _ => srs_i_offsets(from, to),
    }
}

/// Standard TETR.IO `o` has `disallow_kick=true`; `oo_kicks` belongs to the
/// separate non-standard `oo` piece. Its rotation may still succeed at the
/// implicit origin attempt, which Clearra represents explicitly.
pub fn srs_x_o_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    if twelve_direction_transitions().contains(&(from, to)) {
        offsets([(0, 0)])
    } else {
        vec![]
    }
}

fn offsets<const N: usize>(values: [(i8, i8); N]) -> Vec<KickOffset> {
    values
        .into_iter()
        .map(|(dx, dy)| KickOffset::new(dx, dy))
        .collect()
}
