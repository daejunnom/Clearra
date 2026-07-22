use clearra_core_domain::ids::piece_id::PieceDefinitionId;

use crate::{
    custom::{CustomOperationTableSchema, CustomOperationTableSchemaError},
    registry::{
        CustomPieceOperationTable, GenericOperationTableDescriptor, StandardTetrominoOperationTable,
    },
};

use super::{piece_set_profile_id, MixedPieceSet, MixedPieceSetEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PieceRegistryBridge {
    piece_set_id: String,
    stable_piece_ids: Vec<PieceDefinitionId>,
    piece_area_multiset: Vec<usize>,
    custom_operation_tables: Vec<CustomOperationTableSchema>,
    generic_operation_descriptors: Vec<GenericOperationTableDescriptor>,
    runtime_path: PieceRegistryRuntimePath,
    piece_definition_id_fingerprint: u64,
    piece_area_multiset_fingerprint: u64,
    piece_set_profile_id: u64,
    unsupported_reason: Option<&'static str>,
    mixed_unsupported_reason: Option<&'static str>,
}

impl PieceRegistryBridge {
    pub fn from_mixed_piece_set(
        piece_set: &MixedPieceSet,
    ) -> Result<Self, PieceRegistryBridgeError> {
        let stable_piece_ids = piece_set.stable_piece_ids();
        let piece_area_multiset = piece_set
            .entries()
            .iter()
            .map(MixedPieceSetEntry::area)
            .collect::<Vec<_>>();
        let custom_operation_tables = piece_set
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                MixedPieceSetEntry::Custom(definition) => Some(definition),
                MixedPieceSetEntry::Standard(_) => None,
            })
            .map(CustomOperationTableSchema::from_definition)
            .collect::<Result<Vec<_>, _>>()?;
        let generic_operation_descriptors = if custom_operation_tables.is_empty() {
            vec![GenericOperationTableDescriptor::from_standard(
                StandardTetrominoOperationTable::new(),
            )]
        } else {
            custom_operation_tables
                .iter()
                .map(|schema| {
                    let table = CustomPieceOperationTable::new(schema.clone());
                    GenericOperationTableDescriptor::from_custom_table(&table)
                })
                .collect()
        };
        let runtime_path = if custom_operation_tables.is_empty() {
            PieceRegistryRuntimePath::StandardFastPath
        } else {
            PieceRegistryRuntimePath::UnsupportedExtension
        };
        let unsupported_reason = (runtime_path == PieceRegistryRuntimePath::UnsupportedExtension)
            .then_some("custom_piece_runtime_not_connected");
        let mixed_unsupported_reason = (runtime_path
            == PieceRegistryRuntimePath::UnsupportedExtension)
            .then_some("mixed_piece_runtime_not_connected");

        Ok(Self {
            piece_set_id: piece_set.id().to_owned(),
            piece_definition_id_fingerprint: piece_definition_id_fingerprint(&stable_piece_ids),
            piece_area_multiset_fingerprint: piece_area_multiset_fingerprint(&piece_area_multiset),
            piece_set_profile_id: piece_set_profile_id(piece_set.id()),
            stable_piece_ids,
            piece_area_multiset,
            custom_operation_tables,
            generic_operation_descriptors,
            runtime_path,
            unsupported_reason,
            mixed_unsupported_reason,
        })
    }
}
impl PieceRegistryBridge {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl PieceRegistryBridge {
    pub fn stable_piece_ids(&self) -> &[PieceDefinitionId] {
        &self.stable_piece_ids
    }
}
impl PieceRegistryBridge {
    pub fn piece_area_multiset(&self) -> &[usize] {
        &self.piece_area_multiset
    }
}
impl PieceRegistryBridge {
    pub fn custom_operation_tables(&self) -> &[CustomOperationTableSchema] {
        &self.custom_operation_tables
    }
}
impl PieceRegistryBridge {
    pub fn generic_operation_descriptors(&self) -> &[GenericOperationTableDescriptor] {
        &self.generic_operation_descriptors
    }
}
impl PieceRegistryBridge {
    pub fn runtime_path(&self) -> PieceRegistryRuntimePath {
        self.runtime_path
    }
}
impl PieceRegistryBridge {
    pub fn standard_fast_path_unaffected(&self) -> bool {
        self.runtime_path == PieceRegistryRuntimePath::StandardFastPath
    }
}
impl PieceRegistryBridge {
    pub fn piece_definition_id_fingerprint(&self) -> u64 {
        self.piece_definition_id_fingerprint
    }
}
impl PieceRegistryBridge {
    pub fn piece_area_multiset_fingerprint(&self) -> u64 {
        self.piece_area_multiset_fingerprint
    }
}
impl PieceRegistryBridge {
    pub fn piece_set_profile_id(&self) -> u64 {
        self.piece_set_profile_id
    }
}
impl PieceRegistryBridge {
    pub fn unsupported_reason(&self) -> Option<&'static str> {
        self.unsupported_reason
    }
}
impl PieceRegistryBridge {
    pub fn mixed_unsupported_reason(&self) -> Option<&'static str> {
        self.mixed_unsupported_reason
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceRegistryRuntimePath {
    StandardFastPath,
    UnsupportedExtension,
}

impl PieceRegistryRuntimePath {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StandardFastPath => "standard-fast-path",
            Self::UnsupportedExtension => "unsupported-extension",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PieceRegistryBridgeError {
    CustomOperationTable(CustomOperationTableSchemaError),
}

impl From<CustomOperationTableSchemaError> for PieceRegistryBridgeError {
    fn from(value: CustomOperationTableSchemaError) -> Self {
        Self::CustomOperationTable(value)
    }
}

pub fn piece_definition_id_fingerprint(stable_piece_ids: &[PieceDefinitionId]) -> u64 {
    let mut ids = stable_piece_ids.to_vec();
    ids.sort();

    let mut hash = fnv_offset();
    hash = mix_str(hash, "piece-definition-ids:v1");
    for id in ids {
        hash = mix_str(hash, id.as_str());
        hash = mix_u64(hash, 0xff);
    }
    hash
}

pub fn piece_area_multiset_fingerprint(piece_areas: &[usize]) -> u64 {
    let mut areas = piece_areas.to_vec();
    areas.sort_unstable();

    let mut hash = fnv_offset();
    hash = mix_str(hash, "piece-area-multiset:v1");
    for area in areas {
        hash = mix_u64(hash, area as u64);
    }
    hash
}

fn fnv_offset() -> u64 {
    0xcbf29ce484222325
}

fn mix_str(mut hash: u64, value: &str) -> u64 {
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn mix_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "piece_registry_bridge_tests.rs"]
mod tests;
