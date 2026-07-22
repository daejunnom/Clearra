use clearra_core_domain::{ids::piece_id::PieceDefinitionId, piece::rotation::RotationState};

use crate::registry::piece_registry::ShapeCell;

use super::CustomPieceDefinition;

pub const CUSTOM_OPERATION_TABLE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomOperationTableSchema {
    piece_id: PieceDefinitionId,
    piece_area: usize,
    schema_version: u32,
    operations: Vec<CustomOperationSchema>,
}

impl CustomOperationTableSchema {
    pub fn from_definition(
        definition: &CustomPieceDefinition,
    ) -> Result<Self, CustomOperationTableSchemaError> {
        let mut rotations = definition.rotations().to_vec();
        rotations.sort_by_key(|rotation| rotation.state().quarter_turns());

        let operations = rotations
            .iter()
            .enumerate()
            .map(|(index, rotation)| {
                let operation_id = u16::try_from(index).map_err(|_| {
                    CustomOperationTableSchemaError::TooManyOperations {
                        operation_count: rotations.len(),
                    }
                })?;
                CustomOperationSchema::new(
                    operation_id,
                    definition.id().clone(),
                    rotation.state(),
                    rotation.cells().to_vec(),
                    definition.area(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            piece_id: definition.id().clone(),
            piece_area: definition.area(),
            schema_version: CUSTOM_OPERATION_TABLE_SCHEMA_VERSION,
            operations,
        })
    }
}
impl CustomOperationTableSchema {
    pub fn piece_id(&self) -> &PieceDefinitionId {
        &self.piece_id
    }
}
impl CustomOperationTableSchema {
    pub fn piece_area(&self) -> usize {
        self.piece_area
    }
}
impl CustomOperationTableSchema {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl CustomOperationTableSchema {
    pub fn operations(&self) -> &[CustomOperationSchema] {
        &self.operations
    }
}
impl CustomOperationTableSchema {
    pub fn rotation_states(&self) -> Vec<RotationState> {
        self.operations
            .iter()
            .map(CustomOperationSchema::rotation_state)
            .collect()
    }
}
impl CustomOperationTableSchema {
    pub fn piece_definition_fingerprint(&self) -> u64 {
        let mut hash = fnv_offset();
        hash = mix_str(hash, self.piece_id.as_str());
        hash = mix_u64(hash, self.piece_area as u64);
        hash = mix_u64(hash, u64::from(self.schema_version));
        for operation in &self.operations {
            hash = mix_u64(hash, u64::from(operation.operation_id()));
            hash = mix_u64(hash, u64::from(operation.rotation_state().quarter_turns()));
            for cell in operation.cells() {
                hash = mix_i8(hash, cell.x());
                hash = mix_i8(hash, cell.y());
            }
        }
        hash
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomOperationSchema {
    operation_id: u16,
    piece_id: PieceDefinitionId,
    rotation_state: RotationState,
    cells: Vec<ShapeCell>,
    bounds: CustomOperationBounds,
    piece_area: usize,
    stable_key: String,
}

impl CustomOperationSchema {
    pub fn new(
        operation_id: u16,
        piece_id: PieceDefinitionId,
        rotation_state: RotationState,
        cells: Vec<ShapeCell>,
        expected_area: usize,
    ) -> Result<Self, CustomOperationTableSchemaError> {
        if cells.len() != expected_area {
            return Err(CustomOperationTableSchemaError::OperationAreaMismatch {
                rotation_state,
                expected: expected_area,
                actual: cells.len(),
            });
        }
        let bounds = CustomOperationBounds::from_cells(&cells)?;
        let stable_key = stable_operation_key(&piece_id, rotation_state, &cells);
        Ok(Self {
            operation_id,
            piece_id,
            rotation_state,
            cells,
            bounds,
            piece_area: expected_area,
            stable_key,
        })
    }
}
impl CustomOperationSchema {
    pub fn operation_id(&self) -> u16 {
        self.operation_id
    }
}
impl CustomOperationSchema {
    pub fn piece_id(&self) -> &PieceDefinitionId {
        &self.piece_id
    }
}
impl CustomOperationSchema {
    pub fn rotation_state(&self) -> RotationState {
        self.rotation_state
    }
}
impl CustomOperationSchema {
    pub fn cells(&self) -> &[ShapeCell] {
        &self.cells
    }
}
impl CustomOperationSchema {
    pub fn bounds(&self) -> CustomOperationBounds {
        self.bounds
    }
}
impl CustomOperationSchema {
    pub fn piece_area(&self) -> usize {
        self.piece_area
    }
}
impl CustomOperationSchema {
    pub fn stable_key(&self) -> &str {
        &self.stable_key
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomOperationBounds {
    min_x: i8,
    max_x: i8,
    min_y: i8,
    max_y: i8,
}

impl CustomOperationBounds {
    pub fn from_cells(cells: &[ShapeCell]) -> Result<Self, CustomOperationTableSchemaError> {
        let Some(first) = cells.first().copied() else {
            return Err(CustomOperationTableSchemaError::EmptyOperationCells);
        };
        let mut min_x = first.x();
        let mut max_x = first.x();
        let mut min_y = first.y();
        let mut max_y = first.y();
        for cell in cells.iter().copied() {
            min_x = min_x.min(cell.x());
            max_x = max_x.max(cell.x());
            min_y = min_y.min(cell.y());
            max_y = max_y.max(cell.y());
        }
        Ok(Self {
            min_x,
            max_x,
            min_y,
            max_y,
        })
    }
}
impl CustomOperationBounds {
    pub fn min_x(self) -> i8 {
        self.min_x
    }
}
impl CustomOperationBounds {
    pub fn max_x(self) -> i8 {
        self.max_x
    }
}
impl CustomOperationBounds {
    pub fn min_y(self) -> i8 {
        self.min_y
    }
}
impl CustomOperationBounds {
    pub fn max_y(self) -> i8 {
        self.max_y
    }
}
impl CustomOperationBounds {
    pub fn width(self) -> u8 {
        (self.max_x - self.min_x + 1) as u8
    }
}
impl CustomOperationBounds {
    pub fn height(self) -> u8 {
        (self.max_y - self.min_y + 1) as u8
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomOperationTableSchemaError {
    EmptyOperationCells,
    OperationAreaMismatch {
        rotation_state: RotationState,
        expected: usize,
        actual: usize,
    },
    TooManyOperations {
        operation_count: usize,
    },
}

fn stable_operation_key(
    piece_id: &PieceDefinitionId,
    rotation_state: RotationState,
    cells: &[ShapeCell],
) -> String {
    let mut cells = cells.to_vec();
    cells.sort_unstable();
    let cells = cells
        .iter()
        .map(|cell| format!("{},{}", cell.x(), cell.y()))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "{}:r{}:{}",
        piece_id.as_str(),
        rotation_state.quarter_turns(),
        cells
    )
}

fn fnv_offset() -> u64 {
    0xcbf29ce484222325
}

fn mix_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn mix_i8(hash: u64, value: i8) -> u64 {
    mix_u64(hash, value as i16 as u64)
}

fn mix_str(mut hash: u64, value: &str) -> u64 {
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "custom_operation_table_tests.rs"]
mod custom_operation_table_tests;
