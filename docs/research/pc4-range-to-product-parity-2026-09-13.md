# PC4 range candidates to ordinary App products

## Implemented boundary

`AppContext::start_pc4_candidate_product` accepts only a source-sealed complete
candidate universe and a typed PC/Scenario request. It reuses the existing App
search preparation (no distributed worker or Geometry search is started), the
request/profile/board/queue-bound Core candidate bridge, and the ordinary
cooperative postprocess/finalizer. It does not define a second minimum solver,
probability calculation, replay projection, or public result format.

The borrowed compatibility entrypoint admits the standard PC result, typed chance,
typed minimum-cover and typed replay families, for fixed queues accepted by
the existing candidate bridge. A request for another product is not silently
treated as all-solutions. Typed tiling and ordinary finite-memory handoffs remain
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

### Owned score handoff

`AppOnlinePc4FixedQueueCandidateSession::into_completed_reducer_input` moves
the finished vector instead of cloning it. Graph/replay caches and the family
owner are released with the consumed session. Freshness/cancellation checks
bracket the handoff, and incomplete or poisoned sessions cannot produce input.

`AppContext::start_pc4_owned_candidate_product` additionally admits typed
field-average score, score-minimum and fixed-queue highest-score products.
It measures vector capacity and every owned source/qualification string
capacity, including the separate provenance evidence. App and compiled memory
limits must agree conservatively; an explicit App override cannot be ignored.
The current typed-score contract forbids query-level memory overrides and
chooses its cap through the parent authority. Thus this entrypoint uses that
same fixed-cap policy and rejects an independently supplied finite App cap;
it does not claim arbitrary caller-selected finite-memory support.
These owners must fit the existing closed typed-score retained-owner proof.

Core validates all canonical identities, performs ordinary reachability and
coverage reduction under a child of the existing request authority, then
returns the completed result with its still-live verifier session. The normal
App score pipeline uses that session for future-allocation checks. The child
and candidate input are dropped before rich response construction. Score-
minimum uses the existing cooperative exact-first/lazy-tie portfolio cursor.
No score arithmetic, minimum solver or public result contract is duplicated.
This currently uses an ordered single verifier, not a parallel performance
claim, and does not automatically wire any product transport.

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

The owned score extension adds another three products for each of the same
20 synthetic paths (60 paired cases; 140 total). It compares the complete
typed public score payload, including field scores, fixed-score selection and
the first canonical score-minimum portfolio. Separate tests cover exact memory
boundaries/overflow, rejected-candidate lease release, spare vector/string
capacity, and incomplete/cancelled/stale consuming handoffs. These cases passed
the exact-source PC4 job recorded below. An additional score-summary/portfolio
revocation check now covers source invalidation after Core and on both sides
of the first advance, lease reuse after rejection, and non-resurrection.

## Evidence status

On source `18af7e034ccbb4623d775367d478fd622ffcdffd`, non-publishing run
[34738124227](https://github.com/daejunnom/Clearra/actions/runs/34738124227)
completed every job successfully. Those results
include Core **7 passed**, Tablebase **156 passed** and App **65 passed**, all
with zero failures. They cover the earlier range/materializer contracts, **not** the product adapter or
the 80 paired cases added here. Native CLI passed all **17** process tests in
1,519.58 seconds; the new Setup routing/alias fixture accounts for most of that
time, because its one-piece prefix limit does not bound the 4L completion
geometry. This is not evidence of a hung process or a benchmark speedup.

The next fixture keeps both real CLI family completions and their typed payload
assertions, but uses `IOT / SZJL` (cycle seven, no post-cycle borrow). The compiled
pattern is `[IOT]![SZJL]![^SZJL]!`: `3! * 4! * 3! = 864` queue words and one
10-piece inventory. The old `TI / OS` fixture compiled to
`[IT]![OS]![^OS]!P2`: `2! * 2! * 5! * 7P2 = 20,160` eleven-piece words, including
different hold-slack completion inventories. A compiler assertion pins the
new fixture's exact pattern/count. No product search limit or algorithm is
weakened; its wall time still needs the next exact-source native CI result.

On source `7fcf750817614ecb54901711f79abf005470aba1`, non-publishing run
[34739201596](https://github.com/daejunnom/Clearra/actions/runs/34739201596)
completed source, surface and PC4 jobs successfully. Core **7**, Tablebase
**156** and App **65** tests passed with zero failures. Its App tests executed
the 80 paired ordinary product cases; the App phase took 0.75 seconds. Those
results do **not** cover the later owned score extension described above.

For this change, Rustfmt parsing, diff checks, the PC4 authority mutation
suite and the read-only workflow/jobserver Node tests are local checks. Local
Rust execution remains unavailable after the recorded Windows application
control rejection; the new owned score cases require the next exact-source
non-publishing PC4 CI run. No policy bypass was attempted.

Actual HF completeness (including the 13 unresolved nonempty edges), exact
profile provenance, missing variant-specific indices, ordinary finite resource
authority, hold/pattern composition, transports and
production activation remain open in the implementation plan.

The first submitted product-adapter run, `34739031424` on `e905f9d`, passed
Core 7 and Tablebase 156 but stopped while compiling App. The new adapter
mistakenly called host-memory accessors on `SearchProblemBudget`, which owns
node/time/result/pattern limits. The compiled memory limit actually belongs
to `SearchProblem::backend_policy()`. That access is corrected; the product
matrix was not executed on the failed source.

Owned-handoff run [34740492306](https://github.com/daejunnom/Clearra/actions/runs/34740492306)
on `9cb6d05` passed Core materializer **7**, retained verifier **5**, and
Tablebase **157** tests. App passed **63** but failed the four product-matrix
tests before executing scores: the new fixture incorrectly added a 64-MiB
query override to a typed-score request whose existing contract forbids it.
The correction uses the canonical CPU/no-fallback/pattern-cap policy and
the existing Jstris Ultra/T-spins contract for fixed-score. Product validation
is not relaxed. The new Core unused-must-use test warning is corrected too.

On `3d7da6ed2df15b447c58252a1f00128524ee01ca`, non-publishing run
[34740735499](https://github.com/daejunnom/Clearra/actions/runs/34740735499)
passed the PC4 job: Core materializer **7**, parent-authorized verifier **5**,
Tablebase **157**, App **67**, zero failures. The App phase took **2.01 s** and
executed all 140 paired product cases, including the 60 owned score cases.
Source and surface jobs also passed. Native CLI was still running at that
check; no full-run completion or speedup is claimed yet. Later additions to
the revocation test require their own exact-source check.
