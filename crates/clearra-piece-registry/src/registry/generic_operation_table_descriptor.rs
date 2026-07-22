use clearra_core_domain::ids::piece_id::PieceDefinitionId;

use crate::custom::CustomOperationTableSchema;

pub const STANDARD_OPERATION_TABLE_VERSION: u32 = 1;
pub const GENERIC_OPERATION_TABLE_DESCRIPTOR_SCHEMA_VERSION: u32 = 1;
pub const CUSTOM_CANDIDATE_RUNTIME_UNSUPPORTED: &str = "custom_candidate_runtime_unsupported";
pub const CUSTOM_REACHABILITY_RUNTIME_UNSUPPORTED: &str = "custom_reachability_runtime_unsupported";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericOperationTableKind {
    StandardTetromino,
    CustomPiece,
}

impl GenericOperationTableKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StandardTetromino => "standard-tetromino",
            Self::CustomPiece => "custom-piece",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardTetrominoOperationTable {
    operation_table_version: u32,
    operation_count: u32,
    rotation_state_count: u8,
}

impl StandardTetrominoOperationTable {
    pub const OPERATION_COUNT: u32 = 28;
    pub const ROTATION_STATE_COUNT: u8 = 4;
}
impl StandardTetrominoOperationTable {
    pub const fn new() -> Self {
        Self {
            operation_table_version: STANDARD_OPERATION_TABLE_VERSION,
            operation_count: Self::OPERATION_COUNT,
            rotation_state_count: Self::ROTATION_STATE_COUNT,
        }
    }
}
impl StandardTetrominoOperationTable {
    pub const fn operation_table_version(self) -> u32 {
        self.operation_table_version
    }
}
impl StandardTetrominoOperationTable {
    pub const fn operation_count(self) -> u32 {
        self.operation_count
    }
}
impl StandardTetrominoOperationTable {
    pub const fn rotation_state_count(self) -> u8 {
        self.rotation_state_count
    }
}

impl Default for StandardTetrominoOperationTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomPieceOperationTable {
    schema: CustomOperationTableSchema,
}

impl CustomPieceOperationTable {
    pub fn new(schema: CustomOperationTableSchema) -> Self {
        Self { schema }
    }
}
impl CustomPieceOperationTable {
    pub fn schema(&self) -> &CustomOperationTableSchema {
        &self.schema
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericOperationTableDescriptor {
    schema_version: u32,
    table_kind: GenericOperationTableKind,
    piece_definition_id: Option<PieceDefinitionId>,
    piece_definition_fingerprint: u64,
    piece_area: usize,
    rotation_state_count: u8,
    operation_count: u32,
    operation_table_version: u32,
    candidate_runtime_guard_reason: Option<&'static str>,
    reachability_runtime_guard_reason: Option<&'static str>,
}

impl GenericOperationTableDescriptor {
    pub fn from_standard(table: StandardTetrominoOperationTable) -> Self {
        Self {
            schema_version: GENERIC_OPERATION_TABLE_DESCRIPTOR_SCHEMA_VERSION,
            table_kind: GenericOperationTableKind::StandardTetromino,
            piece_definition_id: None,
            piece_definition_fingerprint: 0,
            piece_area: 4,
            rotation_state_count: table.rotation_state_count(),
            operation_count: table.operation_count(),
            operation_table_version: table.operation_table_version(),
            candidate_runtime_guard_reason: None,
            reachability_runtime_guard_reason: None,
        }
    }
}
impl GenericOperationTableDescriptor {
    pub fn from_custom_table(table: &CustomPieceOperationTable) -> Self {
        let schema = table.schema();
        Self {
            schema_version: GENERIC_OPERATION_TABLE_DESCRIPTOR_SCHEMA_VERSION,
            table_kind: GenericOperationTableKind::CustomPiece,
            piece_definition_id: Some(schema.piece_id().clone()),
            piece_definition_fingerprint: schema.piece_definition_fingerprint(),
            piece_area: schema.piece_area(),
            rotation_state_count: schema.rotation_states().len() as u8,
            operation_count: schema.operations().len() as u32,
            operation_table_version: schema.schema_version(),
            candidate_runtime_guard_reason: Some(CUSTOM_CANDIDATE_RUNTIME_UNSUPPORTED),
            reachability_runtime_guard_reason: Some(CUSTOM_REACHABILITY_RUNTIME_UNSUPPORTED),
        }
    }
}
impl GenericOperationTableDescriptor {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl GenericOperationTableDescriptor {
    pub fn table_kind(&self) -> GenericOperationTableKind {
        self.table_kind
    }
}
impl GenericOperationTableDescriptor {
    pub fn piece_definition_id(&self) -> Option<&PieceDefinitionId> {
        self.piece_definition_id.as_ref()
    }
}
impl GenericOperationTableDescriptor {
    pub fn piece_definition_fingerprint(&self) -> u64 {
        self.piece_definition_fingerprint
    }
}
impl GenericOperationTableDescriptor {
    pub fn piece_area(&self) -> usize {
        self.piece_area
    }
}
impl GenericOperationTableDescriptor {
    pub fn rotation_state_count(&self) -> u8 {
        self.rotation_state_count
    }
}
impl GenericOperationTableDescriptor {
    pub fn operation_count(&self) -> u32 {
        self.operation_count
    }
}
impl GenericOperationTableDescriptor {
    pub fn operation_table_version(&self) -> u32 {
        self.operation_table_version
    }
}
impl GenericOperationTableDescriptor {
    pub fn candidate_runtime_guard_reason(&self) -> Option<&'static str> {
        self.candidate_runtime_guard_reason
    }
}
impl GenericOperationTableDescriptor {
    pub fn reachability_runtime_guard_reason(&self) -> Option<&'static str> {
        self.reachability_runtime_guard_reason
    }
}

pub fn standard_operation_table_unchanged() -> bool {
    let table = StandardTetrominoOperationTable::new();
    table.operation_count() == StandardTetrominoOperationTable::OPERATION_COUNT
        && table.rotation_state_count() == StandardTetrominoOperationTable::ROTATION_STATE_COUNT
}

#[cfg(test)]
#[path = "generic_operation_table_descriptor_tests.rs"]
mod tests;
