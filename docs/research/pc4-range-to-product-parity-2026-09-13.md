# PC4 range candidates to ordinary App products

## Implemented boundary

`AppContext::start_pc4_candidate_product` accepts only a source-sealed complete
candidate universe and a typed PC/Scenario request. It reuses the existing App
search preparation (no distributed worker or Geometry search is started), the
request/profile/board/queue-bound Core candidate bridge, and the ordinary
cooperative postprocess/finalizer. It does not define a second minimum solver,
probability calculation, replay projection, or public result format.

The currently admitted products are the standard PC result, typed chance,
typed minimum-cover and typed replay families, for fixed queues accepted by
the existing candidate bridge. A request for another product is not silently
treated as all-solutions. Typed score/tiling and finite-memory handoffs remain
explicitly unsupported until their retained session authority is connected.
Neither this restriction nor this implementation removes an offline product.

The driver checks cancellation and exact source/snapshot identity before and
after Core reduction and every cooperative advance, including completion. A
freshness failure drops the owned finalizer; restoring a guard cannot resurrect
it. A completed execution cannot emit a second response. The normal App
request rejection is preserved, not translated into a tablebase miss.

No HTTP, fallback, dataset activation, CLI flag, GUI control or Discord route
is introduced by this change. The generation/disclosure/range session still
owns candidate production; adapters must explicitly compose it with this
product entrypoint. This is not a production qualification or release claim.

## Independent provider comparison

The existing synthetic range fixtures drive 1L, 2L, 3L and 4L paths for all
five typed rule profiles. Each resulting complete candidate input is now fed
through four ordinary products: all-solutions, chance, minimum and replay.
That is 80 paired product cases, not 80 independent upstream-domain samples.

The online input is produced from graph/index Range responses and physical
materialization, not copied from an offline result. The offline side starts
the ordinary cooperative App search from the same typed request. Assertions
compare canonical field identities, coverage, completion fields, product type,
probability and exact first canonical minimum selection. Replay checks retain
the ordinary App source/steps after graph-observation paging. Work counts and
timings are deliberately not asserted equal across different producers.

Cancellation, mismatched rule profile, explicit finite memory, snapshot
revocation after reduction, revocation during finalization, and single-shot
completion are rejection cases. Fixture execution uses the existing process
resource-test guard and one CPU worker, keeping this a small contract check,
not a performance benchmark.

## Evidence status

On source `18af7e034ccbb4623d775367d478fd622ffcdffd`, non-publishing run
[34738124227](https://github.com/daejunnom/Clearra/actions/runs/34738124227)
has completed the PC4, source and surface jobs successfully. Those results
include Core **7 passed**, Tablebase **156 passed** and App **65 passed**, all
with zero failures. They cover the earlier range/materializer contracts, **not** the product adapter or
the 80 paired cases added here. Its native CLI job was still running at the
bounded check; no full-run success is claimed.

For this change, Rustfmt parsing, diff checks, the PC4 authority mutation
suite and the read-only workflow/jobserver Node tests are local checks. Local
Rust execution remains unavailable after the recorded Windows application
control rejection; the new product cases require the next exact-source
non-publishing PC4 CI run. No policy bypass was attempted.

Actual HF completeness (including the 13 unresolved nonempty edges), exact
profile provenance, missing variant-specific indices, finite resource
authority, hold/pattern composition, typed score reduction, transports and
production activation remain open in the implementation plan.
