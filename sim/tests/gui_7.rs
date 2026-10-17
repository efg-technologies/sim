//! GUI-7: `Input::event` is what a terminal event is to sim when no reply of a terminal is waited
//! for, which a window has none to wait for: the mouse, Shift-Tab and F1 as sim's own events, and the
//! rest as the core's `convert_all` makes it. `Input::feed` goes through it once it has held back what
//! could be a reply, so the terminal and a window decide what a key is in one place.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
use crossterm::event::{
    Event as Ev, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use sim::color::Input;
use sim::sim::SimEvent;
use sim_core::core::app::{Event, Key};

fn key(code: KeyCode, mods: KeyModifiers) -> Ev {
    Ev::Key(KeyEvent::new(code, mods))
}

fn keys(evs: &[SimEvent]) -> Vec<Key> {
    evs.iter()
        .map(|e| match e {
            SimEvent::Term(Event::Key(k)) => *k,
            other => panic!("not a key: {other:?}"),
        })
        .collect()
}

#[test]
fn gui_7_a_key_is_the_core_s_key() {
    let a = Input::event(key(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(keys(&a), [Key::Char('a')]);
    let c = Input::event(key(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(keys(&c), [Key::Ctrl('p')]);
    // the core's own rules: Ctrl-h is Backspace and Ctrl-[ is Esc
    assert_eq!(
        keys(&Input::event(key(KeyCode::Char('h'), KeyModifiers::CONTROL))),
        [Key::Backspace]
    );
    assert_eq!(keys(&Input::event(key(KeyCode::Char('['), KeyModifiers::CONTROL))), [Key::Esc]);
    assert_eq!(keys(&Input::event(key(KeyCode::Enter, KeyModifiers::NONE))), [Key::Enter]);
}

#[test]
fn gui_7_alt_x_is_escape_then_x_and_a_paste_and_a_resize_are_the_core_s() {
    let alt = Input::event(key(KeyCode::Char('x'), KeyModifiers::ALT));
    assert_eq!(keys(&alt), [Key::Esc, Key::Char('x')]);
    let paste = Input::event(Ev::Paste("one\ntwo".into()));
    assert!(
        matches!(&paste[..], [SimEvent::Term(Event::Paste(t))] if t == "one\ntwo"),
        "{paste:?}"
    );
    let size = Input::event(Ev::Resize(120, 40));
    assert!(matches!(&size[..], [SimEvent::Term(Event::Resize(120, 40))]), "{size:?}");
}

#[test]
fn gui_7_the_mouse_shift_tab_and_f1_are_sim_s_own_events() {
    let m = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 4,
        modifiers: KeyModifiers::NONE,
    };
    let ev = Input::event(Ev::Mouse(m));
    assert!(matches!(&ev[..], [SimEvent::Mouse(got)] if *got == m), "{ev:?}");
    let back = Input::event(key(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(matches!(&back[..], [SimEvent::BackTab]), "{back:?}");
    let help = Input::event(key(KeyCode::F(1), KeyModifiers::NONE));
    assert!(matches!(&help[..], [SimEvent::Help]), "{help:?}");
}

#[test]
fn gui_7_nothing_is_held_back_for_a_reply_that_a_window_never_waits_for() {
    // Alt-] begins the terminal's answer to a color query: `feed` holds it back and waits, `event`
    // never does (a window has no terminal to answer), so the key is Esc then ] at once
    let alt_close = key(KeyCode::Char(']'), KeyModifiers::ALT);
    assert_eq!(keys(&Input::event(alt_close.clone())), [Key::Esc, Key::Char(']')]);
    let mut term = Input::new(true);
    assert!(term.feed(alt_close).is_empty(), "the terminal's path waits for the rest of the reply");
}

#[test]
fn gui_7_the_terminal_s_path_ends_in_the_same_events() {
    // a terminal that asked for nothing (`Input::new(false)`) takes every event the way `event` does
    let mut term = Input::new(false);
    let evs = [
        key(KeyCode::Char('a'), KeyModifiers::NONE),
        key(KeyCode::Char('x'), KeyModifiers::ALT),
        key(KeyCode::Char('r'), KeyModifiers::CONTROL),
        key(KeyCode::Tab, KeyModifiers::NONE),
        key(KeyCode::BackTab, KeyModifiers::SHIFT),
        key(KeyCode::F(1), KeyModifiers::NONE),
        key(KeyCode::Esc, KeyModifiers::NONE),
        Ev::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    ];
    for ev in evs {
        let (a, b) = (term.feed(ev.clone()), Input::event(ev.clone()));
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "{ev:?}");
    }
}
