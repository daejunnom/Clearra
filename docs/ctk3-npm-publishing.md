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

1. Dispatch `Publish CTK3 to npm` on main. Leave `acceptance_run_id` empty for
   a package-only release: the workflow installs the locked CTK3 dependency
   closure, builds and tests CTK3 once, and seals its distribution to this
   source/run/attempt. It does not dispatch an application release, build WASM
   or native applications, or deploy Pages/Discord.
2. Alternatively, supply a successful canonical `Publish Product Release`
   `acceptance_run_id` for the exact current main SHA. This reuses its already
   built and tested CTK3 distribution without rebuilding or reinstalling the
   workspace. It does not accept an unrelated or old-source run.
3. Leave `publish` false to prepare only, or set it true after the npm
   connection is configured and a new CTK3 package version is ready. Both paths
   verify the source/run/attempt-bound distribution, pack it with
   `--ignore-scripts`, and upload a seven-day tarball artifact. The isolated
   publish job uses OIDC, `--ignore-scripts` and provenance on that tarball.

The controller and verified source must be the exact current main SHA. A failed,
running, foreign, or stale-source canonical acceptance cannot authorize reuse.
Package-only verification authorizes npm publication only, not whole-application
acceptance or production deployment.
The packed file list must exactly match the verified distribution plus
`package.json`, `README.md` and `LICENSE`.

Application and npm versions are independent. The v0.8.3 application fix is in
the GUI export adapters; it does not change the CTK3 binary format or the npm
codec API. `ctk3@0.1.1` already exists and must not be republished. The manually
requested OIDC publication uses the next patch, `ctk3@0.1.2`, independently of
the application version. Future publications need their own package version
increment; never select actual publication for an already-published version.

Registering a publisher and preparing a tarball are not proof of a successful
OIDC exchange. Only a successful real npm publish establishes that connection.
