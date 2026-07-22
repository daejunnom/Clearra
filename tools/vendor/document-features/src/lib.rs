//! Documentation-only replacement used to keep proc-macro DLLs out of the
//! Windows product build surface. It has no runtime or feature-selection role.

#[macro_export]
macro_rules! document_features {
    () => {
        ""
    };
}
