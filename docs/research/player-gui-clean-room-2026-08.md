# Player GUI clean-room reference note

## Reference boundary

- Behavioural reference: [fiorescarlatto/four-tris](https://github.com/fiorescarlatto/four-tris/tree/bd736a5cce2ccd6a67e724e1d37ca056d92671c2), pinned at `bd736a5cce2ccd6a67e724e1d37ca056d92671c2`.
- Reference licence: GPL-3.0-or-later. Clearra does not copy, translate, bundle, or derive source code, assets, settings files, names, or UI layout from that repository.
- The reference was inspected only to describe externally observable training-player behaviour. Clearra's implementation is independently specified in TypeScript/Svelte and uses its existing CTK field vocabulary and product shell.
- [Pensil Zen practice](https://pensil.wiki/practice/play?mode=zen) is a separate interaction and settings-semantics reference; its layout and presentation are not copied.

## Behavioural observations

The pinned four-tris revision keeps a mutable playfield with hidden spawn rows, separates input cadence from gravity cadence, exposes configurable movement timings and keys, and supports hold, next queue, ghost, board editing, bag control, undo/redo, and training-mode resets. Its renderer uses a retained back buffer and a dirty marker so an unchanged scene is not rebuilt solely because the main loop iterated.

These observations define behaviours to test, not an implementation to port. Piece geometry, kicks, randomisation, timing, input dispatch, rendering, persistence, and components are written independently for Clearra.

The Pensil reference was inspected through its rendered settings surface on 2026-08-05. Its handling defaults were ARR 0 ms, DAS 83 ms and SDF 41 (instant), with tap-style IRS/IHS. Its visible primary bindings were arrow keys for horizontal movement and soft drop, Space for hard drop, left Control / Arrow Up for counter-clockwise / clockwise rotation, A for 180-degree rotation, left Shift for hold and R for restart. Clearra treats these values as an initial profile only: every binding and timing remains owned by Clearra's validated settings model, and its UI grouping and visual hierarchy are independently designed.

## Adopted decisions

- Keep simulation state allocated across frames and mutate compact numeric board storage in place.
- Run deterministic fixed simulation steps; use `requestAnimationFrame` only to schedule presentation and clamp elapsed time after a hidden or suspended window resumes.
- Render the 10-column playfield with Canvas2D and the shared CTK palette. Svelte receives only low-frequency control/status snapshots.
- Track keyboard state inside the focused Player surface, implement DAS/ARR/soft-drop timing internally, and clear held input on blur, visibility loss, route destruction, and page hide.
- Redraw only while state or animation is dirty. Resize the backing canvas only when its CSS size or device pixel ratio changes.
- Keep all gameplay constants that users may reasonably tune, including gravity, as validated controls rather than mode-owned hard-coded values.
- Use the six-candidate I-piece 180-degree transition from Clearra's deployed compact C SRS+ runtime. The Rust rule layer currently has a narrower two-candidate table; Player does not route through search/WASM replay, and the difference is recorded explicitly rather than silently mixing the two contracts.

## Deliberately not adopted

- No busy, unthrottled native message loop, global Windows keyboard hook, GDI object graph, direct INI writes, or whole-screen copy path.
- No per-frame reconstruction of the field, Svelte cell arrays, DOM cell buttons, settings model, queue, or input maps.
- No CTK3 document encoding/decoding in the simulation loop. CTK conversion is an explicit boundary operation only.
- No reuse of CTK document operation geometry as a gameplay kick system; game rotations and kicks have their own tested rule contract.
- No automatic reset of user-adjustable gravity or handling values merely because a presentation mode changed.
