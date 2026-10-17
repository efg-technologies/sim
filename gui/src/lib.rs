//! GUI-8, GUI-17
//! sim-gui, a window for sim. A renderer and an input adapter; the editor is sim's.
#![forbid(unsafe_code)]
#![allow(clippy::chunks_exact_to_as_chunks)] // (as_chunks is newer than the rust-version this crate keeps)
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod anim;
pub mod api;
pub mod app;
pub mod blocks;
pub mod cells;
pub mod demo;
pub mod depth;
pub mod font;
pub mod frame;
pub mod gpu;
pub mod headless;
pub mod input;
pub mod metrics;
pub mod offscreen;
pub mod overlay;
pub mod safe_io;
pub mod scene;
pub mod session;
pub mod studio;
pub mod theme;
