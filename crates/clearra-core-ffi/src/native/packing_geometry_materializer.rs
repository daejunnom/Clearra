use crate::PackingCandidateBatch;
use clearra_core_domain::resource::ResourceReport;

use super::C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS;

pub const C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS: usize =
    C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct CNativePackingGeometryPath {
    pub skeleton_ids: [u32; C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS],
    pub operation_count: u8,
    pub reserved: [u8; 3],
}

impl CNativePackingGeometryPath {
    pub fn from_skeleton_ids(indices: &[u32]) -> Option<Self> {
        if indices.is_empty() || indices.len() > C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS {
            return None;
        }
        let mut path = Self {
            operation_count: u8::try_from(indices.len()).ok()?,
            ..Self::default()
        };
        path.skeleton_ids[..indices.len()].copy_from_slice(indices);
        Some(path)
    }

    pub fn from_compact_skeleton_ids(indices: &[u16]) -> Option<Self> {
        if indices.is_empty() || indices.len() > C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS {
            return None;
        }
        let mut path = Self {
            operation_count: u8::try_from(indices.len()).ok()?,
            ..Self::default()
        };
        for (target, source) in path.skeleton_ids.iter_mut().zip(indices.iter().copied()) {
            *target = u32::from(source);
        }
        Some(path)
    }

    pub fn skeleton_ids(&self) -> &[u32] {
        &self.skeleton_ids[..usize::from(self.operation_count)]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGeometryMaterializationOutcome {
    pub candidates: PackingCandidateBatch,
    pub resource_report: ResourceReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGeometryStreamOutcome {
    pub status: i32,
    pub resource_report: ResourceReport,
}

const _: () = assert!(core::mem::size_of::<CNativePackingGeometryPath>() == 64);
