//! GUI-10
//! The input mapping end to end: window events (as winit makes them) through `Input` and the path of
//! the binary into a running sim, and what sim does with them. Every key and mode works as in a
//! terminal because sim is the one that decides what a key is.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers; the tests themselves may unwrap
use sim_gui::api::*;
use sim_gui::headless::Headless;
use sim_gui::input::{Action, Input, KeyIn, to_sim};
use sim_gui::metrics::TEST;
use sim_gui::session::{Clipboard, MemClipboard};
use sim_gui::theme::DARK;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Ime, MouseButton as WButton, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

/// A window with sim in it: the input adapter and a headless sim, and a clipboard.
struct Window {
    h: Headless,
    input: Input,
    clip: MemClipboard,
}

impl Window {
    fn open(file: &str) -> Window {
        let h = Headless::demo("input", &[file], (80, 24), &DARK);
        Window { h, input: Input::new(TEST, (80, 24)), clip: MemClipboard::default() }
    }

    fn act(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Send(ev) => {
                    for e in to_sim(ev) {
                        self.h.session.send(e);
                    }
                }
                Action::Paste => {
                    if let Some(t) = self.clip.get() {
                        for e in to_sim(CtEvent::Paste(t)) {
                            self.h.session.send(e);
                        }
                    }
                }
            }
        }
    }

    fn press(&mut self, key: Key, text: Option<&str>) {
        let a = self.input.key(KeyIn { key: &key, bare: None, text, state: ElementState::Pressed });
        self.act(a);
    }

    fn typed(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '\n' => self.named(NamedKey::Enter),
                ' ' => self.press(Key::Named(NamedKey::Space), Some(" ")),
                c => {
                    let t = c.to_string();
                    self.press(Key::Character(SmolStr::new(&t)), Some(&t));
                }
            }
        }
    }

    fn named(&mut self, n: NamedKey) {
        self.press(Key::Named(n), None);
    }

    fn chord(&mut self, mods: ModifiersState, key: &str) {
        self.input.mods = mods;
        self.press(Key::Character(SmolStr::new(key)), None);
        self.input.mods = ModifiersState::empty();
    }

    fn text(&self) -> String {
        self.h.session.sim.bufs[self.h.session.sim.cur].app.buf.text.to_string()
    }

    fn mode(&self) -> Mode {
        self.h.session.sim.bufs[self.h.session.sim.cur].app.vi.mode
    }

    fn at(col: u16, row: u16) -> (f64, f64) {
        (
            f64::from(TEST.pad_x + (f32::from(col) + 0.5) * TEST.cell_w),
            f64::from(TEST.pad_y + (f32::from(row) + 0.5) * TEST.cell_h),
        )
    }

    fn click(&mut self, col: u16, row: u16) {
        let (x, y) = Window::at(col, row);
        let a = self.input.pointer(x, y);
        self.act(a);
        let a = self.input.button(ElementState::Pressed, WButton::Left);
        self.act(a);
        let a = self.input.button(ElementState::Released, WButton::Left);
        self.act(a);
    }
}

#[test]
fn gui_10_typed_keys_are_vi_in_normal_mode_and_text_in_insert_mode() {
    let mut w = Window::open("src/notes.rs");
    assert_eq!(w.mode(), Mode::Normal);
    w.typed("i");
    assert_eq!(w.mode(), Mode::Insert);
    w.typed("XY");
    w.named(NamedKey::Escape);
    assert_eq!(w.mode(), Mode::Normal);
    assert!(w.text().starts_with("XY//! The notes parser."), "{:?}", &w.text()[..30]);
    w.typed("u");
    assert!(w.text().starts_with("//! The notes parser."), "u undoes");
}

#[test]
fn gui_10_japanese_text_from_the_input_method_is_typed_in_insert_mode() {
    let mut w = Window::open("src/notes.rs");
    w.typed("i");
    // the method composes (nothing is sent), then commits
    let a = w.input.ime(&Ime::Preedit("にほんご".into(), Some((0, 12))));
    assert!(a.is_empty());
    assert!(w.input.preedit.is_some());
    let a = w.input.ime(&Ime::Commit("日本語".into()));
    w.act(a);
    assert!(w.input.preedit.is_none());
    w.named(NamedKey::Escape);
    assert!(
        w.text().starts_with("日本語//! The notes parser."),
        "{:?}",
        w.text().chars().take(12).collect::<String>()
    );
}

#[test]
fn gui_10_control_p_opens_the_picker_and_escape_closes_it() {
    let mut w = Window::open("README.md");
    w.chord(ModifiersState::CONTROL, "p");
    assert!(w.h.session.sim.picker_open());
    w.typed("main");
    w.named(NamedKey::Escape);
    assert!(!w.h.session.sim.picker_open());
}

#[test]
fn gui_10_control_n_toggles_the_tree_and_a_click_on_a_row_acts_on_it() {
    let mut w = Window::open("README.md");
    assert!(!w.h.session.sim.panel.open);
    w.chord(ModifiersState::CONTROL, "n");
    assert!(w.h.session.sim.panel.open);
    // the tree's rows start under the box's top border and the header: row 2 is the first entry, so
    // row 6 is the fifth: hidden.txt (directories first, then names)
    w.click(3, 6);
    let open: Vec<_> = w.h.session.sim.bufs.iter().filter_map(|b| b.app.path.clone()).collect();
    assert!(
        open.iter().any(|p| p == "hidden.txt"),
        "a click on a file opens it, as Enter does: {open:?}"
    );
}

#[test]
fn gui_10_a_click_in_the_text_puts_the_cursor_there_and_a_drag_selects() {
    let mut w = Window::open("src/notes.rs");
    let g = w.h.session.sim.geo();
    // the first text column of the fourth line shown
    let (col, row) = (g.tx() + 8, g.y0 + 3);
    w.click(col, row);
    let b = &w.h.session.sim.bufs[0].app.buf;
    assert_eq!(b.line_of(b.cursor), 3, "the line under the pointer");
    // press, move across cells, release: a selection
    let (x, y) = Window::at(col, row);
    let a = w.input.pointer(x, y);
    w.act(a);
    let a = w.input.button(ElementState::Pressed, WButton::Left);
    w.act(a);
    let (x2, y2) = Window::at(col + 6, row);
    let a = w.input.pointer(x2, y2);
    w.act(a);
    assert!(w.mode().visual(), "dragging selects, as in a terminal");
    let a = w.input.button(ElementState::Released, WButton::Left);
    w.act(a);
}

#[test]
fn gui_10_the_wheel_scrolls_the_text_three_lines_a_notch() {
    let mut w = Window::open("src/main.rs");
    // a short screen, so the file is longer than it
    w.h.resize(80, 12);
    let g = w.h.session.sim.geo();
    let (x, y) = Window::at(g.tx() + 4, g.y0 + 2);
    let a = w.input.pointer(x, y);
    w.act(a);
    let top = w.h.session.sim.bufs[0].app.top;
    let a = w.input.wheel(MouseScrollDelta::LineDelta(0.0, -1.0));
    w.act(a);
    assert_eq!(w.h.session.sim.bufs[0].app.top, top + 3);
    let a = w.input.wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    w.act(a);
    assert_eq!(w.h.session.sim.bufs[0].app.top, top);
}

#[test]
fn gui_10_a_paste_is_one_paste_event_and_goes_in_as_text() {
    let mut w = Window::open("src/notes.rs");
    w.clip.set("PASTED\nTEXT\n");
    w.typed("i");
    w.chord(ModifiersState::SUPER, "v");
    w.named(NamedKey::Escape);
    assert!(w.text().starts_with("PASTED\nTEXT\n//! The notes parser."));
}

#[test]
fn gui_10_shift_tab_and_alt_keys_take_the_binary_s_paths() {
    let mut w = Window::open("README.md");
    // a second file, so there is a tab to go back to
    w.typed(":e src/main.rs\n");
    assert_eq!(w.h.session.sim.bufs.len(), 2);
    assert_eq!(w.h.session.sim.cur, 1);
    w.input.mods = ModifiersState::SHIFT;
    w.press(Key::Named(NamedKey::Tab), None);
    w.input.mods = ModifiersState::empty();
    assert_eq!(w.h.session.sim.cur, 0, "Shift-Tab goes back one file");
    // Alt-x is Esc then x, as it is in a terminal without the kitty protocol
    w.typed("i");
    w.input.mods = ModifiersState::ALT;
    w.press(Key::Character(SmolStr::new("x")), Some("x"));
    w.input.mods = ModifiersState::empty();
    assert_eq!(w.mode(), Mode::Normal, "the Esc of Alt-x left Insert mode");
}

#[test]
fn gui_10_pixels_are_cells_at_any_scale() {
    // the same pointer, on a 2x display: its pixels are twice as many, its cell the same
    let m2 = sim_gui::metrics::Metrics {
        cell_w: 20.0,
        cell_h: 40.0,
        pad_x: 32.0,
        pad_y: 32.0,
        scale: 2.0,
        ..TEST
    };
    let mut i = Input::new(m2, (80, 24));
    let moved = i.pointer(32.0 + 5.5 * 20.0, 32.0 + 3.5 * 40.0);
    assert!(moved.is_empty());
    let a = i.button(ElementState::Pressed, WButton::Left);
    match &a[0] {
        Action::Send(CtEvent::Mouse(m)) => assert_eq!((m.column, m.row), (5, 3)),
        other => panic!("{other:?}"),
    }
    let _ = PhysicalPosition::new(0.0, 0.0);
}
