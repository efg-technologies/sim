//! GUI-12, GUI-13
//! Motion: the cursor glides from cell to cell and takes the shape of the mode, the text scrolls
//! smoothly, the fog follows the cursor. sim draws whole screens; this keeps the last one and the one
//! before it, and works out what moves between them. Pure: the time is given (`tick(dt)`), so the same
//! sequence of frames gives the same pixels. With reduced motion (`SIM_GUI_REDUCED_MOTION=1`) every
//! change is at once: no glide, no scroll, no fog in between.
use crate::api::Shape;
use crate::cells::Grid;
use crate::frame::Snapshot;
use crate::metrics::Metrics;
use crate::overlay::shape_rect;
use crate::scene::{CursorSpec, Ghosts, Rect, Scene, View, build};
use std::ops::Range;

/// The time constant of the cursor's glide, in seconds: it covers 63% of the way in this long. Short
/// enough that the cursor is never behind the keys, long enough that a jump is seen as a move.
const CURSOR_TAU: f32 = 0.035;
/// The fog follows the cursor a little slower than the cursor itself.
const FOG_TAU: f32 = 0.070;

/// Whether the environment asks for no motion: `SIM_GUI_REDUCED_MOTION` set to anything but `0` or
/// empty. (The system's own setting is not read yet: NOTES.md.)
pub fn reduced_from_env() -> bool {
    reduced(std::env::var("SIM_GUI_REDUCED_MOTION").ok().as_deref())
}

fn reduced(word: Option<&str>) -> bool {
    word.is_some_and(|v| !v.is_empty() && v != "0")
}

#[derive(Clone, Debug)]
struct Scroll {
    /// Rows the text moved: positive is down the file (the text goes up).
    k: i32,
    /// How long it takes, and how far it has gone, in seconds.
    dur: f32,
    at: f32,
    old: Grid,
}

/// The rows of the old screen that are leaving, how far they are from where they were, and the screen.
type Gone<'a> = (Range<usize>, f32, &'a Grid);

pub struct Animator {
    pub reduced: bool,
    target: Option<Snapshot>,
    cursor: Option<Rect>,
    /// How much like a block the cursor is, 0 to 1 (a bar and an underline are 0).
    block: f32,
    /// The fog of each cell as it is now, and which grid it is for.
    fog: Vec<Vec<f32>>,
    scroll: Option<Scroll>,
}

/// A hash for each row of the text's rows: the text after the gutter (the relative line numbers
/// change with the cursor, the text does not). 0 for a row with no text.
fn body_sig(s: &Snapshot) -> Vec<u64> {
    let Some((l, sign, gutter)) = s.text() else { return vec![] };
    let from = l.rect.x + sign + gutter;
    let to = l.rect.x + l.rect.w;
    (usize::from(l.rect.y)..usize::from(l.rect.y + l.rect.h))
        .map(|r| {
            let Some(row) = s.grid.rows.get(r) else { return 0 };
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            let mut any = false;
            for c in row.cells.iter().filter(|c| c.col >= from && c.col < to) {
                if c.ch != ' ' {
                    any = true;
                }
                for b in c.text().bytes().chain(std::iter::once(0x1f)) {
                    h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
                }
            }
            if any { h | 1 } else { 0 }
        })
        .collect()
}

/// How many rows the text moved from `old` to `new`, if it did: `new[i] == old[i + k]`.
fn shift(old: &[u64], new: &[u64]) -> Option<i32> {
    let n = new.len().min(old.len());
    let text = new.iter().filter(|&&h| h != 0).count();
    if n < 3 || text < 2 {
        return None;
    }
    let matches = |k: i32| {
        (0..n as i32)
            .filter(|&i| {
                let j = i + k;
                j >= 0
                    && (j as usize) < n
                    && new[i as usize] != 0
                    && old[j as usize] == new[i as usize]
            })
            .count()
    };
    let rest = matches(0);
    let (best, count) = (-(n as i32 - 1)..n as i32)
        .filter(|&k| k != 0)
        .map(|k| (k, matches(k)))
        .max_by_key(|&(k, c)| (c, std::cmp::Reverse(k.abs())))?;
    (count >= 2 && count > rest + 1 && count * 5 >= text * 2).then_some(best)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl Animator {
    pub fn new(reduced: bool) -> Animator {
        Animator { reduced, target: None, cursor: None, block: 1.0, fog: vec![], scroll: None }
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.target.as_ref()
    }

    /// A new screen of sim's. What moved between it and the last one starts to move.
    pub fn set_target(&mut self, snap: Snapshot, m: &Metrics) {
        let old = self.target.take();
        let same_layout = old.as_ref().is_some_and(|o| {
            o.text().map(|t| t.0.rect) == snap.text().map(|t| t.0.rect)
                && o.grid.rows.len() == snap.grid.rows.len()
                && !o.has_float()
                && !snap.has_float()
        });
        // scrolling: the text of the rows moved
        self.scroll = None;
        if let (Some(o), true, false) = (&old, same_layout, self.reduced)
            && let Some(k) = shift(&body_sig(o), &body_sig(&snap))
        {
            let dur = (0.080 + 0.006 * k.unsigned_abs() as f32).min(0.160);
            self.scroll = Some(Scroll { k, dur, at: 0.0, old: o.grid.clone() });
        }
        // the fog: each cell starts from what the same char had in the same place (moved with the
        // text when it scrolled), and goes to its own
        let moved = self.scroll.as_ref().map_or(0, |s| s.k);
        let body = snap.body_rows().unwrap_or(0..0);
        let fog: Vec<Vec<f32>> = snap
            .grid
            .rows
            .iter()
            .enumerate()
            .map(|(r, row)| {
                let from = old.as_ref().filter(|_| !self.reduced).and_then(|o| {
                    let rr = usize::try_from(r as i32 + if body.contains(&r) { moved } else { 0 })
                        .ok()?;
                    Some((o.grid.rows.get(rr)?, self.fog.get(rr)?))
                });
                row.cells
                    .iter()
                    .map(|c| {
                        let shown = from.and_then(|(orow, f)| {
                            let j = orow
                                .cells
                                .iter()
                                .position(|oc| oc.col == c.col && oc.ch == c.ch)?;
                            f.get(j).copied()
                        });
                        shown.unwrap_or(f32::from(c.r.fade))
                    })
                    .collect()
            })
            .collect();
        self.fog = fog;
        // the cursor glides from where it is, unless it has none yet or motion is off
        if self.cursor.is_none() || self.reduced {
            self.cursor = Self::cursor_rect(&snap, m);
            self.block = block_of(snap.shape);
        }
        self.target = Some(snap);
        if self.reduced {
            self.settle();
        }
    }

    /// Where the cursor wants to be, in pixels, for its shape; none if it is off the grid.
    fn cursor_rect(s: &Snapshot, m: &Metrics) -> Option<Rect> {
        let (col, row) = s.cursor;
        if usize::from(row) >= s.grid.rows.len() {
            return None;
        }
        let cols = s.grid.rows[usize::from(row)]
            .cells
            .iter()
            .find(|c| c.col == col)
            .map_or(1, |c| c.width.max(1));
        let (x, y) = m.origin(f32::from(col), f32::from(row));
        Some(shape_rect(s.shape, Rect::new(x, y, f32::from(cols) * m.cell_w, m.cell_h), m))
    }

    /// Everything is where it is going: the end of every motion, at once.
    pub fn settle(&mut self) {
        self.scroll = None;
        self.cursor = None; // the goal, from the scene
        if let Some(t) = &self.target {
            self.block = block_of(t.shape);
            self.fog = t
                .grid
                .rows
                .iter()
                .map(|r| r.cells.iter().map(|c| f32::from(c.r.fade)).collect())
                .collect();
        }
    }

    /// Moves time on by `dt` seconds. Whether anything is still moving.
    pub fn tick(&mut self, dt: f32, m: &Metrics) -> bool {
        let Some(t) = &self.target else { return false };
        let mut moving = false;
        if let Some(s) = &mut self.scroll {
            s.at += dt;
            if s.at >= s.dur {
                self.scroll = None;
            } else {
                moving = true;
            }
        }
        // the cursor
        if let Some(goal) = Self::cursor_rect(t, m) {
            let want_block = block_of(t.shape);
            let a = 1.0 - (-dt / CURSOR_TAU).exp();
            let cur = self.cursor.get_or_insert(goal);
            let step = |from: f32, to: f32| {
                let v = from + (to - from) * a;
                if (to - v).abs() < 0.25 { to } else { v }
            };
            let next = Rect::new(
                step(cur.x, goal.x),
                step(cur.y, goal.y),
                step(cur.w, goal.w),
                step(cur.h, goal.h),
            );
            moving |= next != goal;
            *cur = next;
            let nb = step(self.block * 100.0, want_block * 100.0) / 100.0;
            moving |= (nb - want_block).abs() > 0.004;
            self.block = if (nb - want_block).abs() <= 0.004 { want_block } else { nb };
        }
        // the fog
        let af = 1.0 - (-dt / FOG_TAU).exp();
        for (r, row) in t.grid.rows.iter().enumerate() {
            let Some(shown) = self.fog.get_mut(r) else { continue };
            for (k, c) in row.cells.iter().enumerate() {
                let goal = f32::from(c.r.fade);
                if let Some(v) = shown.get_mut(k) {
                    let next = *v + (goal - *v) * af;
                    *v = if (goal - next).abs() < 0.4 { goal } else { next };
                    moving |= *v != goal;
                }
            }
        }
        moving
    }

    /// Whether something is in motion now.
    pub fn moving(&self, m: &Metrics) -> bool {
        let Some(t) = &self.target else { return false };
        if self.scroll.is_some() {
            return true;
        }
        let cursor = Self::cursor_rect(t, m).is_some_and(|g| self.cursor.is_some_and(|c| c != g));
        let want = block_of(t.shape);
        let fog = t.grid.rows.iter().zip(&self.fog).any(|(row, shown)| {
            row.cells.iter().zip(shown).any(|(c, v)| *v != f32::from(c.r.fade))
        });
        cursor || (self.block - want).abs() > 0.004 || fog
    }

    /// The offset of the text and the old rows still showing, at this moment of a scroll.
    fn motion(&self, m: &Metrics) -> (f32, Option<Gone<'_>>) {
        let (Some(s), Some(t)) = (&self.scroll, &self.target) else { return (0.0, None) };
        let p = ease_out(s.at / s.dur);
        let ch = m.cell_h;
        let k = s.k;
        let body = t.body_rows().unwrap_or(0..0);
        let n = body.len() as i32;
        let gone = if k > 0 { 0..k.min(n) } else { (n + k).max(0)..n };
        let rows = body.start + gone.start as usize..body.start + gone.end as usize;
        ((1.0 - p) * k as f32 * ch, Some((rows, -p * k as f32 * ch, &s.old)))
    }

    /// The scene of this moment (the cursor's cell and shape are in it; `cursor_now` says where it is drawn).
    pub fn scene(&self, m: Metrics, size: (u32, u32)) -> Option<Scene> {
        let t = self.target.as_ref()?;
        let (dy, ghost) = self.motion(&m);
        let fog: Vec<Vec<u8>> = self
            .fog
            .iter()
            .map(|r| r.iter().map(|v| v.round().clamp(0.0, 100.0) as u8).collect())
            .collect();
        let view = View {
            grid: &t.grid,
            tones: &t.tones,
            layers: &t.layers,
            cursor: t.cursor,
            shape: t.shape,
            metrics: m,
            size,
            scroll_dy: dy,
            ghosts: ghost.map(|(rows, dy, grid)| Ghosts { grid, rows, dy }),
            fog: Some(&fog),
        };
        Some(build(&view))
    }

    /// Where the cursor is drawn now (with the scroll's offset if it sits on the text) and how
    /// block-like it is.
    pub fn cursor_now(&self, spec: &CursorSpec, m: &Metrics) -> (Rect, f32) {
        let mut r = self.cursor.unwrap_or_else(|| shape_rect(spec.shape, spec.rect, m));
        if spec.in_body {
            r.y += self.motion(m).0;
        }
        (r, self.block)
    }
}

fn block_of(shape: Shape) -> f32 {
    if matches!(shape, Shape::Block) { 1.0 } else { 0.0 }
}

impl Animator {
    /// The rows a running scroll moves by, if one runs (for the tests and the overlay's glue).
    pub fn scrolling(&self) -> Option<i32> {
        self.scroll.as_ref().map(|s| s.k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Resolver;
    use crate::headless::Headless;
    use crate::metrics::TEST;
    use crate::theme::DARK;

    fn long(rows: u16) -> Headless {
        // src/main.rs is 20 lines: on a small screen it scrolls
        Headless::demo("anim", &["src/main.rs"], (100, rows), &DARK)
    }

    fn snap(h: &Headless) -> Snapshot {
        Snapshot::take(h.screen(), &mut Resolver::new())
    }

    #[test]
    fn gui_13_moving_the_view_down_is_a_scroll_by_as_many_rows_as_the_view_moved() {
        let mut h = long(14);
        let mut a = Animator::new(false);
        a.set_target(snap(&h), &TEST);
        assert_eq!(a.scrolling(), None);
        // the cursor goes past the last row on screen: the view follows it
        h.keys("jjjjjjjjjj");
        a.set_target(snap(&h), &TEST);
        a.settle();
        let top = h.session.sim.bufs[0].app.top;
        h.keys("jj");
        let moved = h.session.sim.bufs[0].app.top - top;
        assert!(moved >= 1, "the view moved");
        a.set_target(snap(&h), &TEST);
        assert_eq!(a.scrolling(), Some(moved as i32));
    }

    #[test]
    fn gui_13_moving_the_cursor_inside_the_view_is_not_a_scroll() {
        let mut h = long(24);
        let mut a = Animator::new(false);
        a.set_target(snap(&h), &TEST);
        h.keys("jjj");
        a.set_target(snap(&h), &TEST);
        assert_eq!(a.scrolling(), None, "the relative numbers changed, the text did not move");
    }

    #[test]
    fn gui_13_a_scroll_lasts_a_moment_and_then_the_text_is_at_rest() {
        let mut h = long(14);
        let mut a = Animator::new(false);
        h.keys("jjjjjjjjjjjj");
        a.set_target(snap(&h), &TEST);
        h.keys("jjj");
        a.set_target(snap(&h), &TEST);
        assert!(a.scrolling().is_some());
        assert!(a.tick(0.03, &TEST), "still moving");
        let mid = a.scene(TEST, (1000, 400)).unwrap();
        assert!(
            mid.layers[0].surfaces.iter().any(|s| s.clip.is_some())
                || mid.layers[0].glyphs.iter().any(|g| g.clip.is_some()),
            "the moving text is clipped to its box"
        );
        a.tick(0.5, &TEST);
        assert_eq!(a.scrolling(), None);
        assert!(!a.moving(&TEST));
    }

    #[test]
    fn gui_13_the_cursor_glides_and_arrives() {
        let mut h = long(24);
        let mut a = Animator::new(false);
        a.set_target(snap(&h), &TEST);
        let start = a.cursor_now(&a.scene(TEST, (1000, 600)).unwrap().cursor.unwrap(), &TEST).0;
        h.keys("jjjjj");
        a.set_target(snap(&h), &TEST);
        let spec = a.scene(TEST, (1000, 600)).unwrap().cursor.unwrap();
        let (r0, _) = a.cursor_now(&spec, &TEST);
        assert_eq!(r0.y, start.y, "it starts where it was");
        a.tick(0.02, &TEST);
        let (r1, _) = a.cursor_now(&spec, &TEST);
        assert!(
            r1.y > start.y && r1.y < spec.rect.y,
            "on the way: {} < {} < {}",
            start.y,
            r1.y,
            spec.rect.y
        );
        a.tick(1.0, &TEST);
        let (r2, _) = a.cursor_now(&spec, &TEST);
        assert_eq!(r2.y, spec.rect.y);
        assert!(!a.moving(&TEST));
    }

    #[test]
    fn gui_13_the_cursor_takes_the_shape_of_the_mode() {
        let mut h = long(24);
        let mut a = Animator::new(false);
        a.set_target(snap(&h), &TEST);
        let spec = a.scene(TEST, (1000, 600)).unwrap().cursor.unwrap();
        assert_eq!(spec.shape as u8, Shape::Block as u8);
        h.keys("i");
        a.set_target(snap(&h), &TEST);
        let spec = a.scene(TEST, (1000, 600)).unwrap().cursor.unwrap();
        assert_eq!(spec.shape as u8, Shape::Bar as u8, "Insert mode has a bar");
        a.tick(1.0, &TEST);
        let (r, block) = a.cursor_now(&spec, &TEST);
        assert!(r.w < TEST.cell_w / 2.0, "a bar is narrow: {}", r.w);
        assert_eq!(block, 0.0);
    }

    #[test]
    fn gui_13_reduced_motion_is_any_word_but_empty_or_zero() {
        assert!(!reduced(None) && !reduced(Some("")) && !reduced(Some("0")));
        assert!(reduced(Some("1")) && reduced(Some("yes")) && reduced(Some("true")));
    }

    #[test]
    fn gui_13_with_reduced_motion_everything_is_at_once() {
        let mut h = long(14);
        let mut a = Animator::new(true);
        a.set_target(snap(&h), &TEST);
        h.keys("jjjjjjjjjjjjjjj");
        a.set_target(snap(&h), &TEST);
        assert_eq!(a.scrolling(), None);
        assert!(!a.moving(&TEST));
        let spec = a.scene(TEST, (1000, 400)).unwrap().cursor.unwrap();
        assert_eq!(a.cursor_now(&spec, &TEST).0, shape_rect(spec.shape, spec.rect, &TEST));
    }

    #[test]
    fn gui_12_the_fog_follows_the_cursor_and_never_jumps() {
        let mut h = long(24);
        let mut a = Animator::new(false);
        let old = snap(&h);
        a.set_target(old.clone(), &TEST);
        h.keys("jjjjjjjjjj");
        let new = snap(&h);
        // a char far from the new cursor row that had no fog before and has some now
        let pick = new.body_rows().unwrap().find_map(|r| {
            let k =
                old.grid.rows[r].cells.iter().zip(&new.grid.rows[r].cells).position(|(o, n)| {
                    o.ch == n.ch && o.ch.is_alphanumeric() && !n.r.soft && o.r.fade < n.r.fade
                })?;
            Some((r, k))
        });
        let (r, k) = pick.expect("a char whose fog grows");
        let goal = f32::from(new.grid.rows[r].cells[k].r.fade);
        a.set_target(new, &TEST);
        let v0 = a.fog[r][k];
        assert!(v0 < goal, "it starts from the old fog: {v0} < {goal}");
        a.tick(0.05, &TEST);
        let v1 = a.fog[r][k];
        assert!(v1 > v0 && v1 < goal, "{v0} < {v1} < {goal}");
        a.tick(1.0, &TEST);
        assert_eq!(a.fog[r][k], goal);
    }
}
