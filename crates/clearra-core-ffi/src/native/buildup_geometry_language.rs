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

const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageNode>() == 8);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageEdge>() == 8);
const _: () = assert!(core::mem::size_of::<CNativeBuildUpGeometryLanguageReport>() == 32);
