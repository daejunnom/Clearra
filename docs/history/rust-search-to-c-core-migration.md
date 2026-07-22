# Rust Search To C Core Migration History

This document is a non-normative historical record. Current architecture and
release validation do not read it as a product contract.

Clearra previously used a Rust DFS search crate. That crate and its product
facades were removed after packing, BuildUp, reachability, candidate generation,
and memory ownership moved to the C core. The typed Rust shell remained the
owner of requests, validation, probability, objectives, replay, and output.

| Removed Rust search family | Current owner |
| --- | --- |
| Generic PC solver and search orchestration | `core-c/src/packing`, `core-c/src/buildup`, `clearra-core-executor` |
| Checkpoint execution | `clearra-pc-graph` metadata plus `clearra-core-executor` execution |
| Opening and scenario solver facades | `clearra-app`, `clearra-problem`, `clearra-core-executor` |
| Search result and trace models | `clearra-core-executor` result contracts and `clearra-replay` |
| Board mutation and line-clear search | `core-c/src/board` |
| Placement and reachability search | `core-c/src/candidate`, `core-c/src/reachability` |
| DFS memoization | C packing/BuildUp memoization plus Rust coverage/objective caches |

The migration ended with physical removal of `crates/clearra-search`. No current
product crate or architecture validator reads that deleted path.
