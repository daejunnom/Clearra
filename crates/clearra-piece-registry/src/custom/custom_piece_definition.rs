mod custom_piece {
    use clearra_core_domain::{ids::piece_id::PieceDefinitionId, piece::rotation::RotationState};

    use crate::{
        custom::custom_piece_schema::{
            PieceRotationBounds, PieceSourceProvenance, PieceSpawnOffset,
        },
        registry::piece_registry::ShapeCell,
    };

    use super::{
        custom_piece_validator::{validate_id, validate_rotations},
        CustomPieceDefinitionError, CustomPieceRotation, PieceDisplayMetadata, PieceSpawnBounds,
        PieceSymmetryClass,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct CustomPieceDefinition {
        id: PieceDefinitionId,
        label: String,
        rotations: Vec<CustomPieceRotation>,
        spawn_bounds: PieceSpawnBounds,
        spawn_offsets: Vec<PieceSpawnOffset>,
        display: PieceDisplayMetadata,
        area: usize,
        symmetry: PieceSymmetryClass,
        source_provenance: PieceSourceProvenance,
        canonical_key: String,
    }

    impl CustomPieceDefinition {
        pub fn new(
            id: PieceDefinitionId,
            label: impl Into<String>,
            rotations: Vec<CustomPieceRotation>,
            spawn_bounds: PieceSpawnBounds,
            display: PieceDisplayMetadata,
            symmetry: PieceSymmetryClass,
            canonical_key: impl Into<String>,
        ) -> Result<Self, CustomPieceDefinitionError> {
            validate_id(&id)?;
            let label = label.into();
            if label.trim().is_empty() {
                return Err(CustomPieceDefinitionError::EmptyLabel);
            }
            let canonical_key = canonical_key.into();
            if canonical_key.trim().is_empty() {
                return Err(CustomPieceDefinitionError::EmptyCanonicalKey);
            }
            let area = validate_rotations(&rotations)?;

            Ok(Self {
                id,
                label,
                rotations,
                spawn_bounds,
                spawn_offsets: Vec::new(),
                display,
                area,
                symmetry,
                source_provenance: PieceSourceProvenance::unspecified(),
                canonical_key,
            })
        }
    }
    impl CustomPieceDefinition {
        pub fn with_spawn_offsets(mut self, spawn_offsets: Vec<PieceSpawnOffset>) -> Self {
            self.spawn_offsets = spawn_offsets;
            self
        }
    }
    impl CustomPieceDefinition {
        pub fn with_source_provenance(mut self, source_provenance: PieceSourceProvenance) -> Self {
            self.source_provenance = source_provenance;
            self
        }
    }
    impl CustomPieceDefinition {
        pub fn id(&self) -> &PieceDefinitionId {
            &self.id
        }
    }
    impl CustomPieceDefinition {
        pub fn piece_definition_id(&self) -> &PieceDefinitionId {
            &self.id
        }
    }
    impl CustomPieceDefinition {
        pub fn label(&self) -> &str {
            &self.label
        }
    }
    impl CustomPieceDefinition {
        pub fn display_name(&self) -> &str {
            &self.label
        }
    }
    impl CustomPieceDefinition {
        pub fn rotations(&self) -> &[CustomPieceRotation] {
            &self.rotations
        }
    }
    impl CustomPieceDefinition {
        pub fn rotation_states(&self) -> Vec<RotationState> {
            self.rotations
                .iter()
                .map(CustomPieceRotation::state)
                .collect()
        }
    }
    impl CustomPieceDefinition {
        pub fn cells_by_rotation(&self) -> Vec<(RotationState, Vec<ShapeCell>)> {
            self.rotations
                .iter()
                .map(|rotation| (rotation.state(), rotation.cells().to_vec()))
                .collect()
        }
    }
    impl CustomPieceDefinition {
        pub fn bounds_by_rotation(&self) -> Vec<PieceRotationBounds> {
            self.rotations
                .iter()
                .filter_map(|rotation| {
                    rotation.cells().first().map(|first| {
                        let mut min_x = first.x();
                        let mut max_x = first.x();
                        let mut min_y = first.y();
                        let mut max_y = first.y();
                        for cell in rotation.cells() {
                            min_x = min_x.min(cell.x());
                            max_x = max_x.max(cell.x());
                            min_y = min_y.min(cell.y());
                            max_y = max_y.max(cell.y());
                        }
                        PieceRotationBounds::new(rotation.state(), min_x, max_x, min_y, max_y)
                    })
                })
                .collect()
        }
    }
    impl CustomPieceDefinition {
        pub fn spawn_bounds(&self) -> PieceSpawnBounds {
            self.spawn_bounds
        }
    }
    impl CustomPieceDefinition {
        pub fn spawn_offsets(&self) -> &[PieceSpawnOffset] {
            &self.spawn_offsets
        }
    }
    impl CustomPieceDefinition {
        pub fn display(&self) -> &PieceDisplayMetadata {
            &self.display
        }
    }
    impl CustomPieceDefinition {
        pub fn color_hint(&self) -> Option<&str> {
            self.display.color()
        }
    }
    impl CustomPieceDefinition {
        pub fn area(&self) -> usize {
            self.area
        }
    }
    impl CustomPieceDefinition {
        pub fn symmetry(&self) -> PieceSymmetryClass {
            self.symmetry
        }
    }
    impl CustomPieceDefinition {
        pub fn symmetry_class(&self) -> PieceSymmetryClass {
            self.symmetry
        }
    }
    impl CustomPieceDefinition {
        pub fn source_provenance(&self) -> &PieceSourceProvenance {
            &self.source_provenance
        }
    }
    impl CustomPieceDefinition {
        pub fn canonical_key(&self) -> &str {
            &self.canonical_key
        }
    }
    impl CustomPieceDefinition {
        pub fn name(&self) -> &str {
            self.id.as_str()
        }
    }
    impl CustomPieceDefinition {
        pub fn cell_count(&self) -> usize {
            self.area
        }
    }
}
mod custom_piece_definition_error {
    use clearra_core_domain::piece::rotation::RotationState;

    use crate::registry::piece_registry::ShapeCell;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum CustomPieceDefinitionError {
        EmptyId,
        EmptyLabel,
        EmptyCanonicalKey,
        EmptyRotations,
        EmptyRotationCells {
            state: RotationState,
        },
        DuplicateRotationState {
            state: RotationState,
        },
        DuplicateCell {
            state: RotationState,
            cell: ShapeCell,
        },
        InconsistentRotationArea {
            state: RotationState,
            expected: usize,
            actual: usize,
        },
    }
}
mod custom_piece_rotation {
    use clearra_core_domain::piece::rotation::RotationState;

    use crate::registry::piece_registry::ShapeCell;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct CustomPieceRotation {
        state: RotationState,
        cells: Vec<ShapeCell>,
    }

    impl CustomPieceRotation {
        pub fn new(state: RotationState, cells: Vec<ShapeCell>) -> Self {
            Self { state, cells }
        }
    }
    impl CustomPieceRotation {
        pub fn state(&self) -> RotationState {
            self.state
        }
    }
    impl CustomPieceRotation {
        pub fn cells(&self) -> &[ShapeCell] {
            &self.cells
        }
    }
}
mod custom_piece_validator {
    use clearra_core_domain::ids::piece_id::PieceDefinitionId;

    use super::{CustomPieceDefinitionError, CustomPieceRotation};

    pub(super) fn validate_id(id: &PieceDefinitionId) -> Result<(), CustomPieceDefinitionError> {
        if id.as_str().trim().is_empty() {
            Err(CustomPieceDefinitionError::EmptyId)
        } else {
            Ok(())
        }
    }

    pub(super) fn validate_rotations(
        rotations: &[CustomPieceRotation],
    ) -> Result<usize, CustomPieceDefinitionError> {
        let Some(first) = rotations.first() else {
            return Err(CustomPieceDefinitionError::EmptyRotations);
        };
        let area = first.cells().len();
        if area == 0 {
            return Err(CustomPieceDefinitionError::EmptyRotationCells {
                state: first.state(),
            });
        }

        let mut seen_states = Vec::new();
        for rotation in rotations {
            if seen_states.contains(&rotation.state()) {
                return Err(CustomPieceDefinitionError::DuplicateRotationState {
                    state: rotation.state(),
                });
            }
            seen_states.push(rotation.state());

            if rotation.cells().is_empty() {
                return Err(CustomPieceDefinitionError::EmptyRotationCells {
                    state: rotation.state(),
                });
            }
            if rotation.cells().len() != area {
                return Err(CustomPieceDefinitionError::InconsistentRotationArea {
                    state: rotation.state(),
                    expected: area,
                    actual: rotation.cells().len(),
                });
            }

            let mut seen_cells = Vec::new();
            for cell in rotation.cells() {
                if seen_cells.contains(cell) {
                    return Err(CustomPieceDefinitionError::DuplicateCell {
                        state: rotation.state(),
                        cell: *cell,
                    });
                }
                seen_cells.push(*cell);
            }
        }

        Ok(area)
    }
}
mod piece_display_metadata {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PieceDisplayMetadata {
        color: Option<String>,
        glyph: Option<String>,
    }

    impl PieceDisplayMetadata {
        pub fn new(color: Option<String>, glyph: Option<String>) -> Self {
            Self { color, glyph }
        }
    }
    impl PieceDisplayMetadata {
        pub fn color(&self) -> Option<&str> {
            self.color.as_deref()
        }
    }
    impl PieceDisplayMetadata {
        pub fn glyph(&self) -> Option<&str> {
            self.glyph.as_deref()
        }
    }
}
mod piece_spawn_bounds {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PieceSpawnBounds {
        min_x: i8,
        max_x: i8,
        min_y: i8,
        max_y: i8,
    }

    impl PieceSpawnBounds {
        pub fn new(
            min_x: i8,
            max_x: i8,
            min_y: i8,
            max_y: i8,
        ) -> Result<Self, PieceSpawnBoundsError> {
            if min_x > max_x {
                return Err(PieceSpawnBoundsError::InvalidXRange);
            }
            if min_y > max_y {
                return Err(PieceSpawnBoundsError::InvalidYRange);
            }
            Ok(Self {
                min_x,
                max_x,
                min_y,
                max_y,
            })
        }
    }
    impl PieceSpawnBounds {
        pub fn min_x(self) -> i8 {
            self.min_x
        }
    }
    impl PieceSpawnBounds {
        pub fn max_x(self) -> i8 {
            self.max_x
        }
    }
    impl PieceSpawnBounds {
        pub fn min_y(self) -> i8 {
            self.min_y
        }
    }
    impl PieceSpawnBounds {
        pub fn max_y(self) -> i8 {
            self.max_y
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PieceSpawnBoundsError {
        InvalidXRange,
        InvalidYRange,
    }
}
mod piece_symmetry_class {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum PieceSymmetryClass {
        #[default]
        None,
        MirrorX,
        MirrorY,
        Rotate180,
        Full,
    }

    impl PieceSymmetryClass {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::None => "none",
                Self::MirrorX => "mirror-x",
                Self::MirrorY => "mirror-y",
                Self::Rotate180 => "rotate-180",
                Self::Full => "full",
            }
        }
    }
}

pub use custom_piece::CustomPieceDefinition;
pub use custom_piece_definition_error::CustomPieceDefinitionError;
pub use custom_piece_rotation::CustomPieceRotation;
pub use piece_display_metadata::PieceDisplayMetadata;
pub use piece_spawn_bounds::{PieceSpawnBounds, PieceSpawnBoundsError};
pub use piece_symmetry_class::PieceSymmetryClass;

#[cfg(test)]
use crate::registry::piece_registry::ShapeCell;
#[cfg(test)]
use clearra_core_domain::ids::piece_id::PieceDefinitionId;

#[cfg(test)]
#[path = "custom_piece_definition_tests.rs"]
mod tests;
