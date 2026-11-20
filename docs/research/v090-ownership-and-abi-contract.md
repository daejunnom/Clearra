# Rust convergence: operation, equivalence and ABI contract

Source audit at `e7d267ad0276bb8a063280cded433805c9567935`; comparison baseline
`0aa78ef4454c16214ee81bb1cfadec38f72f1e60`. This is a source/ownership contract
for F02–F05, R01 and S01 of the [roadmap](rust-core-and-relation-roadmap-2026-11-20.md),
not qualification, ABI retirement authorization, or release acceptance.

## Product and proof equivalence

Compare the same validated request: initial occupancy, dimensions, target
and original-row frame, rule/shape identities, supply language and hold
contract, objective, and requested evidence level. Two results are equivalent
only for the claims both prove. In particular:

- Piece kind only identifies a supply category. Geometry skeletons identify
  concrete canonical placements in their catalog, not a physical lock by
  themselves. Keep distinct realizations and full original deleted-row mask
  `D`; equal clear counts or equal physical masks do not imply equal
  rotation, kick, witness or canonical candidate identity.
- A necessary condition can reject a proven contradiction in its domain but
  cannot certify an exact realization. An exact geometry realization does not
  prove a path; one reachable witness does not prove a complete lock family,
  all paths, spin/finesse labels, or `CountAll` evidence.
- Keep the selected candidate/operation, executed legal lock, and consumed
  source occurrence distinct. A lock and its simultaneous line clears are a
  single transition, with new deleted rows lifted through the old `D` before
  union. Terminal labels preserve the observations requested by the query;
  deduplicating output identities cannot deduplicate distinct required
  executions.
- Coverage is the weighted union of accepted queues over the same declared
  supply universe and denominator, not a sum of successful traces. It does
  not change with worker count, traversal order, backend, or presenter.
  Conversely, coincidentally equal probability does not establish replay,
  candidate-family, or `CountAll` parity.
- For a complete result compare canonical identity sequence and requested
  family/coverage/terminal evidence. For non-complete results compare the
  reason and authority: an unavailable optional proof is `Unknown`;
  resource-truncated required work is incomplete or a typed error; cancellation
  is distinct. None is a complete negative. Do not read absence in a cache,
  missing adapter, or unvisited branch as a proof of infeasibility.

## Operational owners and lifetimes

`W/` is [the executor's existing Rust CPU implementation](../../crates/clearra-core-executor/src/backend/wasm_cpu/).
The [roadmap §7.3](rust-core-and-relation-roadmap-2026-11-20.md#73-전환재사용의-실제-단위-12개-내부-연산군)
locates its native C counterparts. A key may omit immutable owner parameters
only while the key and value cannot outlive that owner. Replacing an owner
invalidates its IDs; comparing hashes alone cannot establish key equality.

| Group | Current Rust owner and representative structure | Reuse and invalidation contract |
| --- | --- | --- |
| OP01 Row/clear frame | Executor [`CompactedRowFrame`](../../crates/clearra-core-executor/src/row_frame.rs); [`LocalRelationRowFrame`](../../crates/clearra-core-executor/src/conditioned_local_relation.rs) wraps its narrower relation domain | Reuse surviving-row map by target height and exact original `D`; new clears form a new frame. Do not extend 10 × 1–6 compact-mask authority to the native u16 target row descriptor or extended board kernel. |
| OP02 Shape/kicks | `clearra-piece-registry` shape definitions, [`clearra-rules` normalized kicks](../../crates/clearra-rules/src/kicks/kick_table.rs) shared by compact, extended and finesse; [stable replay numbers](../../crates/clearra-rules/src/profile/replay_encoding.rs) | Order, first-success, orientation/center and 180 policy remain part of the rule. A profile ID, its replay number, the C encoding, and a template identity are separate; custom content changes invalidate dependent proof. |
| OP03 Projection filter | `W/inverse_projection.rs::ProjectionRowFilter`, used by compact/extended inverse catalogs | Reuse column requirements/allowed rows by projection policy, geometry, rotation/x and available-row assumptions; if those change, rebuild. Necessary condition only. |
| OP04 Realizations | `W/catalog.rs::GeometryCatalog`, its realization alternatives and instantiation table | Pin catalog owner, skeleton, concrete alternative and full `D`; keep lock pose and exact target reprojection. An alternative's required-deletion subset alone is not the exact domain. Missing optional instantiation table uses the raw exact path. |
| OP05 Support/dependencies | Catalog incidence/support and `W/placement_conditions.rs::PlacementConditionCatalog`; `W/buildup.rs` for physical grounding | Concrete supporters and candidate-specific row contributors are scoped by placement, frame and applicable prefix. Direct contact is not the whole movement dependency; do not reuse evidence across different prefix occupancy. |
| OP06 APDP/SameTile/Hall | `W/geometry_domain.rs::DomainPropagation`; `W/geometry_apdp.rs::ExactArmPairIndex` | Track concrete full-parent alternatives, residual cells, admissible piece mask and residual inventory bound. The APDP prune requires a complete supported parent domain, including after branch rollback; do not infer temporal support from a static-exact subset. |
| OP07 Geometry suffix | `W/geometry.rs` residual, available-row keys and completion oracle; `W/geometry_family.rs::GeometrySolutionFamily` | A compiler-owned residual/family ID is local to that catalog and target/clear policy. It can share geometry suffixes, not prefix support, movement, goal labels, or a supply product; product decomposition needs its own independence proof. |
| OP08 Reachability | `W/reachability.rs::SharedReachabilityTemplates`, `ReachabilityWorkspace`, `ReachabilityCache`; separate extended kernel | Share immutable rule/shape/layout/spawn templates at request scope. Exact occupancy, frame, admitted entry states and query evidence authority constrain cache results. Keep mutable frontier/cache worker-scoped; incomplete cache cannot return an exhaustive negative. |
| OP09 Transitions and labels | `W/buildup.rs::CandidateProjection`, `BuildOrderGraph`, exact execution graphs; `W/result.rs` prepares execution evidence | Reuse physical lock+clear under matching board, concrete operation, `D`, rule and trace mode. Do not collapse terminal scoring/B2B/finesse evidence; interpretation remains with replay/postprocess. Retain candidate graph only for the consumers that require it. |
| OP10 Supply/coverage | [`SupplyExecutionAutomaton`](../../crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs); `W/standard_bag_coverage.rs`, `W/standard_bag_product_memo.rs`; coverage reducer | Source identity, observation, cursor/occurrence, hold policy and state, bag epoch/remainder and denominator belong to the contract. Short node IDs and reduced standard-bag keys stay within their language owner and worker/request epoch. |
| OP11 Completion/ownership | [`ExecutionControl`](../../crates/clearra-core-domain/src/execution_cancellation.rs), executor request/workspace/candidate/result owners; native FFI-owned contexts/scopes | Invalidate stale or dropped candidate evidence and clear native callbacks before ending their owner. Cache admission failure is a miss; optional proof failure is unknown; required exhaustion is incomplete. Do not hold a request alive via returned candidate data unless the result contract needs it. |
| OP12 Services and ABI | [`clearra-app` service dispatch](../../crates/clearra-app/src/app_services.rs), typed executor, and native FFI where selected | Dispatch and support are request-specific; use the [host matrix](v090-rust-host-contract-2026-11-20.md). Migration retires an implementation only after its last required consumer moves, not when an enum name or include changes. |

Shared catalogs/templates, worker-private scratch/cache, candidate-scoped
temporary work, and retained result evidence are four different lifetimes.
Reuse does not prescribe one representation or a new crate per operation.
The disabled complete realization-feasibility prepass in
`W/realization_feasibility.rs` is not evidence of a connected exact service.

## C consumers and compatibility

[CMake's native target](../../core-c/cmake/library_target.cmake) builds
`clearra_core` as a static library and marks `core-c/include` a public include
directory. [`clearra_core.h`](../../core-c/include/clearra_core.h) declares ABI
23 and the version functions. This is a buildable C boundary, not evidence of
a separately published C SDK or of an external deployment using it.

| Class | Checked-in evidence and disposition |
| --- | --- |
| Live Clearra native feature | [Raw imports](../../crates/clearra-core-ffi/src/raw/bindings.rs) under `native-c-core` link the C archive, as do native memory/GPU adapters under their owning features. Catalog and graph handles, path sinks, workspaces, BuildUp buffers/language snapshots, cancellation and profiling remain live contracts when that feature is selected. [Release CI](../../.github/workflows/release-cli.yml) builds a native-C ProductE2E CLI input; its later release CLI build uses `wasm-cpu-runtime,webgpu-search`. The native test input is not proof that a published CLI selects C. |
| Internal C calls and tool consumers | `core-c/src` private headers/TUs, native tests, and `core-c/tools/geometry_benchmark.c` consume C internals or public headers. Private implementation names are not external ABI promises; an include or text validator is not an external consumer. |
| Qualification-only reference | The independent packing enumerator declares three Rust imports in the distinct `clearra_core_test_oracle` link block, restricted to `test`/`test-support`. CMake defaults `CLEARRA_BUILD_TEST_ORACLE` off and [lists its source files separately](../../core-c/cmake/source_manifest.cmake). `W/reachability_reference_tests.rs` provides an independent primitive Rust BFS reference. Keep independent comparisons and adverse fixtures even if native production code is retired. A symbol named `*_for_test` is not automatically absent from production; check its actual definition/link. |
| Confirmed external consumer | None identified by this tracked-source review. It does not inspect distributed binaries, users' static links or downstream source builds. Public headers/symbols without a known internal caller remain external-usage **unknown**, not cleared for removal. |

The native feature itself crosses multiple contract types:

| Symbol/structure family | Declaration owner | Known Clearra boundary and resource contract |
| --- | --- | --- |
| `clearra_geometry_catalog_compile/borrow_view/release`, exact-cover family/split/stream calls | [Internal C packing header](../../core-c/src/packing/packing_problem.h), not `core-c/include` | Known internal C-to-Rust ABI even though the archive has linkable symbols. FFI-owned catalog/graph handles cannot expose ownerless row IDs or borrowed slices; path sink context and views live only for the defined callback scope. |
| `clr_buildup_workspace_create/release`, `clr_buildup_worker_verify_into_buffer`, `clr_buildup_geometry_language_*_v2` | [Public BuildUp variant](../../core-c/include/clr_build_variant.h) and [language](../../core-c/include/clr_buildup_geometry_language.h) headers | Native verifier and result adapter consume capacity-bearing buffers and prepared workspace snapshots. Preserve candidate/operation-set identity, index widths, completion flag, transition mode and bounds. |
| `clr_board_descriptor`, `clr_rule_profile_descriptor`, GPU capability and batch descriptors | Public [board](../../core-c/include/clr_board.h), [rule](../../core-c/include/clr_rules.h) and [GPU](../../core-c/include/clr_gpu.h) headers | FFI problem/rule/native GPU adapters map sizes, capability/status and numeric encodings. A persisted replay ID is not an ABI enum ordinal. Native GPU capability is not implied by the Rust CPU route. |
| `clr_mem_context_* / clr_scope_*` and `clr_execution_control_install/clear` | Public [memory](../../core-c/include/clr_memory.h) and [control](../../core-c/include/clr_execution_control.h) headers | RAII FFI context/scope owners and request execution control retain allocator, teardown and callback obligations; Rust-only computation need not reproduce those C allocation categories. |

Before changing a maintained C ABI, record the identified consumers and exact
target ABI. Verify linkage and version functions, enum/status numeric values,
alignment, field widths/offsets and calling convention against a C caller.
Keep descriptor capacities, full/partial result and callback semantics, and
ownership across create/borrow/release pairs. In particular: catalog/workspace
borrows cannot become retained raw pointers; a
[v2 BuildUp snapshot](../../core-c/include/clr_buildup_geometry_language.h)
must keep prepare/copy semantics without rerunning search; a
[cancellation pointer](../../core-c/include/clr_execution_control.h)
must remain valid until clear; [context/scope](../../core-c/include/clr_memory.h)
objects must be destroyed by the owner and allocator that created them.
Keep the Rust replay numbers, result labels and canonical order as separate
contracts regardless of how C callers are handled.

Retain the ABI or approve a documented break after consumer review. Add a
Rust-exported compatibility shim only for a confirmed preservation scope,
with separate targets for Rust-to-C import and Rust-to-consumer export so
identically named symbols are never linked into the same artifact. Keep the
qualification archive and raw C toolchain out of any future Rust-only product
path once that path, its artifacts, and ABI decision have been verified.

## Tracked tree inventory and retention

Run the [read-only inventory](../../scripts/tools/inventory-tracked-tree.mjs)
from the repository root:

```text
node scripts/tools/inventory-tracked-tree.mjs 0aa78ef4454c16214ee81bb1cfadec38f72f1e60
node scripts/tools/inventory-tracked-tree.mjs e7d267ad0276bb8a063280cded433805c9567935
```

The command accepts a full commit SHA (default `HEAD`) and emits aggregate
counts and blob bytes by ordered, disjoint class and top-level owner area. It
uses `git ls-tree` metadata only, not file contents or working-tree enumeration;
it never follows a symlink/gitlink or inspects ignored build/cache roots.
Inline unit tests remain in their source files. `tools/vendor/` takes priority
over descendant tests. Tracked-generated is an allowlist (lockfiles, atlas and
desktop icon outputs), not an extension-based inference.

| Tracked class | Roadmap baseline paths / blob bytes | Audit SHA paths / blob bytes |
| --- | ---: | ---: |
| First-party source/config | 2,837 / 30,345,766 | 2,843 / 30,654,131 |
| Tests/fixtures and architecture checks | 1,046 / 9,785,291 | 1,049 / 9,839,506 |
| Vendor | 158 / 2,995,205 | 158 / 2,995,205 |
| Tracked generated | 6 / 248,669 | 6 / 248,669 |
| Retained evidence/research/provenance | 101 / 1,413,840 | 102 / 1,426,381 |
| Other docs | 65 / 1,527,507 | 66 / 1,535,881 |
| Gitlinks | 0 / 0 | 0 / 0 |
| Total | 4,213 / 46,316,278 | 4,224 / 46,699,773 |

A logical byte is counted for every path, not only unique blobs. These
figures cannot estimate physical space or decide what to delete. Source
retirement requires consumer, compatibility and qualification evidence.
Reproducible untracked build/transient files are managed separately by the
registered output root, owner, lease and retention policy. Keep raw receipts
under their managed evidence roots and human-reviewed summaries in
`docs/research/`; do not treat all untracked files as caches, or all generated
content as disposable. A release/qualification receipt, vendor license,
active asset catalog, or live reconstruction input requires its owner
decision before retirement. This inventory does not modify storage policy or
delete anything.
