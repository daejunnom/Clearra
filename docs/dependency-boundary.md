# Dependency Boundary

Clearra routes every product request through one typed boundary:

`CLI / GUI / WASM Command Runtime -> AppRequest -> clearra-app -> validation -> SearchProblem compile -> clearra-core-executor -> C PackingProblem -> C Geometry Skeleton Exact Cover -> Host reducer -> C BuildUp BFS -> CoverageRow -> CoverageMatrix -> ObjectiveResult -> Replay / PostProcess / Scoring / Fumen / Render / Output -> AppResponse`.

## Owners

Rust owns product contracts, validation, diagnostics, profiles, supply models, coverage probability, objective policy, replay models, score and postprocess orchestration, output, GUI, WASM contracts, fixtures, and golden tests.

C core owns board operations, piece operation tables, immutable realization/skeleton catalogs, geometry exact-cover, candidate generation, reachability, BuildUp BFS, coverage row generation, memory scopes, arenas, release queues, and portable/native GPU bridges.

## Allowed Edges

- `clearra-cli -> clearra-app`
- `clearra-app -> clearra-validation`
- `clearra-app -> clearra-problem`
- `clearra-app -> clearra-core-executor`
- `clearra-app -> clearra-postprocess`
- `clearra-app -> clearra-output`
- `clearra-core-executor -> clearra-core-ffi -> core-c`
- `clearra-coverage -> clearra-core-domain`
- `clearra-objectives -> clearra-coverage`
- `clearra-postprocess -> clearra-replay`
- `clearra-postprocess -> clearra-spin`
- `clearra-postprocess -> clearra-scoring`
- `clearra-postprocess-gpu -> clearra-postprocess`
- `clearra-output -> clearra-replay`
- `clearra-output -> clearra-fumen`
- `clearra-output -> clearra-render`

## Forbidden Edges

- `clearra-cli -> clearra-core-ffi`
- `clearra-cli -> core-c`
- `clearra-gui-host -> clearra-cli`
- `clearra-gui-host -> core-c raw pointer`
- `clearra-coverage -> clearra-scoring`
- `clearra-spin -> clearra-scoring`
- `clearra-scoring -> core-c raw binding`
- `clearra-postprocess -> clearra-core-executor`
- `clearra-postprocess-gpu -> clearra-core-executor`
- `clearra-postprocess-gpu -> clearra-core-ffi`
- `clearra-render -> clearra-core-executor`
- `clearra-fumen -> clearra-core-executor`
- `clearra-core-executor -> clearra-render`
- `clearra-core-executor -> clearra-fumen codec internals`
- `clearra-core-executor -> clearra-scoring` at runtime

`clearra-core-executor` test support may use spin-target fixtures, but product
execution ends at BuildVariant, CoverageRow, and replay seed.
`clearra-app` hands that seed to `clearra-postprocess`, which owns scoring and spin interpretation.

## Required Gates

- `architecture_validation_rejects_cli_to_core_ffi`
- `architecture_validation_rejects_gui_to_cli`
- `architecture_validation_rejects_render_to_solver`
- `architecture_validation_rejects_fumen_to_solver`
- `architecture_validation_rejects_coverage_to_scoring`
- `architecture_validation_rejects_spin_to_scoring`
- `architecture_validation_rejects_core_executor_runtime_scoring`

Temporary boundary exceptions are forbidden. New product features must add typed contracts instead of weakening these dependency rules.
