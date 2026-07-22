use super::*;

#[test]
fn occupancy_field_uses_bottom_up_row_major_bit_index() {
    let field = OccupancyField::empty(10, 4).expect("field");

    assert_eq!(field.bit_index(0, 0), Ok(0));
    assert_eq!(field.bit_index(3, 2), Ok(23));
}

#[test]
fn occupancy_field_has_no_color() {
    let field = OccupancyField::new(10, 2, 0x3ff).expect("field");

    assert_eq!(field.width, 10);
    assert_eq!(field.height, 2);
    assert_eq!(field.mask, 0x3ff);
}

#[test]
fn occupancy_field_has_no_owner() {
    let field = OccupancyField::new(4, 4, 0b1010).expect("field");

    assert_eq!(field.field_mask(), 0xffff);
    assert_eq!(field.is_occupied(1, 0), Ok(true));
}

#[test]
fn occupancy_field_rejects_mask_outside_field() {
    let err = OccupancyField::new(4, 4, 1_u64 << 20).expect_err("outside field");

    assert_eq!(
        err,
        OccupancyFieldError::MaskOutsideField {
            mask: 1_u64 << 20,
            field_mask: 0xffff
        }
    );
}
