# v0.8.1 exact accelerator ingress parity

## Scope and authority

This is a reviewed source/focused-test record, not a benchmark, complete
candidate-family comparison, profile qualification or release receipt. The
v0.9.0 Tablebase work remains frozen. No unchanged profile asset is regenerated.
The existing production command registry, hardware admission and reducer
semantics remain authoritative.

## Corrected boundary

Build v2 and Setup-score already reject repeated or conflicting exact
accelerator selections. Generic PC, Setup and Build-probability previously
overwrote the earlier selection. Native compatibility PC/Setup did so too.

Each legal-board and conditioned-reachability selector now has one occurrence
per request. Both conflicting orders and repeated identical flags are rejected
before typed lowering; one legal selector and one conditioned selector remain
independent. Implicit defaults remain enabled. Other native compatibility
options retain their existing grammar.

Build-probability tiling now rejects explicitly enabled reachability
accelerators, matching the existing PC tiling inactive-option contract. An
explicit disabled selection and an implicit default are still accepted; no
accelerator is made active in the tiling solver.

Discord continues forwarding these selectors on its permitted command paths.
It must not deduplicate an ambiguous request and thereby hide it from the CLI
parser. The curated Discord Build v2 registry still rejects explicit
accelerator overrides: this change does not widen its closed option surface.

## Focused validation coverage

- Shared CLI text/argv: default policy, all four independent toggle pairs,
  duplicate/conflicting occurrences and tiling inactive options. Generic and
  named PC products, Setup and Build request paths are included.
- Native compatibility CLI: duplicate/conflicting selectors in legacy PC,
  Setup and the Setup-finder alias.
- Production Desktop parser, shared CLI argv and Web command text: full typed
  `AppRequest` equality, not just the two boolean fields. The matrix includes
  five profiles, worker requests 1/2/11, four toggle pairs, PC products and six
  Build-probability result modes. No executor or legacy Desktop DTO is used.
- Desktop worker admission uses the real hardware limit. A runner without
  capacity for a requested count must reject it on every surface; it does not
  clamp workers or pretend to execute an 11-worker search.
- Discord forwarding/idempotence and closed Build v2 policy; non-publishing CI
  registration contracts.

The matrix compares ingress meaning only. Worker activity, installed-pack hits,
reducer results, replay/copy rendering and aggregate memory are not measured by
these tests.

## Local evidence

| Check | State | Scope |
| --- | --- | --- |
| Shared CLI exact-accelerator tests | 5/5 passed | Includes three new ingress tests; no solver execution |
| Discord and focused CI source contracts | 8/8 passed | Three forwarding/closed-policy tests and five workflow contracts |
| Cargo formatting and whitespace check | Passed | Changed Rust crates and patch only |
| Production Desktop typed-request matrix | 2/2 passed | 780 request combinations plus 78 ambiguity cases; real Desktop parser, no search |
| Native compatibility selector and assembler tests | 6/6 passed | Includes 24 new ambiguity cases; defaults, tiling, Setup-score help and typed assembly, no search |

The shared CLI supervised run
`1790524017031420800-40116-runtime.json` reports normal return 0, no descendant
at exit, and a stopped process tree. Raw output remains in declared local
receipts; it is not copied into this document.

The Desktop supervised run
`1790524233176755000-41872-runtime.json` also reports normal return 0, no
descendant at exit and a stopped tree. It reports two memory-pressure events
during compilation; the run is not used as performance evidence. For matrix
rows above the runner's actual hardware admission, successful rejection is the
expected outcome, not a valid search sample.

The native selector/assembler supervised run is
`1790524466603181400-41928-runtime.json`, with normal return 0, no descendant
at exit and a stopped tree. Two compile-time memory-pressure events remain in
the receipt and are not discarded. This record does not grant release authority
or prove that a selected accelerator is resident or used.

The new coverage is registered in the existing branch-only, non-publishing
`v081-selective-source-ci.yml`. Previous successful CI is evidence only for its
previous source; the next run must validate this patch independently. There is
no workflow dispatch to production or main promotion in this change.

## Still open

1. Complete candidate/reducer/output parity through CLI, Web, Desktop and
   Discord, including minimum/lazy ties and replay/copy/page.
2. Actual qualified-pack browser multi-verifier hit/fallback, cancellation and
   replacement readback; input equality is not asset execution evidence.
3. Active-session shared peak and performance adoption gates, deliberately
   deferred to the later benchmark phase.
4. Exact-source acceptance, main promotion, deployment and rollback readback.

Earlier source, CI and asset evidence remains bound to its original identity.
This focused correction does not close any of the above gates.
