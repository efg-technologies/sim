//! UI-15: a char that is shown in another form (SAFE-8's `^[` and `<U+202E>`) is drawn in the alert
//! in flat and depth and inverse in calm, so that the literal text of the form is not mistaken for it.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use si::core::view::Style;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Painter, Screen};
use sim::sim::SimEvent;

const BG: Rgb = (0x1e, 0x1e, 0x1e);
/// A control char, a bidirectional one, and the literal text of each (the file's own chars).
const TEXT: &str = "a\u{1b}b \u{202e}c <U+202E> ^[ end\n";

fn shown(name: &str, skin: Skin) -> Rig {
    sized(name, skin, (60, 10))
}

fn sized(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let file = format!("h.txt={TEXT}");
    let mut r = rig_env(name, &[&file], &["h.txt"], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn col(text: &str, needle: &str) -> u16 {
    text[..text.find(needle).unwrap()].chars().count() as u16
}

fn alert_at(r: &Rig, y: usize, x: u16) -> bool {
    let t = Tones::new(&palette(BG));
    r.shot.paints[y].iter().any(|(c, p)| c.contains(&x) && p.fg == Some(t.alert_fg()))
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

#[test]
fn ui_15_in_flat_and_depth_a_hidden_char_is_in_the_alert_and_the_same_text_typed_is_not() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("hid-alert", skin);
        let y = r.shot.frame.rows.iter().position(|x| x.text.contains("^[b")).unwrap();
        let text = r.shot.frame.rows[y].text.clone();
        let (esc, bidi) = (col(&text, "^[b"), col(&text, "<U+202E>c"));
        for x in [esc, esc + 1, bidi, bidi + 4, bidi + 7] {
            assert!(alert_at(&r, y, x), "{skin:?}: cell {x} of {text:?} is the alert's");
        }
        // the literal text is eight and two ordinary chars
        let literal = text.rfind("<U+202E>").unwrap();
        let (literal, caret) = (text[..literal].chars().count() as u16, col(&text, "^[ end"));
        for x in [literal, literal + 3, literal + 7, caret, caret + 1] {
            assert!(!alert_at(&r, y, x), "{skin:?}: cell {x} is plain text");
        }
        assert!(
            !alert_at(&r, y, esc - 1) && !alert_at(&r, y, bidi + 8),
            "{skin:?}: and nothing around"
        );
        // the layout took the spans: the Painter has nothing left to draw inverse
        assert!(r.shot.frame.rows[y].spans.iter().all(|s| s.1 != Style::Hidden), "{skin:?}");
        assert_eq!(r.text(), TEXT.trim_end_matches('\n'), "the file's chars are the file's");
    }
}

#[test]
fn ui_15_in_calm_a_hidden_char_is_inverse_and_there_is_no_hue() {
    let r = shown("hid-calm", Skin::Calm);
    let y = r.shot.frame.rows.iter().position(|x| x.text.contains("^[b")).unwrap();
    let hidden: Vec<_> = r.shot.frame.rows[y]
        .spans
        .iter()
        .filter(|s| s.1 == Style::Hidden)
        .map(|s| s.0.clone())
        .collect();
    assert_eq!(hidden.len(), 2, "{:?}", r.shot.frame.rows[y]);
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm draws no paint, so no hue");
    let out = draw(r.shot.clone());
    let attrs = |what: &str| -> Vec<String> {
        let i = out.find(what).unwrap();
        let seq = out[..i].rsplit("\x1b[").next().unwrap();
        seq.trim_end_matches('m')
            .split(';')
            .take_while(|p| p.len() == 1)
            .map(String::from)
            .collect()
    };
    assert!(attrs("^[").contains(&"7".to_string()), "reverse video: {:?}", attrs("^["));
    assert!(attrs("<U+202E>").contains(&"7".to_string()));
    // the literal text after them is not
    let lit = out.rfind("<U+202E> ^[").unwrap();
    let seq = out[..lit].rsplit("\x1b[").next().unwrap();
    assert!(!seq.split(';').take_while(|p| p.len() == 1).any(|p| p == "7"), "{seq:?}");
}

#[test]
fn ui_15_the_pickers_preview_draws_them_the_same_way() {
    let mut r = sized("hid-preview", Skin::Flat, (130, 40));
    r.keys("<C-f>end");
    let w = r.win().unwrap();
    let pane = r.pane();
    let at = pane.iter().position(|l| l.contains("^[b")).unwrap();
    let y = usize::from(w.pane.y) + at;
    let text = &r.shot.frame.rows[y].text;
    let x = col(text, "^[b");
    assert!(alert_at(&r, y, x) && alert_at(&r, y, x + 1), "{text:?}");
    let literal = text[..text.rfind("<U+202E>").unwrap()].chars().count() as u16;
    assert!(!alert_at(&r, y, literal));
}
