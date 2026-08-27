pub const STATIC_PARITY_REPORT_CONTRACT: &str = "parity-report.v1";
pub const STATIC_PARITY_COORDINATE_BASIS: &str = "global-bottom-left-y-up";
pub const STATIC_PARITY_PRUNING_AUTHORITY: &str = "none";

/// A representation-only observation over one immutable field page.
///
/// This report deliberately does not decide tileability or search
/// feasibility. Checker parity changes across some line-clear timelines, so a
/// static document observation cannot be promoted to a pruning certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticParityObservation {
    width: u16,
    height: u16,
    occupied_cell_count: u64,
    checker_black_count: u64,
    checker_white_count: u64,
    checker_delta: i64,
    four_color_counts: [u64; 4],
    even_column_count: u64,
    odd_column_count: u64,
    column_parity_delta: i64,
}

impl StaticParityObservation {
    pub fn from_row_major_occupancy(
        width: u16,
        height: u16,
        occupancy: &[bool],
    ) -> Result<Self, StaticParityObservationError> {
        if width == 0 || height == 0 {
            return Err(StaticParityObservationError::EmptyDimensions);
        }
        let expected = usize::from(width)
            .checked_mul(usize::from(height))
            .ok_or(StaticParityObservationError::CellCountOverflow)?;
        if occupancy.len() != expected {
            return Err(StaticParityObservationError::CellCountMismatch {
                expected,
                actual: occupancy.len(),
            });
        }

        let mut occupied_cell_count = 0_u64;
        let mut checker_black_count = 0_u64;
        let mut checker_white_count = 0_u64;
        let mut four_color_counts = [0_u64; 4];
        let mut even_column_count = 0_u64;
        let mut odd_column_count = 0_u64;
        let width_usize = usize::from(width);
        for (index, occupied) in occupancy.iter().copied().enumerate() {
            if !occupied {
                continue;
            }
            occupied_cell_count += 1;
            let x = index % width_usize;
            let y = index / width_usize;
            if (x + y) & 1 == 0 {
                checker_black_count += 1;
            } else {
                checker_white_count += 1;
            }
            four_color_counts[(x & 1) | ((y & 1) << 1)] += 1;
            if x & 1 == 0 {
                even_column_count += 1;
            } else {
                odd_column_count += 1;
            }
        }

        Ok(Self {
            width,
            height,
            occupied_cell_count,
            checker_black_count,
            checker_white_count,
            checker_delta: signed_delta(checker_black_count, checker_white_count)?,
            four_color_counts,
            even_column_count,
            odd_column_count,
            column_parity_delta: signed_delta(even_column_count, odd_column_count)?,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        STATIC_PARITY_REPORT_CONTRACT
    }

    pub const fn coordinate_basis(&self) -> &'static str {
        STATIC_PARITY_COORDINATE_BASIS
    }

    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub const fn occupied_cell_count(&self) -> u64 {
        self.occupied_cell_count
    }

    pub const fn checker_black_count(&self) -> u64 {
        self.checker_black_count
    }

    pub const fn checker_white_count(&self) -> u64 {
        self.checker_white_count
    }

    pub const fn checker_delta(&self) -> i64 {
        self.checker_delta
    }

    /// `(x mod 2, y mod 2)` in the order `(0,0), (1,0), (0,1), (1,1)`.
    pub const fn four_color_counts(&self) -> [u64; 4] {
        self.four_color_counts
    }

    pub const fn even_column_count(&self) -> u64 {
        self.even_column_count
    }

    pub const fn odd_column_count(&self) -> u64 {
        self.odd_column_count
    }

    pub const fn column_parity_delta(&self) -> i64 {
        self.column_parity_delta
    }

    pub const fn occupied_area_mod_four(&self) -> u8 {
        (self.occupied_cell_count % 4) as u8
    }

    pub const fn feasibility_claim(&self) -> bool {
        false
    }

    pub const fn pruning_authority(&self) -> &'static str {
        STATIC_PARITY_PRUNING_AUTHORITY
    }
}

fn signed_delta(positive: u64, negative: u64) -> Result<i64, StaticParityObservationError> {
    let positive =
        i64::try_from(positive).map_err(|_| StaticParityObservationError::SignedCountOverflow)?;
    let negative =
        i64::try_from(negative).map_err(|_| StaticParityObservationError::SignedCountOverflow)?;
    positive
        .checked_sub(negative)
        .ok_or(StaticParityObservationError::SignedCountOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticParityObservationError {
    EmptyDimensions,
    CellCountOverflow,
    CellCountMismatch { expected: usize, actual: usize },
    SignedCountOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_global_checker_four_color_and_column_signatures() {
        // y=1  X . X .
        // y=0  X X . .
        let report = StaticParityObservation::from_row_major_occupancy(
            4,
            2,
            &[true, true, false, false, true, false, true, false],
        )
        .expect("static observation");

        assert_eq!(report.contract_id(), "parity-report.v1");
        assert_eq!(report.coordinate_basis(), "global-bottom-left-y-up");
        assert_eq!(report.occupied_cell_count(), 4);
        assert_eq!(report.checker_black_count(), 1);
        assert_eq!(report.checker_white_count(), 3);
        assert_eq!(report.checker_delta(), -2);
        assert_eq!(report.four_color_counts(), [1, 1, 2, 0]);
        assert_eq!(report.even_column_count(), 3);
        assert_eq!(report.odd_column_count(), 1);
        assert_eq!(report.column_parity_delta(), 2);
        assert_eq!(report.occupied_area_mod_four(), 0);
        assert!(!report.feasibility_claim());
        assert_eq!(report.pruning_authority(), "none");
    }

    #[test]
    fn coordinate_shift_changes_checker_observation_without_becoming_a_claim() {
        let left = StaticParityObservation::from_row_major_occupancy(2, 1, &[true, false]).unwrap();
        let right =
            StaticParityObservation::from_row_major_occupancy(2, 1, &[false, true]).unwrap();
        assert_eq!(left.checker_delta(), 1);
        assert_eq!(right.checker_delta(), -1);
        assert!(!left.feasibility_claim());
        assert!(!right.feasibility_claim());
    }

    #[test]
    fn rejects_shape_mismatch_instead_of_truncating_or_padding() {
        assert_eq!(
            StaticParityObservation::from_row_major_occupancy(2, 2, &[true]),
            Err(StaticParityObservationError::CellCountMismatch {
                expected: 4,
                actual: 1,
            })
        );
    }
}
