use crate::{WebCommandError, WebCommandErrorCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Ctk3FieldMask {
    occupied_mask: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Ctk3BoardMask {
    words: [u64; 4],
}

impl Ctk3BoardMask {
    pub(crate) const fn words(self) -> [u64; 4] {
        self.words
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.words[0] == 0 && self.words[1] == 0 && self.words[2] == 0 && self.words[3] == 0
    }

    pub(crate) fn count_ones(self) -> u32 {
        self.words.iter().map(|word| word.count_ones()).sum()
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.words
            .iter()
            .zip(other.words)
            .any(|(left, right)| left & right != 0)
    }

    pub(crate) fn visible_height(self) -> u8 {
        for bit in (0..240usize).rev() {
            if self.contains(bit) {
                return (bit / 10 + 1) as u8;
            }
        }
        0
    }

    pub(crate) fn contains_completed_row(self, height: u8) -> bool {
        (0..usize::from(height)).any(|y| (0..10usize).all(|x| self.contains(y * 10 + x)))
    }

    pub(crate) fn cli_hex(self) -> String {
        let canonical = format!(
            "{:016x}{:016x}{:016x}{:016x}",
            self.words[3], self.words[2], self.words[1], self.words[0]
        );
        let trimmed = canonical.trim_start_matches('0');
        format!("0x{}", if trimmed.is_empty() { "0" } else { trimmed })
    }

    fn contains(self, bit: usize) -> bool {
        self.words
            .get(bit / 64)
            .is_some_and(|word| word & (1u64 << (bit % 64)) != 0)
    }
}

impl Ctk3FieldMask {
    pub(crate) const fn occupied_mask(self) -> u64 {
        self.occupied_mask
    }

    pub(crate) fn visible_height(self) -> u8 {
        if self.occupied_mask == 0 {
            0
        } else {
            ((u64::BITS - self.occupied_mask.leading_zeros() - 1) / 10 + 1) as u8
        }
    }
}

pub(crate) fn parse_ctk3_field_mask(value: &str) -> Result<Ctk3FieldMask, WebCommandError> {
    if value.len() != 16
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "--field-mask-v1 must be one canonical 16-digit lowercase hexadecimal mask",
        ));
    }
    let occupied_mask = u64::from_str_radix(value, 16)
        .map_err(|_| invalid("--field-mask-v1 contains an invalid mask"))?;
    Ok(Ctk3FieldMask { occupied_mask })
}

pub(crate) fn parse_ctk3_board_mask(
    value: &str,
    option: &str,
) -> Result<Ctk3BoardMask, WebCommandError> {
    if value.len() != 60 || !is_canonical_hex(value) {
        return Err(invalid(format!(
            "{option} must be one canonical 60-digit lowercase hexadecimal mask"
        )));
    }
    let mut words = [0u64; 4];
    let mut end = value.len();
    for word in &mut words {
        let start = end.saturating_sub(16);
        *word = u64::from_str_radix(&value[start..end], 16)
            .map_err(|_| invalid(format!("{option} contains an invalid mask")))?;
        end = start;
        if end == 0 {
            break;
        }
    }
    Ok(Ctk3BoardMask { words })
}

fn is_canonical_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::InvalidValue, message)
}

#[cfg(test)]
mod tests {
    use super::{parse_ctk3_board_mask, parse_ctk3_field_mask};

    #[test]
    fn field_mask_preserves_board64_bit_sixty_three() {
        let field = parse_ctk3_field_mask("8000000000000000").expect("field mask");
        assert_eq!(field.occupied_mask(), 1u64 << 63);
        assert_eq!(field.visible_height(), 7);
    }

    #[test]
    fn field_mask_rejects_noncanonical_hex() {
        assert!(parse_ctk3_field_mask("000000000000000A").is_err());
        assert!(parse_ctk3_field_mask("0").is_err());
        assert!(parse_ctk3_field_mask("0000000000000000:0000000000000000").is_err());
    }

    #[test]
    fn board_mask_preserves_the_full_24_row_contract() {
        let mask = parse_ctk3_board_mask(
            "80000000000000000000000000000000000000000000000000000000000f",
            "--target-mask-v1",
        )
        .expect("board mask");
        assert_eq!(mask.visible_height(), 24);
        assert_eq!(mask.count_ones(), 5);
        assert_eq!(
            mask.cli_hex(),
            "0x80000000000000000000000000000000000000000000000000000000000f"
        );
    }

    #[test]
    fn board_mask_detects_overlap_and_completed_rows() {
        let full = parse_ctk3_board_mask(
            "000000000000000000000000000000000000000000000000003ff00003ff",
            "--base-mask-v1",
        )
        .expect("board mask");
        let low = parse_ctk3_board_mask(
            "000000000000000000000000000000000000000000000000000000000001",
            "--target-mask-v1",
        )
        .expect("board mask");
        assert!(full.intersects(low));
        assert!(full.contains_completed_row(3));
    }

    #[test]
    fn board_mask_rejects_noncanonical_hex() {
        assert!(parse_ctk3_board_mask(&"0".repeat(59), "--base-mask-v1").is_err());
        assert!(parse_ctk3_board_mask(&"0".repeat(61), "--base-mask-v1").is_err());
        assert!(
            parse_ctk3_board_mask(&format!("{}A", "0".repeat(59)), "--target-mask-v1",).is_err()
        );
    }
}
