#[cfg(feature = "native-c-core")]
use crate::problem::CBuildUpProblem;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpGeometryLanguageNode {
    first_edge: u32,
    edge_count: u16,
    accepting: u8,
    depth: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpGeometryLanguageEdge {
    child_node_index: u32,
    operation_index: u16,
    piece: u8,
    reserved: u8,
}

pub(crate) type CNativeBuildUpGeometryLanguageNode = BuildUpGeometryLanguageNode;
pub(crate) type CNativeBuildUpGeometryLanguageEdge = BuildUpGeometryLanguageEdge;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BuildUpGeometryTransitionMode {
    #[default]
    Reachable,
    GeometryOnly,
}

impl BuildUpGeometryTransitionMode {
    #[cfg(feature = "native-c-core")]
    pub(crate) const fn as_native(self) -> i32 {
        match self {
            Self::Reachable => 0,
            Self::GeometryOnly => 1,
        }
    }

    #[cfg(any(feature = "native-c-core", test))]
    const fn from_native(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Reachable),
            1 => Some(Self::GeometryOnly),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpGeometryLanguageNodeV2 {
    board_mask: u64,
    reachability_relevant_state: u64,
    first_edge: u32,
    edge_count: u16,
    remaining_operations: u16,
    deleted_row_mask: u16,
    deleted_count: u8,
    cleared_lines: u8,
    accepting: u8,
    depth: u8,
    reserved: [u8; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpGeometryLanguageEdgeV2 {
    target_mask: u64,
    child_node_index: u32,
    operation_index: u16,
    cleared_row_mask: u16,
    x: i8,
    adjusted_y: i8,
    piece: u8,
    rotation: u8,
    cleared_lines: u8,
    reserved: [u8; 3],
}

pub(crate) type CNativeBuildUpGeometryLanguageNodeV2 = BuildUpGeometryLanguageNodeV2;
pub(crate) type CNativeBuildUpGeometryLanguageEdgeV2 = BuildUpGeometryLanguageEdgeV2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CNativeBuildUpGeometryLanguageReportV2 {
    pub candidate_id: u64,
    pub canonical_operation_set_id: u64,
    pub snapshot_id: u64,
    pub root_node_index: u32,
    pub node_count: u32,
    pub edge_count: u32,
    pub complete: u8,
    pub transition_mode: u8,
    pub format_version: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CNativeBuildUpGeometryLanguageReport {
    pub candidate_id: u64,
    pub canonical_operation_set_id: u64,
    pub root_node_index: u32,
    pub node_count: u32,
    pub edge_count: u32,
    pub complete: u8,
    pub reserved: [u8; 3],
}

impl BuildUpGeometryLanguageNode {
    pub const fn first_edge(self) -> usize {
        self.first_edge as usize
    }

    pub const fn edge_count(self) -> usize {
        self.edge_count as usize
    }

    pub const fn accepting(self) -> bool {
        self.accepting != 0
    }

    pub const fn depth(self) -> usize {
        self.depth as usize
    }
}

impl BuildUpGeometryLanguageEdge {
    pub const fn child_node_index(self) -> usize {
        self.child_node_index as usize
    }

    pub const fn operation_index(self) -> u16 {
        self.operation_index
    }

    pub const fn piece(self) -> u8 {
        self.piece
    }
}

impl BuildUpGeometryLanguageNodeV2 {
    pub const fn board_mask(self) -> u64 {
        self.board_mask
    }

    pub const fn reachability_relevant_state(self) -> u64 {
        self.reachability_relevant_state
    }

    pub const fn first_edge(self) -> usize {
        self.first_edge as usize
    }

    pub const fn edge_count(self) -> usize {
        self.edge_count as usize
    }

    pub const fn remaining_operations(self) -> u16 {
        self.remaining_operations
    }

    pub const fn deleted_row_mask(self) -> u16 {
        self.deleted_row_mask
    }

    pub const fn deleted_count(self) -> u8 {
        self.deleted_count
    }

    pub const fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }

    pub const fn accepting(self) -> bool {
        self.accepting != 0
    }

    pub const fn depth(self) -> usize {
        self.depth as usize
    }
}

impl BuildUpGeometryLanguageEdgeV2 {
    pub const fn target_mask(self) -> u64 {
        self.target_mask
    }

    pub const fn child_node_index(self) -> usize {
        self.child_node_index as usize
    }

    pub const fn operation_index(self) -> u16 {
        self.operation_index
    }

    pub const fn cleared_row_mask(self) -> u16 {
        self.cleared_row_mask
    }

    pub const fn x(self) -> i8 {
        self.x
    }

    pub const fn adjusted_y(self) -> i8 {
        self.adjusted_y
    }

    pub const fn piece(self) -> u8 {
        self.piece
    }

    pub const fn rotation(self) -> u8 {
        self.rotation
    }

    pub const fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildUpGeometryLanguage {
    candidate_id: u64,
    canonical_operation_set_id: u64,
    root_node_index: u32,
    complete: bool,
    nodes: Vec<BuildUpGeometryLanguageNode>,
    edges: Vec<BuildUpGeometryLanguageEdge>,
}

impl BuildUpGeometryLanguage {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn canonical_operation_set_id(&self) -> u64 {
        self.canonical_operation_set_id
    }

    pub const fn root_node_index(&self) -> usize {
        self.root_node_index as usize
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn nodes(&self) -> &[BuildUpGeometryLanguageNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[BuildUpGeometryLanguageEdge] {
        &self.edges
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildUpGeometryLanguageV2 {
    candidate_id: u64,
    canonical_operation_set_id: u64,
    snapshot_id: u64,
    root_node_index: u32,
    complete: bool,
    transition_mode: BuildUpGeometryTransitionMode,
    nodes: Vec<BuildUpGeometryLanguageNodeV2>,
    edges: Vec<BuildUpGeometryLanguageEdgeV2>,
}

impl BuildUpGeometryLanguageV2 {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn canonical_operation_set_id(&self) -> u64 {
        self.canonical_operation_set_id
    }

    pub const fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    pub const fn root_node_index(&self) -> usize {
        self.root_node_index as usize
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub const fn transition_mode(&self) -> BuildUpGeometryTransitionMode {
        self.transition_mode
    }

    pub fn nodes(&self) -> &[BuildUpGeometryLanguageNodeV2] {
        &self.nodes
    }

    pub fn edges(&self) -> &[BuildUpGeometryLanguageEdgeV2] {
        &self.edges
    }
}

#[cfg(feature = "native-c-core")]
pub(crate) fn export_with_workspace(
    workspace: &mut crate::raw::buildup_workspace::RawBuildUpWorkspace,
    problem: &CBuildUpProblem,
) -> Result<BuildUpGeometryLanguage, i32> {
    let mut report = CNativeBuildUpGeometryLanguageReport::default();
    let status = workspace.query_geometry_language(problem, &mut report);
    if status != super::C_BUILDUP_STATUS_OK {
        return Err(status);
    }
    if report.complete == 0 {
        return Ok(language_from_native(report, Vec::new(), Vec::new()));
    }

    let mut nodes = vec![CNativeBuildUpGeometryLanguageNode::default(); report.node_count as usize];
    let mut edges = vec![CNativeBuildUpGeometryLanguageEdge::default(); report.edge_count as usize];
    let expected = report;
    let status = workspace.export_geometry_language(problem, &mut nodes, &mut edges, &mut report);
    if status != super::C_BUILDUP_STATUS_OK {
        return Err(status);
    }
    if report != expected || !native_language_is_valid(report, &nodes, &edges) {
        return Err(super::C_BUILDUP_STATUS_INVALID_PROBLEM);
    }
    Ok(language_from_native(report, nodes, edges))
}

#[cfg(feature = "native-c-core")]
pub(crate) fn export_v2_with_workspace(
    workspace: &mut crate::raw::buildup_workspace::RawBuildUpWorkspace,
    problem: &CBuildUpProblem,
    transition_mode: BuildUpGeometryTransitionMode,
) -> Result<BuildUpGeometryLanguageV2, i32> {
    let mut report = CNativeBuildUpGeometryLanguageReportV2::default();
    let status =
        workspace.prepare_geometry_language_v2(problem, transition_mode.as_native(), &mut report);
    if status != super::C_BUILDUP_STATUS_OK {
        return Err(status);
    }
    if report.complete == 0 {
        return language_v2_from_native(report, Vec::new(), Vec::new());
    }

    let mut nodes = fallible_zeroed_vec(
        usize::try_from(report.node_count)
            .map_err(|_| super::C_BUILDUP_STATUS_CAPACITY_EXCEEDED)?,
    )?;
    let mut edges = fallible_zeroed_vec(
        usize::try_from(report.edge_count)
            .map_err(|_| super::C_BUILDUP_STATUS_CAPACITY_EXCEEDED)?,
    )?;
    let prepared = report;
    let status = workspace.copy_prepared_geometry_language_v2(&mut nodes, &mut edges, &mut report);
    if status != super::C_BUILDUP_STATUS_OK {
        return Err(status);
    }
    if report != prepared || !native_language_v2_is_valid(report, &nodes, &edges) {
        return Err(super::C_BUILDUP_STATUS_INVALID_PROBLEM);
    }
    language_v2_from_native(report, nodes, edges)
}

#[cfg(feature = "native-c-core")]
fn native_language_is_valid(
    report: CNativeBuildUpGeometryLanguageReport,
    nodes: &[CNativeBuildUpGeometryLanguageNode],
    edges: &[CNativeBuildUpGeometryLanguageEdge],
) -> bool {
    report.complete != 0
        && report.root_node_index < report.node_count
        && nodes[report.root_node_index as usize].depth == 0
        && nodes.iter().all(|node| {
            let begin = node.first_edge as usize;
            begin
                .checked_add(node.edge_count as usize)
                .is_some_and(|end| end <= edges.len())
        })
        && nodes.iter().all(|node| {
            let begin = node.first_edge as usize;
            let end = begin + node.edge_count as usize;
            edges[begin..end].iter().all(|edge| {
                edge.child_node_index < report.node_count
                    && edge.operation_index < crate::problem::C_BUILDUP_MAX_OPERATIONS as u16
                    && (1..=7).contains(&edge.piece)
                    && nodes[edge.child_node_index as usize].depth == node.depth.saturating_add(1)
            })
        })
}

#[cfg(feature = "native-c-core")]
fn language_from_native(
    report: CNativeBuildUpGeometryLanguageReport,
    nodes: Vec<CNativeBuildUpGeometryLanguageNode>,
    edges: Vec<CNativeBuildUpGeometryLanguageEdge>,
) -> BuildUpGeometryLanguage {
    BuildUpGeometryLanguage {
        candidate_id: report.candidate_id,
        canonical_operation_set_id: report.canonical_operation_set_id,
        root_node_index: report.root_node_index,
        complete: report.complete != 0,
        nodes,
        edges,
    }
}

#[cfg(any(feature = "native-c-core", test))]
fn native_language_v2_is_valid(
    report: CNativeBuildUpGeometryLanguageReportV2,
    nodes: &[CNativeBuildUpGeometryLanguageNodeV2],
    edges: &[CNativeBuildUpGeometryLanguageEdgeV2],
) -> bool {
    report.complete == 1
        && report.reserved == 0
        && report.snapshot_id != 0
        && report.format_version == 2
        && BuildUpGeometryTransitionMode::from_native(report.transition_mode).is_some()
        && usize::try_from(report.node_count).ok() == Some(nodes.len())
        && usize::try_from(report.edge_count).ok() == Some(edges.len())
        && report.root_node_index < report.node_count
        && nodes[report.root_node_index as usize].depth == 0
        && native_language_v2_ranges_are_contiguous(nodes, edges.len())
        && nodes.iter().all(|node| {
            let begin = node.first_edge as usize;
            begin
                .checked_add(node.edge_count as usize)
                .is_some_and(|end| end <= edges.len())
                && node.reserved == [0; 2]
                && node.accepting <= 1
                && node.depth <= crate::problem::C_BUILDUP_MAX_OPERATIONS as u8
                && (node.accepting == 0
                    || (node.board_mask == 0
                        && node.remaining_operations == 0
                        && node.edge_count == 0))
        })
        && nodes.iter().all(|node| {
            let begin = node.first_edge as usize;
            let end = begin + node.edge_count as usize;
            edges[begin..end].iter().all(|edge| {
                edge.child_node_index < report.node_count
                    && edge.operation_index < crate::problem::C_BUILDUP_MAX_OPERATIONS as u16
                    && (1..=7).contains(&edge.piece)
                    && edge.rotation < 4
                    && edge.reserved == [0; 3]
                    && edge.target_mask.count_ones() == 4
                    && edge.cleared_row_mask.count_ones() == u32::from(edge.cleared_lines)
                    && edge.cleared_lines <= 4
                    && node.board_mask & edge.target_mask == 0
                    && node.remaining_operations & (1_u16 << edge.operation_index) != 0
                    && nodes[edge.child_node_index as usize].remaining_operations
                        == node.remaining_operations & !(1_u16 << edge.operation_index)
                    && nodes[edge.child_node_index as usize].depth == node.depth.saturating_add(1)
            })
        })
}

#[cfg(any(feature = "native-c-core", test))]
fn native_language_v2_ranges_are_contiguous(
    nodes: &[CNativeBuildUpGeometryLanguageNodeV2],
    edge_count: usize,
) -> bool {
    let mut cursor = 0_usize;
    for node in nodes {
        if node.first_edge as usize != cursor {
            return false;
        }
        let Some(end) = cursor.checked_add(node.edge_count as usize) else {
            return false;
        };
        if end > edge_count {
            return false;
        }
        cursor = end;
    }
    cursor == edge_count
}

#[cfg(feature = "native-c-core")]
fn fallible_zeroed_vec<T: Clone + Default>(length: usize) -> Result<Vec<T>, i32> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| super::C_BUILDUP_STATUS_CAPACITY_EXCEEDED)?;
    values.resize(length, T::default());
    Ok(values)
}

#[cfg(any(feature = "native-c-core", test))]
fn language_v2_from_native(
    report: CNativeBuildUpGeometryLanguageReportV2,
    nodes: Vec<CNativeBuildUpGeometryLanguageNodeV2>,
    edges: Vec<CNativeBuildUpGeometryLanguageEdgeV2>,
) -> Result<BuildUpGeometryLanguageV2, i32> {
    let transition_mode = BuildUpGeometryTransitionMode::from_native(report.transition_mode)
        .ok_or(super::C_BUILDUP_STATUS_INVALID_PROBLEM)?;
    Ok(BuildUpGeometryLanguageV2 {
        candidate_id: report.candidate_id,
        canonical_operation_set_id: report.canonical_operation_set_id,
        snapshot_id: report.snapshot_id,
        root_node_index: report.root_node_index,
        complete: report.complete != 0,
        transition_mode,
        nodes,
        edges,
    })
}

const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageNode>() == 8);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageEdge>() == 8);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageReport>() == 32);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageNodeV2>() == 32);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageEdgeV2>() == 24);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageReportV2>() == 40);
const _: () = assert!(
    core::mem::align_of::<CNativeBuildUpGeometryLanguageNodeV2>() == core::mem::align_of::<u64>()
);
const _: () = assert!(
    core::mem::align_of::<CNativeBuildUpGeometryLanguageEdgeV2>() == core::mem::align_of::<u64>()
);
const _: () = assert!(
    core::mem::align_of::<CNativeBuildUpGeometryLanguageReportV2>() == core::mem::align_of::<u64>()
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_validation_requires_semantic_depth_and_valid_edges() {
        let report = CNativeBuildUpGeometryLanguageReportV2 {
            candidate_id: 7,
            canonical_operation_set_id: 9,
            snapshot_id: 11,
            root_node_index: 0,
            node_count: 2,
            edge_count: 1,
            complete: 1,
            transition_mode: 1,
            format_version: 2,
            reserved: 0,
        };
        let nodes = [
            BuildUpGeometryLanguageNodeV2 {
                first_edge: 0,
                edge_count: 1,
                remaining_operations: 1,
                ..Default::default()
            },
            BuildUpGeometryLanguageNodeV2 {
                first_edge: 1,
                edge_count: 0,
                accepting: 1,
                depth: 1,
                ..Default::default()
            },
        ];
        let edges = [BuildUpGeometryLanguageEdgeV2 {
            target_mask: 0xf,
            child_node_index: 1,
            operation_index: 0,
            piece: 1,
            ..Default::default()
        }];

        assert!(native_language_v2_is_valid(report, &nodes, &edges));
        let language = language_v2_from_native(report, nodes.to_vec(), edges.to_vec())
            .expect("valid v2 language");
        assert_eq!(language.snapshot_id(), 11);
        assert_eq!(
            language.transition_mode(),
            BuildUpGeometryTransitionMode::GeometryOnly
        );
        assert_eq!(language.nodes()[0].remaining_operations(), 1);
        assert_eq!(language.edges()[0].target_mask(), 0xf);
    }
}
