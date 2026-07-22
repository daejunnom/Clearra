use super::*;

#[test]
fn rejects_zero_dimensions() {
    assert_eq!(BoardSize::new(0, 20), Err(BoardSizeError::ZeroWidth));
    assert_eq!(BoardSize::new(10, 0), Err(BoardSizeError::ZeroHeight));
}

#[test]
fn computes_area_without_truncating_to_u16() {
    let size = BoardSize::new(400, 400).expect("valid board size");
    assert_eq!(size.area(), 160_000);
}
