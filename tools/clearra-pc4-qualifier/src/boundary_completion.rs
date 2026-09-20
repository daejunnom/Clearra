use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use std::collections::{BTreeSet, HashSet};

const WIDTH: u8 = 10;
const HEIGHT: u8 = 4;
const ROW_MASK: u64 = (1_u64 << WIDTH) - 1;
const FIELD_MASK: u64 = (1_u64 << (WIDTH * HEIGHT)) - 1;
const PROJECTED_COVER_STATE_LIMIT: usize = 250_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompletionNecessity {
    Retain,
    DeadFullColumnStrip,
    DeadProjectedCover,
    UnknownProjectedCover,
}

pub(crate) struct CompletionNecessaryOracle {
    domains: [ProjectedCoverDomain; (HEIGHT + 1) as usize],
}

impl CompletionNecessaryOracle {
    pub(crate) fn compile() -> Self {
        Self {
            domains: std::array::from_fn(|height| ProjectedCoverDomain::compile(height as u8)),
        }
    }

    pub(crate) fn classify(&self, cells: u64) -> Result<CompletionNecessity, String> {
        if cells & !FIELD_MASK != 0 {
            return Err("completion-necessity field lies outside four rows".to_owned());
        }
        if indivisible_full_column_strip(cells) {
            return Ok(CompletionNecessity::DeadFullColumnStrip);
        }
        let mut cleared_prefix = 0_u8;
        while cleared_prefix < HEIGHT
            && (cells >> (usize::from(cleared_prefix) * usize::from(WIDTH))) & ROW_MASK == ROW_MASK
        {
            cleared_prefix += 1;
        }
        for row in cleared_prefix..HEIGHT {
            if (cells >> (usize::from(row) * usize::from(WIDTH))) & ROW_MASK == ROW_MASK {
                return Err(
                    "completion-necessity field has a noncanonical full-row prefix".to_owned(),
                );
            }
        }
        let height = HEIGHT - cleared_prefix;
        if height == 0 {
            return Ok(CompletionNecessity::Retain);
        }
        let normalized = cells >> (usize::from(cleared_prefix) * usize::from(WIDTH));
        let active_mask = (1_u64 << (usize::from(height) * usize::from(WIDTH))) - 1;
        let empty = active_mask & !normalized;
        if !empty.count_ones().is_multiple_of(4) {
            return Ok(CompletionNecessity::DeadProjectedCover);
        }
        Ok(match self.domains[usize::from(height)].has_cover(empty) {
            ProjectedCoverResult::Cover => CompletionNecessity::Retain,
            ProjectedCoverResult::NoCover => CompletionNecessity::DeadProjectedCover,
            ProjectedCoverResult::Unknown => CompletionNecessity::UnknownProjectedCover,
        })
    }
}

struct ProjectedCoverDomain {
    supports: [Vec<u64>; (WIDTH * HEIGHT) as usize],
}

impl ProjectedCoverDomain {
    fn compile(height: u8) -> Self {
        let mut masks = BTreeSet::new();
        if height != 0 {
            let registry = standard_tetromino_registry();
            for piece in PieceKind::STANDARD_TETROMINOES {
                let definition = registry
                    .get(piece)
                    .expect("standard tetromino registry is complete");
                for rotation in RotationState::ALL {
                    let shape = definition.shape(rotation);
                    if shape.height() > height {
                        continue;
                    }
                    for rows in 1_u8..(1_u8 << height) {
                        if rows.count_ones() != u32::from(shape.height()) {
                            continue;
                        }
                        let lifted = (0..height)
                            .filter(|row| rows & (1 << row) != 0)
                            .collect::<Vec<_>>();
                        for x in 0..=WIDTH - shape.width() {
                            let mut mask = 0_u64;
                            for cell in shape.cells() {
                                let target_y = lifted[cell.y() as usize];
                                let target_x = usize::from(x) + cell.x() as usize;
                                mask |= 1_u64
                                    << (usize::from(target_y) * usize::from(WIDTH) + target_x);
                            }
                            masks.insert(mask);
                        }
                    }
                }
            }
        }
        let supports = std::array::from_fn(|cell| {
            masks
                .iter()
                .copied()
                .filter(|mask| mask & (1_u64 << cell) != 0)
                .collect()
        });
        Self { supports }
    }

    fn has_cover(&self, empty: u64) -> ProjectedCoverResult {
        projected_cover(empty, &self.supports, &mut HashSet::new(), &mut 0_usize)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectedCoverResult {
    Cover,
    NoCover,
    Unknown,
}

fn projected_cover(
    empty: u64,
    supports: &[Vec<u64>; (WIDTH * HEIGHT) as usize],
    dead: &mut HashSet<u64>,
    visited: &mut usize,
) -> ProjectedCoverResult {
    if empty == 0 {
        return ProjectedCoverResult::Cover;
    }
    if dead.contains(&empty) {
        return ProjectedCoverResult::NoCover;
    }
    if *visited >= PROJECTED_COVER_STATE_LIMIT {
        return ProjectedCoverResult::Unknown;
    }
    *visited += 1;

    let mut cells = empty;
    let mut selected_cell = None;
    let mut selected_support_count = usize::MAX;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        let support_count = supports[cell]
            .iter()
            .filter(|mask| **mask & !empty == 0)
            .count();
        if support_count == 0 {
            dead.insert(empty);
            return ProjectedCoverResult::NoCover;
        }
        if support_count < selected_support_count {
            selected_cell = Some(cell);
            selected_support_count = support_count;
        }
    }

    let mut unknown = false;
    for mask in &supports[selected_cell.expect("nonempty cover state has a selected cell")] {
        if mask & !empty != 0 {
            continue;
        }
        match projected_cover(empty ^ mask, supports, dead, visited) {
            ProjectedCoverResult::Cover => return ProjectedCoverResult::Cover,
            ProjectedCoverResult::Unknown => unknown = true,
            ProjectedCoverResult::NoCover => {}
        }
    }
    if unknown {
        ProjectedCoverResult::Unknown
    } else {
        dead.insert(empty);
        ProjectedCoverResult::NoCover
    }
}

fn indivisible_full_column_strip(cells: u64) -> bool {
    let mut empty = 0_usize;
    for x in 0..WIDTH {
        let vacancies = (0..HEIGHT)
            .filter(|y| {
                cells & (1_u64 << (usize::from(*y) * usize::from(WIDTH) + usize::from(x))) == 0
            })
            .count();
        if vacancies == 0 {
            if !empty.is_multiple_of(4) {
                return true;
            }
            empty = 0;
        } else {
            empty += vacancies;
        }
    }
    !empty.is_multiple_of(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn necessary_oracle_retains_known_coverable_fields() {
        let oracle = CompletionNecessaryOracle::compile();
        let full = FIELD_MASK;
        let canonical_o_vacancy = (3 << 20) | (3 << 30);
        assert_eq!(oracle.classify(0), Ok(CompletionNecessity::Retain));
        assert_eq!(oracle.classify(full), Ok(CompletionNecessity::Retain));
        assert_eq!(
            oracle.classify(full ^ canonical_o_vacancy),
            Ok(CompletionNecessity::Retain)
        );
    }

    #[test]
    fn necessary_oracle_proves_indivisible_column_strips_dead() {
        assert_eq!(
            CompletionNecessaryOracle::compile().classify(2_149_584_127),
            Ok(CompletionNecessity::DeadFullColumnStrip)
        );
    }

    #[test]
    fn projected_cover_budget_never_becomes_a_negative_proof() {
        let domain = ProjectedCoverDomain::compile(4);
        let mut dead = HashSet::new();
        let mut visited = PROJECTED_COVER_STATE_LIMIT;
        assert_eq!(
            projected_cover(FIELD_MASK, &domain.supports, &mut dead, &mut visited),
            ProjectedCoverResult::Unknown
        );
        assert!(dead.is_empty());
    }
}
