//! GUI-8
//! The one file that names sim's types (the editor `sim` and its core `sim_core`). Everything else
//! in this crate reaches sim through here, so a renamed or moved item of sim is one edit, and a test
//! (`tests/api_seam.rs`) fails when another file names them.
pub use crossterm::event::{
    Event as CtEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
pub use sim::color::{Chg, Input as SimInput, Syntax, Tones, mix, to_lch};
pub use sim::io as sio;
pub use sim::layout::Skin;
pub use sim::paint::{Attrs, Layer, LayerKind, Look, Resolved, Resolver, Role, Screen};
pub use sim::pick::Rect as CellRect;
pub use sim::rank::History;
pub use sim::shell::{Config, Out, Shell};
pub use sim::sim::{Env, Id, Sim, SimEvent, start};
pub use sim_core::core::app::{Event as TermEvent, Key as SiKey};
pub use sim_core::core::buffer::hash;
pub use sim_core::core::color::{Palette, Rgb, apca, palette};
pub use sim_core::core::vi::Mode;
pub use sim_core::core::view::{Frame, Shape, Style, width as cell_width};
pub use sim_core::io::{file, filter, journal};
