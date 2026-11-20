//! Test-only arithmetic model, not an extended pruning mode. Coordinates
//! remain in the original ILC frame even when individual masks span gaps.

use std::collections::BTreeSet;

use super::*;

const WIDTH: u16 = 10;
const HEIGHT: u16 = 24;
const CENTER: i16 = 64;

type Projection = (u8, i16);

fn represented_bit(checker_halved: i16) -> Option<u128> {
    u32::try_from(CENTER + checker_halved)
        .ok()
        .and_then(|bit| 1_u128.checked_shl(bit))
}

fn original_frame_projection(cells: ExtendedBoard) -> Projection {
    let mut column_sum = 0_u16;
    let mut checker_delta = 0_i16;
    for cell in cells.cells() {
        let (x, y) = (cell % WIDTH, cell / WIDTH);
        column_sum += x;
        checker_delta += if (x + y) % 2 == 0 { 1 } else { -1 };
    }
    assert_eq!(checker_delta.rem_euclid(2), 0);
    ((column_sum % 4) as u8, checker_delta / 2)
}

fn vertical_frame_tiles(light: bool) -> Vec<ExtendedBoard> {
    // For each column and color, partition its 12 cells among three vertical
    // I skeletons. Gaps are possible in an ILC mask; this is a partition of
    // the original frame, not a claim about a legal line-clear schedule.
    (0..WIDTH)
        .flat_map(|x| {
            (0..3).map(move |block| {
                let mut cells = ExtendedBoard::EMPTY;
                let first_y = (x + u16::from(!light)) % 2 + 8 * block;
                for offset in [0, 2, 4, 6] {
                    assert!(cells.insert((first_y + offset) * WIDTH + x));
                }
                cells
            })
        })
        .collect()
}

fn integer_convolution(
    domain: &BTreeSet<Projection>,
    options: &[Projection],
) -> BTreeSet<Projection> {
    domain
        .iter()
        .flat_map(|&(m, d)| options.iter().map(move |&(n, e)| ((m + n) % 4, d + e)))
        .collect()
}

#[test]
fn wide_checker_projection_covers_every_aligned_disjoint_prefix_balance() {
    // Each complete original row has five cells of each color regardless
    // of its original y. A chosen subset or initial occupancy only reduces
    // those capacities. Tetromino prefixes always have an aligned area.
    for lines in 1..=HEIGHT {
        let capacity = (WIDTH / 2 * lines) as i16;
        for light in 0..=capacity {
            for dark in 0..=capacity {
                if (light + dark) % 4 != 0 {
                    continue;
                }
                let half = (light - dark) / 2;
                assert!(half.abs() <= 60);
                assert!(represented_bit(half).is_some());
            }
        }
    }
    assert!(represented_bit(-60).is_some());
    assert!(represented_bit(60).is_some());
    assert!(represented_bit(-66).is_none());
    assert!(represented_bit(64).is_none());
}

#[test]
fn wide_checker_projection_keeps_disjoint_temporal_tiles_when_overlap_sums_clip() {
    let light = vertical_frame_tiles(true);
    let dark = vertical_frame_tiles(false);
    let mut horizontal = ExtendedBoard::EMPTY;
    for x in 0..4 {
        assert!(horizontal.insert(x));
    }
    let options = [light[0], dark[0], horizontal].map(original_frame_projection);
    assert_eq!(options, [(0, 2), (0, -2), (2, 0)]);

    // Keep a dark skeleton's cells as pre-existing occupancy. A witness
    // partitions the remaining 236 cells, ordered all light then all dark.
    let initial = dark[0];
    let mut prefix = ExtendedBoard::EMPTY;
    let encoded_options = options.into_iter().fold(0_u32, |mask, (residue, half)| {
        mask | (1 << (i16::from(residue) * 5 + half + 2))
    });
    let mut integer_domain = BTreeSet::from([(0, 0)]);
    let mut packed = [represented_bit(0).unwrap(), 0, 0, 0];

    for (index, tile) in light
        .into_iter()
        .chain(dark.into_iter().skip(1))
        .enumerate()
    {
        assert_eq!(tile.count_ones(), 4);
        assert!(!prefix.intersects(tile) && !initial.intersects(tile));
        assert!(options.contains(&original_frame_projection(tile)));
        prefix = prefix.union(tile);
        integer_domain = integer_convolution(&integer_domain, &options);
        packed = convolve_column_checker(packed, encoded_options);

        // This step must hold for each prefix in a valid disjoint cover, not
        // only for its terminal sum. Clipped overflow may affect fictitious
        // sums from repeated/intersecting rows but never this invariant.
        let (residue, half) = original_frame_projection(prefix);
        assert!(integer_domain.contains(&(residue, half)));
        assert_ne!(
            packed[usize::from(residue)] & represented_bit(half).unwrap(),
            0
        );

        if index < 31 {
            let mut independently_encoded = [0_u128; 4];
            for &(residue, half) in &integer_domain {
                independently_encoded[usize::from(residue)] |= represented_bit(half).unwrap();
            }
            assert_eq!(packed, independently_encoded);
        }
        if index == 29 {
            assert_eq!((residue, half), (0, 60));
        }
        if index == 31 {
            // 32 all-light choices are legal in the additive relaxation but
            // require 128 light cells; the concrete frame has only 120.
            assert!(integer_domain.contains(&(0, 64)));
            assert!(represented_bit(64).is_none());
        }
    }
    assert_eq!(prefix.count_ones(), 236);
    assert_eq!(
        prefix.union(initial).count_ones(),
        u32::from(WIDTH * HEIGHT)
    );
    assert_eq!(original_frame_projection(prefix), (0, 2));
}
