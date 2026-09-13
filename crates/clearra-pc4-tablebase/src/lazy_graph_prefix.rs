//! Persistent traversal prefixes; flat paths are created only for page output.

use std::{collections::TryReserveError, fmt, sync::Arc};

use crate::{FixedQueueGraphPath, QualifiedPc4GraphEdge};

#[derive(Clone, Debug)]
pub(crate) struct PendingGraphPath(Repr);

#[derive(Clone, Debug)]
enum Repr {
    Shared {
        source: u32,
        length: usize,
        tail: Option<Arc<PrefixNode>>,
    },
    // Pre-change copying behavior for test-only A/B, absent from products.
    #[cfg(test)]
    Copied(FixedQueueGraphPath),
}

struct PrefixNode {
    parent: Option<Arc<PrefixNode>>,
    edge: QualifiedPc4GraphEdge,
}

impl fmt::Debug for PrefixNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrefixNode")
            .field("edge", &self.edge)
            .field("has_parent", &self.parent.is_some())
            .finish()
    }
}

impl Drop for PrefixNode {
    fn drop(&mut self) {
        // Deep caller-specified queues must not create recursive Arc teardown.
        let mut parent = self.parent.take();
        while let Some(node) = parent {
            match Arc::try_unwrap(node) {
                Ok(mut node) => parent = node.parent.take(),
                Err(_) => break,
            }
        }
    }
}

impl PendingGraphPath {
    pub(crate) fn root(source: u32) -> Self {
        Self(Repr::Shared {
            source,
            length: 0,
            tail: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn copied_root(source: u32) -> Self {
        Self(Repr::Copied(FixedQueueGraphPath::from_parts(
            source,
            Vec::new(),
        )))
    }

    pub(crate) fn consumed_pieces(&self) -> usize {
        match &self.0 {
            Repr::Shared { length, .. } => *length,
            #[cfg(test)]
            Repr::Copied(path) => path.consumed_pieces(),
        }
    }

    pub(crate) fn terminal_field_id(&self) -> u32 {
        match &self.0 {
            Repr::Shared { source, tail, .. } => tail
                .as_ref()
                .map_or(*source, |node| node.edge.target_field_id()),
            #[cfg(test)]
            Repr::Copied(path) => path.terminal_field_id(),
        }
    }

    pub(crate) fn extended(&self, edge: QualifiedPc4GraphEdge) -> Self {
        match &self.0 {
            Repr::Shared {
                source,
                length,
                tail,
            } => Self(Repr::Shared {
                source: *source,
                // The caller checks its finite path budget and overflow first.
                length: length + 1,
                tail: Some(Arc::new(PrefixNode {
                    parent: tail.clone(),
                    edge,
                })),
            }),
            #[cfg(test)]
            Repr::Copied(path) => {
                let mut next = path.clone();
                next.push_edge(edge);
                Self(Repr::Copied(next))
            }
        }
    }

    pub(crate) fn into_graph_path(self) -> Result<FixedQueueGraphPath, TryReserveError> {
        match self.0 {
            Repr::Shared {
                source,
                length,
                tail,
            } => {
                let mut edges = Vec::new();
                edges.try_reserve_exact(length)?;
                let mut current = tail.as_deref();
                while let Some(node) = current {
                    edges.push(node.edge.clone());
                    current = node.parent.as_deref();
                }
                edges.reverse();
                Ok(FixedQueueGraphPath::from_parts(source, edges))
            }
            #[cfg(test)]
            Repr::Copied(path) => Ok(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        manifest::tests::qualified_target_identity, Pc4GraphPiece, Pc4RuleProfile,
        Pc4TerminalUseCase,
    };

    fn edge(source: u32, destination: u32) -> QualifiedPc4GraphEdge {
        QualifiedPc4GraphEdge::from_qualified_record(
            &qualified_target_identity(
                "prefix-generation",
                "prefix-manifest",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                4,
            ),
            source,
            Pc4GraphPiece::I,
            destination,
        )
    }

    #[test]
    fn branches_share_prefixes_but_emit_independent_exact_paths() {
        let prefix = PendingGraphPath::root(7).extended(edge(7, 8));
        let left = prefix.extended(edge(8, 9));
        let right = prefix.extended(edge(8, 10));
        let clone = left.clone();
        let tail = |path: &PendingGraphPath| match &path.0 {
            Repr::Shared { tail, .. } => tail.clone().unwrap(),
            Repr::Copied(_) => panic!("unexpected legacy path"),
        };
        assert!(Arc::ptr_eq(&tail(&left), &tail(&clone)));
        assert!(Arc::ptr_eq(
            tail(&left).parent.as_ref().unwrap(),
            tail(&right).parent.as_ref().unwrap()
        ));
        let left_path = left.into_graph_path().unwrap();
        let right_path = right.into_graph_path().unwrap();
        assert_eq!(left_path.edges(), &[edge(7, 8), edge(8, 9)]);
        assert_eq!(right_path.edges(), &[edge(7, 8), edge(8, 10)]);
        assert_eq!(clone.into_graph_path().unwrap(), left_path);
        assert_eq!(prefix.into_graph_path().unwrap().consumed_pieces(), 1);
    }

    #[test]
    fn deep_prefix_release_is_iterative_and_empty_path_is_exact() {
        assert_eq!(
            PendingGraphPath::root(7).into_graph_path().unwrap(),
            FixedQueueGraphPath::from_parts(7, Vec::new())
        );
        let mut prefix = PendingGraphPath::root(7);
        let repeated = edge(7, 7);
        for _ in 0..10_000 {
            prefix = prefix.extended(repeated.clone());
        }
        assert_eq!(prefix.consumed_pieces(), 10_000);
        drop(prefix);
    }
}
