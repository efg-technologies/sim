//! GUI-7, GUI-10
//! Window events to the events a terminal would have given sim. A key, an IME commit, a paste and
//! the mouse become the `crossterm` events that crossterm's parser makes of the bytes a terminal
//! sends (xterm keys, SGR 1006 mouse, bracketed paste); from there the path is the binary's own:
//! `si::io::term::convert_all` for the keys, `SimEvent::Mouse`, `SimEvent::BackTab`. Nothing here
//! knows what a key means to sim, so every key and mode works as it does in a terminal.
//!
//! Why events and not bytes: crossterm's parser is private to it and reads only the process's own
//! tty, so bytes could only be turned back into these same events by a second parser kept in step
//! with it. SPEC (GUI-10) has the table of what each input is.
use crate::api::*;
use crate::metrics::Metrics;
use winit::event::{ElementState, Ime, MouseButton as WButton, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// How many lines sim scrolls for one event of the wheel (`sim::sim::ptr::NOTCH`): a pixel-precise
/// device (a trackpad) sends one event for this many lines of finger travel, so the text follows it.
const NOTCH_LINES: f64 = 3.0;

/// What the adapter asks of the window's owner: an event for sim, or the system clipboard's text (a
/// paste key is not a byte: a terminal emulator reads the clipboard and sends a bracketed paste).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Send(CtEvent),
    Paste,
}

/// The text being composed by an input method, and where its cursor is (byte offsets), if the
/// method says. Drawn inline at sim's cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

/// A key press as the window gives it, with only the parts the mapping uses.
pub struct KeyIn<'a> {
    pub key: &'a Key,
    /// The key with no modifier applied (Ctrl-Shift-P is `p`), if the platform tells.
    pub bare: Option<&'a Key>,
    /// The text the press produced (dead keys and layouts applied).
    pub text: Option<&'a str>,
    pub state: ElementState,
}

pub struct Input {
    pub metrics: Metrics,
    pub grid: (u16, u16),
    pub mods: ModifiersState,
    pub preedit: Option<Preedit>,
    pos: Option<(f64, f64)>,
    cell: Option<(u16, u16)>,
    held: Vec<MouseButton>,
    wheel: (f64, f64),
}

impl Input {
    pub fn new(metrics: Metrics, grid: (u16, u16)) -> Input {
        Input {
            metrics,
            grid,
            mods: ModifiersState::empty(),
            preedit: None,
            pos: None,
            cell: None,
            held: vec![],
            wheel: (0.0, 0.0),
        }
    }

    /// A key of the window. Printable text is the text the press made (so a dead key or a layout
    /// is honored); a chord with Ctrl or Alt is the key itself, bare, as a terminal sends it.
    pub fn key(&mut self, k: KeyIn) -> Vec<Action> {
        if k.state != ElementState::Pressed {
            return vec![];
        }
        let m = self.mods;
        let (ctrl, alt, shift, logo) = (m.control_key(), m.alt_key(), m.shift_key(), m.super_key());
        let is_v = matches!(k.key, Key::Character(c) if c.eq_ignore_ascii_case("v"));
        let insert = matches!(k.key, Key::Named(NamedKey::Insert));
        if (is_v && ((logo && !ctrl && !alt) || (ctrl && shift && !alt)))
            || (insert && shift && !ctrl && !alt && !logo)
        {
            return vec![Action::Paste];
        }
        if logo {
            return vec![]; // the command key reaches no terminal program
        }
        let mut mods = KeyModifiers::NONE;
        mods.set(KeyModifiers::SHIFT, shift);
        mods.set(KeyModifiers::CONTROL, ctrl);
        mods.set(KeyModifiers::ALT, alt);
        let send = |code, mods| Action::Send(CtEvent::Key(KeyEvent::new(code, mods)));
        match k.key {
            Key::Named(n) => {
                use NamedKey::*;
                let code = match n {
                    Enter => KeyCode::Enter,
                    Escape => KeyCode::Esc,
                    Tab if shift => KeyCode::BackTab,
                    Tab => KeyCode::Tab,
                    Backspace => KeyCode::Backspace,
                    Delete => KeyCode::Delete,
                    ArrowUp => KeyCode::Up,
                    ArrowDown => KeyCode::Down,
                    ArrowLeft => KeyCode::Left,
                    ArrowRight => KeyCode::Right,
                    Home => KeyCode::Home,
                    End => KeyCode::End,
                    PageUp => KeyCode::PageUp,
                    PageDown => KeyCode::PageDown,
                    F1 => KeyCode::F(1),
                    Space => KeyCode::Char(' '),
                    _ => return vec![],
                };
                vec![send(code, mods)]
            }
            Key::Character(c) if ctrl || alt => {
                let bare = match k.bare {
                    Some(Key::Character(b)) => b.as_str(),
                    _ => c.as_str(),
                };
                let ch =
                    bare.chars().next().map(|ch| if ctrl { ch.to_ascii_lowercase() } else { ch });
                ch.map(|ch| send(KeyCode::Char(ch), mods)).into_iter().collect()
            }
            Key::Character(c) => {
                let text = k.text.filter(|t| !t.is_empty()).unwrap_or(c.as_str());
                text.chars().filter(|ch| !ch.is_control()).map(typed).map(Action::Send).collect()
            }
            _ => vec![], // a dead key, an unidentified one, a function key sim has no use for
        }
    }

    /// The input method's: preedit is kept for drawing at the cursor, a commit is the text typed.
    pub fn ime(&mut self, ime: &Ime) -> Vec<Action> {
        match ime {
            Ime::Preedit(text, cursor) => {
                self.preedit =
                    (!text.is_empty()).then(|| Preedit { text: text.clone(), cursor: *cursor });
                vec![]
            }
            Ime::Commit(text) => {
                self.preedit = None;
                text.chars()
                    .filter_map(|ch| match ch {
                        '\n' | '\r' => Some(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        '\t' => Some(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                        c if c.is_control() => None,
                        c => Some(typed_key(c)),
                    })
                    .map(|k| Action::Send(CtEvent::Key(k)))
                    .collect()
            }
            Ime::Enabled => vec![],
            Ime::Disabled => {
                self.preedit = None;
                vec![]
            }
        }
    }

    fn mouse(&self, kind: MouseEventKind) -> Action {
        let (column, row) = self.cell.unwrap_or((0, 0));
        let mut modifiers = KeyModifiers::NONE;
        modifiers.set(KeyModifiers::SHIFT, self.mods.shift_key());
        modifiers.set(KeyModifiers::CONTROL, self.mods.control_key());
        modifiers.set(KeyModifiers::ALT, self.mods.alt_key());
        Action::Send(CtEvent::Mouse(MouseEvent { kind, column, row, modifiers }))
    }

    /// The pointer moved. Like a terminal in mode 1002 this reports a move only with a button down,
    /// and only when it enters another cell.
    pub fn pointer(&mut self, x: f64, y: f64) -> Vec<Action> {
        self.pos = Some((x, y));
        let c = self.metrics.cell_at(x, y, self.grid.0, self.grid.1);
        let moved = self.cell != Some(c);
        self.cell = Some(c);
        match self.held.first() {
            Some(b) if moved => vec![self.mouse(MouseEventKind::Drag(*b))],
            _ => vec![],
        }
    }

    pub fn button(&mut self, state: ElementState, b: WButton) -> Vec<Action> {
        let b = match b {
            WButton::Left => MouseButton::Left,
            WButton::Right => MouseButton::Right,
            WButton::Middle => MouseButton::Middle,
            _ => return vec![],
        };
        if let Some((x, y)) = self.pos {
            self.cell = Some(self.metrics.cell_at(x, y, self.grid.0, self.grid.1));
        }
        match state {
            ElementState::Pressed => {
                self.held.push(b);
                vec![self.mouse(MouseEventKind::Down(b))]
            }
            ElementState::Released => {
                self.held.retain(|h| *h != b);
                vec![self.mouse(MouseEventKind::Up(b))]
            }
        }
    }

    /// The wheel: a notch is an event; a device that reports pixels sends one for each
    /// `NOTCH_LINES` lines of travel, and keeps the rest.
    pub fn wheel(&mut self, d: MouseScrollDelta) -> Vec<Action> {
        let mut out = vec![];
        let (mut up, mut right) = (0i64, 0i64);
        match d {
            MouseScrollDelta::LineDelta(x, y) => {
                let n = |v: f32| {
                    if v == 0.0 {
                        0
                    } else {
                        i64::from(v.signum() as i8) * (v.abs().round() as i64).max(1)
                    }
                };
                (right, up) = (n(x), n(y));
            }
            MouseScrollDelta::PixelDelta(p) => {
                let notch = NOTCH_LINES * f64::from(self.metrics.cell_h);
                self.wheel.0 += p.x;
                self.wheel.1 += p.y;
                while self.wheel.1 >= notch {
                    (self.wheel.1, up) = (self.wheel.1 - notch, up + 1);
                }
                while self.wheel.1 <= -notch {
                    (self.wheel.1, up) = (self.wheel.1 + notch, up - 1);
                }
                while self.wheel.0 >= notch {
                    (self.wheel.0, right) = (self.wheel.0 - notch, right + 1);
                }
                while self.wheel.0 <= -notch {
                    (self.wheel.0, right) = (self.wheel.0 + notch, right - 1);
                }
            }
        }
        for _ in 0..up.unsigned_abs() {
            out.push(self.mouse(if up > 0 {
                MouseEventKind::ScrollUp
            } else {
                MouseEventKind::ScrollDown
            }));
        }
        for _ in 0..right.unsigned_abs() {
            out.push(self.mouse(if right > 0 {
                MouseEventKind::ScrollRight
            } else {
                MouseEventKind::ScrollLeft
            }));
        }
        out
    }
}

/// A typed char as a terminal's key event: a capital carries Shift.
fn typed_key(ch: char) -> KeyEvent {
    let shift = if ch.is_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
    KeyEvent::new(KeyCode::Char(ch), shift)
}

fn typed(ch: char) -> CtEvent {
    CtEvent::Key(typed_key(ch))
}

/// A key event in the notation of sim's tests, for scripts: a char is itself, `<C-p>` is Ctrl-p,
/// `<Esc> <CR> <BS> <Tab> <S-Tab> <Del> <Up> <Down> <Left> <Right> <Home> <End> <PageUp> <PageDown>
/// <lt>` are the named keys.
pub fn notation(s: &str) -> Vec<CtEvent> {
    let key = |code, mods| CtEvent::Key(KeyEvent::new(code, mods));
    let none = KeyModifiers::NONE;
    let (mut out, mut rest) = (vec![], s);
    while let Some(c) = rest.chars().next() {
        let named =
            rest.strip_prefix('<').and_then(|r| r.split_once('>')).and_then(|(name, tail)| {
                let ev = match name {
                    "Esc" => key(KeyCode::Esc, none),
                    "CR" => key(KeyCode::Enter, none),
                    "BS" => key(KeyCode::Backspace, none),
                    "Tab" => key(KeyCode::Tab, none),
                    "S-Tab" => key(KeyCode::BackTab, KeyModifiers::SHIFT),
                    "Del" => key(KeyCode::Delete, none),
                    "Up" => key(KeyCode::Up, none),
                    "Down" => key(KeyCode::Down, none),
                    "Left" => key(KeyCode::Left, none),
                    "Right" => key(KeyCode::Right, none),
                    "Home" => key(KeyCode::Home, none),
                    "End" => key(KeyCode::End, none),
                    "PageUp" => key(KeyCode::PageUp, none),
                    "PageDown" => key(KeyCode::PageDown, none),
                    "lt" => key(KeyCode::Char('<'), none),
                    n if n.len() == 3 && n.starts_with("C-") => {
                        key(KeyCode::Char(n.chars().nth(2)?), KeyModifiers::CONTROL)
                    }
                    _ => return None,
                };
                Some((ev, tail))
            });
        if let Some((ev, tail)) = named {
            out.push(ev);
            rest = tail;
            continue;
        }
        out.push(match c {
            '\n' => key(KeyCode::Enter, none),
            '\x1b' => key(KeyCode::Esc, none),
            '\t' => key(KeyCode::Tab, none),
            c => typed(c),
        });
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The path from a terminal event into sim: sim's own (`Input::event`: the mouse, Shift-Tab and F1, and
/// the core's `convert_all` for the rest), without the state machines that wait for a terminal's replies
/// (a window has no terminal to answer, and an Alt-`]` is a key).
pub fn to_sim(ev: CtEvent) -> Vec<SimEvent> {
    SimInput::event(ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::TEST;
    use winit::keyboard::SmolStr;

    fn input() -> Input {
        Input::new(TEST, (80, 24))
    }

    fn ch(s: &str) -> Key {
        Key::Character(SmolStr::new(s))
    }

    fn press(i: &mut Input, key: Key, text: Option<&str>) -> Vec<Action> {
        i.key(KeyIn { key: &key, bare: None, text, state: ElementState::Pressed })
    }

    fn keys(a: Vec<Action>) -> Vec<(KeyCode, KeyModifiers)> {
        a.into_iter()
            .map(|a| match a {
                Action::Send(CtEvent::Key(k)) => (k.code, k.modifiers),
                other => panic!("not a key: {other:?}"),
            })
            .collect()
    }

    /// What sim gets for a window key: the whole path.
    fn sim_keys(i: &mut Input, key: Key, text: Option<&str>) -> Vec<SiKey> {
        let evs: Vec<SimEvent> = press(i, key, text)
            .into_iter()
            .filter_map(|a| if let Action::Send(e) = a { Some(e) } else { None })
            .flat_map(to_sim)
            .collect();
        evs.into_iter()
            .map(|e| match e {
                SimEvent::Term(TermEvent::Key(k)) => k,
                other => panic!("not a key for sim: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn gui_10_a_letter_is_the_char_the_layout_made() {
        let mut i = input();
        assert_eq!(
            keys(press(&mut i, ch("a"), Some("a"))),
            [(KeyCode::Char('a'), KeyModifiers::NONE)]
        );
        assert_eq!(
            keys(press(&mut i, ch("A"), Some("A"))),
            [(KeyCode::Char('A'), KeyModifiers::SHIFT)]
        );
        // a dead-key result is its text, not the key that was pressed
        assert_eq!(
            keys(press(&mut i, ch("e"), Some("é"))),
            [(KeyCode::Char('é'), KeyModifiers::NONE)]
        );
        assert_eq!(sim_keys(&mut i, ch(":"), Some(":")), [SiKey::Char(':')]);
    }

    #[test]
    fn gui_10_space_is_a_char() {
        let mut i = input();
        assert_eq!(sim_keys(&mut i, Key::Named(NamedKey::Space), Some(" ")), [SiKey::Char(' ')]);
    }

    #[test]
    fn gui_10_control_p_reaches_sim_as_its_own_key() {
        let mut i = input();
        i.mods = ModifiersState::CONTROL;
        assert_eq!(
            keys(press(&mut i, ch("p"), Some("\u{10}"))),
            [(KeyCode::Char('p'), KeyModifiers::CONTROL)]
        );
        assert_eq!(sim_keys(&mut i, ch("p"), Some("\u{10}")), [SiKey::Ctrl('p')]);
        assert_eq!(sim_keys(&mut i, ch("n"), None), [SiKey::Ctrl('n')]);
        // Ctrl-Shift-F is Ctrl-f to sim, as it is in a terminal
        i.mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(sim_keys(&mut i, ch("F"), Some("\u{6}")), [SiKey::Ctrl('f')]);
    }

    #[test]
    fn gui_10_control_h_and_control_bracket_are_si_s_backspace_and_escape() {
        let mut i = input();
        i.mods = ModifiersState::CONTROL;
        assert_eq!(sim_keys(&mut i, ch("h"), None), [SiKey::Backspace]);
        assert_eq!(sim_keys(&mut i, ch("["), None), [SiKey::Esc]);
    }

    #[test]
    fn gui_10_the_named_keys_are_the_keys_of_a_terminal() {
        use NamedKey::*;
        let mut i = input();
        let want = [
            (Enter, SiKey::Enter),
            (Escape, SiKey::Esc),
            (Tab, SiKey::Tab),
            (Backspace, SiKey::Backspace),
            (Delete, SiKey::Delete),
            (ArrowUp, SiKey::Up),
            (ArrowDown, SiKey::Down),
            (ArrowLeft, SiKey::Left),
            (ArrowRight, SiKey::Right),
            (Home, SiKey::Home),
            (End, SiKey::End),
            (PageUp, SiKey::PageUp),
            (PageDown, SiKey::PageDown),
        ];
        for (named, key) in want {
            assert_eq!(sim_keys(&mut i, Key::Named(named), None), [key], "{named:?}");
        }
    }

    #[test]
    fn gui_10_shift_tab_is_the_back_tab_sim_has_its_own_event_for() {
        let mut i = input();
        i.mods = ModifiersState::SHIFT;
        let sent: Vec<_> = press(&mut i, Key::Named(NamedKey::Tab), None)
            .into_iter()
            .filter_map(|a| if let Action::Send(e) = a { Some(e) } else { None })
            .flat_map(to_sim)
            .collect();
        assert!(matches!(sent[..], [SimEvent::BackTab]), "{sent:?}");
    }

    #[test]
    fn gui_10_alt_x_is_escape_then_x_as_the_binary_makes_it() {
        let mut i = input();
        i.mods = ModifiersState::ALT;
        assert_eq!(sim_keys(&mut i, ch("x"), Some("x")), [SiKey::Esc, SiKey::Char('x')]);
    }

    #[test]
    fn gui_10_a_release_a_bare_modifier_and_a_dead_key_send_nothing() {
        let mut i = input();
        let up = i.key(KeyIn {
            key: &ch("a"),
            bare: None,
            text: Some("a"),
            state: ElementState::Released,
        });
        assert!(up.is_empty());
        assert!(press(&mut i, Key::Named(NamedKey::Shift), None).is_empty());
        assert!(press(&mut i, Key::Named(NamedKey::Control), None).is_empty());
        assert!(press(&mut i, Key::Dead(Some('`')), None).is_empty());
        assert!(press(&mut i, Key::Named(NamedKey::F5), None).is_empty());
    }

    #[test]
    fn gui_10_the_command_key_sends_no_key_to_sim() {
        let mut i = input();
        i.mods = ModifiersState::SUPER;
        assert!(press(&mut i, ch("s"), Some("s")).is_empty());
    }

    #[test]
    fn gui_10_paste_shortcuts_ask_for_the_clipboard() {
        let mut i = input();
        i.mods = ModifiersState::SUPER;
        assert_eq!(press(&mut i, ch("v"), Some("v")), [Action::Paste]);
        i.mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(press(&mut i, ch("V"), Some("\u{16}")), [Action::Paste]);
        i.mods = ModifiersState::SHIFT;
        assert_eq!(press(&mut i, Key::Named(NamedKey::Insert), None), [Action::Paste]);
        // a plain Ctrl-v is a key of sim's (Visual block in vi)
        i.mods = ModifiersState::CONTROL;
        assert_eq!(sim_keys(&mut i, ch("v"), None), [SiKey::Ctrl('v')]);
    }

    #[test]
    fn gui_10_a_paste_is_bracketed_paste_to_sim() {
        let evs = to_sim(CtEvent::Paste("a\nb".into()));
        assert!(matches!(&evs[..], [SimEvent::Term(TermEvent::Paste(s))] if s == "a\nb"));
    }

    #[test]
    fn gui_10_an_ime_commit_is_the_text_typed_a_char_at_a_time() {
        let mut i = input();
        let sent = i.ime(&Ime::Commit("日本語".into()));
        assert_eq!(
            keys(sent),
            [
                (KeyCode::Char('日'), KeyModifiers::NONE),
                (KeyCode::Char('本'), KeyModifiers::NONE),
                (KeyCode::Char('語'), KeyModifiers::NONE)
            ]
        );
    }

    #[test]
    fn gui_10_a_commit_keeps_a_line_break_and_drops_the_other_controls() {
        let mut i = input();
        let sent = keys(i.ime(&Ime::Commit("a\n\u{7}b\t".into())));
        let codes: Vec<_> = sent.into_iter().map(|k| k.0).collect();
        assert_eq!(codes, [KeyCode::Char('a'), KeyCode::Enter, KeyCode::Char('b'), KeyCode::Tab]);
    }

    #[test]
    fn gui_10_preedit_is_kept_for_the_cursor_and_sends_nothing_until_it_is_committed() {
        let mut i = input();
        assert!(i.ime(&Ime::Preedit("にほ".into(), Some((0, 3)))).is_empty());
        assert_eq!(i.preedit.as_ref().map(|p| p.text.as_str()), Some("にほ"));
        assert!(i.ime(&Ime::Preedit(String::new(), None)).is_empty());
        assert!(i.preedit.is_none());
        i.ime(&Ime::Preedit("に".into(), None));
        let sent = i.ime(&Ime::Commit("日".into()));
        assert_eq!(sent.len(), 1);
        assert!(i.preedit.is_none(), "a commit ends the preedit");
        i.ime(&Ime::Preedit("x".into(), None));
        i.ime(&Ime::Disabled);
        assert!(i.preedit.is_none());
    }

    fn at(col: f64, row: f64) -> (f64, f64) {
        // the middle of a cell of the test metrics (10 x 20, margin 16)
        (16.0 + (col + 0.5) * 10.0, 16.0 + (row + 0.5) * 20.0)
    }

    fn mouse(a: Vec<Action>) -> Vec<(MouseEventKind, u16, u16)> {
        a.into_iter()
            .map(|a| match a {
                Action::Send(CtEvent::Mouse(m)) => (m.kind, m.column, m.row),
                other => panic!("not a mouse event: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn gui_10_a_click_is_a_press_and_a_release_at_the_cell() {
        let mut i = input();
        let (x, y) = at(3.0, 2.0);
        assert!(i.pointer(x, y).is_empty(), "a terminal reports no motion without a button");
        assert_eq!(
            mouse(i.button(ElementState::Pressed, WButton::Left)),
            [(MouseEventKind::Down(MouseButton::Left), 3, 2)]
        );
        assert_eq!(
            mouse(i.button(ElementState::Released, WButton::Left)),
            [(MouseEventKind::Up(MouseButton::Left), 3, 2)]
        );
    }

    #[test]
    fn gui_10_a_drag_is_reported_when_the_cell_changes_and_not_inside_one() {
        let mut i = input();
        let (x, y) = at(3.0, 2.0);
        i.pointer(x, y);
        i.button(ElementState::Pressed, WButton::Left);
        assert!(i.pointer(x + 2.0, y + 3.0).is_empty(), "same cell");
        let (x2, y2) = at(6.0, 2.0);
        assert_eq!(mouse(i.pointer(x2, y2)), [(MouseEventKind::Drag(MouseButton::Left), 6, 2)]);
        assert!(i.pointer(x2 + 1.0, y2).is_empty());
        i.button(ElementState::Released, WButton::Left);
        let (x3, y3) = at(9.0, 4.0);
        assert!(i.pointer(x3, y3).is_empty(), "no button, no motion");
    }

    #[test]
    fn gui_10_a_press_outside_the_grid_lands_on_its_edge_cell() {
        let mut i = input();
        i.pointer(1.0, 1.0);
        let m = mouse(i.button(ElementState::Pressed, WButton::Right));
        assert_eq!(m, [(MouseEventKind::Down(MouseButton::Right), 0, 0)]);
    }

    #[test]
    fn gui_10_modifiers_go_with_the_mouse() {
        let mut i = input();
        i.mods = ModifiersState::SHIFT | ModifiersState::CONTROL;
        let (x, y) = at(1.0, 1.0);
        i.pointer(x, y);
        let sent = i.button(ElementState::Pressed, WButton::Left);
        let Action::Send(CtEvent::Mouse(m)) = &sent[0] else { panic!("{sent:?}") };
        assert_eq!(m.modifiers, KeyModifiers::SHIFT | KeyModifiers::CONTROL);
    }

    #[test]
    fn gui_10_a_wheel_notch_is_one_event_and_the_direction_is_the_terminal_s() {
        let mut i = input();
        let (x, y) = at(5.0, 5.0);
        i.pointer(x, y);
        assert_eq!(
            mouse(i.wheel(MouseScrollDelta::LineDelta(0.0, 1.0))),
            [(MouseEventKind::ScrollUp, 5, 5)]
        );
        assert_eq!(
            mouse(i.wheel(MouseScrollDelta::LineDelta(0.0, -2.0))),
            [(MouseEventKind::ScrollDown, 5, 5), (MouseEventKind::ScrollDown, 5, 5)]
        );
    }

    #[test]
    fn gui_10_a_trackpad_scrolls_one_event_for_three_lines_of_travel() {
        use winit::dpi::PhysicalPosition;
        let mut i = input();
        let (x, y) = at(5.0, 5.0);
        i.pointer(x, y);
        let px = |dy: f64| MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, dy));
        // three lines of the test metrics are 60 px
        assert!(i.wheel(px(-25.0)).is_empty());
        assert!(i.wheel(px(-25.0)).is_empty());
        assert_eq!(mouse(i.wheel(px(-25.0))), [(MouseEventKind::ScrollDown, 5, 5)]);
        // the rest (-15 px) is kept for the next move
        assert!(i.wheel(px(40.0)).is_empty());
        assert_eq!(mouse(i.wheel(px(40.0))), [(MouseEventKind::ScrollUp, 5, 5)]);
    }

    #[test]
    fn gui_10_a_resize_is_the_size_in_cells() {
        let i = input();
        assert_eq!(i.metrics.grid_for(832, 512), (80, 24));
    }

    #[test]
    fn gui_10_the_notation_of_scripts_is_the_notation_of_sim_s_tests() {
        let evs = notation("i<C-p>a<Esc><CR>");
        let codes: Vec<_> = evs
            .iter()
            .map(|e| match e {
                CtEvent::Key(k) => (k.code, k.modifiers),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            codes,
            [
                (KeyCode::Char('i'), KeyModifiers::NONE),
                (KeyCode::Char('p'), KeyModifiers::CONTROL),
                (KeyCode::Char('a'), KeyModifiers::NONE),
                (KeyCode::Esc, KeyModifiers::NONE),
                (KeyCode::Enter, KeyModifiers::NONE)
            ]
        );
        let back = notation("<S-Tab>");
        assert!(matches!(&back[0], CtEvent::Key(k) if k.code == KeyCode::BackTab));
    }
}
