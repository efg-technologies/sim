//! SAFE-9
//! Exposes core/ and io/ to main.rs and tests/. Nothing in the crate reads settings or
//! commands from file contents, and nothing touches the network.
#![forbid(unsafe_code)]

pub mod core;
pub mod io;
