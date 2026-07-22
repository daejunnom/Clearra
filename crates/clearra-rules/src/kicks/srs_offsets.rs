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

pub fn srs_plus_i_180_offsets(from: RotationState, to: RotationState) -> Vec<KickOffset> {
    use RotationState::{Left, Right, Two, Zero};

    match (from, to) {
        (Zero, Two) => offsets([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)]),
        (Right, Left) => offsets([(1, 1), (1, 0), (0, 0), (2, 0), (0, 1), (2, 1)]),
        (Two, Zero) => offsets([(-1, -1), (0, -1), (0, 1), (0, 0), (-1, 1), (-1, 0)]),
        (Left, Right) => offsets([(0, 0), (-1, 0), (-1, 2), (-1, 1), (0, 2), (0, 1)]),
        _ => vec![],
    }
}

fn offsets<const N: usize>(values: [(i8, i8); N]) -> Vec<KickOffset> {
    values
        .into_iter()
        .map(|(dx, dy)| KickOffset::new(dx, dy))
        .collect()
}
