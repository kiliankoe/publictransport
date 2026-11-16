//! The `publictransport` library as a shared library that the bindings for
//! Python, Swift and Kotlin load.
//!
//! The exported types and functions are those of `publictransport` with its
//! `uniffi` feature. This crate only links them into one library.

publictransport::uniffi_reexport_scaffolding!();
