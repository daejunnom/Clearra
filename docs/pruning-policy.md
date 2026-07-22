# Pruning Policy

Pruning means removing a search candidate from the product PC path. Clearra uses
only exact pruning reasons in product search.

## Allowed Pruning Conditions

The allowed pruning reasons are:

- collision
- bounds overflow
- target mask overflow
- area overflow
- piece count overflow
- row capacity overflow
- exact hash confirm dedupe
- coverage universe identity mismatch reject
- BuildUp full-key memo dedupe
- HoldAutomaton impossible
- Reachability impossible
- independently proven `BuildOrders(P) intersection HoldReachableOrders(Q) is empty`

These conditions are exact guards. They can reject impossible states or dedupe
states proven identical by exact identity, but they must not silently downgrade
an incomplete search into a complete result.

`BuildOrders(P) intersection HoldReachableOrders(Q) is empty` is not currently
a connected pruning authority. Both languages would have to be generated
independently by a complete engine before this condition could return. The
test-only explicit-order scaffold and any accumulator populated from an already
accepted BuildVariant are witnessed coverage, not pruning proofs.

All-state domain pruning is not connected. Domain propagation currently emits
conditional evidence only. It cannot mint a global drop proof until a complete
producer computes reachable-state and candidate-table digests
(`clear_state_set_digest` and `candidate_domain_table_digest`) and binds the
operation-table, piece-set, rule-profile, and kick-profile identity.

## Forbidden Pruning Reasons

The following reasons are not exact and must never delete candidates:

- MCTS low score
- rare piece heuristic
- bad shape heuristic
- probably impossible
- no immediate placement
- target-frame floating
- spin classifier unknown
- score below threshold
- first witness missing
- representative order failed
- Bloom filter false positive
- resource cap reached

`resource cap reached` means the result is incomplete. It is not proof that the
candidate is impossible, and it is not a zero-probability result.

## Witness And Coverage Rules

`verify_first` and representative-order replay are witness tools. They may
produce a visual or diagnostic example, but they must not source all-solution
coverage, minimum cover, exact probability, or negative proof.

The validator markers for this policy are:

- `architecture_validation_rejects_mitm_pc_backend`
- `architecture_validation_rejects_heuristic_prune_reason`
- `architecture_validation_rejects_representative_order_only_coverage`
- `architecture_validation_rejects_first_witness_coverage`

## Proof-carrying pruning

Only the connected native packing engine may remove a candidate. Its current
engine factories cover `PlacementCollision` and `TargetMaskOverflow`; no Rust
executor proof factory is exported. `PruneReason`, `ProofLevel`, and
`PruningProofLedgerEntry` are reporting metadata and cannot authorize removal.
Unconnected all-state, reachability, and language reasons are rejected by the
Rust evidence ledger and keep the candidate on a less aggressive path.

The ledger gate is mandatory in both CPU and GPU packing. A null ledger or an
invalid static-prune context is an error before any candidate is removed. The
context uses the actual problem or GPU batch identity, BFS layer, piece,
rotation, coordinate, operation id, operation-table version, piece-set id,
rule profile, and kick profile. Evidence retention is best effort: a full
ledger increments truncation counters but does not change an otherwise exact
search result.

Evidence retention has two explicit policies. `BestEffort` retains entries up
to the configured limit, then preserves summary counts and marks
`evidence_truncated=true` while search continues. `CompleteRequired` is used by
verification and release exactness audits. It may expand storage or split a
batch, but when complete evidence still cannot be retained it must not remove
the candidate. The candidate is kept and sent to BuildUp, and the report sets
`complete_required_capacity_hit=true` plus
`candidates_kept_due_to_evidence_capacity`. Evidence capacity pressure is not a
logical impossibility proof and does not make a candidate disappear.

Authorized removal kinds are `PlacementCollision`, `TargetMaskOverflow`,
`PieceCountOverflow`, `LineClearOrderImpossible`,
`AllReachableStatesDomainEmpty`, and `ReachabilityExhaustivelyImpossible`.
Collision and target overflow are minted inside the static packing producer
after checking actual masks. Piece-count removal is minted only from a complete
materialized multiset family, and line-clear-order removal is minted only after
the operation-subset search exhausts every dependency order. Domain and
reachability authorization can only be minted from their complete engine-result
types. `HoldLanguageEmpty` is absent until an independent language engine is
connected.

`CellDomainEmptyUnderClearState` and `ForcedPieceFamilyUnderClearState` are
constraint evidence only. A caller cannot label either fact `GlobalSafe` or
pass it directly to the drop gate.
`ResourceBudgetExceeded` is reportable evidence for an incomplete/truncated
result, but it is not a candidate drop proof even when a local propagation pass
is under pressure.
`FloatingInTargetFrame`, `MctsLowScore`, and `RareShape` remain forbidden prune
reasons.

Count-only promotion is not GlobalSafe:
`cannot_promote_by_count_only_without_clear_state_set_digest`. If a clear-state
set is truncated or any proof budget is exhausted, no authorized candidate-drop
value can be constructed and the candidate must be kept or sent to BuildUp.

The C ABI follows the same boundary. The former raw
`clr_pruning_candidate_drop_allowed` and boolean-filled global-domain proof API
are removed. `clr_pruning_proof_ledger_record` records evidence only. The
connected C producer factories are collision, target-mask overflow, complete
multiset piece-count overflow, and exhaustive line-clear-order impossibility,
exposed through `clr_prune_reason_has_connected_engine_factory`. Reachability,
domain, and language reasons cannot be promoted by filling a ledger entry
manually. An incomplete multiset family, cancelled subset search, or strict
evidence-capacity failure keeps the candidate.
