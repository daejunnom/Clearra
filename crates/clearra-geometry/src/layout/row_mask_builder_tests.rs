use super::*;

#[test]
fn creates_masks_for_each_row() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");

    assert_eq!(row_mask(layout, 0), Some(0x03ff));
    assert_eq!(row_mask(layout, 1), Some(0x03ff << 10));
    assert_eq!(row_mask(layout, 6), None);
}

#[test]
fn creates_board128_masks_for_each_row() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("10x12 fits in u128");

    assert_eq!(row_mask128(layout, 0), Some(0x03ff));
    assert_eq!(row_mask128(layout, 11), Some(0x03ff_u128 << 110));
    assert_eq!(row_mask128(layout, 12), None);
}
