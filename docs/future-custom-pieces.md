# Future Custom Pieces

Custom pieces, mixed piece sets, custom bags, custom widths, Board128/Board256/Wide layouts, DLX, and area decomposition are future paths. MVP1/MVP2 runtime paths must return explicit unsupported contracts instead of panicking.

MVP3 can progress in parallel on schema and registry contracts before generalized search runtime lands.

- Custom piece definitions use stable `PieceDefinitionId` strings, not enum ordering.
- A definition carries label, cells per rotation, rotation states, spawn bounds, display metadata, area, symmetry, and canonical key.
- The custom operation table schema is derived from interpreted `CustomPieceDefinition` values only. It carries stable operation ids, piece_area, rotation states, operation bounds, stable operation keys, and a schema version; it is not a raw text parser and is not connected to runtime search yet.
- Mixed piece sets may contain standard tetromino entries and custom entries together.
- The piece registry bridge lowers a `MixedPieceSet` into stable piece definition ids, custom operation table schemas, an area multiset, and an explicit runtime path. Standard-only sets keep the standard fast path unaffected; mixed/custom sets expose `custom_piece_runtime_not_connected`.
- Custom bag profiles are linked to a mixed piece set and reference stable piece definition ids from that set.
- `standard-7-bag` is the special case where the bag profile contains seven entries with multiplicity `1` and weight `1`.
- Custom bag profile entries carry `piece_id`, `multiplicity`, and `weight`; fixed sequence, observed window, and bag-aligned boundary support are explicit boundary-model flags.
- Search memo keys, placement table caches, and output trace keys must not depend on registry order.
- C hot-path cache identity includes a piece definition id fingerprint and piece area multiset fingerprint so two mixed registries with the same profile label cannot share cache entries by accident.
- Current runtime validation rejects custom and mixed custom piece sets with `E_CUSTOM_PIECE_UNSUPPORTED_MVP`.
- Current runtime validation rejects custom bag profiles with `E_CUSTOM_BAG_UNSUPPORTED_MVP` until `PieceDefinitionId`-based supply and placement runtime are enabled.
- Observed queue expansion is bag-profile based for standard `PieceKind` runtimes; large custom bag probability universes must expose a sparse-recommended hint instead of assuming dense `PatternBitSet` is always sufficient.
- Board backends are split by area: `Board64` remains the <=64 cell fast path, `Board128` covers 65..=128 cells, `Board256` covers 129..=256 cells, and `Wide` is the dynamic descriptor/validation path above 256 cells. Standard 10-wide 7..24L PC uses the separate fixed-word extended contract rather than the custom-board path.
- C board backend dispatch owns generic row mask, generic operation mask routing, and `clr_board_backend_capability`. Board64/Board128/Board256 operation masks are concrete; Wide operation masks return an explicit unsupported status until the wide search runtime exists, so custom widths cannot silently fall back to Board64.
- Board capability reasons are stable: `board_width_out_of_scope`, `board_backend_not_connected`, and `wide_board_runtime_not_connected`.
- Rust geometry metadata bridges `BoardBackendKind` into the compact C board descriptor through `CBoardDescriptor.backend_kind` and `cell_count`; UI/schema code may display this metadata but must not treat it as runtime support.
- Rust extension schemas use `CustomPieceOperationTable` and `GenericOperationTableDescriptor` to preserve identity and geometry metadata. They are not mirrored into stable C/FFI ABI.
- Custom/Board128/Board256/Wide operation schemas are rejected before C candidate and reachability execution until a complete runtime ships with its ABI.
- Search board operations must route through `BoardStateBackend` for collision, place, clear, row mask, singleton cell masks, and occupied count instead of spreading raw `u64` assumptions into new code.
- Area decomposition uses `BoardStateBackend` and explicit `AreaScope` values so component pruning is shared by `Board64`, `Board128`, `Board256`, and `Wide` without treating empty sky as a scenario target.
- Generic area multiset feasibility uses bounded subset-sum over the active piece areas. It must not use `missing_cells % 4` to decide custom or mixed-piece feasibility; standard tetromino checks remain the fast path special case.
- `AreaScopeDescriptor` keeps scenario pruning explicit: target rows, interpreted target cells, or whole board only when the whole board is truly the target region. `AreaMultisetFeasibility` and `CompileAreaPruner` return reject/search-may-continue decisions only; area feasible is not solution found.
- Generic exact-cover uses `ExactCoverProblemSchema` for cell universe, piece usage, slot, area, required column, optional column, and candidate row contracts. DLX reports `complete`, `searched_nodes`, and `truncation_reason`, and its handoff is `DlxSolution -> operation candidates -> BuildUpProblem -> C BuildUp`; DLX solution is not a BuildVariant.
- Stable BuildUp ABI contains only the connected Board64 runtime. Board128/Board256/Wide and operation counts above 15 return `CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE`; speculative state layouts are not reserved in advance.
- Custom rule editing uses `CustomRuleEditorSchema` for rotation states, spawn rules, kick transitions, first-success order, 180 support, piece-specific overrides, line-clear policy, and lock/reachability mode. `CustomRuleVerificationReport` must report missing transitions, duplicate transitions, invalid rotations, unsupported pieces, unsupported board backends, and unsupported runtime features before `VerifiedCustomRuleProfile` exists. Only verified profiles can pass through `CustomRuleDescriptorCompiler`; raw editor schemas stop with `unverified_custom_rule_rejected_before_execution`.
- Generic GPU is not present in the default ABI or runtime. A future custom-piece GPU package must be default-off, preserve wide masks and piece identity, and add a complete CPU-confirmed execution path before its capability can leave `Unsupported`.
- Custom skin and theme editing uses `CustomSkinThemeSchema` for `skin_id`, `palette_id`, `piece_mapping`, `grid_style`, `background`, `line_clear_highlight`, `ownership_color_mode`, `export_limits`, and `provenance`. User-imported assets must stay in `user_config_directory` or `user_cache_directory`, not repository assets. Manifest/provenance is required, previews are PNG-atlas only, and `runtime_raw_svg_allowed=false` keeps raw SVG out of the runtime renderer.
- Current runtime validation rejects `Board128`, `Board256`, and `Wide` search paths with `E_CUSTOM_BOARD_UNSUPPORTED_MVP` until placement tables and PC search are generated per board backend.

Fixture contract:

- `tests/fixtures/pieces/mixed_custom_piece_set.json` pins the native MVP3 schema shape.
- `clearra-invariant-tests::custom_piece_contract_tests` reads that fixture, builds the registry and bag models, checks stable piece ids, and verifies the unsupported diagnostic guards.
- `clearra-invariant-tests::board_backend_contract_tests` verifies `Board64`, `Board128`, and `Wide` implement the backend boundary while custom board runtime paths stay guarded.
- `clearra-invariant-tests::area_decomposition_contract_tests` verifies backend-generic connected components, scoped scenario pruning, and tetromino/custom-area tileability rules.
