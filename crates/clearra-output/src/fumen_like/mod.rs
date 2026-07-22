//! Compatibility bridge for fumen-like output.
//!
//! The codec itself is owned by `clearra-fumen`; `clearra-output` only keeps this
//! re-export so existing output callers can request the fumen-like render format.

pub use clearra_fumen::codec::{
    fumen_like_reader, fumen_like_trace, fumen_like_writer, FumenLikeReadError, FumenLikeReader,
    FumenLikeTrace, FumenLikeTraceError, FumenLikeWriteError, FumenLikeWriter,
};
