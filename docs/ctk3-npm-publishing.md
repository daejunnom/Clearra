# CTK3 npm Trusted Publishing

The `Publish CTK3 to npm` workflow is `.github/workflows/publish-ctk3.yml`.
It does not publish on push, tag creation, or completion of the application
release. A manual run defaults to package preparation only.

## npm connection

In the npm `ctk3` package settings, add a GitHub Actions Trusted Publisher:

| Setting | Value |
| --- | --- |
| Organization or user | `daejunnom` |
| Repository | `Clearra` |
| Workflow filename | `publish-ctk3.yml` (filename only) |
| Environment name | Empty; this job does not select a GitHub Environment |
| Allowed actions | Allow direct `npm publish` |

No npm write token, `NPM_TOKEN`, `NODE_AUTH_TOKEN`, or private configuration
file is required. Do not send credential contents to a chat. The workflow
uses a GitHub-hosted runner, Node 22.23.2 and npm 11.5.1, with `id-token: write`
restricted to the explicit publishing job. npm generates provenance for this
public repository. See the [official npm guide](https://docs.npmjs.com/trusted-publishers/).

## Prepare, then publish

1. Run the canonical `Publish Product Release` acceptance for the exact current
   main source. This is the existing application acceptance, not a second npm
   build. Its CTK3 job builds and tests the distribution once.
2. Dispatch `Publish CTK3 to npm` on main with that successful
   `acceptance_run_id`, leaving `publish` false. The workflow verifies and
   downloads the exact source/run/attempt-bound CTK3 distribution, packs it with
   `--ignore-scripts`, and uploads a seven-day npm tarball artifact. It neither
   executes `prepack` nor installs or rebuilds the workspace.
3. After the npm connection is configured and a new CTK3 package version is
   ready, dispatch with `publish` true. The isolated job publishes that run's
   tarball using OIDC, `--ignore-scripts` and provenance.

The controller and accepted source must be the exact current main SHA. A failed,
running, foreign, or stale-source acceptance cannot authorize publication.
The packed file list must exactly match the verified distribution plus
`package.json`, `README.md` and `LICENSE`.

Application and npm versions are independent. The v0.8.3 application fix is in
the GUI export adapters; it does not change the CTK3 binary format or the npm
codec API. `ctk3@0.1.1` already exists and must not be republished. A future
library change needs its own package version increment before actual npm
publication. Preparing its current tarball is harmless; do not select actual
publication for an already-published version.

Registering a publisher and preparing a tarball are not proof of a successful
OIDC exchange. Only a successful real npm publish establishes that connection.
