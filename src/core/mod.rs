//! The functional core: pure, and never touches std::fs, std::process, clocks or the terminal.

pub mod app;
pub mod buffer;
pub mod color;
pub mod edit;
pub mod proposal;
pub mod vi;
pub mod view;
