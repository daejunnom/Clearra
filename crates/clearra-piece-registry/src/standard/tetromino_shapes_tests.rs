use clearra_core_domain::piece::rotation::RotationState;

use super::*;

fn cells(definition: PieceDefinition, rotation: RotationState) -> Vec<(i8, i8)> {
    let mut cells = definition
        .shape(rotation)
        .cells()
        .into_iter()
        .map(|cell| (cell.x(), cell.y()))
        .collect::<Vec<_>>();
    cells.sort_unstable();
    cells
}

fn rotate_clockwise_and_normalize(cells: &[(i8, i8)]) -> Vec<(i8, i8)> {
    let mut rotated = cells.iter().map(|(x, y)| (*y, -*x)).collect::<Vec<_>>();
    let min_x = rotated.iter().map(|(x, _)| *x).min().expect("cells");
    let min_y = rotated.iter().map(|(_, y)| *y).min().expect("cells");
    for (x, y) in &mut rotated {
        *x -= min_x;
        *y -= min_y;
    }
    rotated.sort_unstable();
    rotated
}

#[test]
fn every_standard_definition_has_four_cells_per_rotation() {
    for definition in STANDARD_TETROMINO_DEFINITIONS {
        for shape in definition.rotations() {
            assert_eq!(shape.cells().len(), 4);
            assert!(shape.width() >= 1);
            assert!(shape.height() >= 1);
        }
    }
}

#[test]
fn expected_bounding_boxes_are_available() {
    assert_eq!(I_DEFINITION.shape(RotationState::Zero).width(), 4);
    assert_eq!(I_DEFINITION.shape(RotationState::Right).height(), 4);
    assert_eq!(O_DEFINITION.shape(RotationState::Zero).width(), 2);
    assert_eq!(O_DEFINITION.shape(RotationState::Zero).height(), 2);
}

#[test]
fn standard_rotation_states_follow_bottom_up_clockwise_geometry() {
    for definition in STANDARD_TETROMINO_DEFINITIONS {
        let zero = cells(definition, RotationState::Zero);
        let right = cells(definition, RotationState::Right);
        let reverse = cells(definition, RotationState::Two);
        let left = cells(definition, RotationState::Left);

        assert_eq!(rotate_clockwise_and_normalize(&zero), right);
        assert_eq!(rotate_clockwise_and_normalize(&right), reverse);
        assert_eq!(rotate_clockwise_and_normalize(&reverse), left);
        assert_eq!(rotate_clockwise_and_normalize(&left), zero);
    }
}

#[test]
fn s_and_z_names_match_bottom_up_spawn_geometry() {
    assert_eq!(
        cells(S_DEFINITION, RotationState::Zero),
        vec![(0, 0), (1, 0), (1, 1), (2, 1)]
    );
    assert_eq!(
        cells(Z_DEFINITION, RotationState::Zero),
        vec![(0, 1), (1, 0), (1, 1), (2, 0)]
    );
}
