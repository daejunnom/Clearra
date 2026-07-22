use crate::RenderError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderCell {
    Empty,
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
    Garbage,
}

impl RenderCell {
    pub fn from_char(value: char) -> Result<Self, RenderError> {
        match value.to_ascii_uppercase() {
            '.' | '_' | ' ' => Ok(Self::Empty),
            'I' => Ok(Self::I),
            'O' => Ok(Self::O),
            'T' => Ok(Self::T),
            'S' => Ok(Self::S),
            'Z' => Ok(Self::Z),
            'J' => Ok(Self::J),
            'L' => Ok(Self::L),
            'G' | 'X' => Ok(Self::Garbage),
            unknown => Err(RenderError::UnknownCell { value: unknown }),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderBoard {
    width: usize,
    height: usize,
    cells: Vec<RenderCell>,
}

impl RenderBoard {
    pub fn from_rows(rows: &[&str]) -> Result<Self, RenderError> {
        let first = rows.first().ok_or(RenderError::InvalidBoardRows)?;
        let width = first.chars().count();
        if width == 0 || rows.iter().any(|row| row.chars().count() != width) {
            return Err(RenderError::InvalidBoardRows);
        }

        let cells = rows
            .iter()
            .flat_map(|row| row.chars())
            .map(RenderCell::from_char)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            width,
            height: rows.len(),
            cells,
        })
    }
}
impl RenderBoard {
    pub const fn width(&self) -> usize {
        self.width
    }
}
impl RenderBoard {
    pub const fn height(&self) -> usize {
        self.height
    }
}
impl RenderBoard {
    pub fn cell(&self, x: usize, y: usize) -> RenderCell {
        self.cells[y * self.width + x]
    }
}
impl RenderBoard {
    pub fn occupied_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut found = false;

        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(x, y) == RenderCell::Empty {
                    continue;
                }
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found = true;
            }
        }

        found.then_some((min_x, min_y, max_x, max_y))
    }
}
