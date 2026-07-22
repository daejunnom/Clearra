# Forward Spin Optimization Record

This record keeps exactness evidence beside forward-search performance results and prevents rejected experiments from being repeated. Raw browser-product reports live outside the repository under `%LOCALAPPDATA%/Clearra/reports/forward-spin-optimization`.

## Benchmark Contract

- Board: `0x280f8ffff8f`, height 8.
- Queue: `IOTSZ`, hold enabled, SRS+, one worker.
- Target: one-line T spin.
- Profiles: T-Spins, T-Spins+, and All-Mini+ as the unchanged generic reference path.
- Outcome identity excludes only the arrival-order `id`; all remaining fields are recursively canonicalized and sorted before SHA-256.
- A change is accepted only when status, solution count, and canonical outcome hash remain exact.

## Exact Results

| Profile | Baseline | Accepted, two runs | Count | Canonical outcome SHA-256 |
|---|---:|---:|---:|---|
| T-Spins | 3,299.685 ms, 3,279.480 ms | 248.480 ms, 253.335 ms | 417 | `cef7f7a253c46d69f344bcbd1306dfea42685d621e61cd4c21c77f01e9604740` |
| T-Spins+ | 3,460.980 ms, 3,442.715 ms | 303.515 ms, 312.890 ms | 3,668 | `654776c66f2bb2df52aaf3779218d1064dc7397548fd9263f286a05d5a7f2347` |
| All-Mini+ generic reference | 6,521.075 ms | 2,821.585 ms, 2,909.415 ms | 3,668 | `654776c66f2bb2df52aaf3779218d1064dc7397548fd9263f286a05d5a7f2347` |

The dedicated T profiles visit 4,155 states with a peak frontier of 3,832, down from 107,463 and 73,823. The generic profile keeps its exact 115,449 states and 80,038 peak frontier while benefiting from the shared hot-loop changes.

## Accepted Changes

| Change | Exactness authority | Decision |
|---|---|---|
| T-only structural producer | It checks only T availability, final rotation, corner count, and T-Spins+ immobility; `SpinDetector` remains the final authority | Keep |
| Dense reachability lock table | Direct state/evidence slots preserve each scoring-relevant class and deterministic output order | Keep |
| Equal active/hold branch collapse | The duplicate swap has identical supply state and placement semantics; canonical no-hold evidence is retained | Keep |
| Packed-row line clear | Height selects one, two, or four active words; the 10-column row representation is unchanged | Keep |
| Post-score operation equivalence | Dedupe uses exact placement mask and exact spin result before trace allocation | Keep |
| Immutable pattern queue catalog | Layered worker tasks carry exact pattern identity and retrieve the initialized concrete queue | Keep |
| Tiered board catalog | 64-, 128-, and 256-bit exact keys are selected once by height; hashes choose slots only and full keys confirm equality | Keep |
| Active-word reachability | One, two, or four word monomorphized collision operations match the selected board tier | Keep |

The tiered board catalog increased a 1.6-million-node stress run from 34.526 s to 35.071 s, about 1.58%, while removing roughly 32 bytes of repeated board payload from each frontier state and its index key. The memory reduction is retained under the project's large-search policy.

## Rejected Change

A 64-entry worker-local exact reachability lock cache was measured and removed. Its generic reference average regressed from about 4,114 ms to 4,210 ms, and the T-only path did not recover enough work to justify the lookup cost. Saturation was correctness-safe, but the representation had no measured product value.

## Boundary Checks

- `TOTT` with hold preserves 1,108 outcomes and canonical hash `2c229815483b8643d9c18d4c1664a099c4dc697b5d5f807e92c7124750bc0ca7` while improving from 674.330 ms to 366.795 ms.
- Two concrete six-piece patterns execute through eight product workers in 940.460 ms with exact pattern identity.
- Height 6 and height 24 product runs both complete successfully, covering the 64-bit and 256-bit board tiers.
- Source snapshot SHA-256: `d74ac3489240c1df0b3d1fac672fb604a49fd7ef9a467d39ad0ed5e78ae5a5ee`.
- Product WASM SHA-256: `37c293585a8a14f724e1e177d6315a4065f3dc4532fd97192d0bea17e57d3ea4`.

## Validation

- `cargo fmt --all -- --check` passes.
- `cargo check -p clearra-forward-search -p clearra-wasm -p clearra-web-command -p clearra-app --all-targets` passes.
- The final WASM was rebuilt into `apps/clearra-web/static/wasm` and all measurements used the real browser Web Worker command surface.
- Windows Application Control blocks newly built native test executables with OS error 4551. No bypass, alternate native execution surface, or signing workaround was used.
