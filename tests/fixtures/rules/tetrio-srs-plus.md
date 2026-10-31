# TETR.IO SRS+ source fixture

The authority for this fixture is <https://tetr.io/js/tetrio.js>.
The client bundle was retrieved on **2026-10-31** and audited at
**2026-10-31T13:55:17Z**. A precise download time was not recorded.

- Source bytes: `2547967`.
- Source SHA-256: `9e57c958c43520deb5dd91f5e1773bf779bfb91e7d273dc8494a93a3a0866d2f`.
- Normalized transition SHA-256: `ad25936ae9e4ff0c60b18f75a464ca7b17dd26fa9ac9c6688d242672d404d2a5`.
- Comparison baseline: Clearra commit `5c98563332706a718330bc7ae8e403d2583afe9d`.

The complete client bundle is not redistributed in this repository. The JSON
contains the relevant numeric kick tables, standard piece geometry, source byte
spans and their hashes, and one short origin-guard excerpt. The C header contains
the same normalized numeric transitions for the native regression suite.

## Interpretation

TETR.IO's `SRS+` key is case-sensitive. Its source uses positive y downward.
The selected profile has empty `additional_offsets` and `spawn_rotation` maps.
Clearra preserves the rotation labels and x direction, and negates every kick's
y coordinate. A piece's fixed matrix anchor is distinct from the normalized
bounding-box origin used by some C candidate routines. The source resolves an
explicit selected kickset first and falls back to `SRS+`; no standard piece has
a profile override here. I uses the `i` special family, JLSTZ use the default
family, and the standard O has no special-family alias.

`ComputeKick` attempts the unshifted destination first, before checking whether
the piece is allowed to kick. It then visits the listed offsets in their source
order and returns the first legal result. Consequently each Clearra sequence
explicitly starts with `(0, 0)` even though that attempt is absent from the
source's numeric arrays.

The standard I piece selects `i_kicks`. Its half turns have exactly two attempts
after the implicit origin is included:

| Transition | Clearra y-up attempts |
| --- | --- |
| 0 to 2 | `(0,0), (0,1)` |
| 1 to 3 | `(0,0), (1,0)` |
| 2 to 0 | `(0,0), (0,-1)` |
| 3 to 1 | `(0,0), (-1,0)` |

The standard O piece has `disallow_kick`, but its unchanged occupied cells can
pass the initial origin attempt for both quarter and half turns. It does not
select `oo_kicks`. There are therefore **84** standard transitions, including
four O half turns with the single attempt `(0,0)`.

This fixture describes discrete rotation geometry and candidate order. It does
not claim equivalence for all timing, fractional gravity, lock-delay or top-out
behavior in the complete TETR.IO engine. In particular, application-specific
ceiling policies are separate from this kick profile.

## Baseline comparison and execution contract

All transitions were compared with the source-derived fixture, including
compiling the original C table and the original Rust profile from the baseline.

| Baseline implementation | Matching transitions | Differences |
| --- | ---: | --- |
| Rust SRS+ | 80 of 84 | Four O half turns were absent. |
| C builtin SRS+ | 76 of 84 | Four I half-turn sequences differed and four O half turns were absent. |

The Rust I half-turn sequences already matched TETR.IO. Copying the old six-entry
C sequences into Rust would introduce a regression. The corrected Rust profile
is the canonical default for Rust callers and is projected through the verified
descriptor ABI when invoking C. Direct C callers retain a compatible builtin
table, checked against the same source fixture. SRS, SRS-X and Jstris behavior
are not changed by this correction.

The existing ABI already allows 84 transitions and 12 offsets. Adding the four
O transitions changes no public layout or ABI version. Descriptor and table
identities must still be derived from their complete contents; previously
generated acceleration data must pass the normal identity checks.

## Reproduction

Download the authority with the environment's normal authenticated proxy and
TLS settings, saving it below `_local/artifacts/kick-authority/`. Record the
actual retrieval date and audit time. Do not commit the complete bundle.

```sh
python scripts/tools/extract-tetrio-srs-plus.py \
  _local/artifacts/kick-authority/tetrio.js \
  --output-dir _local/artifacts/kick-authority \
  --retrieved-date 2026-10-31 \
  --audited-utc 2026-10-31T13:55:17Z
```

For a byte-identical copy of this snapshot, the two generated files reproduce:

- `tests/fixtures/rules/tetrio_srs_plus_standard_tetromino_kicks.json`
- `core-c/tests/fixtures/tetrio_srs_plus_fixture.h`

The extractor parses literal data only. It neither evaluates JavaScript nor
executes the bundle. A changed origin guard, nonempty extra offsets, or changed
O dispatch requires a fresh audit rather than silently applying this converter.
The transition hash is SHA-256 of UTF-8 JSON for `transitions_y_up`, using sorted
keys and separators `(',', ':')`; offset order inside each sequence is retained.

The Rust rules test checks all 84 transitions and the source y conversion. The
FFI test checks the complete canonical projection. The C `rule_profile_tests`
checks all 84 native transitions; `candidate_tests` also checks the normalized
I half-turn displacement geometry. `reachability_tests` exercises the native
rotation/reachability path. The generator and fixtures are review inputs, not a
network dependency of ordinary tests or builds.
