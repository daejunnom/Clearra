# PC4 normalized graph rows are not a fixed ILC frame

## Confirmed implementation gap

`pc4_graph_materializer.rs` assumes source cells are a subset of target cells
and uses `target & !source` as the four placement cells. This is false when a
non-bottom row is cleared and the graph moves complete rows to the bottom.
The existing `merge_deleted_rows == target_deleted` comparison also assumes
original deleted-row positions equal a normalized bottom prefix.

Upstream description:
https://github.com/muse918/hydra-optimal/blob/856b67b079ea3e6d2648eb4f4e03025c226130f6/README.md#graph-and-field-format

The locally inspected tetra-tools gameplay `Piece::place` also moves full
rows to the bottom while preserving the order of other rows. No upstream
implementation was copied into Clearra.

## Small physical counterexample

In Clearra bit order (x increases with bit index), source row 0 is 0b11000000,
row 1 is 0b0000111111, and rows 2/3 are empty. A horizontal I locks at x=6,
y=1: no overlap, supported by row-0 cells, accessible vertically from above.
Row 1 becomes full. After clearing, the surviving physical row is the old
row 0. The graph's normalized target is full row 0 plus 0b11000000 in row 1.

Source has 8 cells and target 12. But six source cells are absent from the
normalized target. Current materializer rejects this before geometry with
`TransitionRemovesLogicalCells`. This proves a coordinate-contract defect;
it does not claim these particular nodes were fetched from the upstream graph.

## Implemented foundation and scope

`Pc4RowFrame` tracks original row IDs per concrete path. It lifts physical lock
masks into the request's original frame, removes physical cleared rows without
renumbering original identities, and separately reconstructs graph-normalized
cells from a compact physical board. Invalid masks are rejected. Transitions
return a new value so another branch or a cancelled transaction keeps its frame.

Three focused tests passed under the managed build policy: the counterexample,
every valid initial cleared prefix and two successive physical clear masks with
every surviving cell mapped independently, and invalid/terminal boundaries.
These tests do not claim the existing materializer is fixed.

## Required integration before activation

1. Enumerate possible physical clear-row masks for an edge, reverse the target
   normalization, and verify all reachable physical locks using the exact core.
   Do not infer locks by raw normalized graph-mask subtraction.
2. Carry each actual clear history through concrete path materialization and
   lift placements into one request-wide ILC identity before candidate reduction.
   A converging graph node must not merge distinct row correspondences.
3. Apply the same framing to fixed-queue and observation/pattern paths, replay,
   initial fields, and 1-4L target terminals. No-clear-only coverage is inadequate.
4. Run exact differential cases with upper-row clears and multiple histories,
   then upstream sampled edges and the relevant product reducers.

This is a v0.9.0 correctness blocker, not a v0.8.1 performance regression or a
reason to change the offline solver. No profile was enabled and no release was
dispatched as part of this audit.
