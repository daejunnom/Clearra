# v0.8.1 conditioned-reachability source domain

The five `*.solver-cover.v1.json` files are source declarations for separate
SRS, SRS+, SRS-X, Jstris 180 and no-kick relation packs. They are not the
binary assets or a release signature.

Each profile declares exactly 56 actual BuildUp `sky` entry/first-exit Boolean
contexts: every standard tetromino at heights 1–6, with original-row frame 0
at all heights and frames 1 and 2 also at height 2. In each context the lowest
eight physical-board occupancy bits are free; every other physical cell is
fixed empty. This is a **bounded support domain**, not all boards or all entry
pose sets. Source parser and product verifier independently reject missing,
duplicated or narrowed contexts. Within every declared context the generator
must cover *all* 256 allowed occupancies (or the smaller placeable subset),
using audited collision-dependency cubes and a symbolic completeness proof.

The solver's common entry/window derivation, profile rule identity and original
row frame are part of the source binding. A query outside this support region,
or one requiring witness, spin, finesse or CountAll evidence, stays on the
existing exact reachability path. A signed product catalog is required before
any pack can be installed as release authority. Source-bound candidate checks
alone never mark a profile `Qualified`.

The source files use per-context limits of 1,024 records and 1,000,000 proof
nodes. The combined parser cap is 65,536 records and 64,000,000 proof nodes.
The final encoded pack must be at most 16 MiB per profile and the five-pack
aggregate at most 80 MiB. Performance A/B is deliberately outside the current
data-completion task; these limits do not imply a speed improvement.
