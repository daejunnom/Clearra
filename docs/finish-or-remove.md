# Finish-or-Remove

Clearra product code exposes only three stable capability outcomes:

- `ConnectedExact`: the product route is connected and satisfies its exactness contract.
- `ConnectedApproximate`: a general runtime algorithm is connected and its accuracy limits are reported.
- `Unsupported`: no product execution route exists; the request returns an explicit unsupported error and capability reason.

`Preview`, `Scaffold`, `Placeholder`, `ExampleResult`, `FixtureFallback`, and
`WillBeConnectedLater` are not product states. Experimental implementations
must live in a separate package behind a default-off Cargo or CMake feature.

Fixture files may supply typed test or user input, but they must not synthesize
packing candidates, BuildVariants, coverage rows, replay traces, or successful
`AppResponse` results. An unavailable runtime returns `AppStatus::Unsupported`
with no `AppResult` and no render model.

The default release static gate rejects known fallback/result-construction APIs
and obsolete capability states. Runtime correctness remains the authority of
the executed native and adversarial gates.
