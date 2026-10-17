//! UI-6: whitespace drawn in a dim color, with marks that no code has: a tab as `»` and blanks, a
//! trailing blank as `·`, a no-break space as `⍽`, nothing at the end of a line, and none of it in
//! the text.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::Paint;
use sim::sim::SimEvent;
use unicode_width::UnicodeWidthChar;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn body(name: &str, skin: Skin, text: &str, size: (u16, u16)) -> Rig {
    let file = format!("a.txt={text}");
    let mut r = rig_env(name, &[&file], &["a.txt"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

/// The text of the screen row of line `n` (1-based), from the first column of the text.
fn line(r: &Rig, n: usize) -> String {
    let g = r.sim.geo();
    let y = usize::from(g.y0) + n - 1;
    let text: String = r.shot.frame.rows[y].text.chars().skip(usize::from(g.tx()) + 5).collect();
    text.trim_end_matches(['┃', '│', ' ', '⎺', '⎻', '⎼', '⎽']).to_string()
}

fn paint(r: &Rig, y: usize, x: u16) -> Paint {
    let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold(Paint::default(), |a, p| Paint {
        fg: a.fg.or(p.fg),
        bg: a.bg.or(p.bg),
        fade: if a.fade > 0 { a.fade } else { p.fade },
        bold: a.bold || p.bold,
        soft: a.soft || p.soft,
    })
}

#[test]
fn ui_6_no_line_ends_with_a_mark_and_a_blank_inside_a_line_is_a_blank() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui6-eol", skin, "one two\n\nthree\n", (60, 12));
        assert_eq!(line(&r, 1), "one two", "{skin:?}");
        assert_eq!(line(&r, 2), "", "{skin:?}: an empty line too");
        assert_eq!(line(&r, 3), "three", "{skin:?}");
        assert!(
            !r.shot.frame.rows.iter().any(|x| x.text.contains('↲')),
            "{skin:?}: no `↲` anywhere"
        );
        // the marks of `listchars` that are chars of code are not drawn either
        assert!(!r.shot.frame.rows.iter().any(|x| x.text.contains('%')));
        assert_eq!(r.text(), "one two\n\nthree");
    }
}

#[test]
fn ui_6_trailing_blanks_are_dots_and_the_blanks_before_the_text_are_not() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui6-trail", skin, "a  \nb c \n   \n", (60, 12));
        assert_eq!(line(&r, 1), "a··", "{skin:?}");
        assert_eq!(line(&r, 2), "b c·", "{skin:?}: the blank between words stays a blank");
        assert_eq!(line(&r, 3), "···", "{skin:?}: a line of blanks is trailing blanks");
        let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
        assert_eq!(paint(&r, usize::from(g.y0), g.tx() + 5 + 1).fg, Some(t.line), "{skin:?}: dim");
    }
}

#[test]
fn ui_6_a_tab_is_a_chevron_and_blanks_to_the_width_it_is_shown_at_and_a_no_break_space_a_bowl() {
    let r = body("ui6-tab", Skin::Flat, "\tx\nab\tcd\n\t\t\n\u{a0}a\u{a0}b\n", (60, 12));
    assert_eq!(line(&r, 1), "»   x", "no `-` fill: a tab is a chevron and blanks");
    assert_eq!(line(&r, 2), "ab» cd", "to the next multiple of 4");
    assert_eq!(line(&r, 3), "»   »", "(the last tab's blanks are blanks)");
    assert_eq!(line(&r, 4), "⍽a⍽b", "not a `%`");
    assert_eq!(r.text(), "\tx\nab\tcd\n\t\t\n\u{a0}a\u{a0}b");
}

#[test]
fn ui_6_the_marks_are_where_the_chars_are_after_wide_ones_and_a_wrapped_line_has_none_at_its_end() {
    let r = body("ui6-wide", Skin::Flat, "日本 \n", (60, 12));
    assert_eq!(line(&r, 1), "日本·");
    let g = r.sim.geo();
    // The dot is on the column after the two wide chars.
    let y = usize::from(g.y0);
    let text = &r.shot.frame.rows[y].text;
    let cols: Vec<(char, u16)> = text
        .chars()
        .scan(0u16, |c, ch| {
            let at = *c;
            *c += ch.width().unwrap_or(0) as u16;
            Some((ch, at))
        })
        .collect();
    let dot = cols.iter().find(|(c, _)| *c == '·').unwrap().1;
    assert_eq!(dot, g.tx() + 5 + 4);
    let t = Tones::new(&palette(BG));
    assert_eq!(paint(&r, y, dot).fg, Some(t.line));
    // A line that wraps ends once, with nothing after its last char
    let long = "word ".repeat(20) + "end";
    let r = body("ui6-wrapped", Skin::Flat, &format!("{long}\n"), (50, 12));
    let y =
        (0..r.shot.frame.rows.len()).find(|&y| r.shot.frame.rows[y].text.contains("end")).unwrap();
    assert!(!r.shot.frame.rows[y].text.contains("end↲"));
    assert!(!r.shot.frame.rows.iter().any(|x| x.text.contains('↲')));
}

#[test]
fn ui_6_calm_and_the_preview_show_no_marks_and_a_click_on_a_mark_is_a_click_on_its_char() {
    let calm = rig_env("ui6-calm", &["a.txt=a \tb\n"], &["a.txt"], (60, 12), |_| {});
    assert!(!calm.screen.rows.iter().any(|x| x.text.contains('·') || x.text.contains('»')));
    let mut r = body("ui6-click", Skin::Flat, "ab\tcd\n", (60, 12));
    let g = r.sim.geo();
    r.click(g.tx() + 5 + 3, g.y0); // on the blank after the chevron: the tab
    assert_eq!(r.app().buf.cursor, 2);
}
