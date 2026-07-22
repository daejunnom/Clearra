#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OccupancyField {
    pub width: u8,
    pub height: u8,
    pub mask: u64,
}

impl OccupancyField {
    pub fn new(width: u8, height: u8, mask: u64) -> Result<Self, OccupancyFieldError> {
        if width == 0 || height == 0 {
            return Err(OccupancyFieldError::EmptyDimensions);
        }
        let cells = u16::from(width) * u16::from(height);
        if cells > 64 {
            return Err(OccupancyFieldError::Board64LimitExceeded { cells });
        }
        let field_mask = field_mask(cells);
        if (mask & !field_mask) != 0 {
            return Err(OccupancyFieldError::MaskOutsideField { mask, field_mask });
        }

        Ok(Self {
            width,
            height,
            mask,
        })
    }
}
impl OccupancyField {
    pub fn empty(width: u8, height: u8) -> Result<Self, OccupancyFieldError> {
        Self::new(width, height, 0)
    }
}
impl OccupancyField {
    pub fn bit_index(self, x: u8, y: u8) -> Result<u8, OccupancyFieldError> {
        if x >= self.width || y >= self.height {
            return Err(OccupancyFieldError::CoordinateOutOfBounds { x, y });
        }
        Ok(y * self.width + x)
    }
}
impl OccupancyField {
    pub fn is_occupied(self, x: u8, y: u8) -> Result<bool, OccupancyFieldError> {
        let bit = self.bit_index(x, y)?;
        Ok((self.mask & (1_u64 << bit)) != 0)
    }
}
impl OccupancyField {
    pub fn field_mask(self) -> u64 {
        field_mask(u16::from(self.width) * u16::from(self.height))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OccupancyFieldError {
    EmptyDimensions,
    Board64LimitExceeded { cells: u16 },
    MaskOutsideField { mask: u64, field_mask: u64 },
    CoordinateOutOfBounds { x: u8, y: u8 },
}

fn field_mask(cells: u16) -> u64 {
    if cells == 64 {
        u64::MAX
    } else {
        (1_u64 << cells) - 1
    }
}

#[cfg(test)]
#[path = "occupancy_field_tests.rs"]
mod tests;
