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
assertions, but uses `IOT / [SZJL]!` (cycle seven, no post-cycle borrow). After
the ordered-QB contract was introduced, plain `SZJL` means exactly that order;
the explicit group keeps the fixture's historical unordered meaning. The
canonical compiled universe is `[IOT]!P7` (30,240 words), and intersection with
the QB expression leaves `3! * 4! * 3! = 864` conditioned queue words and one
10-piece inventory. The old `TI / [OS]!` fixture is now represented by the
canonical `[IT]!P7P2` universe plus the explicit QB intersection; its conditioned
count remains `2! * 2! * 5! * 7P2 = 20,160` eleven-piece words, including
different hold-slack completion inventories. A compiler assertion pins the
new fixture's exact pattern/count. No product search limit or algorithm is
weakened. On `3d7da6e`, native job `103679903987` passed all 17 process tests in
**71.35 s**, compared with the previous fixture's **1,519.58 s**. This is a
smaller routing-test input domain, not a product algorithm speedup. Its native
test compilation took 2 min 15 s and is separate from those execution times.

On source `7fcf750817614ecb54901711f79abf005470aba1`, non-publishing run
[34739201596](https://github.com/daejunnom/Clearra/actions/runs/34739201596)
completed source, surface and PC4 jobs successfully. Core **7**, Tablebase
**156** and App **65** tests passed with zero failures. Its App tests executed
the 80 paired ordinary product cases; the App phase took 0.75 seconds. Those
results do **not** cover the later owned score extension described above.

For this change, Rustfmt parsing, diff checks, the PC4 authority mutation
suite and the read-only workflow/jobserver Node tests are local checks. Local
Rust execution remains unavailable after the recorded Windows application
control rejection; Rust execution evidence comes from the exact-source
non-publishing PC4 CI runs below. No policy bypass was attempted.

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
Source, surface and native CLI jobs also passed. The native timing is recorded
above separately from this PC4 test result.

On `dca1172336a8d91f42ae9c7091951fe40a1d6594`, non-publishing run
[34740907051](https://github.com/daejunnom/Clearra/actions/runs/34740907051)
completed all four jobs successfully. Core **7 + 5**, Tablebase **157** and
App **67** tests passed; App took **2.14 s**. This exact source includes the
additional owned-score revocation/non-resurrection and parent-lease reuse
assertions. It does not cover the subsequent fixed/no-draw change below.

## Fixed/no-draw observation union follow-up

The input policy correctly omits bag state when hidden draws are zero, but
the observation adapter previously required a bag even for that scope.
Observation paging now distinguishes real bag reveals from the single
probability-one no-draw outcome. `fixed_queue` declares the entire finite
queue without inventing bag provenance; the existing hold expansion and
aggregate graph budgets remain the sole owners of supply/graph enumeration.
The reveal ledger records probability once, not once per controllable hold
choice, including a zero-solution or queue-exhausted outcome.

The complete candidate adapter admits that fixed scope only when queue,
hold-bound source identity, zero hidden draws, absent bag state and the full
board-area-derived placement horizon agree. A shortened horizon cannot seal
an empty subset as complete. This is a pure composition change, not yet a
new HTTP observation session or a public product route.

New source tests cover transactional fixed-page cancellation/budgets/cursor
identity, real-bag delegation, hold-disabled/empty/occupied supply parity,
no phantom current after exhaustion, zero-draw input acceptance and hidden
draw substitution rejection. A separate adapter fixture covers all five
typed profiles, seven finite-queue cases and page sizes 1/8, retaining one
reveal outcome and the original request identity. These tests require their
own exact-source CI result; the successful runs above are not substituted.

The first no-draw run, [34741935450](https://github.com/daejunnom/Clearra/actions/runs/34741935450)
on `f3731a9`, passed both Core groups and the source/surface jobs, but the
Tablebase test target stopped with E0277. The independent hold-comparison
fixture passed a raw closure where `FixedQueueHoldExpansionGuard` requires
the existing frontier guard adapter. That test call is corrected without
changing the hold algorithm or cancellation contract. The new App cases did
not execute on the failed source.

On `25871f7`, run [34742054330](https://github.com/daejunnom/Clearra/actions/runs/34742054330)
passed Core **7 + 5** and Tablebase **163** (including the new fixed/no-draw
contracts). App compilation then exposed a fixture's nonexistent `Cli`
surface variant; the contract distinguishes interactive and non-interactive
CLI. The fixture now explicitly uses `NonInteractiveCli`, proving a disclosed
fixed queue needs no bag prompt in that stricter surface as well.

Run [34742209353](https://github.com/daejunnom/Clearra/actions/runs/34742209353)
on `004fbe128c55d97a5b311601fb56db9838e187d1` completed all four jobs
successfully. Core **7 + 5**, Tablebase **163**, App **69** passed; App took
**2.11 s**. This covers the fixed/no-draw pure union, not the following Range
composition extension.

## Shared Range lifecycle for observation candidates

The new observation request binds disclosure-ready input to the same source,
board, hold and full placement horizon. Its runtime uses the existing
observation graph/candidate session and physical Core ILC materializer over
one qualified record cache. Missing adjacency or materialization records
become `NeedLookup`; transactional cursors are retried only after the shared
owner admits the exact record. A purported missing record already present in
the cache fails instead of creating a repeated lookup loop.

`AppOnlinePc4CandidateSession` now selects either the existing single-queue
producer or this observation producer. The old fixed-queue name is a type
alias and its constructor remains supported. Lookup IDs, Range validation,
generation pinning and failure/cancellation handling are not copied into a
second controller. Graph/cache budgets span hold/reveal siblings. Progress
reports reveal memberships separately from unique canonical candidate count;
the complete probability ledger is available separately from the union.

The common controller checks cancellation and source/snapshot revocation
even while awaiting a response, and clears the pending lookup when terminal.
It also verifies the initial record's bitmap against the source board before
allowing an empty or nonempty complete result. Selecting an unrelated valid
node must not turn the original request into a supposedly complete empty set.

New source tests drive actual Range admission and Core materialization for
1..4L, five typed profiles and five hold/queue cases, then compare all/chance/
minimum/replay through the existing product path (400 paired cases). Separate
tests cover hidden I-or-O bag outcomes, zero-hit probability mass, immediate
pending-response cancellation/revocation, common cache exhaustion and initial
node substitution. These are synthetic-qualified graphs; they do not prove
real HF completeness or authorize a profile. Execution of this extension
requires the next exact-source CI run. No public CLI/GUI/Discord route or
network transport has been activated by these changes.

### Hold parity exposed a pre-existing replay projection defect

Run [34743209645](https://github.com/daejunnom/Clearra/actions/runs/34743209645)
on `4195f3f300e81aec97a144f9269570e1f69d5d00` passed source, surfaces and native
CLI. Core **7 + 5** and Tablebase **163** passed; App completed **72 of 74**
tests successfully. One failure was an obsolete stale-source diagnostic
assertion after the shared controller's earlier rejection. The other was
ordinary offline `pc.path` (before the TB result was consumed): the replay
language rejected nonempty initial hold because the trace builder projected
every step as cursor `i -> i+1`, hold `None -> None`. Empty-hold store was also
projected with the wrong consumed-piece count, even when both sides agreed.

The correction binds exact replay/scoring traces and language labels to the
batch's initial cursor/hold and selected supply transitions. Store consumes
two queue entries, swap consumes one and updates hold, and playing current
preserves hold. The existing projected terminal-release marker retains its
own supply semantics; it does not authorize a phantom queue draw. Every
counted edge checks its projected output against the existing supply
automaton, including unselected witnesses. Legacy geometry-only callers keep
their explicit synthetic projection. No supply enumeration algorithm is
replaced and no invalid-evidence guard is disabled.

Additional source tests cover occupied/unused/identical hold, empty store,
nonzero initial cursor, canonical count/rank/select versus exhaustive replay,
and invalid transitions/overflow. The non-publishing PC4 job now also runs
Replay library and Postprocess score-batch tests. These changes still require
their own exact-source CI evidence before marking the hold product parity
complete.

The first supply-bound source `8bd310c` passed Core **7 + 5**, Tablebase
**163** and Replay **20** in run
[34743824489](https://github.com/daejunnom/Clearra/actions/runs/34743824489).
The newly included Postprocess test target then exposed two lexical-order
fixture calls still using the old synthetic-label signature (E0061). Those
fixtures now supply actual `PieceDecision` values; lexical ordering and the
maximum cursor label size remain asserted. App hold parity did not execute
on that failed source.

On `584ff49`, run [34743906554](https://github.com/daejunnom/Clearra/actions/runs/34743906554)
passed the same Core/Tablebase/Replay groups and **40 of 41** Postprocess
tests. The remaining historical fixture explicitly expected synthetic empty
hold in every key. It now asserts real store/swap/terminal-release cursor and
hold transitions; exhaustive and rank/select agreement was already passing.
The replay source digest's projection schema is also advanced, preventing
old cached page authority from being reused with the corrected witness keys.

The follow-up Range product matrix adds all three owned score products to the
five supply cases across 1..4L and all profiles: **700 paired cases** rather
than 400. Replay assertions now include the fixture's independently known
consumed queue length and terminal hold, so matching two equally synthetic
results cannot pass. App replay paging/digest tests are included in the same
managed, non-publishing CI transaction. This expansion is pending execution.

On `d5e6265`, run [34744089489](https://github.com/daejunnom/Clearra/actions/runs/34744089489)
passed Core **7 + 5**, Tablebase **163**, Replay **20** and all **41**
Postprocess tests. App passed **73 of 74**: the expanded score fixture used
queue length as its placement window. Canonical score correctly rejected
the empty-hold case's extra source piece. The fixture now keeps placement
horizon equal to the empty-cell requirement and lets the existing compiler
resolve the independently longer disclosed supply window. The product's
fixed-cap/request validation is unchanged.

A further shared-lifecycle audit closes observed cancellation or snapshot
revocation during `admit_range` immediately. Such a rejected response cannot
become a live request again simply because the next guard is fresh; both
fixed and observation sessions retain their terminal state and discard the
pending lookup. Malformed/misrouted response rejection remains transactional
and retryable, as it does not revoke the request itself. A source test covers
late-response non-resurrection without any intervening `step` polling.

On `24c124d5d5195432b105ffa8780d00ba4ed6f42a`, run
[34744298236](https://github.com/daejunnom/Clearra/actions/runs/34744298236)
passed Core **7 + 5**, Tablebase **163**, Replay **20**, Postprocess **41**,
and all **75** PC4 App tests (**11.89 s**). This executes all 700 paired
fixed/hold product cases and the admission-time non-resurrection test.
The separate App replay group passed **12 of 16**, including the P7/lazy-page
contracts, but four tiny memory fixtures inherited Auto workers while directly
calling Core built without the `parallel` feature. They failed immediately
with `WorkerPoolUnavailable`, before constructing any replay owner. Those
memory-only fixtures now explicitly request one worker; production worker
admission and the test resource lock remain unchanged. The complete latest
CI still needs a clean result, not an inferred pass from this partial result.

On `2bfa8cedb9357d50c8d4a9c4e303be2589e166e9`, non-publishing run
[34744560333](https://github.com/daejunnom/Clearra/actions/runs/34744560333)
passed the complete PC4 job: Core **7 + 5**, Tablebase **163**, Replay **20**,
Postprocess **41**, PC4 App **75** (**7.91 s**) and App replay **16**
(**34.25 s**), all zero failures. All four workflow jobs completed successfully;
native CLI passed **17** real process tests (**107.41 s**), alongside source
and surface checks.
The reported durations are CI fixture-group timings, not product-performance
claims. In particular, this is synthetic-qualified Range/hold/product evidence,
not proof of actual HF format/completeness, public network adapters or release
readiness. The pattern-source binding and production-profile gates remain open.
