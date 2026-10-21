//! ICON-1: the icons need a Nerd Font, and a terminal does not say whether it has one. They start
//! on only where the terminal is known to bring the symbols with it; a console, a dumb terminal or
//! a locale that is not UTF-8 never gets them; elsewhere they start off, once with a hint; and
//! what `zi` chose last always wins. With them off, no private-use code point is drawn anywhere.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::io::{Icons, icons_by, load_icons, save_icons, start_icons};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

/// A terminal's environment: `UTF-8` locale unless the pairs say otherwise.
fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string())
}

const UTF8: (&str, &str) = ("LANG", "en_US.UTF-8");

#[test]
fn icon_1_they_start_on_only_where_the_terminal_brings_the_symbols() {
    // Ghostty embeds a symbols-only Nerd Font (src/font/embedded.zig) and sets TERM_PROGRAM=ghostty
    // and GHOSTTY_RESOURCES_DIR; kitty has a builtin one (its FAQ) and sets KITTY_PID; WezTerm bundles
    // one (its font docs) and sets TERM_PROGRAM=WezTerm and WEZTERM_PANE. These are read where
    // tmux, which sets TERM_PROGRAM=tmux, hides the first of them.
    let on: [&[(&str, &str)]; 6] = [
        &[("TERM_PROGRAM", "ghostty")],
        &[("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")],
        &[("KITTY_PID", "123")],
        &[("TERM_PROGRAM", "WezTerm")],
        &[("WEZTERM_PANE", "0")],
        &[("TERM_PROGRAM", "tmux"), ("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")],
    ];
    for vars in on {
        let all: Vec<_> = vars.iter().copied().chain([UTF8, ("TERM", "xterm-256color")]).collect();
        assert_eq!(icons_by(env(&all)), Icons::On, "{vars:?}");
    }
    // anywhere else they start off, with the hint
    let hint: [&[(&str, &str)]; 4] = [
        &[],
        &[("TERM_PROGRAM", "iTerm.app")],
        &[("TERM_PROGRAM", "Apple_Terminal")],
        &[("TERM_PROGRAM", "tmux")],
    ];
    for vars in hint {
        let all: Vec<_> = vars.iter().copied().chain([UTF8, ("TERM", "xterm-256color")]).collect();
        assert_eq!(icons_by(env(&all)), Icons::Hint, "{vars:?}");
    }
}

#[test]
fn icon_1_a_console_a_dumb_terminal_or_a_locale_that_is_not_utf_8_never_gets_them() {
    for term in ["linux", "dumb", "vt100", "vt220", "vt525"] {
        // not even in a terminal that brings the symbols (a console over one is no such terminal)
        let all = [("TERM", term), ("TERM_PROGRAM", "ghostty"), UTF8];
        assert_eq!(icons_by(env(&all)), Icons::Off, "{term}");
    }
    let ghostty = ("TERM_PROGRAM", "ghostty");
    for vars in [
        vec![ghostty], // no locale at all: C
        vec![ghostty, ("LANG", "C")],
        vec![ghostty, ("LANG", "POSIX")],
        vec![ghostty, ("LANG", "en_US.ISO-8859-1")],
        vec![ghostty, ("LC_ALL", "C"), UTF8], // LC_ALL beats LANG
        vec![ghostty, ("LC_CTYPE", "ja_JP.eucJP"), UTF8], // LC_CTYPE beats LANG
    ] {
        assert_eq!(icons_by(env(&vars)), Icons::Off, "{vars:?}");
    }
    for vars in [
        vec![ghostty, ("LANG", "en_US.utf8")],
        vec![ghostty, ("LC_ALL", ""), UTF8], // an empty one is unset
        vec![ghostty, ("LC_ALL", "ja_JP.UTF-8"), ("LANG", "C")],
    ] {
        assert_eq!(icons_by(env(&vars)), Icons::On, "{vars:?}");
    }
}

#[test]
fn icon_1_what_zi_chose_last_always_wins_and_the_hint_comes_once() {
    let ghostty = [("TERM_PROGRAM", "ghostty"), UTF8, ("TERM", "xterm-256color")];
    let console = [("TERM", "linux"), UTF8];
    let plain = [UTF8, ("TERM", "xterm-256color")];
    let dir = temp_dir("icon-1-choice");
    // nothing chosen: the default, and the hint only where the default is off for want of a font
    assert_eq!(start_icons(Some(&dir.0), false, env(&ghostty)), (true, false));
    assert_eq!(
        start_icons(Some(&dir.0), false, env(&console)),
        (false, false),
        "a console is told nothing"
    );
    assert_eq!(start_icons(Some(&dir.0), false, env(&plain)), (false, true), "off, and the hint");
    assert_eq!(start_icons(Some(&dir.0), false, env(&plain)), (false, false), "the hint came once");
    // chosen: that, in any terminal, with no hint
    save_icons(&dir.0, true).unwrap();
    assert_eq!(load_icons(&dir.0), Some(true));
    for vars in [&ghostty[..], &console[..], &plain[..]] {
        assert_eq!(
            start_icons(Some(&dir.0), false, env(vars)),
            (true, false),
            "on stays on: {vars:?}"
        );
    }
    save_icons(&dir.0, false).unwrap();
    for vars in [&ghostty[..], &console[..], &plain[..]] {
        assert_eq!(
            start_icons(Some(&dir.0), false, env(vars)),
            (false, false),
            "off stays off: {vars:?}"
        );
    }
    // no state directory: no choice to read, and no way to say the hint was given: no hint
    assert_eq!(start_icons(None, false, env(&plain)), (false, false));
    assert_eq!(start_icons(None, false, env(&ghostty)), (true, false));
}

#[test]
fn icon_1_plain_mode_neither_reads_nor_writes_the_marker_and_the_editor_after_it_gets_the_hint() {
    let terminal = [UTF8, ("TERM", "xterm-256color")];
    let dir = temp_dir("icon-1-plain-marker");
    let marker = dir.0.join("icons-hint");
    // plain mode, run after run: no hint, and nothing made in the state directory
    for _ in 0..2 {
        assert_eq!(start_icons(Some(&dir.0), true, env(&terminal)), (false, false));
    }
    assert!(!marker.exists(), "plain mode used up the hint");
    // the editor's first run after them gets it, once
    assert_eq!(start_icons(Some(&dir.0), false, env(&terminal)), (false, true));
    assert_eq!(start_icons(Some(&dir.0), false, env(&terminal)), (false, false));
    // a plain run leaves the marker as it was; what `zi` chose is still what it starts with
    let was = std::fs::read(&marker).unwrap();
    assert_eq!(start_icons(Some(&dir.0), true, env(&terminal)), (false, false));
    assert_eq!(std::fs::read(&marker).unwrap(), was);
    save_icons(&dir.0, true).unwrap();
    assert_eq!(start_icons(Some(&dir.0), true, env(&terminal)), (true, false));
}

#[test]
fn icon_1_the_hint_names_the_key_and_the_font() {
    let hinted =
        |hint: bool| rig_env("icon-1-hint", &["a.txt"], &["a.txt"], (80, 24), |e| e.hint = hint);
    let r = hinted(true);
    assert!(r.status().contains("zi: icons (needs a Nerd Font)"), "{:?}", r.status());
    let mut r = hinted(true);
    r.keys("j");
    assert!(!r.status().contains("Nerd Font"), "the next key puts it out: {:?}", r.status());
    assert!(!hinted(false).status().contains("Nerd Font"));
}

#[test]
fn icon_1_plain_mode_never_shows_the_hint_asked_for_or_privileged() {
    // `zi` does nothing in plain mode (PLAIN-1), so the hint would name a key that is refused.
    let asked =
        rig_env("icon-1-plain", &["a.txt"], &["--plain", "a.txt"], (80, 24), |e| e.hint = true);
    let privileged = rig_env("icon-1-root", &["a.txt"], &["a.txt"], (80, 24), |e| {
        (e.hint, e.plain) = (true, true)
    });
    for (how, r) in [("--plain", asked), ("privileged", privileged)] {
        assert!(!r.status().contains("Nerd Font"), "{how}: {:?}", r.status());
        assert_eq!(r.status(), "a.txt  1:1  NORMAL  plain", "{how}: the status line says plain");
    }
}

/// A Rig with the interface drawn and the icons as asked, on a repository with a few files open.
fn dressed(name: &str, skin: Skin, icons: bool) -> Rig {
    let files = ["src/main.rs=fn main() {}\n", "src/lib.rs", "README.md=# t\n", ".gitignore=x\n"];
    let mut r = rig_env(name, &files, &[], (100, 30), |e| {
        e.skin = skin;
        e.chrome = true;
        e.icons = icons;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

fn every_row(r: &Rig) -> String {
    r.shot.frame.rows.iter().map(|row| row.text.clone() + "\n").collect()
}

#[test]
fn icon_1_with_the_icons_off_no_private_use_code_point_is_drawn_anywhere() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = dressed("icon-1-off", skin, false);
        // the tree, the tabs, the status line, a second tab, a search, the folders opened
        for step in ["", "<Down><Right>", "<CR>", "<C-w>l", "ix<Esc>", "<C-w>h<Down><CR>", "/fn\n"]
        {
            r.keys(step);
            assert_eq!(private_use(&every_row(&r)), None, "{skin:?} after {step:?}");
        }
    }
}

#[test]
fn icon_1_with_the_icons_on_they_are_drawn_and_zi_puts_them_out_and_back() {
    let mut r = dressed("icon-1-on", Skin::Depth, true);
    r.keys("<Down><CR>");
    assert!(private_use(&every_row(&r)).is_some(), "the tree has icons:\n{}", every_row(&r));
    r.keys("zi"); // (the text has the focus: the file was opened from the tree)
    assert_eq!(private_use(&every_row(&r)), None, "zi: off\n{}", every_row(&r));
    assert!(r.status().contains("icons off"), "{:?}", r.status());
    r.keys("zi");
    assert!(private_use(&every_row(&r)).is_some(), "zi: on");
}
