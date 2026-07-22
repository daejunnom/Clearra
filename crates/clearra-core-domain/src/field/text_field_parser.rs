use super::occupancy_field::{OccupancyField, OccupancyFieldError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextFieldParserError {
    EmptyRows,
    WidthTooLarge {
        width: usize,
    },
    HeightTooLarge {
        height: usize,
    },
    RowWidthMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidCell {
        row: usize,
        column: usize,
        value: char,
    },
    Occupancy(OccupancyFieldError),
}

impl From<OccupancyFieldError> for TextFieldParserError {
    fn from(error: OccupancyFieldError) -> Self {
        Self::Occupancy(error)
    }
}

pub struct TextFieldParser;

impl TextFieldParser {
    pub fn parse_top_down_rows(rows: &[&str]) -> Result<OccupancyField, TextFieldParserError> {
        if rows.is_empty() {
            return Err(TextFieldParserError::EmptyRows);
        }
        let width = rows[0].chars().count();
        if width > usize::from(u8::MAX) {
            return Err(TextFieldParserError::WidthTooLarge { width });
        }
        if rows.len() > usize::from(u8::MAX) {
            return Err(TextFieldParserError::HeightTooLarge { height: rows.len() });
        }

        let height = rows.len();
        let mut mask = 0_u64;
        for (top_down_row, row) in rows.iter().enumerate() {
            let actual_width = row.chars().count();
            if actual_width != width {
                return Err(TextFieldParserError::RowWidthMismatch {
                    expected: width,
                    actual: actual_width,
                });
            }
            let internal_y = height - 1 - top_down_row;
            for (column, value) in row.chars().enumerate() {
                let occupied = match value {
                    '.' | '0' | '_' | ' ' => false,
                    '#' | 'X' | 'x' | '1' => true,
                    value => {
                        return Err(TextFieldParserError::InvalidCell {
                            row: top_down_row,
                            column,
                            value,
                        })
                    }
                };
                if occupied {
                    let bit = internal_y * width + column;
                    mask |= 1_u64 << bit;
                }
            }
        }

        OccupancyField::new(width as u8, height as u8, mask).map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "text_field_parser_tests.rs"]
mod tests;
