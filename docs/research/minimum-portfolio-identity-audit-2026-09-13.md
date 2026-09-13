# Suspected duplicate minimum portfolios: focused identity audit

## Scope and disposition

The user reported that identical solution sets might be numbered as different
equal-cardinality minimum portfolios. They subsequently clarified that there
was no remembered page pair or reproduction sequence and that the observation
might have been mistaken. This audit records **not reproduced**, not a confirmed
fix and not proof that every possible input is unaffected.

Workspace: `codex/v0.9.0-stacked-on-v0.8.1-20260912`, HEAD `71b26f4`, with the
in-progress host-only Range changes. The 4194 engine remains the existing
`6ca1dde572979b1fd4201e045fc34af80a2503de` WASM; there was no new Rust build,
artifact replacement, deployment, or minimum-search algorithm change.

## Browser evidence

Input: `ctk3_w0kCQBjwwAMPPAD37g`, P7, target 4L, Jstris 180, hold enabled,
all supplied queue visible, minimum solutions, TB disabled, 11 compute slots.
The completed first result reported cardinality 25 and 16.4 s.

For alternatives 40 and 41, the rendered 10x4 cell piece classes were read from
all 25 visible boards. Comparisons removed labels and compared both ordered
lists and sorted field multisets:

- Both pages contained 25 distinct rendered fields.
- The ordered lists differed, and the unordered field multisets also differed.
- Exactly 19 fields were shared. Each page had six fields absent from the other.
- Returning from 41 to 40 restored the recorded 40 field list exactly.
- A separately observed completed page 23 also differed from both 40 and 41.

This is a bounded DOM-level comparison, not an extraction of hidden worker
state or a complete enumeration. Other page-number movements during the shared
interactive session are not attributed to a single automated click: no isolated
causal trace establishes whether user interaction overlapped those observations.

## Source boundaries checked

- `CoveragePortfolioAlternativeSet` sorts normalized candidate keys and rejects
  duplicate keys before assigning its dense candidate map.
- `ExactMinimumCoverPortfolioEnumerator` only emits strictly increasing row
  vectors of the proven cardinality that cover the required universe and are
  not below the current frontier. After emission, it advances to the numeric
  successor and clears the completed pending search. Thus a member permutation
  is not an independent combination, and the same row vector is below the next
  frontier rather than an additional tie.
- The App page store projects those row vectors directly to candidate IDs.
  Evicted-page replay uses its own cursor; it does not rewind the next-page
  high-water enumerator.
- The shared GUI pager checks source/map identity and exact decimal alternative
  and member-page coordinates. Its member/render/export bindings include the
  selected alternative, rather than cardinality alone.

Existing Rust regressions cover numeric lexicographic family order, exhaustive
small-matrix comparison with brute force and checkpoint suffixes, parallel-first
to serial-continuation handoff, and replay of evicted exact identities. These
tests were inspected, **not rerun as Rust tests in this audit**.

## Executed validation

- `productResultPager.contract.ts` passed in memory, including demand-only lazy
  paging, backtracking, cache eviction, stale requests, and exact large indices.
- Selected-portfolio export tests passed, including `1 -> 2 -> 3 -> 4 -> 2 -> 1`
  cache churn, full-set CTK3 export, duplicate member rejection and source/outer
  alternative identity checks.
- The combined focused Node invocation passed 59 tests (including the prior
  Range/host changes); three in-memory TypeScript contracts passed.

No speculative UI deduplication was added. Equal coverage does not make two
different fields the same solution; collapsing such rows would incorrectly
remove valid minimum alternatives. A future confirmed reproduction should retain
the input/profile and selected page pair before changing the shared owner logic.
