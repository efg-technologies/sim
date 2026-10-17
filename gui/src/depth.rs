//! GUI-9
//! Depth: what each of sim's layers stands on. The floor is the window; the boxes of the tree and
//! of the text are raised from it by one level, and every text lies on the page color sim's palette
//! is made for (`Palette::bg`) -- so each contrast guarantee sim proves (UI-12, LOOK-9, the fog's
//! floors) holds on the pixels as it does in a terminal. What is raised is lit by a soft shadow
//! under it and a thin rim, and the higher it is the larger and softer the shadow is.
//!
//! | level | what | shadow (dp: down / blur) | radius |
//! |-------|------|--------------------------|--------|
//! | 0 | the floor, the window | none | |
//! | 1 | the tree's box and the text's box | 4 / 16 | 10 |
//! | 2 | a band (heading, fence), a chip (tab, badge), the alert's pill | 1 / 4 | 6 |
//! | 3 | a window (the picker, which-key), with the content behind a modal one dimmed | 16 / 40 | 14 |
use crate::api::*;
use crate::metrics::Metrics;

/// Straight (not premultiplied) RGBA, 0 to 1.
pub type Color = [f32; 4];

pub fn rgb(c: Rgb) -> Color {
    [f32::from(c.0) / 255.0, f32::from(c.1) / 255.0, f32::from(c.2) / 255.0, 1.0]
}

pub fn rgba(c: Rgb, a: f32) -> Color {
    let [r, g, b, _] = rgb(c);
    [r, g, b, a]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// How far below the surface the shadow's center lies, and how soft its edge is, in pixels.
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
}

/// The levels of depth above the floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Box,
    Chip,
    Float,
}

/// The numbers of depth for a palette and a scale.
#[derive(Clone, Copy, Debug)]
pub struct Depth {
    pub light: bool,
    pub m: Metrics,
    /// The window behind the boxes: darker than the page, so what lies on the page is raised.
    pub floor: Rgb,
    pub page: Rgb,
}

impl Depth {
    pub fn new(pal: &Palette, m: Metrics) -> Depth {
        let light = to_lch(pal.bg)[0] > 0.6;
        let floor = mix(pal.bg, (0, 0, 0), if light { 0.08 } else { 0.30 });
        Depth { light, m, floor, page: pal.bg }
    }

    /// The shadow under a surface of this level.
    pub fn shadow(&self, level: Level) -> Shadow {
        let (dy, blur, dark, pale) = match level {
            Level::Box => (4.0, 16.0, 0.60, 0.26),
            Level::Chip => (1.0, 4.0, 0.40, 0.18),
            Level::Float => (16.0, 40.0, 0.70, 0.34),
        };
        let a = if self.light { pale } else { dark };
        Shadow { dy: self.m.dpf(dy), blur: self.m.dpf(blur), color: [0.0, 0.0, 0.0, a] }
    }

    /// The corner radius of a level, in pixels.
    pub fn radius(&self, level: Level) -> f32 {
        self.m.dpf(match level {
            Level::Box => 10.0,
            Level::Chip => 6.0,
            Level::Float => 14.0,
        })
    }

    /// What lies over everything under a floating surface: the content dims (and a light page
    /// greys a little), so that what floats is the brightest thing on the screen.
    pub fn scrim(&self) -> Color {
        [0.0, 0.0, 0.0, if self.light { 0.30 } else { 0.50 }]
    }

    /// The rim of a raised surface: a thin light line on a dark page, a thin dark one on a light page.
    pub fn rim(&self, focused: bool) -> Color {
        match (self.light, focused) {
            (false, false) => [1.0, 1.0, 1.0, 0.10],
            (false, true) => [1.0, 1.0, 1.0, 0.22],
            (true, false) => [0.0, 0.0, 0.0, 0.10],
            (true, true) => [0.0, 0.0, 0.0, 0.24],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::TEST;

    fn depth(bg: Rgb) -> Depth {
        Depth::new(&palette(bg), TEST)
    }

    const DARK: Rgb = (0x28, 0x2c, 0x34);
    const LIGHT: Rgb = (0xef, 0xf1, 0xf5);

    #[test]
    fn gui_9_the_higher_a_level_the_larger_and_softer_its_shadow_and_the_rounder_its_corners() {
        for bg in [DARK, LIGHT] {
            let d = depth(bg);
            let (chip, boxed, float) =
                (d.shadow(Level::Chip), d.shadow(Level::Box), d.shadow(Level::Float));
            assert!(chip.dy < boxed.dy && boxed.dy < float.dy, "{bg:?}");
            assert!(chip.blur < boxed.blur && boxed.blur < float.blur, "{bg:?}");
            assert!(
                d.radius(Level::Chip) < d.radius(Level::Box)
                    && d.radius(Level::Box) < d.radius(Level::Float)
            );
            // the numbers of the table, in dp (the test metrics are at scale 1)
            assert_eq!((boxed.dy, boxed.blur), (4.0, 16.0));
            assert_eq!((chip.dy, chip.blur), (1.0, 4.0));
            assert_eq!((float.dy, float.blur), (16.0, 40.0));
        }
    }

    #[test]
    fn gui_9_the_floor_is_darker_than_the_page_so_that_the_boxes_are_raised_and_the_page_is_exact()
    {
        for bg in [DARK, LIGHT] {
            let d = depth(bg);
            assert!(to_lch(d.floor)[0] < to_lch(bg)[0], "{bg:?}");
            assert_eq!(d.page, bg, "text lies on the page color sim's proofs are made for");
        }
        assert!(!depth(DARK).light && depth(LIGHT).light);
    }

    #[test]
    fn gui_9_a_modal_window_dims_what_is_behind_it_and_the_rim_of_the_focus_is_brighter() {
        for bg in [DARK, LIGHT] {
            let d = depth(bg);
            assert!(d.scrim()[3] >= 0.3 && d.scrim()[3] <= 0.6);
            assert!(
                d.rim(true)[3] > d.rim(false)[3],
                "the box with the focus has the brighter rim"
            );
        }
        // a dark page is lit by a light rim, a light page edged by a dark one
        assert!(depth(DARK).rim(false)[0] > 0.9 && depth(LIGHT).rim(false)[0] < 0.1);
        // and the scrim of a light page is lighter than a dark page's
        assert!(depth(LIGHT).scrim()[3] < depth(DARK).scrim()[3]);
    }
}
