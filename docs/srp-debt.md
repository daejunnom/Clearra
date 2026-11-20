# SRP Work Guidelines

The repository work rules in [AGENTS.md](../AGENTS.md) govern code organization.
SRP is a design and review guideline, not a static gate or a requirement that
each file have exactly one responsibility. Rust organization follows data
ownership, invariants, computational reuse, and execution requirements.

There are no line-count limits, preferred size bands, required `SRP rationale:`
comments, or file/type/crate-count targets. File layout, helper names, and marker
text do not establish compliance. Do not reinstate the former structural gate
or require a textual exemption for a larger or combined module.

## Organize Around Useful Ownership

- Start with the existing owner of the data and computation. A public contract,
  independent algorithm, or independently reusable stage can justify its own
  module; that choice does not require a separate file for every type or method.
- Keep private helpers, constructors, and state transitions together when they
  share an invariant or lifetime. Split a module when the resulting boundary
  makes the implementation and its consumers easier to reason about.
- Check reuse at the level of operations and derived results. Sharing a source
  file or wrapper is insufficient if each caller reconstructs the same catalog,
  projection, relation, or evidence. Preserve all inputs and scope conditions
  that make a previously computed result valid.
- Prefer existing Rust structs, enums, and inherent implementations where they
  express real ownership. Do not add class-like layers, traits, dynamic dispatch,
  or shared mutable owners merely to satisfy a decomposition pattern.
- Treat a small work item as a reviewable change, not a request for a new file.
  Related methods, tests, and manifest entries may remain together. A useful
  public boundary can remain small without being considered debt.

## Performance and Memory Tradeoffs

Execution time and memory efficiency can justify keeping multiple conventional
responsibilities in one module or fusing their operations. This is appropriate
when a split would duplicate calculations, allocate or copy intermediate data,
retain results longer, or add avoidable dispatch and synchronization.

Evaluate the affected computation and data lifetime rather than assuming that
more abstraction improves the design. Explain material tradeoffs in the change
review using algorithmic reasoning and relevant measurements when available.
No fixed comment, per-file statement, or exception approval mechanism is
required. Conversely, a vague performance claim does not justify an unrelated
redesign or the creation of another independent solver.

Keep exactness, completeness, resource reporting, public API contracts, and
unsafe-boundary isolation intact. Performance-driven organization does not
permit dropping valid solutions or confusing incomplete evidence with an
impossibility proof.

## Tests and Tooling

Group tests around observable behavior and reusable fixtures. BuildUp, coverage,
resource outcomes, replay, kick evidence, and goal conditions can have distinct
test owners without a mandatory file per behavior or case. Preserve independent
oracles when implementation helpers are shared.

Product E2E still needs build, execution, assertions, and report behavior, but
their physical file boundaries should follow reuse and data ownership rather
than a fixed function-to-file mapping. Avoid creating unintended Cargo test
targets when placing Rust integration-test helpers.

The historical `validate_srp_policy.ps1` name currently refers to PowerShell
syntax parsing. `validate_file_size.ps1` supplies its source list. Neither checks
SRP, file length, mandatory comments, or module ownership. Syntax, dependency,
ABI, and safety checks retain their own meanings; passing them does not prove
SRP or solver correctness. See [Architecture Validation](architecture-validation.md).

## Review Questions

1. Which existing object or computation already owns this meaning?
2. Are repeated derivations being reused under the same valid inputs and scope?
3. Does the proposed boundary clarify invariants and data lifetime?
4. Would splitting introduce duplicate work, copies, allocations, retention,
   dispatch, or synchronization that a combined implementation avoids?
5. Are public API, exactness, resource, and unsafe contracts still preserved?
6. Do the relevant tests exercise behavior and counterexamples independently of
   marker text or implementation naming?

## Historical Inventory

The earlier July 2026 consolidation record reported roughly 19,791 entries with
a median size near 9 lines, followed by 2,102 source files with a 65-line median.
It reported 214 files at or below 10 lines and 383 at or below 20 lines. These
are historical inventory figures, not current measurements, recommended sizes,
warning thresholds, or acceptance criteria.
