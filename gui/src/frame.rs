//! GUI-1, GUI-3
//! One settled frame of sim, as the renderer needs it: the resolved cells, the tones, the layers sim
//! says it has and the cursor. Taken from the `Screen` sim just drew.
use crate::api::*;
use crate::cells::{Grid, grid};
use crate::metrics::Metrics;
use crate::scene::{Ghosts, View};
use crate::theme::DARK;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub grid: Grid,
    pub tones: Tones,
    pub layers: Vec<Layer>,
    pub cursor: (u16, u16),
    pub shape: Shape,
}

impl Snapshot {
    pub fn take(screen: &Screen, resolver: &mut Resolver) -> Snapshot {
        let fallback = palette(DARK.bg);
        let pal = screen.frame.pal.unwrap_or(fallback);
        Snapshot {
            grid: grid(screen, resolver, &fallback),
            tones: Tones::with(&pal, &screen.syn),
            layers: screen.layers.clone(),
            cursor: screen.frame.cursor,
            shape: screen.frame.shape,
        }
    }

    /// The text layer, if the interface has one: where the sign column, the numbers and the lines are.
    pub fn text(&self) -> Option<(&Layer, u16, u16)> {
        self.layers.iter().find_map(|l| match l.kind {
            LayerKind::Text { sign, gutter } => Some((l, sign, gutter)),
            _ => None,
        })
    }

    /// The rows the text is on, if there is a text layer.
    pub fn body_rows(&self) -> Option<std::ops::Range<usize>> {
        self.text()
            .map(|(l, ..)| usize::from(l.rect.y)..usize::from(l.rect.y) + usize::from(l.rect.h))
    }

    /// Whether a window is over the rest.
    pub fn has_float(&self) -> bool {
        self.layers.iter().any(|l| matches!(l.kind, LayerKind::Float { .. }))
    }

    /// The view of this frame when nothing moves.
    pub fn view(&self, metrics: Metrics, size: (u32, u32)) -> View<'_> {
        View {
            grid: &self.grid,
            tones: &self.tones,
            layers: &self.layers,
            cursor: self.cursor,
            shape: self.shape,
            metrics,
            size,
            scroll_dy: 0.0,
            ghosts: None::<Ghosts>,
            fog: None,
        }
    }
}
