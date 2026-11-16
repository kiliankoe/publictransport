//! Client for public transport APIs such as EFA and MOTIS.
//!
//! This crate does no I/O. It builds HTTP requests and parses HTTP responses,
//! so each platform can send them with its own HTTP stack.
