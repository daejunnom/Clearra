# PC4 ledger integration checkpoint — 2026-09-13

This is implementation evidence, not profile qualification or release acceptance.
The v0.8.1/v0.9.0 scope and release order remain in the active release plan.

## Integrated changes

- `8030715` integrates the graph-derived observation outcome ledger. Every bag
  reveal remains in the exact probability universe, including zero-solution
  outcomes. Hold siblings do not duplicate random mass. Graph traversal,
  concrete materialization and the reveal ledger must all exhaust before the
  canonical candidate union can acquire reducer authority.
- `5d4bfba` updates the architecture mutation test to require the actual sealed
  evidence constructor, not an obsolete raw evidence struct literal.
- `6d55e0e` replaces the Windows managed compiler's cmd transport with a native
  argv launcher. The shared Node guard still owns output and transaction checks.
  The launcher is built inside the same generation and has no external cache.
  Bootstrap failure follows normal owner cleanup, including failed product
  deletion and lease release.

## Confirmed verification

- Managed `cargo test --locked --offline -p clearra-pc4-tablebase --lib`:
  **147 passed**, zero failures; test runtime 0.05 seconds. These are local
  algorithm/contract fixtures, not upstream graph completeness evidence.
- Node build policy suite: **11 passed**, including a Windows argument-vector
  test exceeding cmd's command-line limit with quotes, Unicode and backslashes.
- PowerShell artifact path/lifecycle suite: passed, including injected launcher
  failure cleanup, mixed Windows/WSL metadata, product history and stale leases.
  Its deliberate owner-mismatch fixture emits a lease-preservation warning;
  that warning is expected negative-path evidence, not a leaked real build.
- PC4 full-solution authority validator mutation suite: passed.
- Native launcher rustfmt check and Git whitespace check: passed.

## App validation and discovered constraints

The original Windows cmd wrapper failed while compiling `windows-sys` with
`The command line is too long`. The native launcher passed that same dependency
compilation boundary without removing the guard.

The default-feature App unit-test binary with `online-pc4-tablebase` subsequently
failed in LLVM with `out of memory`. No App test success is claimed for that run.
An attempted no-default-features test exposed a separate feature boundary:
`commands/render_app_command.rs` imports bitmap-render-only output types even
when `bitmap-render` is disabled. That configuration cannot currently compile;
it is not a PC4 algorithm test failure and must not be used as a passing shortcut.

A lower-symbol retry uses the real default product features, sets only
`CARGO_PROFILE_TEST_DEBUG=0`, bounds Cargo jobs to two, and selects App tests
matching `pc4_`. This preserves test assertions and the product feature set.
This retry completed compilation and ran **60 tests: 59 passed, one failed**.
The failure was the synthetic Setup helper `admitted_for`, which attached a 4L
candidate target to requests for every target from 1L through 4L. Production
correctly refused that mismatch. The helper now passes the request's exact
target to `reducer_for_target`; this fixture correction still needs a compiled
rerun and is not reported as a passing 60-test suite.

Before changing that fixture, the same completed test executable was reused
without rebuilding to run `pc4_observation_candidate_adapter`: **19 passed**,
zero failures, 0.01 seconds. It covers zero-solution mass, hold deduplication,
scope/profile/source binding, cancellation/rollback, budgets, and delayed
finalization. Its SHA-256 was
`DC8777D40CB25952ACD9BAF69C009DB50E1C9350946ACC28658AF092C975D900`.
The compiled tracked source was `6d55e0e`; the subsequent fixture edit is not
covered by this binary. There are no running build/test jobs at this checkpoint.

## Remaining integration boundaries

- Inspect and remove/clarify the unpublished compatibility aliases
  `successful_reveals`, `observed_successful_reveals`, and
  `observed_successful_reveal_count`: the ledger implementation now includes
  unsuccessful outcomes, so those names are unsafe guides for future callers.
- Repair the App bitmap-render feature boundary with appropriate feature-on/off
  coverage rather than silently enabling an excluded feature.
- Complete surface/reducer and fallback integration with actual runtime tests.
- Qualify each independently activated HF profile and target using upstream
  bytes/KAT/completeness and signed generation evidence. Synthetic fixtures do
  not authorize a production profile. V*/policy/Krylov remain out of this runtime.
- Final exact-SHA v0.8.1 acceptance/deployment must precede v0.9.0 activation.
  Main has not been merged, pushed, or deployed by this checkpoint.
