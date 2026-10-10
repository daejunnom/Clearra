//! Presentation mask with an explicit compact/full frame, never a low-word cast.
use clearra_core_domain::board::standard_pc_board::Board256Mask;
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcPathBoardMask {
    Compact(u64),
    FullHeight { height: u8, occupied: Board256Mask },
}

impl PcPathBoardMask {
    pub const fn compact(self) -> Option<u64> {
        match self {
            Self::Compact(mask) => Some(mask),
            Self::FullHeight { .. } => None,
        }
    }
    pub const fn words(self) -> [u64; 4] {
        match self {
            Self::Compact(mask) => [mask, 0, 0, 0],
            Self::FullHeight { occupied, .. } => occupied.words(),
        }
    }
    pub const fn is_empty(self) -> bool {
        match self {
            Self::Compact(mask) => mask == 0,
            Self::FullHeight { occupied, .. } => occupied.is_empty(),
        }
    }
}

// Compact empty checks stay exact. A high-word board cannot masquerade as its
// u64 prefix. No bit operations or implicit narrowing conversions are offered.
impl PartialEq<u64> for PcPathBoardMask {
    fn eq(&self, value: &u64) -> bool {
        self.words() == [*value, 0, 0, 0]
    }
}
impl fmt::LowerHex for PcPathBoardMask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compact(mask) => fmt::LowerHex::fmt(mask, formatter),
            Self::FullHeight { occupied, .. } => {
                let [a, b, c, d] = occupied.words();
                write!(formatter, "{d:016x}{c:016x}{b:016x}{a:016x}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_frame_hex_and_empty_checks_never_drop_high_words() {
        let mask = PcPathBoardMask::FullHeight {
            height: 24,
            occupied: Board256Mask::from_words([0, 0, 0, 1]),
        };
        assert_ne!(mask, 0);
        assert_eq!(mask.compact(), None);
        assert_eq!(
            format!("0x{mask:016x}"),
            "0x0000000000000001000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            format!("0x{:016x}", PcPathBoardMask::Compact(1)),
            "0x0000000000000001"
        );
    }
}
