# PC4 normalized graph rows are not a fixed ILC frame

## Confirmed implementation gap before the follow-up

The pre-fix `pc4_graph_materializer.rs` assumes source cells are a subset of target cells
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
normalized target. The pre-fix materializer rejects this before geometry with
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

## Implemented edge and path integration follow-up

The core now enumerates every physical clear-row mask with the required number
of newly cleared lines (at most 16 masks). For each mask it inserts full rows
into the compact target, preserves all surviving rows in order, and subtracts
the physical source only in this reconstructed pre-clear frame. Four-cell
candidates go through the existing geometry and reachability engine. A final
forward place-and-clear must exactly match both the physical clear mask and
the compact target. A count decrease or malformed normalized field remains an
error; non-monotone bit positions alone are no longer rejected.

Completeness of the row reconstruction: any actual lock has some physical
clear mask among these masks. Given that mask and the compact target, its
pre-clear occupied board is uniquely reconstructed by inserting full rows.
Subtracting the unchanged physical source then yields that lock's cells.
This argument is about coordinate reconstruction, not upstream graph coverage
or an independent proof of the existing reachability engine.

The App adapter attaches source-prefix and physical-clear metadata to each
edge placement. Common concrete-path paging lifts selected alternatives through
their own `Pc4RowFrame` before returning occupied cells to either fixed-queue or
observation reducers. Lock x/y remain physical for replay. Metadata is consumed
once, and mixed frames or inconsistent prefix histories are typed errors.
Paging performs this on temporary data before committing cursor progress.

Managed follow-up batch completed: core **4 passed**, tablebase **154 passed**,
including two different clear histories converging on the same normalized frame
but requiring different original-cell identities. App PC4 **60 passed** (0.02s),
and the batch exited zero. A subsequent tablebase-only run adding two paging
regressions passed **156 tests** (0.04s). Those verify different histories across
page boundaries, physical replay coordinates, one-page/two-page equality, and
no cursor progress when a late row-frame error discards a temporary page.
The later source edits outside these tests were comments/documentation only.
No build/test jobs remain running. These results do not claim release acceptance.

## Remaining verification before activation

1. Add end-to-end reducer cases for
   upper-row clear histories rather than only independent core/path unit cases.
2. Verify replay, initial fields, and every enabled 1-4L target terminal with
   the path framing. No-clear-only coverage is inadequate.
3. Run exact differential cases with upper-row clears and multiple histories,
   then upstream sampled edges and the relevant product reducers.

This is a v0.9.0 correctness blocker, not a v0.8.1 performance regression or a
reason to change the offline solver. No profile was enabled and no release was
dispatched as part of this audit.
