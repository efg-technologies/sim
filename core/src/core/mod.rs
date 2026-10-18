//! The functional core: pure, and never touches std::fs, std::process, clocks or the terminal.

pub mod app;
pub mod buffer;
pub mod cli;
pub mod color;
pub mod disk;
pub mod edit;
pub mod ex;
pub mod merge;
pub mod reg;
pub mod vi;
pub mod view;
