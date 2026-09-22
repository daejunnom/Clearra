# Python management implementation v1 (historical)

This directory preserves the Python management implementation that was active
through commit `3a3d86cd1fc8a48a36f3ac242f0d2672ea404ede`.

It is retained for design history, receipt interpretation, and regression
research. It is not an executable project entrypoint. The implementation mixed
generated-output policy and process-memory safety with Git convergence,
dependency installation, toolchain synchronization, package publication, and
repository-wide process registration. That broad scope was retired when the
active manager moved to `tools/clearra-manage`.

The historical tests intentionally remain next to the old modules. They may be
read or adapted as evidence, but CI does not import or execute them.
