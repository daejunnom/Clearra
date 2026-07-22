use super::*;

#[test]
fn accepts_even_positive_line_targets() {
    assert_eq!(PcTarget::new(2).map(PcTarget::lines), Ok(2));
    assert_eq!(PcTarget::new(6).map(PcTarget::lines), Ok(6));
}

#[test]
fn rejects_zero_and_odd_line_targets() {
    assert_eq!(PcTarget::new(0), Err(PcTargetError::ZeroLines));
    assert_eq!(
        PcTarget::new(3),
        Err(PcTargetError::OddLineCount { lines: 3 })
    );
}
