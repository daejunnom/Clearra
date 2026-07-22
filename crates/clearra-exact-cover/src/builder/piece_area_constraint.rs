#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PieceAreaConstraint {
    target_area: usize,
    available_piece_areas: Vec<usize>,
}

impl PieceAreaConstraint {
    pub fn new(
        target_area: usize,
        available_piece_areas: impl IntoIterator<Item = usize>,
    ) -> Result<Self, PieceAreaConstraintError> {
        if target_area == 0 {
            return Err(PieceAreaConstraintError::ZeroTargetArea);
        }

        let available_piece_areas = available_piece_areas.into_iter().collect::<Vec<_>>();
        if available_piece_areas.is_empty() {
            return Err(PieceAreaConstraintError::EmptyPieceAreas);
        }
        if available_piece_areas.contains(&0) {
            return Err(PieceAreaConstraintError::ZeroPieceArea);
        }

        Ok(Self {
            target_area,
            available_piece_areas,
        })
    }
}
impl PieceAreaConstraint {
    pub fn target_area(&self) -> usize {
        self.target_area
    }
}
impl PieceAreaConstraint {
    pub fn available_piece_areas(&self) -> &[usize] {
        &self.available_piece_areas
    }
}
impl PieceAreaConstraint {
    pub fn can_fill_target(&self) -> bool {
        bounded_area_subset_sum(self.target_area, &self.available_piece_areas)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PieceAreaConstraintError {
    ZeroTargetArea,
    EmptyPieceAreas,
    ZeroPieceArea,
}

fn bounded_area_subset_sum(target: usize, piece_areas: &[usize]) -> bool {
    let mut reachable = vec![false; target + 1];
    reachable[0] = true;

    for area in piece_areas {
        if *area > target {
            continue;
        }
        for candidate in (*area..=target).rev() {
            reachable[candidate] = reachable[candidate] || reachable[candidate - *area];
        }
    }

    reachable[target]
}

#[cfg(test)]
#[path = "piece_area_constraint_tests.rs"]
mod tests;
